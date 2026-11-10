//! HTML → LWC template IR via parse5-rust.

use crate::config::NormalizedConfig;
use lwc_shared::template_ir::*;
use lwc_shared::errors::ParserDiagnostics;
use lwc_shared::js::{js_trim, js_ws};
use lwc_shared::{CompilerDiagnostic, DiagnosticLevel};
use parse5::{Location, NodeData, NodeRef, ParserOptions};
use std::collections::HashSet;

mod tables;
/// Generated from @lwc/template-compiler parser/constants.ts + shared/constants.ts and
/// parser/utils/html-element-attributes.ts (tools/gen-shared-tables). Unused until AB-0011 (b)
/// moves the tables.rs callers here.
#[allow(dead_code)]
mod constants;
#[allow(dead_code)]
mod utils;
mod expr_check;
pub(crate) mod expression_complex;
mod validate;
mod diags;
use diags::Diags;
use element_check::attribute::{is_data_attribute, ParsedAttribute};
pub(crate) use element_check::attribute::{is_attribute, ElementKind};
pub(crate) mod element_check;

/// Parser state threaded through lowering (TS `ParserCtx` subset).
pub(crate) struct Ctx<'a> {
    /// The compile's arena: complex expression ASTs and their sources live here (AB-D0004).
    alloc: &'a oxc_allocator::Allocator,
    diags: Diags,
    /// Slot names seen in the current if-chain scope (TS `seenSlots`, approximated).
    seen_slots: HashSet<String>,
    /// Direct children of a `<template for:each|iterator:*>`.
    in_loop: bool,
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
    /// experimentalComputedMemberExpression without experimentalComplexExpressions: computed member
    /// chains are typed IR ([`non_binding_expr`]).
    legacy_computed: bool,
    /// TS `ctx.instrumentation`.
    instrumentation: Option<lwc_shared::InstrumentationHandle>,
    /// Template source as UTF-16 code units (parse5 offsets index this); see [`raw_attr`] / [`raw_source`].
    src16: Vec<u16>,
    /// TS `seenIds` (1042).
    seen_ids: HashSet<String>,
    /// TS `ParserCtx.elementScopes`: one scope per element being parsed, holding the nodes its
    /// directive parsers and parseBaseElement added. A parseElement that throws never pops its scope
    /// (withErrorRecovery skips endElementScope), so it leaks into every later ancestor query.
    scopes: Vec<Vec<ScopeNode>>,
    /// TS `ifContext.currentNode` kind for the current sibling scope (`None`: no open chain).
    chain: Option<Chain>,
    /// TS `parse5ParentLocation`: the parse5 location of the element whose children are being parsed
    /// (the root template's at the top level), [`elem_parse5_loc`]'s last fallback. Kept raw: like TS, its
    /// start tag is only read where a node is created from it.
    parent_loc: parse5::ElementLocation,
}

/// A TS `ctx.throw*` inside parseChildren's `withErrorRecovery`: `diag` is reported and the rest of
/// that child's parse is skipped (no endElementScope, no if-chain bookkeeping).
struct Thrown<'a> {
    diag: CompilerDiagnostic,
    /// The node TS had already pushed into the parent before the throw (an lwc:if/elseif/else block, a
    /// created element left childless, or an element / fragment whose validateChildren threw).
    kept: Option<Lowered<'a>>,
}

/// What stops one child's parse: a recoverable `ctx.throw*` ([`Thrown`]), or an internal error, which
/// TS's withErrorRecovery rethrows and which ends the whole parse.
enum Throw<'a> {
    Recovered(Thrown<'a>),
    Internal(InternalError),
}

impl<'a> From<Thrown<'a>> for Throw<'a> { fn from(t: Thrown<'a>) -> Self { Throw::Recovered(t) } }
impl From<InternalError> for Throw<'_> { fn from(e: InternalError) -> Self { Throw::Internal(e) } }

/// One parse5 child lowered (TS parseElement / parseTextNode / parseComment).
type Parsed<'a> = Result<Option<Lowered<'a>>, Throw<'a>>;

/// A node in a TS element scope (only what the ancestor queries look at).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ScopeNode {
    IfBlock,
    ElseifBlock,
    ElseBlock,
    /// `for:each`, with its index name.
    ForEach(Option<String>),
    /// `iterator:<name>`.
    ForOf(String),
    If,
    ScopedSlotFragment,
    /// A base element (Element / Component / Slot / ExternalComponent / LwcComponent).
    Base,
}

/// The current node of an open if-chain (TS `ifContext.currentNode`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Chain { If, Elseif, Else }

impl<'a> Ctx<'a> {
    /// A `{…}` value as an [`Expression::Complex`] in this parse's arena, located at its owner.
    fn complex(&self, raw: &str, location: SourceLocation) -> Located<Expression<'a>> {
        expression_complex::build(self.alloc, raw, location).map(Expression::Complex)
    }
    fn count(&self, m: lwc_shared::CompilerMetric) { if let Some(i) = &self.instrumentation { i.increment_counter(m); } }
    fn push(&mut self, d: CompilerDiagnostic) { self.diags.push(d); }
    /// TS addNodeCurrentElementScope.
    pub(super) fn add_scope_node(&mut self, n: ScopeNode) {
        if let Some(s) = self.scopes.last_mut() { s.push(n); }
    }
    /// TS `findAncestor(pred, ({ parent }) => parent && !isBaseElement(parent))` over the flattened
    /// scopes, from the innermost node outwards (getForOfParent / getForEachParent).
    fn find_before_base<T>(&self, f: impl Fn(&ScopeNode) -> Option<T>) -> Option<T> {
        let flat: Vec<&ScopeNode> = self.scopes.iter().flatten().collect();
        for i in (0..flat.len()).rev() {
            if let Some(t) = f(flat[i]) { return Some(t); }
            match i.checked_sub(1).map(|j| flat[j]) { Some(p) if *p != ScopeNode::Base => {} _ => return None }
        }
        None
    }
    /// TS getForOfParent: the iterator name.
    pub(super) fn for_of_parent(&self) -> Option<String> {
        self.find_before_base(|n| match n { ScopeNode::ForOf(s) => Some(s.clone()), _ => None })
    }
    /// TS getForEachParent: the for:each index name.
    pub(super) fn for_each_parent(&self) -> Option<Option<String>> {
        self.find_before_base(|n| match n { ScopeNode::ForEach(i) => Some(i.clone()), _ => None })
    }
    /// TS isInIteration: any ForEach / ForOf in scope.
    pub(super) fn in_iteration(&self) -> bool {
        self.scopes.iter().flatten().any(|n| matches!(n, ScopeNode::ForEach(_) | ScopeNode::ForOf(_)))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateParseResult<'a> {
    pub root: Option<Root<'a>>,
    pub warnings: Vec<CompilerDiagnostic>,
}

fn loc(l: &Location) -> SourceLocation {
    SourceLocation {
        start_line: l.start_line as u32, start_column: l.start_col as u32,
        end_line: l.end_line as u32, end_column: l.end_col as u32,
        start: l.start_offset as u32, end: l.end_offset as u32,
    }
}

