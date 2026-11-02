//! HTML → LWC template IR via parse5-rust.

use crate::config::NormalizedConfig;
use lwc_shared::template_ir::*;
use lwc_shared::{CompilerDiagnostic, DiagnosticLevel};
use parse5::{Location, NodeData, NodeRef, ParserOptions};
use std::collections::HashSet;

mod tables;
mod expr_check;
mod validate;
pub(crate) mod element_check;
/// Diagnostic table. validate.rs still carries its own `#[path]` copy until its owner switches to
/// `super::messages`; this copy is unused until then.
#[allow(dead_code)]
mod messages;

/// Parser state threaded through lowering (TS `ParserCtx` subset).
pub(crate) struct Ctx {
    diags: Vec<CompilerDiagnostic>,
    /// Slot names seen in the current if-chain scope (TS `seenSlots`, approximated).
    seen_slots: HashSet<String>,
    /// Direct children of a `<template for:each|iterator:*>`.
    in_loop: bool,
    /// Nesting depth of for:each / iterator:* (TS `isInIteration`).
    iter_depth: u32,
    /// Nearest rendered element/slot ancestor, reset at components (TS `validateSlotAttribute`).
    slot_ancestor: Option<String>,
    /// TS parseScopedSlotFragment 1178: the nearest ancestor, skipping lwc:if/elseif/else blocks,
    /// is a Component or `lwc:component`.
    slot_data_parent_ok: bool,
    /// TS `seenScopedSlots` (not if-context aware).
    seen_scoped_slots: HashSet<String>,
    /// Root `lwc:render-mode="light"`.
    light: bool,
    /// Inside a ScopedSlotFragment (any depth): TS validateChildren applies 1177 to every
    /// template directive's direct children there.
    in_slot_fragment: bool,
    /// TS `ctx.instrumentation`.
    instrumentation: Option<lwc_shared::InstrumentationHandle>,
    /// Template source as UTF-16 code units (parse5 offsets index this); see [`raw_attr`] / [`raw_source`].
    src16: Vec<u16>,
    /// TS `seenIds` (1042).
    seen_ids: HashSet<String>,
    /// Nearest `iterator:<name>` in the current element scope (TS getForOfParent; reset under base elements).
    for_of: Option<String>,
    /// Nearest `for:each` (its index name) in the current element scope (TS getForEachParent).
    for_each: Option<Option<String>>,
    /// TS `getSiblingIfNode()` is an open IfBlock/ElseifBlock chain among the current siblings.
    chain_open: bool,
}

impl Ctx {
    fn count(&self, m: lwc_shared::CompilerMetric) { if let Some(i) = &self.instrumentation { i.increment_counter(m); } }
    fn push(&mut self, d: CompilerDiagnostic) { self.diags.push(d); }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateParseResult {
    pub root: Option<Root>,
    pub warnings: Vec<CompilerDiagnostic>,
}

fn loc(l: &Location) -> SourceLocation {
    SourceLocation {
        start_line: l.start_line as u32, start_column: l.start_col as u32,
        end_line: l.end_line as u32, end_column: l.end_col as u32,
        start: Some(l.start_offset as u32), end: Some(l.end_offset as u32),
    }
}

fn node_loc(n: &NodeRef) -> SourceLocation {
    n.borrow().source_code_location().map(|l| loc(&l.location)).unwrap_or_default()
}

/// TS `cleanTextNode` (babel JSX text cleaning): per line, tabs become spaces, leading spaces are
/// trimmed except on the first line and trailing spaces except on the last; non-empty lines are joined
/// with a single space after every line but the last non-empty one.
fn clean_text_node(value: &str) -> String {
    let lines: Vec<&str> = value.split("\r\n").flat_map(|l| l.split(['\n', '\r'])).collect();
    let last_non_empty = lines.iter().rposition(|l| l.chars().any(|c| c != ' ' && c != '\t')).unwrap_or(0);
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let mut t = line.replace('\t', " ");
        if i != 0 { t = t.trim_start_matches(' ').to_string(); }
        if i != lines.len() - 1 { t = t.trim_end_matches(' ').to_string(); }
        if !t.is_empty() {
            if i != last_non_empty { t.push(' '); }
            out.push_str(&t);
        }
    }
    out
}

/// JS `String.prototype.trim` emptiness (Unicode whitespace plus U+FEFF).
fn js_blank(s: &str) -> bool { s.chars().all(|c| c.is_whitespace() || c == '\u{feff}') }

/// TS parseTextNode: `cleanTextNode(rawSource)`; no node when it is blank. The emptiness test reads the
/// undecoded source (`&nbsp;` keeps a node); the returned text is the cleaned decoded value.
fn clean_text(value: &str, raw: &str) -> Option<String> {
    let probe = if raw.is_empty() { value } else { raw };
    if js_blank(&clean_text_node(probe)) { return None; }
    Some(clean_text_node(value))
}

/// Split text into alternating literal / `{expr}` segments (brace-balanced). Unclosed `{` stays literal.
fn split_mustache(s: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if c != '{' { lit.push(c); continue; }
        let mut depth = 1;
        let mut end = None;
        for (j, cj) in s[i + 1..].char_indices() {
            match cj { '{' => depth += 1, '}' => { depth -= 1; if depth == 0 { end = Some(i + 1 + j); break; } } _ => {} }
        }
        match end {
            Some(e) => {
                if !lit.is_empty() { out.push((std::mem::take(&mut lit), false)); }
                out.push((s[i..=e].to_string(), true));
                while matches!(it.peek(), Some(&(k, _)) if k <= e) { it.next(); }
            }
            None => lit.push(c),
        }
    }
    if !lit.is_empty() { out.push((lit, false)); }
    out
}

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(f) if f.is_ascii_alphabetic() || f == '_' || f == '$')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// `{a}` / `{a.b.c}` → Identifier / nested MemberExpression. No brackets/calls yet.
pub(crate) fn parse_binding(v: &str) -> Option<Expression> {
    let inner = v.strip_prefix('{')?.strip_suffix('}')?.trim();
    let (e, rest) = member_chain(inner)?;
    rest.trim().is_empty().then_some(e)
}

/// `primary ('.' ident)*` with `primary := ident | '(' member_chain ')'`. Parentheses are transparent, as in
/// the TS (acorn/ESTree) AST: `((foo).bar)` is the member expression `foo.bar`.
fn member_chain(s: &str) -> Option<(Expression, &str)> {
    let s = s.trim_start();
    let (mut e, mut rest) = if let Some(r) = s.strip_prefix('(') {
        let (e, r) = member_chain(r)?;
        (e, r.trim_start().strip_prefix(')')?)
    } else {
        let end = s.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(s.len());
        let first = &s[..end];
        if !is_ident(first) || expr_check::is_reserved(first) { return None; }
        (Expression::Identifier(Identifier { name: first.to_string() }), &s[end..])
    };
    loop {
        let t = rest.trim_start();
        let Some(r) = t.strip_prefix('.') else { return Some((e, rest)) };
        let r = r.trim_start();
        let end = r.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(r.len());
        let p = &r[..end];
        if !is_ident(p) { return None; }
        e = Expression::Member(MemberExpression {
            object: Box::new(e),
            property: Box::new(Expression::Identifier(Identifier { name: p.to_string() })),
            computed: false,
        });
        rest = &r[end..];
    }
}