/// A node's parse5 location; `None` only where TS sees `sourceCodeLocation` undefined.
fn node_loc(n: &NodeRef) -> Option<SourceLocation> {
    n.borrow().source_code_location().map(|l| loc(&l.location))
}

/// A node's location where TS requires one (parseTextNode, parseComment): `missing` is the internal
/// error TS throws at that read.
fn required_loc(n: &NodeRef, missing: &'static str) -> Located<SourceLocation> {
    node_loc(n).ok_or(InternalError(missing))
}

const NO_ROOT_LOCATION: &str = "An internal parsing error occurred during node creation; the root template node does not have a sourceCodeLocation.";
const NO_TEXT_LOCATION: &str = "An internal parsing error occurred during node creation; a text node was found without a sourceCodeLocation.";
const NO_COMMENT_LOCATION: &str = "An internal parsing error occurred during node creation; a comment node was found without a sourceCodeLocation.";
const NO_ATTR_LOCATION: &str = "An internal parsing error occurred while parsing attributes; attributes were found without a location.";
/// The `TypeError` TS `sourceLocation(parse5Elm.sourceCodeLocation.startTag!)` raises without a start tag.
const NO_START_TAG: &str = "Cannot read properties of undefined (reading 'startLine')";

/// TS parser's istanbul-ignored "internal parsing error" throws (parseRoot, parseTextNode, parseComment,
/// parseAttributes, elementSourceLocation's `startTag!`) for a node parse5 left without a location, raised
/// by the location readers at the read where TS throws. They are plain `Error`s: withErrorRecovery
/// rethrows them and `compile` turns the first into its only diagnostic (see [`InternalError::diagnostic`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InternalError(&'static str);

/// A location read that may raise TS's internal parsing error.
pub(crate) type Located<T> = Result<T, InternalError>;

impl InternalError {
    pub(crate) fn new(msg: &'static str) -> Self { InternalError(msg) }

    /// `normalizeToDiagnostic(GENERIC_PARSING_ERROR, error)` prefixed `Unexpected compilation error: `, with
    /// no location.
    pub(crate) fn diagnostic(self) -> CompilerDiagnostic {
        let info = &ParserDiagnostics::GENERIC_PARSING_ERROR;
        let message = format!("Unexpected compilation error: {}", lwc_shared::generate_error_message(info, Some(&[self.0])));
        CompilerDiagnostic::new(Some(info.code), message, info.level, None, None)
    }
}

/// A located element's parse5 location as the IR's (TS `elementSourceLocation`).
/// Reads the start tag, so it runs where TS creates the node (ast.root / ast.element / ...).
fn element_location(l: &parse5::ElementLocation) -> Located<ElementSourceLocation> {
    Ok(ElementSourceLocation {
        location: loc(&l.location),
        start_tag: loc(l.start_tag.as_ref().ok_or(InternalError(NO_START_TAG))?),
        end_tag: l.end_tag.as_ref().map(loc),
    })
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

/// TS parseTextNode: `cleanTextNode(rawSource)`; no node when it is blank. The emptiness test reads the
/// undecoded source (`&nbsp;` keeps a node); the returned text is the cleaned decoded value.
fn clean_text(value: &str, raw: &str) -> Option<String> {
    let probe = if raw.is_empty() { value } else { raw };
    if js_trim(&clean_text_node(probe)).is_empty() { return None; }
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
pub(crate) fn parse_binding<'a>(v: &str) -> Option<Expression<'a>> { braced_chain(v, false) }

/// `{chain}` where `chain` is a [`member_chain`] (with `[chain]` steps when `computed`).
fn braced_chain<'a>(v: &str, computed: bool) -> Option<Expression<'a>> {
    let inner = js_trim(v.strip_prefix('{')?.strip_suffix('}')?);
    let (e, rest) = member_chain(inner, computed)?;
    js_trim(rest).is_empty().then_some(e)
}

/// `primary ('.' ident)*` with `primary := ident | '(' member_chain ')'`, plus `'[' member_chain ']'` steps
/// when `computed`. Parentheses are transparent, as in the TS (acorn/ESTree) AST: `((foo).bar)` is the
/// member expression `foo.bar`.
fn member_chain<'a>(s: &str, computed: bool) -> Option<(Expression<'a>, &str)> {
    let s = s.trim_start_matches(js_ws);
    let (mut e, mut rest) = if let Some(r) = s.strip_prefix('(') {
        let (e, r) = member_chain(r, computed)?;
        (e, r.trim_start_matches(js_ws).strip_prefix(')')?)
    } else {
        let end = s.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(s.len());
        let first = &s[..end];
        if !is_ident(first) || expr_check::is_reserved(first) { return None; }
        (Expression::Identifier(Identifier { name: first.to_string() }), &s[end..])
    };
    loop {
        let t = rest.trim_start_matches(js_ws);
        if let Some(r) = t.strip_prefix('[').filter(|_| computed) {
            let (p, r) = member_chain(r, true)?;
            rest = r.trim_start_matches(js_ws).strip_prefix(']')?;
            e = Expression::Member(MemberExpression { object: Box::new(e), property: Box::new(p), computed: true });
            continue;
        }
        let Some(r) = t.strip_prefix('.') else { return Some((e, rest)) };
        let r = r.trim_start_matches(js_ws);
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

/// An attribute's location in its element's parse5 location `l` (TS parseAttributes:
/// `attrLocations[attributeName(attr).toLowerCase()]`, else the internal error).
fn attr_loc_in(l: &parse5::ElementLocation, name: &str) -> Located<SourceLocation> {
    l.attrs.get(&name.to_lowercase()).map(loc).ok_or(InternalError(NO_ATTR_LOCATION))
}

/// [`attr_loc_in`] the location TS parseElementLocation gives element `n` ([`elem_parse5_loc`]): an element
/// parse5 recreated without a location reads its attributes from the ancestor location it falls back to.
pub(crate) fn attr_loc(n: &NodeRef, name: &str, d: &Ctx) -> Located<SourceLocation> {
    attr_loc_in(&elem_parse5_loc(n, d), name)
}

/// One attribute as written in the source (TS `getTemplateAttribute` inputs).
pub(crate) struct RawAttr {
    /// Source text of the whole attribute (`name="v"`, entities undecoded), trailing whitespace trimmed
    /// (parse5-rust stretches the last attribute's end over whitespace before `>`).
    pub raw: String,
    /// Text after the first `=` (quotes kept, undecoded); `None` when there is no `=`.
    pub raw_value: Option<String>,
    pub location: SourceLocation,
}

/// Raw source of attribute `a` on element `n`. Falls back to `name="value"` when parse5 has no location.
pub(crate) fn raw_attr(n: &NodeRef, a: &parse5::Attribute, d: &Ctx) -> Located<RawAttr> {
    let location = attr_loc(n, &a.name, d)?;
    let raw = if location.end > location.start { raw_source(&location, d).trim_end().to_string() } else { String::new() };
    let raw = if raw.is_empty() { if a.value.is_empty() { a.name.clone() } else { format!("{}=\"{}\"", a.name, a.value) } } else { raw };
    let raw_value = raw.find('=').map(|i| raw[i + 1..].to_string());
    Ok(RawAttr { raw, raw_value, location })
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
    let (s, e) = (l.start as usize, l.end as usize);
    if s <= e && e <= d.src16.len() { String::from_utf16_lossy(&d.src16[s..e]) } else { String::new() }
}

enum Loop<'a> {
    Each { expression: Expression<'a>, item: Identifier, index: Option<Identifier>, directive_location: SourceLocation },
    Of { expression: Expression<'a>, iterator: Identifier, directive_location: SourceLocation },
}

/// `for:each={x} for:item="i" [for:index="j"]` or `iterator:it={x}`.
/// TS parseForEach then parseForOf: picks for:each / for:item / for:index and the first `iterator:*`.
fn loop_directive<'a>(p: &mut ParsedAttribute, n: &NodeRef, d: &Ctx<'a>) -> Located<Option<Loop<'a>>> {
    let (each, item, index) = (p.pick("for:each"), p.pick("for:item"), p.pick("for:index"));
    let iterator = p.pick_prefix("iterator:");
    if let Some(a) = each {
        let Some(expression) = bound_expr(&a.value, || attr_loc(n, &a.name, d), d)? else { return Ok(None) };
        // TODO: diagnostic when for:item is missing / not an identifier.
        let Some(item) = item.filter(|a| is_ident(&a.value)) else { return Ok(None) };
        let index = index.filter(|a| is_ident(&a.value)).map(|a| Identifier { name: a.value.clone() });
        return Ok(Some(Loop::Each { expression, item: Identifier { name: item.value.clone() }, index, directive_location: attr_loc(n, &a.name, d)? }));
    }
    let Some(a) = iterator else { return Ok(None) };
    let name = &a.name["iterator:".len()..];
    if !is_ident(name) { return Ok(None); }
    let Some(expression) = bound_expr(&a.value, || attr_loc(n, &a.name, d), d)? else { return Ok(None) };
    Ok(Some(Loop::Of { expression, iterator: Identifier { name: name.to_string() }, directive_location: attr_loc(n, &a.name, d)? }))
}

/// The parsed expression of a `{…}` directive value (for:each, iterator:*, lwc:spread/on/slot-bind/is/…):
/// a member path, or (complex expressions, quoted complex values, computed members) opaque source.
/// Values that failed to parse were rejected by the element pre-pass. `location` (the attribute's) is
/// read only for a complex value.
fn bound_expr<'a>(v: &str, location: impl FnOnce() -> Located<SourceLocation>, d: &Ctx<'a>) -> Located<Option<Expression<'a>>> {
    if let Some(e) = parse_binding(v) { return Ok(Some(e)); }
    if !expr_check::is_expression(v) { return Ok(None); }
    non_binding_expr(v, location, d).map(Some)
}

fn wrap_loop<'a>(l: Loop<'a>, children: Vec<ChildNode<'a>>, location: SourceLocation) -> ChildNode<'a> {
    match l {
        Loop::Each { expression, item, index, directive_location } => ChildNode::ForEach(ForEach { expression, item, index, children, location, directive_location }),
        Loop::Of { expression, iterator, directive_location } => ChildNode::ForOf(ForOf { expression, iterator, children, location, directive_location }),
    }
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
fn split_props<'a>(attrs: Vec<Attribute<'a>>, keep: impl Fn(&str) -> bool, d: &mut Ctx) -> (Vec<Attribute<'a>>, Vec<Property<'a>>) {
    let mut props: Vec<Property<'a>> = Vec::new();
    let attrs = attrs.into_iter().filter_map(|a| {
        // Remaining `lwc:*` (lwc:on, lwc:slot-data, …) are directives TS strips before isAttribute;
        // codegen still reads them from attributes, so they never become props.
        if keep(&a.name) || a.name.starts_with("lwc:") { return Some(a); }
        let name = attr_to_prop(&a.name);
        let p = Property { name: name.clone(), attribute_name: a.name, value: a.value, location: a.location };
        match props.iter_mut().find(|x| x.name == name) {
            Some(x) => {
                d.push(ParserDiagnostics::DUPLICATE_ATTR_PROP_TRANSFORM.generate(&[&x.attribute_name, &p.attribute_name, &name], p.location.clone()));
                *x = p;
            }
            None => props.push(p),
        }
        None
    }).collect();
    (attrs, props)
}

/// TS applyKey + LWC_DIRECTIVE_PROCESSORS, in their order: key, lwc:external, lwc:dynamic, lwc:is,
/// lwc:dom, lwc:inner-html, lwc:ref, lwc:spread, lwc:on, lwc:slot-bind. Each picks its attribute;
/// values were validated by the element pre-pass. Returns the directives and whether lwc:external was set.
fn element_directives<'a>(p: &mut ParsedAttribute, key: Option<Expression<'a>>, n: &NodeRef, d: &Ctx<'a>) -> Located<(Vec<ElementDirective<'a>>, bool)> {
    let bound = |p: &mut ParsedAttribute, name: &str, f: fn(Expression<'a>) -> ElementDirective<'a>| -> Located<Option<ElementDirective<'a>>> {
        match p.pick(name) { Some(a) => Ok(bound_expr(&a.value, || attr_loc(n, &a.name, d), d)?.map(f)), None => Ok(None) }
    };
    let external = p.pick("lwc:external").is_some();
    let dynamic = bound(p, "lwc:dynamic", ElementDirective::Dynamic)?;
    let is = bound(p, "lwc:is", ElementDirective::Is)?;
    let dom = p.pick("lwc:dom").map(|a| ElementDirective::Dom(LiteralValue::String(a.value.clone())));
    let inner = match p.pick("lwc:inner-html") {
        Some(a) => Some(ElementDirective::InnerHtml(match bound_expr(&a.value, || attr_loc(n, &a.name, d), d)? {
            Some(e) => AttributeValue::Expression(e),
            None => AttributeValue::Literal(Literal { value: LiteralValue::String(a.value.clone()) }),
        })),
        None => None,
    };
    // `lwc:ref="foo"`: string literal only. TODO: diagnostics for boolean/empty/expression values (skipped for now).
    let lwc_ref = p.pick("lwc:ref").map(|a| a.value.clone())
        .filter(|v| !v.is_empty() && parse_binding(v).is_none() && !(v.starts_with('{') && v.ends_with('}')))
        .map(ElementDirective::Ref);
    let spread = bound(p, "lwc:spread", ElementDirective::Spread)?;
    let on = bound(p, "lwc:on", ElementDirective::OnDirective)?;
    let slot_bind = bound(p, "lwc:slot-bind", ElementDirective::SlotBind)?;
    let out = [key.map(ElementDirective::Key), dynamic, is, dom, inner, lwc_ref, spread, on, slot_bind].into_iter().flatten().collect();
    Ok((out, external))
}

/// TS parseElementLocation: an element parse5 inserted while recovering (e.g. an implied `<tbody>`) has no
/// location; it takes the nearest element ancestor's (attributes and start/end tags included), else
/// `parse5ParentLocation`.
pub(crate) fn elem_parse5_loc(n: &NodeRef, d: &Ctx) -> parse5::ElementLocation {
    let mut cur = n.clone();
    loop {
        let parent = { let b = cur.borrow(); if b.source_code_location_ref().is_some() { break; } b.parent() };
        match parent { Some(p) if matches!(p.borrow().data, NodeData::Element { .. }) => cur = p, _ => break }
    }
    let l = cur.borrow().source_code_location();
    l.unwrap_or_else(|| d.parent_loc.clone())
}