fn attr_loc(n: &NodeRef, name: &str) -> SourceLocation {
    n.borrow().source_code_location().and_then(|l| l.attrs.get(name).or_else(|| l.attrs.get(&name.to_lowercase())).map(loc)).unwrap_or_default()
}

/// One attribute as written in the source (TS `getTemplateAttribute` inputs).
pub(crate) struct RawAttr {
    /// Source text of the whole attribute (`name="v"`, entities undecoded), trailing whitespace trimmed
    /// (parse5-rust stretches the last attribute's end over whitespace before `>`).
    pub raw: String,
    /// Text after the first `=` (quotes kept, undecoded); `None` when there is no `=`.
    pub raw_value: Option<String>,
    /// TS `isQuotedAttribute(rawAttrVal)`: starts and ends with `"`.
    pub quoted: bool,
    pub location: SourceLocation,
}

/// Raw source of attribute `a` on element `n`. Falls back to `name="value"` when parse5 has no location.
pub(crate) fn raw_attr(n: &NodeRef, a: &parse5::Attribute, d: &Ctx) -> RawAttr {
    let location = attr_loc(n, &a.name);
    let raw = if location.end > location.start { raw_source(&location, d).trim_end().to_string() } else { String::new() };
    let raw = if raw.is_empty() { if a.value.is_empty() { a.name.clone() } else { format!("{}=\"{}\"", a.name, a.value) } } else { raw };
    let raw_value = raw.find('=').map(|i| raw[i + 1..].to_string());
    let quoted = raw_value.as_deref().is_some_and(|v| v.starts_with('"') && v.ends_with('"'));
    RawAttr { raw, raw_value, quoted, location }
}

/// Source text at `l` (UTF-16 offsets), e.g. a text node's undecoded raw text. Empty when out of range.
/// TS `decodeTextContent` (he.decode): decode character references the way the HTML tokenizer does in
/// RCDATA, by parsing the text inside a `<title>`.
fn decode_text_content(s: &str) -> String {
    if !s.contains('&') { return s.to_string(); }
    let out = parse5::parse_fragment_with_errors(&format!("<title>{}</title>", s.replace('<', "&lt;")), ParserOptions::default());
    let doc = out.document.borrow();
    let Some(title) = doc.child_nodes().and_then(|c| c.first().cloned()) else { return s.to_string() };
    let t = title.borrow();
    let kids = t.child_nodes().cloned().unwrap_or_default();
    kids.iter().map(|k| match &k.borrow().data { NodeData::Text { value } => value.clone(), _ => String::new() }).collect()
}

pub(crate) fn raw_source(l: &SourceLocation, d: &Ctx) -> String {
    let (s, e) = l.span();
    let (s, e) = (s as usize, e as usize);
    if s <= e && e <= d.src16.len() { String::from_utf16_lossy(&d.src16[s..e]) } else { String::new() }
}

enum Loop {
    Each { expression: Expression, item: Identifier, index: Option<Identifier>, directive_location: SourceLocation },
    Of { expression: Expression, iterator: Identifier, directive_location: SourceLocation },
}

/// `for:each={x} for:item="i" [for:index="j"]` or `iterator:it={x}`.
fn loop_directive(attrs: &[parse5::Attribute], n: &NodeRef) -> Option<Loop> {
    if let Some(a) = attrs.iter().find(|a| a.name == "for:each") {
        let expression = bound_expr(&a.value)?;
        // TODO: diagnostic when for:item is missing / not an identifier.
        let item = attrs.iter().find(|a| a.name == "for:item").filter(|a| is_ident(&a.value))?;
        let index = attrs.iter().find(|a| a.name == "for:index").filter(|a| is_ident(&a.value))
            .map(|a| Identifier { name: a.value.clone() });
        return Some(Loop::Each { expression, item: Identifier { name: item.value.clone() }, index, directive_location: attr_loc(n, &a.name) });
    }
    let a = attrs.iter().find(|a| a.name.starts_with("iterator:"))?;
    let name = &a.name["iterator:".len()..];
    if !is_ident(name) { return None; }
    Some(Loop::Of { expression: bound_expr(&a.value)?, iterator: Identifier { name: name.to_string() }, directive_location: attr_loc(n, &a.name) })
}

/// The parsed expression of a `{…}` directive value (for:each, iterator:*, lwc:spread/on/slot-bind/is/…):
/// a member path, or (complex expressions, quoted complex values, computed members) opaque source.
/// Values that failed to parse were rejected by the element pre-pass.
fn bound_expr(v: &str) -> Option<Expression> {
    parse_binding(v).or_else(|| expr_check::is_expression(v).then(|| complex_source(v)))
}

fn wrap_loop(l: Loop, children: Vec<ChildNode>, location: SourceLocation) -> ChildNode {
    match l {
        Loop::Each { expression, item, index, directive_location } => ChildNode::ForEach(ForEach { expression, item, index, children, location, directive_location }),
        Loop::Of { expression, iterator, directive_location } => ChildNode::ForOf(ForOf { expression, iterator, children, location, directive_location }),
    }
}

fn is_directive_attr(name: &str) -> bool {
    matches!(name, "lwc:if" | "lwc:elseif" | "lwc:else" | "if:true" | "if:false" | "if:strict-true" | "for:each" | "for:item" | "for:index" | "key" | "lwc:ref" | "lwc:dom" | "lwc:inner-html" | "lwc:spread" | "lwc:dynamic" | "lwc:is" | "lwc:on" | "lwc:slot-bind") || name.starts_with("iterator:")
}

/// TS `isAttribute` for Component / LwcComponent: style, class, key, slot, exportparts and `data-*`
/// stay attributes; everything else becomes a Property.
fn component_keeps_attr(name: &str) -> bool {
    matches!(name, "style" | "class" | "key" | "slot" | "exportparts") || name.starts_with("data-")
}