/// TS `ast.sourceLocation(parse5ElmLocation)`: the span of [`elem_parse5_loc`], no start tag read.
pub(crate) fn elem_span(n: &NodeRef, d: &Ctx) -> SourceLocation { loc(&elem_parse5_loc(n, d).location) }

/// TS `isComplexTemplateExpressionEnabled`: the flag and API version ≥ 66.
fn complex_enabled(cfg: &NormalizedConfig) -> bool {
    cfg.experimental_complex_expressions && lwc_shared::api_version::is_api_feature_enabled(lwc_shared::api_version::ApiFeature::EnableComplexTemplateExpressions, cfg.api_version)
}

/// A `{…}` value that is not a simple binding. Without complex expressions, TS validateExpression admits
/// only Identifier / MemberExpression trees, so under experimentalComputedMemberExpression a computed
/// member chain is typed ([`legacy_computed_chain`]); anything else is an oxc-parsed `Complex`.
/// `location` is read only for the complex case.
fn non_binding_expr<'a>(v: &str, location: impl FnOnce() -> Located<SourceLocation>, d: &Ctx<'a>) -> Located<Expression<'a>> {
    match legacy_computed_chain(v, d) { Some(e) => Ok(e), None => d.complex(v, location()?) }
}

/// Legacy mode under experimentalComputedMemberExpression: `{a[b].c}` as a typed computed member chain
/// (`computed: true`) that codegen binds like TS bindExpression. Frozen legacy (AB-L0058); deleted with
/// legacy mode in AB-D0004 S6.
fn legacy_computed_chain<'a>(v: &str, d: &Ctx<'a>) -> Option<Expression<'a>> {
    if d.legacy_computed { braced_chain(v, true) } else { None }
}

/// TS getTemplateAttribute + normalizeAttributeValue value lowering: an expression (member path, or
/// opaque source for anything acorn parsed), `true` for a valueless attribute, else the string.
/// `v` is the element pre-pass's normalized value (the [`element_check::CheckedAttr`] pair); elements whose
/// values failed to normalize or parse were dropped there.
fn lower_attr<'a>(a: &parse5::Attribute, v: &element_check::Av, n: &NodeRef, d: &Ctx<'a>) -> Located<Attribute<'a>> {
    let location = attr_loc(n, &a.name, d)?;
    let string = |s: String| {
        let v = if a.name == "class" { s.split_ascii_whitespace().collect::<Vec<_>>().join(" ") } else { s };
        AttributeValue::Literal(Literal { value: LiteralValue::String(v) })
    };
    let value = match v {
        element_check::Av::Expr(v) => AttributeValue::Expression(match parse_binding(v) { Some(e) => e, None => non_binding_expr(v, || Ok(location), d)? }),
        element_check::Av::Bool => AttributeValue::Literal(Literal { value: LiteralValue::Bool(true) }),
        element_check::Av::Str(v) => string(v.clone()),
    };
    Ok(Attribute { name: a.name.clone(), value, location })
}

/// `if:true={x}` / `if:false={x}`; when both are present LWC keeps `if:true` only.
/// TS parseIf: `pickAll(IF_RE)`, the first one is the directive (extras warn 1182 in validate::multiple_if).
fn if_directive<'a>(p: &mut ParsedAttribute, n: &NodeRef, d: &Ctx) -> Located<Option<(String, Expression<'a>, SourceLocation)>> {
    let Some(a) = p.pick_all_prefix("if:").first().copied() else { return Ok(None) };
    // TODO: diagnostic for non-identifier values.
    let Some(e) = braced_chain(&a.value, d.legacy_computed) else { return Ok(None) };
    Ok(Some((a.name["if:".len()..].to_string(), e, attr_loc(n, &a.name, d)?)))
}

enum Lwc<'a> { If(Expression<'a>, SourceLocation), Elseif(Expression<'a>, SourceLocation), Else(SourceLocation) }

/// `lwc:if={x}` / `lwc:elseif={x}` / `lwc:else`. TODO: diagnostics (bad values, multiple directives).
///
/// The role is kept even when the condition is not a simple binding: TS parses the value with
/// acorn (`{isTrue()}` is a CallExpression under experimentalComplexExpressions) and the element
/// still opens/continues the chain. Dropping the role here turned `<template lwc:if={f()}>` into a
/// directive-less template (1153, dropped) and its `lwc:else` sibling into an orphan (1165).
/// Non-binding conditions become `Expression::Complex` (oxc-parsed inner source); invalid ones were already
/// reported by `check_expr`.
fn lwc_directive<'a>(p: &mut ParsedAttribute, n: &NodeRef, d: &Ctx<'a>) -> Located<Option<Lwc<'a>>> {
    // TS DIRECTIVE_PARSERS order (IfBlock, ElseifBlock, ElseBlock), not attribute order: on a dropped
    // `<template lwc:else lwc:if>` (1162) the node TS already pushed is the lwc:if block. Each parser
    // picks its own attribute.
    let picked = [p.pick("lwc:if"), p.pick("lwc:elseif"), p.pick("lwc:else")];
    let Some(a) = picked.into_iter().flatten().next() else { return Ok(None) };
    let l = attr_loc(n, &a.name, d)?;
    Ok(Some(match a.name.as_str() {
        "lwc:if" => Lwc::If(condition_expr(&a.value, l, d)?, l),
        "lwc:elseif" => Lwc::Elseif(condition_expr(&a.value, l, d)?, l),
        _ => Lwc::Else(l),
    }))
}

/// Simple binding → Identifier/Member; anything else → [`non_binding_expr`].
fn condition_expr<'a>(v: &str, location: SourceLocation, d: &Ctx<'a>) -> Located<Expression<'a>> {
    match parse_binding(v) { Some(e) => Ok(e), None => non_binding_expr(v, || Ok(location), d) }
}

/// One lowered sibling: its branch content, optional `lwc:*` role, and element location.
struct Lowered<'a> { content: Vec<ChildNode<'a>>, lwc: Option<Lwc<'a>>, location: SourceLocation }

/// TS `isCustomElementTag`: dashed, but not one of the dashed built-in SVG/MathML tags.
fn is_custom_element_tag(tag: &str) -> bool { tag.contains('-') && !tables::DASHED_TAGNAME_ELEMENT_SET.contains(&tag) }

fn lower_one<'a>(n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx<'a>, in_loop: bool) -> Parsed<'a> {
    let b = n.borrow();
    match &b.data {
        NodeData::Text { value } => {
            let l = required_loc(n, NO_TEXT_LOCATION)?;
            let src = raw_source(&l, d);
            let Some(v) = clean_text(value, &src) else { return Ok(None) };
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
            if validate::text_expressions(&segments, &l, cfg, d)? { return Ok(None); }
            // TS parseText / parseTextComplex: every `{…}` token is an expression (complex mode parses any
            // `{`; otherwise EXPRESSION_RE needs a non-empty body). Non-member expressions stay opaque source.
            let complex = complex_enabled(cfg);
            // TS Text.raw is the token from the cleaned undecoded source; fall back to the decoded token
            // when the source split does not align.
            let raw_tokens = split_mustache(&clean_text_node(&src));
            let aligned = raw_tokens.len() == segments.len() && raw_tokens.iter().zip(&segments).all(|(a, b)| a.1 == b.1);
            let content = segments.into_iter().enumerate().map(|(i, (seg, is_expr))| {
                let raw = if aligned { raw_tokens[i].0.clone() } else { seg.clone() };
                let tv = if !is_expr { TextValue::Literal(seg) } else {
                    match parse_binding(&seg) {
                        Some(e) => TextValue::Expression(e),
                        // TS transformText: `isStringLiteral(value) ? value.value : bindExpression(value)`.
                        // Inner text comes from build()/ComplexExpression.source (AB-L0055 S3 (a)); the
                        // S4: StringLiteral comes off the AST via as_literal (isStringLiteral).
                        None if complex || expr_check::is_expression(&seg) => match legacy_computed_chain(&seg, d) {
                            Some(e) => TextValue::Expression(e),
                            None => {
                                let c = expression_complex::build(d.alloc, &seg, l)?;
                                match c.as_literal() {
                                    Some(lwc_shared::template_ir::LiteralView::String(text)) => TextValue::Literal(text.to_string()),
                                    _ => TextValue::Expression(Expression::Complex(c)),
                                }
                            }
                        }
                        None => TextValue::Literal(seg),
                    }
                };
                Ok(ChildNode::Text(Text { value: tv, raw, location: l }))
            }).collect::<Located<Vec<_>>>()?;
            Ok(Some(Lowered { content, lwc: None, location: l }))
        }
        NodeData::Comment { data } => {
            // TS parseChildren pushes every comment (parseComment reads its location); whether it is
            // preserved is decided by its consumers (codegen, validateChildren), as in TS.
            let location = required_loc(n, NO_COMMENT_LOCATION)?;
            // TS parseComment: value = decodeTextContent(data).
            let comment = Comment { value: decode_text_content(data), raw: format!("<!--{data}-->"), location: location.clone() };
            Ok(Some(Lowered { content: vec![ChildNode::Comment(comment)], lwc: None, location }))
        }
        NodeData::Element { tag_name, attrs, namespace_uri, child_nodes, template_content, .. } => {
            // Foreign attrs (`xlink:href`) carry a prefix; LWC uses the qualified name.
            let qualified: Vec<parse5::Attribute> = attrs.iter().map(|a| match &a.prefix {
                // TS attributeName: `prefix ? `${prefix}:${name}` : name` (parse5 gives `xmlns` prefix "").
                Some(p) if !p.is_empty() => parse5::Attribute { name: format!("{p}:{}", a.name), ..a.clone() },
                _ => a.clone(),
            }).collect();
            // TS parseElementLocation: an element parse5 recreated without a location reads its attributes
            // (in element_errors, TS parseAttributes) and takes its span from the ancestor it falls back to.
            let el = elem_parse5_loc(n, d);
            let span = loc(&el.location);
            // TS parseElement throws (withErrorRecovery): the element and its subtree are dropped.
            validate::html_recovery(n, d);
            let pre = element_check::element_errors(tag_name, namespace_uri, &qualified, n, cfg, d)?;
            // TS parseAttributes: the one ParsedAttribute every directive parser picks from, over the
            // pre-pass (attribute, normalized value) pairs. Built only when there are pairs to pick from
            // (AB-X0017 follow-up: no dead unwrap_or_default over a missing lwc_parsed).
            let chk = match &pre {
                Ok(chk) => chk,
                // TS parseIfBlock/parseElseifBlock/parseElseBlock push the (empty) block into the parent and
                // update the sibling if-chain before a later throw drops the element.
                Err(e) => {
                    let kept = if let Some(attrs) = e.lwc_parsed.as_deref() {
                        let mut parsed = ParsedAttribute::new(attrs);
                        Some(Lowered { content: Vec::new(), lwc: lwc_directive(&mut parsed, n, d)?, location: span })
                    } else {
                        None
                    };
                    return Err(Thrown { diag: e.diag.clone(), kept }.into());
                }
            };
            let mut parsed = ParsedAttribute::new(&chk.attrs);
            // A childless (post-creation throw) element: TS runs no further checks on it.
            let threw = chk.thrown.is_some();
            // A bare (post-creation throw) element still sits in the IfBlock / Elseif / Else its directive
            // parser created, so its lwc role is picked first; none of its other attributes were applied.
            let lwc = lwc_directive(&mut parsed, n, d)?;
            if chk.bare { parsed.clear(); }
            let attrs: &[parse5::Attribute] = if chk.bare { &[] } else { &qualified };
            if tag_name != "template" && !threw { validate::non_template_element(tag_name, namespace_uri, attrs, n, cfg, d)?; }
            let lp = loop_directive(&mut parsed, n, d)?;
            let dir = if_directive(&mut parsed, n, d)?;
            if !threw { validate::multiple_if(tag_name, attrs, n, d); }
            if tag_name == "template" {
                // No base element: TS reads only `sourceLocation(parse5ElmLocation)`, never its start tag.
                if let Some(sd) = parsed.pick("lwc:slot-data") {
                    return scoped_slot_fragment(sd, parsed, dir.is_some() || lp.is_some() || lwc.is_some(), template_content, child_nodes, n, cfg, d, el);
                }
                // TS validateTemplate reports 1145 (every attribute parseElementDirectives did not pick,
                // including key / lwc:dom / ...) before the 1153 "no directive" warning.
                if validate::template(parsed.get_attributes(), dir.is_some() || lp.is_some() || lwc.is_some(), &span, d) { return Ok(None); }
                // `<template>` in foreign (svg) content has no template_content; children are direct.
                let kids = match template_content { Some(c) => c.borrow().child_nodes().cloned().unwrap_or_default(), None => child_nodes.clone() };
                d.in_loop = lp.is_some();
                // lwc:if/elseif/else blocks are transparent for the slot-data parent check; for:each / if:* are not.
                let prev_sd = d.slot_data_parent_ok; if dir.is_some() || lp.is_some() { d.slot_data_parent_ok = false; }
                let prev_parent = std::mem::replace(&mut d.parent_loc, el);
                let children = lower_children(&kids, cfg, d);
                d.slot_data_parent_ok = prev_sd; d.parent_loc = prev_parent;
                let mut children = children?;
                // TS validateChildren throws after the children were parsed: the node stays in the tree.
                let mut slot_content = Vec::new();
                if d.in_slot_fragment { validate::non_element_slot_content(&children, cfg.preserve_html_comments, &mut slot_content); }
                if let Some((modifier, condition, directive_location)) = dir {
                    children = vec![ChildNode::If(If { modifier, condition, children, location: span.clone(), directive_location })];
                }
                if let Some(l) = lp { children = vec![wrap_loop(l, children, span.clone())]; }
                let node = Lowered { content: children, lwc, location: span };
                return match slot_content.into_iter().next() {
                    Some(diag) => Err(Thrown { diag, kept: Some(node) }.into()),
                    None => Ok(Some(node)),
                };
            }
            // TS parseBaseElement creates the element (ast.element / ast.component / ...: elementSourceLocation
            // reads the start tag).
            let location = element_location(&el)?;
            // TS applyKey + applyLwcDirectives (elements only: on `<template>` every one of these stays
            // unpicked and validateTemplate reports it, 1145).
            // Any expression value is the key (element_check reported the non-expression and index cases), under
            // complex expressions too (`key="{x()}"` → `api_key(n, $cmp.x())`).
            let mut key = match parsed.pick("key") { Some(a) => bound_expr(&a.value, || attr_loc(n, &a.name, d), d)?, None => None };
            if !validate::key_in_iteration(tag_name, key.is_some(), lp.is_some(), in_loop, n, d) { key = None; }
            let (directives, external) = element_directives(&mut parsed, key, n, d)?;
            // `on<event>={handler}` → EventListener (stripped from attributes).
            let mut listeners = Vec::new();
            let mut attrs = Vec::new();
            for c in parsed.get_attributes().iter().copied() {
                let a = c.attr();
                let is_on = a.name.len() > 2 && a.name.starts_with("on") && a.name.as_bytes()[2].is_ascii_alphabetic();
                // TS applyHandlers: any expression value is a listener; under complex expressions that includes
                // non-member sources (arrows, calls, computed members). Only `on*` values are lowered: a
                // non-handler `{…}` is lowered as an attribute below (AB-D0004 S0: `data-foo={}` built and
                // discarded a handler oxc cannot parse). Read the pre-pass value (AB-X0017 follow-up).
                if !is_on { attrs.push(c); continue; }
                let handler = match c.value() {
                    element_check::Av::Expr(v) => match parse_binding(v) {
                        Some(e) => Some(e),
                        None if complex_enabled(cfg) => Some(d.complex(v, attr_loc(n, &a.name, d)?)?),
                        None => None,
                    },
                    _ => None,
                };
                match handler {
                    Some(handler) => listeners.push(EventListener { name: a.name[2..].to_string(), handler, location: attr_loc(n, &a.name, d)? }),
                    None => attrs.push(c),
                }
            }
            let attributes = attrs.iter().map(|c| lower_attr(c.attr(), c.value(), n, d)).collect::<Located<Vec<_>>>()?;
            let is_comp = is_custom_element_tag(tag_name);
            // `<lwc:component>` is transparent for the slot-ancestor lookup (its children are slotted into the dynamic component).
            let anc = if tag_name == "lwc:component" { d.slot_ancestor.clone() } else if is_comp { None } else { Some(tag_name.clone()) };
            let prev_anc = std::mem::replace(&mut d.slot_ancestor, anc);
            let is_cmp_parent = tag_name == "lwc:component" || (is_comp && tag_name != "slot" && !external);
            let prev_sd = std::mem::replace(&mut d.slot_data_parent_ok, is_cmp_parent && dir.is_none() && lp.is_none());
            let prev_parent = std::mem::replace(&mut d.parent_loc, el);
            let children = if threw { Ok(Vec::new()) } else { lower_children(child_nodes, cfg, d) };
            d.slot_ancestor = prev_anc; d.slot_data_parent_ok = prev_sd; d.parent_loc = prev_parent;
            let children = children?;
            // TS validateChildren: lwc:dom / lwc:inner-html elements must be empty; `effectiveChildren`
            // counts comments only when preserved. Thrown after the children were parsed, so the element
            // stays in the tree.
            let mut thrown = chk.thrown.clone();
            if children.iter().any(|c| cfg.preserve_html_comments || !matches!(c, ChildNode::Comment(_))) {
                if directives.iter().any(|x| matches!(x, ElementDirective::Dom(_))) { thrown = Some(ParserDiagnostics::LWC_DOM_INVALID_CONTENTS.generate(&[], location.location.clone())); }
                else if directives.iter().any(|x| matches!(x, ElementDirective::InnerHtml(_))) { thrown = Some(ParserDiagnostics::LWC_INNER_HTML_INVALID_CONTENTS.generate(&[&format!("<{tag_name}>")], location.location.clone())); }
            }
            let node = if tag_name == "slot" {
                // `name` stays in attributes too; slot_name is the TS `slotName`. Read the pre-pass
                // value (AB-X0017 follow-up), not raw parse5 attr.value.
                let slot_name = attrs.iter().find(|c| c.attr().name == "name").and_then(|c| match c.value() {
                    element_check::Av::Str(s) => Some(s.clone()),
                    _ => None,
                }).unwrap_or_default();
                ChildNode::Slot(Slot { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, directives: directives.clone(), listeners, children, location: location.clone(), slot_name, properties: Vec::new() })
            } else if is_custom_element_tag(tag_name) && external {
                // TS: `isCustomElementTag && parsedAttr.get(External)` → ExternalComponent; attrs stay
                // attributes (no prop split) and applyLwcExternalDirective picked the directive.
                ChildNode::ExternalComponent(ExternalComponent { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, directives: directives.clone(), listeners, children, location: location.clone(), properties: Vec::new() })
            } else if is_custom_element_tag(tag_name) {
                { let (attributes, properties) = split_props(attributes, |a| is_attribute(ElementKind::Component, tag_name, a), d); ChildNode::Component(Component { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, properties, directives: directives.clone(), listeners, children, location: location.clone() }) }
            } else if tag_name == "lwc:component" {
                // TS parseLwcComponent → ast.lwcComponent; isAttribute uses the component table.
                let (attributes, properties) = split_props(attributes, |a| is_attribute(ElementKind::LwcComponent, tag_name, a), d);
                ChildNode::LwcComponent(LwcComponent { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, properties, directives: directives.clone(), listeners, children, location: location.clone() })
            } else {
                // TS isAttribute: `<input value|checked>` are props.
                let (attributes, properties) = split_props(attributes, |a| is_attribute(ElementKind::Element, tag_name, a), d);
                ChildNode::Element(Element { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, properties, directives: directives.clone(), listeners, children, location: location.clone() })
            };
            // Loop wraps outside; if:* is applied per iteration inside.
            let node = match dir {
                Some((modifier, condition, directive_location)) => ChildNode::If(If { modifier, condition, children: vec![node], location: location.location.clone(), directive_location }),
                None => node,
            };
            let node = match lp { Some(l) => wrap_loop(l, vec![node], location.location.clone()), None => node };
            let node = Lowered { content: vec![node], lwc, location: location.location };
            match thrown {
                Some(diag) => Err(Thrown { diag, kept: Some(node) }.into()),
                None => Ok(Some(node)),
            }
        }
        _ => Ok(None),
    }
}