/// `@lwc/shared` AriaPropertyNames (attr = lowercased `aria-…`; `role` maps to itself).
const ARIA_PROPS: &[&str] = &[
    "ariaActiveDescendant", "ariaAtomic", "ariaAutoComplete", "ariaBusy", "ariaChecked", "ariaColCount", "ariaColIndex",
    "ariaColIndexText", "ariaColSpan", "ariaControls", "ariaCurrent", "ariaDescribedBy", "ariaDescription", "ariaDetails",
    "ariaDisabled", "ariaErrorMessage", "ariaExpanded", "ariaFlowTo", "ariaHasPopup", "ariaHidden", "ariaInvalid",
    "ariaKeyShortcuts", "ariaLabel", "ariaLabelledBy", "ariaLevel", "ariaLive", "ariaModal", "ariaMultiLine",
    "ariaMultiSelectable", "ariaOrientation", "ariaOwns", "ariaPlaceholder", "ariaPosInSet", "ariaPressed", "ariaReadOnly",
    "ariaRelevant", "ariaRequired", "ariaRoleDescription", "ariaRowCount", "ariaRowIndex", "ariaRowIndexText", "ariaRowSpan",
    "ariaSelected", "ariaSetSize", "ariaSort", "ariaValueMax", "ariaValueMin", "ariaValueNow", "ariaValueText",
    "ariaBrailleLabel", "ariaBrailleRoleDescription", "role",
];

/// `@lwc/shared` SPECIAL_PROPERTY_ATTRIBUTE_MAPPING as (attr, prop).
const SPECIAL_ATTR_PROPS: &[(&str, &str)] = &[
    ("accesskey", "accessKey"), ("readonly", "readOnly"), ("tabindex", "tabIndex"), ("bgcolor", "bgColor"),
    ("colspan", "colSpan"), ("rowspan", "rowSpan"), ("contenteditable", "contentEditable"), ("crossorigin", "crossOrigin"),
    ("datetime", "dateTime"), ("formaction", "formAction"), ("ismap", "isMap"), ("maxlength", "maxLength"),
    ("minlength", "minLength"), ("novalidate", "noValidate"), ("usemap", "useMap"), ("for", "htmlFor"),
];

/// TS `attributeToPropertyName`: ATTRS_PROPS_TRANFORMS (special IDL names + ARIA) else `toPropertyName`.
fn attr_to_prop(name: &str) -> String {
    if let Some((_, p)) = SPECIAL_ATTR_PROPS.iter().find(|(a, _)| *a == name) { return p.to_string(); }
    if let Some(p) = ARIA_PROPS.iter().find(|p| p.strip_prefix("aria").map_or(**p == name, |r| name.strip_prefix("aria-").is_some_and(|n| n == r.to_ascii_lowercase()))) {
        return p.to_string();
    }
    let mut o = String::new();
    let mut up = false;
    for c in name.chars() {
        if c == '-' { up = true; } else if up { o.extend(c.to_uppercase()); up = false; } else { o.push(c); }
    }
    o
}

/// TS applyAttributes attr/prop split. `keep(name)` is TS `isAttribute`. Properties are keyed by
/// prop name (a later duplicate replaces the value in place and warns 1147).
fn split_props(attrs: Vec<Attribute>, keep: impl Fn(&str) -> bool, d: &mut Ctx) -> (Vec<Attribute>, Vec<Property>) {
    let mut props: Vec<Property> = Vec::new();
    let attrs = attrs.into_iter().filter_map(|a| {
        // Remaining `lwc:*` (lwc:on, lwc:slot-data, …) are directives TS strips before isAttribute;
        // codegen still reads them from attributes, so they never become props.
        if keep(&a.name) || a.name.starts_with("lwc:") { return Some(a); }
        let name = attr_to_prop(&a.name);
        let p = Property { name: name.clone(), attribute_name: a.name, value: a.value, location: a.location };
        match props.iter_mut().find(|x| x.name == name) {
            Some(x) => {
                d.push(messages::lwc(1147, &[&x.attribute_name, &p.attribute_name, &name], p.location.clone()));
                *x = p;
            }
            None => props.push(p),
        }
        None
    }).collect();
    (attrs, props)
}

/// lwc:dom / lwc:inner-html / lwc:spread / lwc:on / lwc:slot-bind / lwc:dynamic / lwc:is → ElementDirective (TS applyLwcDirectives;
/// values were validated by the element pre-pass).
fn lifted_directives(attrs: &[parse5::Attribute]) -> Vec<ElementDirective> {
    let mut out = Vec::new();
    for a in attrs {
        let dir = match a.name.as_str() {
            "lwc:dynamic" => bound_expr(&a.value).map(ElementDirective::Dynamic),
            "lwc:is" => bound_expr(&a.value).map(ElementDirective::Is),
            "lwc:spread" => bound_expr(&a.value).map(ElementDirective::Spread),
            "lwc:on" => bound_expr(&a.value).map(ElementDirective::OnDirective),
            "lwc:slot-bind" => bound_expr(&a.value).map(ElementDirective::SlotBind),
            "lwc:dom" => Some(ElementDirective::Dom(LiteralValue::String(a.value.clone()))),
            "lwc:inner-html" => Some(ElementDirective::InnerHtml(match bound_expr(&a.value) {
                Some(e) => AttributeValue::Expression(e),
                None => AttributeValue::Literal(Literal { value: LiteralValue::String(a.value.clone()) }),
            })),
            _ => None,
        };
        out.extend(dir);
    }
    out
}

/// TS parseElementLocation: an element parse5 inserted while recovering (e.g. an implied `<tbody>`) has no
/// location; it takes the nearest element ancestor's (start/end tags included).
fn elem_loc(n: &NodeRef) -> ElementSourceLocation {
    let mut cur = n.clone();
    loop {
        let parent = { let b = cur.borrow(); if b.source_code_location_ref().is_some() { break; } b.parent() };
        match parent { Some(p) if matches!(p.borrow().data, NodeData::Element { .. }) => cur = p, _ => break }
    }
    cur.borrow().source_code_location().map(|l| ElementSourceLocation {
        location: loc(&l.location),
        start_tag: l.start_tag.as_ref().map(loc).unwrap_or_default(),
        end_tag: l.end_tag.as_ref().map(loc),
    }).unwrap_or_default()
}

/// TS `isComplexTemplateExpressionEnabled`: the flag and API version ≥ 66.
fn complex_enabled(cfg: &NormalizedConfig) -> bool {
    cfg.experimental_complex_expressions && lwc_shared::api_version::is_api_feature_enabled(lwc_shared::api_version::ApiFeature::EnableComplexTemplateExpressions, cfg.api_version)
}

/// `{ src }` → `Expression::Complex("src")` (opaque JS source, braces stripped).
fn complex_source(v: &str) -> Expression {
    let inner = v.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(v);
    Expression::Complex(inner.trim().to_string())
}