/// `<template lwc:slot-data="d" [slot=...]>` → ScopedSlotFragment (TS `parseScopedSlotFragment`,
/// same validation order: 1179, 1178, 1174, identifier 1211/1059; then 1177 on direct text/comment children).
#[allow(clippy::too_many_arguments)]
fn scoped_slot_fragment<'a>(
    sd: &parse5::Attribute, mut parsed: ParsedAttribute, other_directive: bool,
    template_content: &Option<NodeRef>, child_nodes: &[NodeRef], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx<'a>,
    el: parse5::ElementLocation,
) -> Parsed<'a> {
    let span = loc(&el.location);
    let sd_loc = attr_loc(n, &sd.name, d)?;
    let mut threw = Vec::new();
    let Some(slot_data) = validate::scoped_slot_data(sd, other_directive, &span, &sd_loc, d, &mut threw) else {
        let diag = threw.pop().expect("scoped_slot_data reports the diagnostic it drops the fragment for");
        return Err(Thrown { diag, kept: None }.into());
    };
    d.add_scope_node(ScopeNode::ScopedSlotFragment);
    // TS parseScopedSlotFragment picks `slot`; parseElement then runs validateTemplate on the leftovers.
    let slot_name = parsed.pick_checked("slot").map(|c| lower_attr(c.attr(), c.value(), n, d)).transpose()?.map(|a| a.value)
        .unwrap_or(AttributeValue::Literal(Literal { value: LiteralValue::String(String::new()) }));
    validate::template(parsed.get_attributes(), true, &span, d);
    let kids = match template_content { Some(c) => c.borrow().child_nodes().cloned().unwrap_or_default(), None => child_nodes.to_vec() };
    let prev_sd = std::mem::replace(&mut d.slot_data_parent_ok, false);
    let prev_in = std::mem::replace(&mut d.in_slot_fragment, true);
    let prev_parent = std::mem::replace(&mut d.parent_loc, el);
    let children = lower_children(&kids, cfg, d);
    d.slot_data_parent_ok = prev_sd; d.in_slot_fragment = prev_in; d.parent_loc = prev_parent;
    let children = children?;
    // TS validateChildren throws after the children were parsed: the fragment stays in the tree.
    let mut slot_content = Vec::new();
    validate::non_element_slot_content(&children, cfg.preserve_html_comments, &mut slot_content);
    let frag = ScopedSlotFragment { slot_data, slot_name, children, location: span.clone(), directive_location: sd_loc };
    let node = Lowered { content: vec![ChildNode::ScopedSlotFragment(frag)], lwc: None, location: span };
    match slot_content.into_iter().next() {
        Some(diag) => Err(Thrown { diag, kept: Some(node) }.into()),
        None => Ok(Some(node)),
    }
}

/// Append `branch` at the tail of an if-chain (an Else tail is final: a later branch got 1165).
fn attach_else<'a>(slot: &mut Option<Box<ElseBranch<'a>>>, branch: ElseBranch<'a>) {
    match slot.as_deref_mut() {
        None => *slot = Some(Box::new(branch)),
        Some(ElseBranch::Elseif(e)) => attach_else(&mut e.else_, branch),
        Some(ElseBranch::Else(_)) => {}
    }
}

/// Whitespace text between chain siblings doesn't break an lwc:if chain.
fn continue_marker() -> Option<i32> { Some(2) }

/// TS `ParserCtx.withErrorRecovery`: a throw reports its diagnostic and keeps whatever node TS had
/// already pushed into the parent; an internal error is rethrown.
fn with_error_recovery<'a>(d: &mut Ctx<'a>, f: impl FnOnce(&mut Ctx<'a>) -> Parsed<'a>) -> Located<Option<Lowered<'a>>> {
    match f(d) {
        Ok(l) => Ok(l),
        Err(Throw::Recovered(Thrown { diag, kept })) => { d.push(diag); Ok(kept) }
        Err(Throw::Internal(e)) => Err(e),
    }
}

fn lower_children<'a>(nodes: &[NodeRef], cfg: &NormalizedConfig, d: &mut Ctx<'a>) -> Located<Vec<ChildNode<'a>>> {
    let in_loop = std::mem::replace(&mut d.in_loop, false);
    // TS beginSiblingScope / endSiblingScope (always paired: outside withErrorRecovery).
    let prev_chain = d.chain.take();
    // lwc:if chain slot scoping: each branch starts from the pre-chain set; after the chain, the union.
    let mut chain: Option<(HashSet<String>, HashSet<String>)> = None;
    let mut lowered: Vec<Lowered<'a>> = Vec::new();
    for n in nodes {
        let role = match &n.borrow().data {
            NodeData::Element { attrs, .. } => attrs.iter().find_map(|a| match a.name.as_str() { "lwc:if" => Some(0), "lwc:elseif" | "lwc:else" => Some(1), _ => None }),
            NodeData::Text { value } if js_trim(value).is_empty() => continue_marker(),
            _ => None,
        };
        if role == Some(2) { lowered.extend(with_error_recovery(d, |d| lower_one(n, cfg, d, in_loop))?); continue; }
        if let Some((base, union)) = chain.as_mut() {
            union.extend(d.seen_slots.iter().cloned());
            if role == Some(1) { d.seen_slots = base.clone(); } else { d.seen_slots = union.clone(); chain = None; }
        }
        if role == Some(0) { chain = Some((d.seen_slots.clone(), d.seen_slots.clone())); }
        let is_elem = matches!(&n.borrow().data, NodeData::Element { .. });
        let is_comment = matches!(&n.borrow().data, NodeData::Comment { .. });
        // TS parseChildren, inside withErrorRecovery: beginElementScope; parseElement; node =
        // endElementScope(); then an open chain ends unless node is an If/Elseif block. A throw skips the
        // pop and the chain bookkeeping, leaking the element's scope. Non-whitespace text or a preserved
        // comment ends the chain.
        let lowered_one = with_error_recovery(d, |d| {
            if is_elem {
                d.scopes.push(Vec::new());
                let l = lower_one(n, cfg, d, in_loop)?;
                let node = d.scopes.pop().and_then(|s| s.into_iter().next());
                if node.is_some() && !matches!(node, Some(ScopeNode::IfBlock | ScopeNode::ElseifBlock)) { d.chain = None; }
                return Ok(l);
            }
            let l = lower_one(n, cfg, d, in_loop)?;
            if !is_comment || cfg.preserve_html_comments { d.chain = None; }
            Ok(l)
        })?;
        if let Some(l) = lowered_one { lowered.push(l); }
    }
    d.chain = prev_chain;
    if let Some((_, mut union)) = chain { union.extend(d.seen_slots.drain()); d.seen_slots = union; }
    // TS parseElseifBlock / parseElseBlock: `conditionalParent.else = node` on the chain's current node,
    // which may be separated from it by siblings whose parse threw.
    let mut out = Vec::new();
    let mut cur_if: Option<usize> = None;
    for l in lowered {
        let branch = match l.lwc {
            None => { out.extend(l.content); continue; }
            Some(Lwc::If(condition, directive_location)) => {
                cur_if = Some(out.len());
                out.push(ChildNode::IfBlock(IfBlock { condition, children: l.content, else_: None, location: l.location, directive_location }));
                continue;
            }
            Some(Lwc::Elseif(condition, directive_location)) => ElseBranch::Elseif(ElseifBlock { condition, children: l.content, else_: None, location: l.location, directive_location }),
            Some(Lwc::Else(directive_location)) => ElseBranch::Else(ElseBlock { children: l.content, location: l.location, directive_location }),
        };
        // element_check already reported 1165 for a branch without an open chain; with no IfBlock here, the
        // chain's lwc:if element threw (its node is not lowered) and the compile has failed.
        if let Some(ChildNode::IfBlock(b)) = cur_if.map(|i| &mut out[i]) { attach_else(&mut b.else_, branch); }
    }
    Ok(out)
}