/// The cooked value of `src` when the whole source is one JS string literal (`'a'` / `"a"`), else None.
/// Escapes beyond the simple single-char, `\xHH` and `\uHHHH` forms return None (kept as an expression).
fn js_string_literal(src: &str) -> Option<String> {
    let q = src.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let body = src.get(1..src.len().checked_sub(1)?)?;
    if src.len() < 2 || !src.ends_with(q) { return None; }
    let mut out = String::new();
    let mut it = body.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                let e = it.next()?;
                let hex = |it: &mut std::str::Chars, n: usize| -> Option<char> {
                    let h: String = (0..n).map(|_| it.next()).collect::<Option<String>>()?;
                    char::from_u32(u32::from_str_radix(&h, 16).ok()?)
                };
                out.push(match e {
                    'n' => '\n', 't' => '\t', 'r' => '\r', 'b' => '\u{8}', 'f' => '\u{c}', 'v' => '\u{b}', '0' if !it.clone().next().is_some_and(|c| c.is_ascii_digit()) => '\0',
                    'x' => hex(&mut it, 2)?,
                    'u' if !it.clone().next().is_some_and(|c| c == '{') => hex(&mut it, 4)?,
                    '\'' | '"' | '\\' => e,
                    '\n' => continue,
                    _ => return None,
                });
            }
            '\n' | '\r' => return None,
            c if c == q => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

/// TS getTemplateAttribute + normalizeAttributeValue value lowering: an expression (member path, or
/// opaque source for anything acorn parsed), `true` for a valueless attribute, else the string.
/// Elements whose values failed to normalize or parse were dropped by the element pre-pass.
fn lower_attr(tag: &str, a: &parse5::Attribute, n: &NodeRef, d: &Ctx) -> Attribute {
    let location = n.borrow().source_code_location().and_then(|l| l.attrs.get(&a.name).map(loc)).unwrap_or_default();
    let string = |s: String| {
        let v = if a.name == "class" { s.split_ascii_whitespace().collect::<Vec<_>>().join(" ") } else { s };
        AttributeValue::Literal(Literal { value: LiteralValue::String(v) })
    };
    let value = match element_check::normalize(tag, a, n, d).map(|p| p.v) {
        Ok(element_check::Av::Expr(v)) => AttributeValue::Expression(parse_binding(&v).unwrap_or_else(|| complex_source(&v))),
        Ok(element_check::Av::Bool) => AttributeValue::Literal(Literal { value: LiteralValue::Bool(true) }),
        Ok(element_check::Av::Str(v)) => string(v),
        Err(_) => match parse_binding(&a.value) { Some(e) => AttributeValue::Expression(e), None => string(a.value.clone()) },
    };
    Attribute { name: a.name.clone(), value, location }
}

/// `if:true={x}` / `if:false={x}`; when both are present LWC keeps `if:true` only.
fn if_directive(attrs: &[parse5::Attribute], n: &NodeRef) -> Option<(String, Expression, SourceLocation)> {
    let a = attrs.iter().find(|a| a.name.starts_with("if:"))?;
    // TODO: diagnostic for non-identifier values.
    let e = parse_binding(&a.value)?;
    Some((a.name["if:".len()..].to_string(), e, attr_loc(n, &a.name)))
}

enum Lwc { If(Expression, SourceLocation), Elseif(Expression, SourceLocation), Else(SourceLocation) }

/// `lwc:if={x}` / `lwc:elseif={x}` / `lwc:else`. TODO: diagnostics (bad values, multiple directives).
///
/// The role is kept even when the condition is not a simple binding: TS parses the value with
/// acorn (`{isTrue()}` is a CallExpression under experimentalComplexExpressions) and the element
/// still opens/continues the chain. Dropping the role here turned `<template lwc:if={f()}>` into a
/// directive-less template (1153, dropped) and its `lwc:else` sibling into an orphan (1165).
/// Non-binding conditions become `Expression::Complex(<inner source>)`; invalid ones were already
/// reported by `check_expr`.
fn lwc_directive(attrs: &[parse5::Attribute], n: &NodeRef) -> Option<Lwc> {
    // TS DIRECTIVE_PARSERS order (IfBlock, ElseifBlock, ElseBlock), not attribute order: on a dropped
    // `<template lwc:else lwc:if>` (1162) the node TS already pushed is the lwc:if block.
    let a = ["lwc:if", "lwc:elseif", "lwc:else"].iter().find_map(|k| attrs.iter().find(|a| a.name == *k))?;
    let l = attr_loc(n, &a.name);
    Some(match a.name.as_str() {
        "lwc:if" => Lwc::If(condition_expr(&a.value), l),
        "lwc:elseif" => Lwc::Elseif(condition_expr(&a.value), l),
        _ => Lwc::Else(l),
    })
}

/// Simple binding → Identifier/Member; anything else → opaque `Complex` source (braces stripped).
fn condition_expr(v: &str) -> Expression {
    parse_binding(v).unwrap_or_else(|| {
        let inner = v.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(v);
        Expression::Complex(inner.trim().to_string())
    })
}

/// One lowered sibling: its branch content, optional `lwc:*` role, and element location.
struct Lowered { content: Vec<ChildNode>, lwc: Option<Lwc>, location: SourceLocation }

/// TS `isCustomElementTag`: dashed, but not one of the dashed built-in SVG/MathML tags.
fn is_custom_element_tag(tag: &str) -> bool { tag.contains('-') && !tables::DASHED_TAGNAME_ELEMENT_SET.contains(&tag) }

fn lower_one(n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx, in_loop: bool) -> Option<Lowered> {
    let b = n.borrow();
    let one = |c: ChildNode| Some(Lowered { content: vec![c], lwc: None, location: SourceLocation::default() });
    match &b.data {
        NodeData::Text { value } => {
            let l = node_loc(n);
            let src = raw_source(&l, d);
            let v = clean_text(value, &src)?;
            let mut segments = split_mustache(&v);
            // TS parseText splits the *undecoded* source, so `&#123; x &#125;` stays literal text. Use the raw
            // split (literals decoded); parse5 can also merge text around a dropped end tag (`</input>`), which
            // TS keeps as literal source text.
            if !src.is_empty() {
                let raw_split: Vec<(String, bool)> = split_mustache(&clean_text_node(&src)).into_iter()
                    .map(|(t, e)| if e { (t, e) } else { (decode_text_content(&t), e) }).collect();
                segments = raw_split;
            }
            // TS `withErrorRecovery` around parseTextNode: a text node whose expression fails to parse
            // is dropped entirely (the error is still reported).
            if validate::text_expressions(&segments, &l, cfg, d) { return None; }
            // TS parseText / parseTextComplex: every `{…}` token is an expression (complex mode parses any
            // `{`; otherwise EXPRESSION_RE needs a non-empty body). Non-member expressions stay opaque source.
            let complex = complex_enabled(cfg);
            // TS Text.raw is the token from the cleaned undecoded source; fall back to the decoded token
            // when the source split does not align.
            let raw_tokens = split_mustache(&clean_text_node(&src));
            let aligned = raw_tokens.len() == segments.len() && raw_tokens.iter().zip(&segments).all(|(a, b)| a.1 == b.1);
            let content: Vec<ChildNode> = segments.into_iter().enumerate().map(|(i, (seg, is_expr))| {
                let raw = if aligned { raw_tokens[i].0.clone() } else { seg.clone() };
                let tv = if !is_expr { TextValue::Literal(seg) } else {
                    match parse_binding(&seg) {
                        Some(e) => TextValue::Expression(e),
                        // TS transformText: `isStringLiteral(value) ? value.value : bindExpression(value)`.
                        None if complex || expr_check::is_expression(&seg) => match complex_source(&seg) {
                            Expression::Complex(src) => match js_string_literal(&src) {
                                Some(text) => TextValue::Literal(text),
                                None => TextValue::Expression(Expression::Complex(src)),
                            },
                            e => TextValue::Expression(e),
                        },
                        None => TextValue::Literal(seg),
                    }
                };
                ChildNode::Text(Text { value: tv, raw, location: l.clone() })
            }).collect();
            Some(Lowered { content, lwc: None, location: SourceLocation::default() })
        }
        NodeData::Comment { data } if cfg.preserve_html_comments => {
            // TS parseComment: value = decodeTextContent(data).
            one(ChildNode::Comment(Comment { value: decode_text_content(data), raw: format!("<!--{data}-->"), location: node_loc(n) }))
        }
        NodeData::Element { tag_name, attrs, namespace_uri, child_nodes, template_content, .. } => {
            // Foreign attrs (`xlink:href`) carry a prefix; LWC uses the qualified name.
            let qualified: Vec<parse5::Attribute> = attrs.iter().map(|a| match &a.prefix {
                // TS attributeName: `prefix ? `${prefix}:${name}` : name` (parse5 gives `xmlns` prefix "").
                Some(p) if !p.is_empty() => parse5::Attribute { name: format!("{p}:{}", a.name), ..a.clone() },
                _ => a.clone(),
            }).collect();
            let attrs = &qualified;
            // TS parseElement throws (withErrorRecovery): the element and its subtree are dropped.
            validate::html_recovery(n, d);
            let chk = element_check::element_errors(tag_name, namespace_uri, attrs, n, cfg, d);
            // TS parseIfBlock/parseElseifBlock/parseElseBlock push the (empty) block into the parent and
            // update the sibling if-chain before a later throw drops the element.
            if chk.dropped {
                return chk.lwc_parsed.then(|| Lowered { content: Vec::new(), lwc: lwc_directive(attrs, n), location: elem_loc(n).location });
            }
            // A childless (post-creation throw) element got exactly one error; TS runs no further checks on it.
            let mark = d.diags.len();
            let no_attrs: Vec<parse5::Attribute> = Vec::new();
            let attrs = if chk.bare { &no_attrs } else { attrs };
            if tag_name != "template" { validate::non_template_element(tag_name, namespace_uri, attrs, n, cfg, d); }
            let lwc = lwc_directive(attrs, n);
            let dir = if_directive(attrs, n);
            validate::multiple_if(tag_name, attrs, n, d);
            let lp = loop_directive(attrs, n);
            validate::element_attributes(tag_name, attrs, lp.is_some(), n, d);
            let mut key = attrs.iter().find(|a| a.name == "key").and_then(|a| parse_binding(&a.value)); // TODO: diag on bad key
            if !validate::key_in_iteration(tag_name, key.is_some(), lp.is_some(), in_loop, n, d) { key = None; }
            if chk.childless { d.diags.truncate(mark); }
            // `lwc:ref="foo"`: string literal only. TODO: diagnostics for boolean/empty/expression values (skipped for now).
            let lwc_ref = attrs.iter().find(|a| a.name == "lwc:ref").map(|a| a.value.clone()).filter(|v| !v.is_empty() && parse_binding(v).is_none() && !(v.starts_with('{') && v.ends_with('}')));
            let lifted = lifted_directives(attrs);
            let slot_bind = attrs.iter().find(|a| a.name == "lwc:slot-bind");
            let all_attrs: Vec<&parse5::Attribute> = attrs.iter().collect();
            // TS: element directives are applied to elements only; on `<template>` lwc:on / lwc:slot-bind
            // stay attributes (validateTemplate reports them as unsupported, 1145).
            let attrs: Vec<_> = attrs.iter().filter(|a| !is_directive_attr(&a.name)
                || (tag_name == "template" && matches!(a.name.as_str(), "lwc:on" | "lwc:slot-bind"))).collect();
            let location = elem_loc(n);
            if tag_name == "template" {
                if let Some(sd) = attrs.iter().find(|a| a.name == "lwc:slot-data").copied() {
                    return scoped_slot_fragment(sd, &attrs, dir.is_some() || lp.is_some() || lwc.is_some(), template_content, child_nodes, n, cfg, d, location);
                }
                // TS validateTemplate reports 1145 (every attribute parseElementDirectives did not pick,
                // including key / lwc:dom / ...) before the 1153 "no directive" warning.
                if validate::template(&all_attrs, dir.is_some() || lp.is_some() || lwc.is_some(), &location.location, d) { return None; }
                // `<template>` in foreign (svg) content has no template_content; children are direct.
                let kids = match template_content { Some(c) => c.borrow().child_nodes().cloned().unwrap_or_default(), None => child_nodes.clone() };
                d.in_loop = lp.is_some();
                let depth = d.iter_depth; if lp.is_some() { d.iter_depth += 1; }
                // lwc:if/elseif/else blocks are transparent for the slot-data parent check; for:each / if:* are not.
                let prev_sd = d.slot_data_parent_ok; if dir.is_some() || lp.is_some() { d.slot_data_parent_ok = false; }
                // Templates are transparent for the iteration scope (TS findAncestor through non-base nodes).
                let prev_scope = (d.for_of.clone(), d.for_each.clone());
                if chk.for_of.is_some() { d.for_of = chk.for_of.clone(); }
                if chk.for_each.is_some() { d.for_each = chk.for_each.clone(); }
                let mut children = lower_children(&kids, cfg, d);
                (d.for_of, d.for_each) = prev_scope;
                d.iter_depth = depth; d.slot_data_parent_ok = prev_sd;
                if d.in_slot_fragment { validate::non_element_slot_content(&children, d); }
                if let Some((modifier, condition, directive_location)) = dir {
                    children = vec![ChildNode::If(If { modifier, condition, children, location: location.location.clone(), directive_location })];
                }
                if let Some(l) = lp { children = vec![wrap_loop(l, children, location.location.clone())]; }
                return Some(Lowered { content: children, lwc, location: location.location });
            }
            // `on<event>={handler}` → EventListener (stripped from attributes).
            let mut listeners = Vec::new();
            let attrs: Vec<_> = attrs.into_iter().filter(|a| {
                let is_on = a.name.len() > 2 && a.name.starts_with("on") && a.name.as_bytes()[2].is_ascii_alphabetic();
                // TS applyHandlers: any expression value is a listener; under complex expressions that includes
                // non-member sources (arrows, calls, computed members).
                let handler = parse_binding(&a.value).or_else(|| (complex_enabled(cfg) && a.value.starts_with('{') && a.value.ends_with('}')).then(|| complex_source(&a.value)));
                match (is_on, handler) {
                    (true, Some(handler)) => { listeners.push(EventListener { name: a.name[2..].to_string(), handler, location: attr_loc(n, &a.name) }); false }
                    _ => true,
                }
            }).collect();
            let attributes = attrs.iter().map(|a| lower_attr(tag_name, a, n, d)).collect();
            let directives: Vec<ElementDirective> = key.into_iter().map(ElementDirective::Key).chain(lwc_ref.map(ElementDirective::Ref)).chain(lifted).collect();
            let depth = d.iter_depth; if lp.is_some() { d.iter_depth += 1; }
            let is_comp = is_custom_element_tag(tag_name);
            // `<lwc:component>` is transparent for the slot-ancestor lookup (its children are slotted into the dynamic component).
            let anc = if tag_name == "lwc:component" { d.slot_ancestor.clone() } else if is_comp { None } else { Some(tag_name.clone()) };
            let prev_anc = std::mem::replace(&mut d.slot_ancestor, anc);
            let is_cmp_parent = tag_name == "lwc:component" || (is_comp && tag_name != "slot" && !attrs.iter().any(|a| a.name == "lwc:external"));
            let prev_sd = std::mem::replace(&mut d.slot_data_parent_ok, is_cmp_parent && dir.is_none() && lp.is_none());
            // A base element ends the iteration scope for its children (TS getForOfParent/getForEachParent).
            let prev_scope = (d.for_of.take(), d.for_each.take());
            let children = if chk.childless { Vec::new() } else { lower_children(child_nodes, cfg, d) };
            (d.for_of, d.for_each) = prev_scope;
            d.slot_ancestor = prev_anc; d.iter_depth = depth; d.slot_data_parent_ok = prev_sd;
            // TS validateChildren: lwc:dom / lwc:inner-html elements must be empty (comments count only
            // when preserved, which `children` already reflects). Thrown after the children were parsed,
            // so the element stays in the tree.
            if !children.is_empty() {
                if directives.iter().any(|x| matches!(x, ElementDirective::Dom(_))) { d.push(messages::lwc(1086, &[], location.location.clone())); }
                else if directives.iter().any(|x| matches!(x, ElementDirective::InnerHtml(_))) { d.push(messages::lwc(1142, &[&format!("<{tag_name}>")], location.location.clone())); }
            }
            let node = if tag_name == "slot" {
                // `name` stays in attributes too; slot_name is the TS `slotName`.
                let slot_name = attrs.iter().find(|a| a.name == "name").map(|a| a.value.clone()).unwrap_or_default();
                // TS parseSlot: scoped (`lwc:slot-bind`) slots are light-DOM only and may not be duplicated
                // or mixed with standard slots of the same name.
                // TS parseSlot registers the name before any later (childless) throw empties the attributes.
                let seen_name = qualified.iter().find(|a| a.name == "name").map_or(slot_name.as_str(), |a| a.value.as_str());
                validate::slot(seen_name, slot_bind, &location.location, d);
                ChildNode::Slot(Slot { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, directives: directives.clone(), listeners, children, location: location.clone(), slot_name, ..Default::default() })
            } else if is_custom_element_tag(tag_name) && attrs.iter().any(|a| a.name == "lwc:external") {
                // TS: `isCustomElementTag && parsedAttr.get(External)` → ExternalComponent; attrs stay
                // attributes (no prop split) and the directive itself is consumed.
                let attributes: Vec<Attribute> = attributes.into_iter().filter(|a: &Attribute| a.name != "lwc:external").collect();
                ChildNode::ExternalComponent(ExternalComponent { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, directives: directives.clone(), listeners, children, location: location.clone(), ..Default::default() })
            } else if is_custom_element_tag(tag_name) {
                { let (attributes, properties) = split_props(attributes, component_keeps_attr, d); ChildNode::Component(Component { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, properties, directives: directives.clone(), listeners, children, location: location.clone(), ..Default::default() }) }
            } else if tag_name == "lwc:component" {
                // TS parseLwcComponent → ast.lwcComponent; isAttribute uses the component table.
                let (attributes, properties) = split_props(attributes, component_keeps_attr, d);
                ChildNode::LwcComponent(LwcComponent { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, properties, directives: directives.clone(), listeners, children, location: location.clone() })
            } else {
                // TS isAttribute: `<input value|checked>` are props.
                let (attributes, properties) = if tag_name == "input" { split_props(attributes, |a| a != "value" && a != "checked", d) }
                    else { (attributes, Vec::new()) };
                ChildNode::Element(Element { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, properties, directives: directives.clone(), listeners, children, location: location.clone(), ..Default::default() })
            };
            // Loop wraps outside; if:* is applied per iteration inside.
            let node = match dir {
                Some((modifier, condition, directive_location)) => ChildNode::If(If { modifier, condition, children: vec![node], location: location.location.clone(), directive_location }),
                None => node,
            };
            let node = match lp { Some(l) => wrap_loop(l, vec![node], location.location.clone()), None => node };
            Some(Lowered { content: vec![node], lwc, location: location.location })
        }
        _ => None,
    }
}