/// Parses `source`; complex expression ASTs are allocated in `alloc`, which the IR borrows.
pub fn parse_template<'a>(alloc: &'a oxc_allocator::Allocator, source: &str, config: &NormalizedConfig) -> TemplateParseResult<'a> {
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
        NodeData::Text { value } => !js_trim(value).is_empty(),
        _ => false,
    }).cloned().collect();
    if roots.len() > 1 {
        warnings.push(ParserDiagnostics::MULTIPLE_ROOTS_FOUND.generate_normalized(&[], node_loc(&roots[1])));
        return TemplateParseResult { root: None, warnings };
    }
    let Some(tmpl) = roots.first().filter(|n| matches!(n.borrow().data, NodeData::Element { .. })).cloned() else {
        // TS ctx.throw without a location: normalizeLocation(undefined).
        warnings.push(ParserDiagnostics::MISSING_ROOT_TEMPLATE_TAG.generate_normalized(&[], None));
        return TemplateParseResult { root: None, warnings };
    };
    // TS parseRoot: the internal location check, then 1079 at the root's location.
    let Some(root_loc) = tmpl.borrow().source_code_location() else {
        return internal_failure(InternalError(NO_ROOT_LOCATION));
    };
    if let Some(t) = tmpl.borrow().tag_name().filter(|t| *t != "template") {
        warnings.push(ParserDiagnostics::ROOT_TAG_SHOULD_BE_TEMPLATE.generate(&[t], loc(&root_loc.location)));
        return TemplateParseResult { root: None, warnings };
    }
    let tb = tmpl.borrow();
    let root_attrs: &[parse5::Attribute] = match &tb.data { NodeData::Element { attrs, .. } => attrs, _ => &[] };
    let content = match &tb.data { NodeData::Element { template_content, .. } => template_content.clone(), _ => None };
    let kids = content.map(|c| c.borrow().child_nodes().cloned().unwrap_or_default()).unwrap_or_default();
    let light = matches!(&tb.data, NodeData::Element { attrs, .. } if attrs.iter().any(|a| a.name == "lwc:render-mode" && a.value == "light"));
    let mut cx = Ctx {
        alloc, diags: Diags::new(warnings), seen_slots: HashSet::new(), in_loop: false, slot_ancestor: None, slot_data_parent_ok: false,
        seen_scoped_slots: HashSet::new(), light, in_slot_fragment: false,
        legacy_computed: config.experimental_computed_member_expression && !config.experimental_complex_expressions,
        instrumentation: config.instrumentation.clone(),
        src16: source.encode_utf16().collect(), seen_ids: HashSet::new(), scopes: Vec::new(), chain: None,
        parent_loc: root_loc.clone(),
    };
    // TS parseRoot → parseAttributes(root, rootLocation), ast.root(rootLocation) (its start tag), then the
    // root directives and validateRoot.
    let location = match element_check::root_errors(root_attrs, &tmpl, &root_loc, config, &mut cx) {
        Ok(Some(location)) => location,
        Ok(None) => return TemplateParseResult { root: None, warnings: cx.diags.into_vec() },
        Err(e) => return internal_failure(e),
    };
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
    match lower_children(&kids, config, &mut cx) {
        Ok(children) => TemplateParseResult { root: Some(Root { location, directives, children }), warnings: cx.diags.into_vec() },
        Err(e) => internal_failure(e),
    }
}

/// An internal error ends the parse: `compile` reports it as its only diagnostic and there is no root.
fn internal_failure<'a>(e: InternalError) -> TemplateParseResult<'a> {
    TemplateParseResult { root: None, warnings: vec![e.diagnostic()] }
}

/// TS `isValidHTMLAttribute`, except the `HTML_ATTRIBUTE_ELEMENT_MAP` lookup is own-key only: upstream indexes a
/// plain object, so `constructor` crashes (1052) and `__proto__` skips 1057; Rust treats both as unknown (D-0197).
fn is_valid_html_attribute(tag: &str, attr: &str) -> bool {
    use tables::*;
    let directive = attr == "key" || attr.starts_with("lwc:") || attr.starts_with("if:") || attr.starts_with("for:") || attr.starts_with("iterator:");
    if GLOBAL_ATTRIBUTE.contains(&attr) || ARIA_ATTRS.contains(&attr) || is_data_attribute(attr) || directive
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
    use crate::parser::expression_complex::build;
    use lwc_shared::template_ir::{LiteralView, SourceLocation};
    use oxc_allocator::Allocator;

    const AT: SourceLocation = SourceLocation {
        start_line: 1, start_column: 1, end_line: 1, end_column: 40, start: 0, end: 39,
    };

    fn lit(src: &str) -> Option<String> {
        let alloc = Allocator::default();
        let c = build(&alloc, &format!("{{{src}}}"), AT).ok()?;
        match c.as_literal() {
            Some(LiteralView::String(s)) => Some(s.to_string()),
            _ => None,
        }
    }

    #[test]
    fn as_literal_reads_string_literal_value() {
        assert_eq!(lit("'foo foo'").as_deref(), Some("foo foo"));
        assert_eq!(lit(r#""a\u0041""#).as_deref(), Some("aA"));
        assert_eq!(lit("'a' + 'b'"), None);
        assert_eq!(lit("`a`"), None);
        assert_eq!(lit("foo"), None);
    }
}

#[cfg(test)]
mod generated_tables_tests {
    use super::constants::{DASHED_TAGNAME_ELEMENT_SET, KNOWN_HTML_AND_SVG_ELEMENTS, SUPPORTED_SVG_TAGS};
    use super::utils::html_element_attributes::HTML_ATTRIBUTE_ELEMENT_MAP;

    /// The generated tables are looked up with `binary_search`, so they must be strictly sorted.
    #[test]
    fn tables_sorted() {
        let haem: Vec<&str> = HTML_ATTRIBUTE_ELEMENT_MAP.iter().map(|e| e.0).collect();
        for (name, keys) in [("KNOWN", KNOWN_HTML_AND_SVG_ELEMENTS), ("SVG", SUPPORTED_SVG_TAGS), ("DASHED", DASHED_TAGNAME_ELEMENT_SET), ("HAEM", &haem[..])] {
            assert!(keys.windows(2).all(|w| w[0] < w[1]), "{name} not strictly sorted");
        }
        assert_eq!((KNOWN_HTML_AND_SVG_ELEMENTS.len(), SUPPORTED_SVG_TAGS.len(), DASHED_TAGNAME_ELEMENT_SET.len(), HTML_ATTRIBUTE_ELEMENT_MAP.len()), (145, 69, 8, 177));
        assert!(SUPPORTED_SVG_TAGS.binary_search(&"clipPath").is_ok() && SUPPORTED_SVG_TAGS.binary_search(&"clippath").is_err());
    }
}