/// `<template lwc:slot-data="d" [slot=...]>` → ScopedSlotFragment (TS `parseScopedSlotFragment`,
/// same validation order: 1179, 1178, 1174, identifier 1211/1059; then 1177 on direct text/comment children).
#[allow(clippy::too_many_arguments)]
fn scoped_slot_fragment(
    sd: &parse5::Attribute, attrs: &[&parse5::Attribute], other_directive: bool,
    template_content: &Option<NodeRef>, child_nodes: &[NodeRef], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx,
    location: ElementSourceLocation,
) -> Option<Lowered> {
    let loc = location.location.clone();
    let sd_loc = attr_loc(n, &sd.name);
    let slot_data = validate::scoped_slot_data(sd, other_directive, &loc, &sd_loc, d)?;
    let slot_name = attrs.iter().find(|a| a.name == "slot").map(|a| lower_attr("template", a, n, d).value)
        .unwrap_or(AttributeValue::Literal(Literal { value: LiteralValue::String(String::new()) }));
    let kids = match template_content { Some(c) => c.borrow().child_nodes().cloned().unwrap_or_default(), None => child_nodes.to_vec() };
    let prev_sd = std::mem::replace(&mut d.slot_data_parent_ok, false);
    let prev_in = std::mem::replace(&mut d.in_slot_fragment, true);
    let children = lower_children(&kids, cfg, d);
    d.slot_data_parent_ok = prev_sd; d.in_slot_fragment = prev_in;
    validate::non_element_slot_content(&children, d);
    let frag = ScopedSlotFragment { slot_data, slot_name, children, location: loc.clone(), directive_location: sd_loc };
    Some(Lowered { content: vec![ChildNode::ScopedSlotFragment(frag)], lwc: None, location: loc })
}

/// Build the `else_` chain from consecutive `lwc:elseif` / `lwc:else` siblings.
fn else_chain(it: &mut std::iter::Peekable<std::vec::IntoIter<Lowered>>) -> Option<Box<ElseBranch>> {
    match it.peek()?.lwc {
        Some(Lwc::Elseif(..)) => {
            let l = it.next().unwrap();
            let Some(Lwc::Elseif(condition, directive_location)) = l.lwc else { unreachable!() };
            let else_ = else_chain(it);
            Some(Box::new(ElseBranch::Elseif(ElseifBlock { condition, children: l.content, else_, location: l.location, directive_location })))
        }
        Some(Lwc::Else(..)) => {
            let l = it.next().unwrap();
            let Some(Lwc::Else(directive_location)) = l.lwc else { unreachable!() };
            Some(Box::new(ElseBranch::Else(ElseBlock { children: l.content, location: l.location, directive_location })))
        }
        _ => None,
    }
}

/// Whitespace text between chain siblings doesn't break an lwc:if chain.
fn continue_marker() -> Option<i32> { Some(2) }

fn lower_children(nodes: &[NodeRef], cfg: &NormalizedConfig, d: &mut Ctx) -> Vec<ChildNode> {
    let in_loop = std::mem::replace(&mut d.in_loop, false);
    let prev_chain = std::mem::replace(&mut d.chain_open, false);
    // lwc:if chain slot scoping: each branch starts from the pre-chain set; after the chain, the union.
    let mut chain: Option<(HashSet<String>, HashSet<String>)> = None;
    let mut lowered: Vec<Lowered> = Vec::new();
    for n in nodes {
        let role = match &n.borrow().data {
            NodeData::Element { attrs, .. } => attrs.iter().find_map(|a| match a.name.as_str() { "lwc:if" => Some(0), "lwc:elseif" | "lwc:else" => Some(1), _ => None }),
            NodeData::Text { value } if value.trim().is_empty() => continue_marker(),
            _ => None,
        };
        if role == Some(2) { if let Some(l) = lower_one(n, cfg, d, in_loop) { lowered.push(l); } continue; }
        if let Some((base, union)) = chain.as_mut() {
            union.extend(d.seen_slots.iter().cloned());
            if role == Some(1) { d.seen_slots = base.clone(); } else { d.seen_slots = union.clone(); chain = None; }
        }
        if role == Some(0) { chain = Some((d.seen_slots.clone(), d.seen_slots.clone())); }
        let is_elem = matches!(&n.borrow().data, NodeData::Element { .. });
        let is_comment = matches!(&n.borrow().data, NodeData::Comment { .. });
        let lowered_one = lower_one(n, cfg, d, in_loop);
        // TS parseChildren chain bookkeeping: an element whose first node is an If/Elseif block keeps the
        // chain open; any other parsed element, non-whitespace text, or a preserved comment ends it.
        // A dropped element (throw / no node) leaves it unchanged.
        if is_elem {
            if let Some(l) = &lowered_one { d.chain_open = matches!(l.lwc, Some(Lwc::If(..) | Lwc::Elseif(..))); }
        } else if !is_comment || cfg.preserve_html_comments {
            d.chain_open = false;
        }
        if let Some(l) = lowered_one { lowered.push(l); }
    }
    d.chain_open = prev_chain;
    if let Some((_, mut union)) = chain { union.extend(d.seen_slots.drain()); d.seen_slots = union; }
    let mut it = lowered.into_iter().peekable();
    let mut out = Vec::new();
    while let Some(l) = it.next() {
        match l.lwc {
            None => out.extend(l.content),
            Some(Lwc::If(condition, directive_location)) => {
                let else_ = else_chain(&mut it);
                out.push(ChildNode::IfBlock(IfBlock { condition, children: l.content, else_, location: l.location, directive_location }));
            }
            // TS throwAtLocation(LWC_IF_SCOPE_NOT_FOUND, sourceLocation(parse5ElmLocation)): the element's
            // location (`Lowered.location`), not the directive attribute's.
            Some(Lwc::Elseif(..)) => validate::orphan_else("lwc:elseif", l.location, d),
            Some(Lwc::Else(..)) => validate::orphan_else("lwc:else", l.location, d),
        }
    }
    out
}

pub fn parse_template(source: &str, config: &NormalizedConfig) -> TemplateParseResult {
    let mut warnings = Vec::new();
    let out = parse5::parse_fragment_with_errors(source, ParserOptions { source_code_location_info: true, ..Default::default() });
    validate::parse5_errors(&out.errors, config, &mut warnings);
    // TS parse(): any error-level parse5 diagnostic → no root.
    if warnings.iter().any(|w| w.level == DiagnosticLevel::Error) {
        return TemplateParseResult { root: None, warnings };
    }
    let frag = out.document.borrow();
    let top = frag.child_nodes().cloned().unwrap_or_default();
    // TS getTemplateRoot: elements and non-blank text nodes are candidate roots.
    let roots: Vec<NodeRef> = top.iter().filter(|n| match &n.borrow().data {
        NodeData::Element { .. } => true,
        NodeData::Text { value } => !value.trim().is_empty(),
        _ => false,
    }).cloned().collect();
    if roots.len() > 1 {
        warnings.push(messages::lwc(1075, &[], node_loc(&roots[1])));
        return TemplateParseResult { root: None, warnings };
    }
    let Some(tmpl) = roots.first().filter(|n| matches!(n.borrow().data, NodeData::Element { .. })).cloned() else {
        // TS ctx.throw without a location: normalizeLocation(undefined) = line 0, column 0.
        let zero = SourceLocation { start_line: 0, start_column: 0, end_line: 0, end_column: 0, start: Some(0), end: Some(0) };
        warnings.push(messages::lwc(1072, &[], zero));
        return TemplateParseResult { root: None, warnings };
    };
    if let Some(t) = tmpl.borrow().tag_name().filter(|t| *t != "template") {
        warnings.push(messages::lwc(1079, &[t], node_loc(&tmpl)));
        return TemplateParseResult { root: None, warnings };
    }
    let tb = tmpl.borrow();
    let content = match &tb.data { NodeData::Element { template_content, .. } => template_content.clone(), _ => None };
    let kids = content.map(|c| c.borrow().child_nodes().cloned().unwrap_or_default()).unwrap_or_default();
    let el = tb.source_code_location();
    let location = el.map(|l| ElementSourceLocation {
        location: loc(&l.location),
        start_tag: l.start_tag.as_ref().map(loc).unwrap_or_default(),
        end_tag: l.end_tag.as_ref().map(loc),
    }).unwrap_or_default();
    let light = matches!(&tb.data, NodeData::Element { attrs, .. } if attrs.iter().any(|a| a.name == "lwc:render-mode" && a.value == "light"));
    let mut cx = Ctx {
        diags: warnings, seen_slots: HashSet::new(), in_loop: false, iter_depth: 0, slot_ancestor: None, slot_data_parent_ok: false,
        seen_scoped_slots: HashSet::new(), light, in_slot_fragment: false, instrumentation: config.instrumentation.clone(),
        src16: source.encode_utf16().collect(), seen_ids: HashSet::new(), for_of: None, for_each: None, chain_open: false,
    };
    if let NodeData::Element { attrs, .. } = &tb.data {
        if element_check::root_errors(attrs, &tmpl, &location, config, &mut cx) {
            return TemplateParseResult { root: None, warnings: cx.diags };
        }
    }
    // TS parseRoot → applyLwcRenderModeDirective: counted once a valid lwc:render-mode is applied.
    if matches!(&tb.data, NodeData::Element { attrs, .. } if attrs.iter().any(|a| a.name == "lwc:render-mode" && (a.value == "light" || a.value == "shadow"))) {
        cx.count(lwc_shared::CompilerMetric::LwcRenderModeDirective);
    }
    // TS ParserCtx: `preserveComments = root lwc:preserve-comments directive ?? config.preserveHtmlComments`
    // (root_errors already rejected a non-boolean value, so presence means true).
    let preserved;
    let config = if matches!(&tb.data, NodeData::Element { attrs, .. } if attrs.iter().any(|a| a.name == "lwc:preserve-comments")) {
        preserved = NormalizedConfig { preserve_html_comments: true, ..config.clone() };
        &preserved
    } else { config };
    // TS applyRootLwcDirectives: renderModeDirective then preserveCommentsDirective, when present
    // (root_errors already rejected invalid values).
    let mut directives = Vec::new();
    if let NodeData::Element { attrs, .. } = &tb.data {
        match attrs.iter().find(|a| a.name == "lwc:render-mode").map(|a| a.value.as_str()) {
            Some("light") => directives.push(RootDirective::RenderMode(RenderMode::Light)),
            Some("shadow") => directives.push(RootDirective::RenderMode(RenderMode::Shadow)),
            _ => {}
        }
        if attrs.iter().any(|a| a.name == "lwc:preserve-comments") { directives.push(RootDirective::PreserveComments(true)); }
    }
    let children = lower_children(&kids, config, &mut cx);
    TemplateParseResult { root: Some(Root { location, directives, children }), warnings: cx.diags }
}

/// TS `isValidHTMLAttribute`.
fn is_valid_html_attribute(tag: &str, attr: &str) -> bool {
    use tables::*;
    let directive = attr == "key" || attr.starts_with("lwc:") || attr.starts_with("if:") || attr.starts_with("for:") || attr.starts_with("iterator:");
    if GLOBAL_ATTRIBUTE.contains(&attr) || ARIA_ATTRS.contains(&attr) || attr.starts_with("data-") || directive
        || SUPPORTED_SVG_TAGS.contains(&tag) || DASHED_TAGNAME_ELEMENT_SET.contains(&tag) || !KNOWN_HTML_AND_SVG_ELEMENTS.contains(&tag) {
        return true;
    }
    HTML_ATTRIBUTE_ELEMENT_MAP.iter().find(|(a, _)| *a == attr).is_some_and(|(_, els)| els.is_empty() || els.contains(&tag))
}

const ROOT_DIRECTIVES: &[&str] = &["lwc:render-mode", "lwc:preserve-comments"];
const DISALLOWED_HTML_TAGS: &[&str] = &["base", "link", "meta", "script", "title"];

const PARSE5_ERROR_ON: &[&str] = &[
    "control-character-in-input-stream", "noncharacter-in-input-stream", "surrogate-in-input-stream",
    "non-void-html-element-start-tag-with-trailing-solidus", "end-tag-with-attributes", "end-tag-with-trailing-solidus",
    "unexpected-solidus-in-tag", "unexpected-null-character", "unexpected-question-mark-instead-of-tag-name",
    "invalid-first-character-of-tag-name", "unexpected-equals-sign-before-attribute-name", "missing-end-tag-name",
    "unexpected-character-in-attribute-name", "unknown-named-character-reference", "missing-semicolon-after-character-reference",
    "unexpected-character-after-doctype-system-identifier", "unexpected-character-in-unquoted-attribute-value",
    "eof-before-tag-name", "eof-in-tag", "missing-attribute-value", "missing-whitespace-between-attributes",
    "missing-whitespace-after-doctype-public-keyword", "missing-whitespace-between-doctype-public-and-system-identifiers",
    "missing-whitespace-after-doctype-system-keyword", "missing-quote-before-doctype-public-identifier",
    "missing-quote-before-doctype-system-identifier", "missing-doctype-public-identifier", "missing-doctype-system-identifier",
    "abrupt-doctype-public-identifier", "abrupt-doctype-system-identifier", "cdata-in-html-content", "incorrectly-opened-comment",
    "eof-in-script-html-comment-like-text", "eof-in-doctype", "nested-comment", "abrupt-closing-of-empty-comment",
    "eof-in-comment", "incorrectly-closed-comment", "eof-in-cdata", "absence-of-digits-in-numeric-character-reference",
    "null-character-reference", "surrogate-character-reference", "character-reference-outside-unicode-range",
    "control-character-reference", "noncharacter-character-reference", "missing-whitespace-before-doctype-name",
    "missing-doctype-name", "invalid-character-sequence-after-doctype-name", "duplicate-attribute",
];
const PARSE5_WARN_OLDER: &[&str] = &[
    "non-conforming-doctype", "missing-doctype", "misplaced-doctype", "end-tag-without-matching-open-element",
    "closing-of-element-with-open-child-elements", "disallowed-content-in-noscript-in-head", "open-elements-left-after-eof",
    "abandoned-head-element-child", "misplaced-start-tag-for-head-element", "nested-noscript-in-head",
    "eof-in-element-that-can-contain-only-text",
];

#[cfg(test)]
mod string_literal_tests {
    use super::js_string_literal as s;
    #[test]
    fn whole_source_string_literals_only() {
        assert_eq!(s("'foo foo'").as_deref(), Some("foo foo"));
        assert_eq!(s(r#""a\"b\x41\u0042""#).as_deref(), Some("a\"bAB"));
        assert_eq!(s("'a' + 'b'"), None);
        assert_eq!(s("`a`"), None);
        assert_eq!(s("'"), None);
        assert_eq!(s("foo"), None);
    }
}
