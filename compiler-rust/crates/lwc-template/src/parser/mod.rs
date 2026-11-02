//! HTML → LWC template IR via parse5-rust.

use crate::config::NormalizedConfig;
use lwc_shared::template_ir::*;
use lwc_shared::{CompilerDiagnostic, DiagnosticLevel};
use parse5::{Location, NodeData, NodeRef, ParserOptions};
use std::collections::HashSet;

mod tables;
mod expr_check;
mod validate;
mod element_check;
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
        start: l.start_offset as u32, end: l.end_offset as u32,
    }
}

fn node_loc(n: &NodeRef) -> SourceLocation {
    n.borrow().source_code_location().map(|l| loc(&l.location)).unwrap_or_default()
}

/// LWC `cleanTextNode`-ish: drop whitespace-only text, trim surrounding whitespace incl. newlines.
fn clean_text(raw: &str) -> Option<String> {
    let t = raw.trim_matches(|c: char| c.is_ascii_whitespace());
    if t.is_empty() { None } else { Some(t.to_string()) }
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
    let mut parts = inner.split('.');
    let first = parts.next()?;
    if !is_ident(first) || expr_check::is_reserved(first) { return None; }
    let mut e = Expression::Identifier(Identifier { name: first.to_string() });
    for p in parts {
        if !is_ident(p) { return None; }
        e = Expression::Member(MemberExpression {
            object: Box::new(e),
            property: Box::new(Expression::Identifier(Identifier { name: p.to_string() })),
            computed: false,
        });
    }
    Some(e)
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
pub(crate) fn raw_source(l: &SourceLocation, d: &Ctx) -> String {
    let (s, e) = (l.start as usize, l.end as usize);
    if s <= e && e <= d.src16.len() { String::from_utf16_lossy(&d.src16[s..e]) } else { String::new() }
}

enum Loop {
    Each { expression: Expression, item: Identifier, index: Option<Identifier>, directive_location: SourceLocation },
    Of { expression: Expression, iterator: Identifier, directive_location: SourceLocation },
}

/// `for:each={x} for:item="i" [for:index="j"]` or `iterator:it={x}`.
fn loop_directive(attrs: &[parse5::Attribute], n: &NodeRef) -> Option<Loop> {
    if let Some(a) = attrs.iter().find(|a| a.name == "for:each") {
        let expression = parse_binding(&a.value)?;
        // TODO: diagnostic when for:item is missing / not an identifier.
        let item = attrs.iter().find(|a| a.name == "for:item").filter(|a| is_ident(&a.value))?;
        let index = attrs.iter().find(|a| a.name == "for:index").filter(|a| is_ident(&a.value))
            .map(|a| Identifier { name: a.value.clone() });
        return Some(Loop::Each { expression, item: Identifier { name: item.value.clone() }, index, directive_location: attr_loc(n, &a.name) });
    }
    let a = attrs.iter().find(|a| a.name.starts_with("iterator:"))?;
    let name = &a.name["iterator:".len()..];
    if !is_ident(name) { return None; }
    Some(Loop::Of { expression: parse_binding(&a.value)?, iterator: Identifier { name: name.to_string() }, directive_location: attr_loc(n, &a.name) })
}

fn wrap_loop(l: Loop, children: Vec<ChildNode>, location: SourceLocation) -> ChildNode {
    match l {
        Loop::Each { expression, item, index, directive_location } => ChildNode::ForEach(ForEach { expression, item, index, children, location, directive_location }),
        Loop::Of { expression, iterator, directive_location } => ChildNode::ForOf(ForOf { expression, iterator, children, location, directive_location }),
    }
}

fn is_directive_attr(name: &str) -> bool {
    matches!(name, "lwc:if" | "lwc:elseif" | "lwc:else" | "if:true" | "if:false" | "for:each" | "for:item" | "for:index" | "key" | "lwc:ref") || name.starts_with("iterator:")
}

/// Component attrs that stay attributes; everything else becomes a camelCased Property.
fn keeps_as_attr(name: &str) -> bool {
    matches!(name, "role" | "style" | "class" | "id" | "slot" | "key") || name.starts_with("aria-") || name.starts_with("data-")
}

fn kebab_to_camel(s: &str) -> String {
    let mut o = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '-' { up = true; } else if up { o.push(c.to_ascii_uppercase()); up = false; } else { o.push(c); }
    }
    o
}

fn split_props(attrs: Vec<Attribute>) -> (Vec<Attribute>, Vec<Property>) {
    let mut props = Vec::new();
    let attrs = attrs.into_iter().filter_map(|a| {
        if keeps_as_attr(&a.name) || a.name.contains(':') { return Some(a); }
        props.push(Property { name: kebab_to_camel(&a.name), attribute_name: a.name, value: a.value, location: a.location });
        None
    }).collect();
    (attrs, props)
}

fn elem_loc(n: &NodeRef) -> ElementSourceLocation {
    n.borrow().source_code_location().map(|l| ElementSourceLocation {
        location: loc(&l.location),
        start_tag: l.start_tag.as_ref().map(loc).unwrap_or_default(),
        end_tag: l.end_tag.as_ref().map(loc),
    }).unwrap_or_default()
}

fn lower_attr(a: &parse5::Attribute, n: &NodeRef) -> Attribute {
    let location = n.borrow().source_code_location().and_then(|l| l.attrs.get(&a.name).map(loc)).unwrap_or_default();
    let value = if let Some(e) = parse_binding(&a.value) {
        AttributeValue::Expression(e)
    } else {
        let v = if a.name == "class" { a.value.split_ascii_whitespace().collect::<Vec<_>>().join(" ") } else { a.value.clone() };
        AttributeValue::Literal(Literal { value: LiteralValue::String(v) })
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
    let a = attrs.iter().find(|a| matches!(a.name.as_str(), "lwc:if" | "lwc:elseif" | "lwc:else"))?;
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
            let v = clean_text(value)?;
            let l = node_loc(n);
            let segments = split_mustache(&v);
            // TS `withErrorRecovery` around parseTextNode: a text node whose expression fails to parse
            // is dropped entirely (the error is still reported).
            if validate::text_expressions(&segments, &l, cfg, d) { return None; }
            let content: Vec<ChildNode> = segments.into_iter().map(|(seg, is_expr)| {
                let tv = if !is_expr { TextValue::Literal(seg) } else {
                    match parse_binding(&seg) { Some(e) => TextValue::Expression(e), None => TextValue::Literal(seg) }
                };
                ChildNode::Text(Text { value: tv, raw: value.clone(), location: l.clone() })
            }).collect();
            Some(Lowered { content, lwc: None, location: SourceLocation::default() })
        }
        NodeData::Comment { data } if cfg.preserve_html_comments => {
            one(ChildNode::Comment(Comment { value: data.clone(), raw: format!("<!--{data}-->"), location: node_loc(n) }))
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
            let chk = element_check::element_errors(tag_name, namespace_uri, attrs, n, cfg, d);
            if chk.dropped { return None; }
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
            let attrs: Vec<_> = attrs.iter().filter(|a| !is_directive_attr(&a.name)).collect();
            let location = elem_loc(n);
            if tag_name == "template" {
                if let Some(sd) = attrs.iter().find(|a| a.name == "lwc:slot-data").copied() {
                    return scoped_slot_fragment(sd, &attrs, dir.is_some() || lp.is_some() || lwc.is_some(), template_content, child_nodes, n, cfg, d, location);
                }
                // TS validateTemplate reports 1145 before the 1153 "no directive" warning.
                if validate::template(&attrs, dir.is_some() || lp.is_some() || lwc.is_some(), &location.location, d) { return None; }
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
                match (is_on, parse_binding(&a.value)) {
                    (true, Some(handler)) => { listeners.push(EventListener { name: a.name[2..].to_string(), handler, location: attr_loc(n, &a.name) }); false }
                    _ => true,
                }
            }).collect();
            let attributes = attrs.iter().map(|a| lower_attr(a, n)).collect();
            let directives: Vec<ElementDirective> = key.into_iter().map(ElementDirective::Key).chain(lwc_ref.map(ElementDirective::Ref)).collect();
            let depth = d.iter_depth; if lp.is_some() { d.iter_depth += 1; }
            let is_comp = is_custom_element_tag(tag_name);
            let prev_anc = std::mem::replace(&mut d.slot_ancestor, if is_comp { None } else { Some(tag_name.clone()) });
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
                if attrs.iter().any(|a| a.name == "lwc:dom") { d.push(messages::lwc(1086, &[], location.location.clone())); }
                else if attrs.iter().any(|a| a.name == "lwc:inner-html") { d.push(messages::lwc(1142, &[&format!("<{tag_name}>")], location.location.clone())); }
            }
            let node = if tag_name == "slot" {
                // `name` stays in attributes too; slot_name is the TS `slotName`.
                let slot_name = attrs.iter().find(|a| a.name == "name").map(|a| a.value.clone()).unwrap_or_default();
                // TS parseSlot: scoped (`lwc:slot-bind`) slots are light-DOM only and may not be duplicated
                // or mixed with standard slots of the same name.
                validate::slot(&slot_name, attrs.iter().copied().find(|a| a.name == "lwc:slot-bind"), &location.location, d);
                ChildNode::Slot(Slot { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, directives: directives.clone(), listeners, children, location: location.clone(), slot_name, ..Default::default() })
            } else if is_custom_element_tag(tag_name) && attrs.iter().any(|a| a.name == "lwc:external") {
                // TS: `isCustomElementTag && parsedAttr.get(External)` → ExternalComponent; attrs stay
                // attributes (no prop split) and the directive itself is consumed.
                let attributes: Vec<Attribute> = attributes.into_iter().filter(|a: &Attribute| a.name != "lwc:external").collect();
                validate::ssr_inner_outer_html(&attributes, tag_name, cfg, d);
                ChildNode::ExternalComponent(ExternalComponent { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, directives: directives.clone(), listeners, children, location: location.clone(), ..Default::default() })
            } else if is_custom_element_tag(tag_name) {
                { let (attributes, properties) = split_props(attributes); ChildNode::Component(Component { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, properties, directives: directives.clone(), listeners, children, location: location.clone(), ..Default::default() }) }
            } else {
                validate::ssr_inner_outer_html(&attributes, tag_name, cfg, d);
                ChildNode::Element(Element { name: tag_name.clone(), namespace: namespace_uri.clone(), attributes, directives: directives.clone(), listeners, children, location: location.clone(), ..Default::default() })
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
    let slot_name = attrs.iter().find(|a| a.name == "slot").map(|a| lower_attr(a, n).value)
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
    if source.trim().is_empty() {
        return TemplateParseResult { root: None, warnings };
    }
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
        let mut e = messages::lwc(1072, &[], SourceLocation::default());
        e.location = None;
        warnings.push(e);
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
        if element_check::root_errors(attrs, &tmpl, &location, &mut cx) {
            return TemplateParseResult { root: None, warnings: cx.diags };
        }
    }
    // TS parseRoot → applyLwcRenderModeDirective: counted once a valid lwc:render-mode is applied.
    if matches!(&tb.data, NodeData::Element { attrs, .. } if attrs.iter().any(|a| a.name == "lwc:render-mode" && (a.value == "light" || a.value == "shadow"))) {
        cx.count(lwc_shared::CompilerMetric::LwcRenderModeDirective);
    }
    let children = lower_children(&kids, config, &mut cx);
    TemplateParseResult { root: Some(Root { location, directives: vec![], children }), warnings: cx.diags }
}

/// Directives consumed from a nested `<template>` (anything else → INVALID_TEMPLATE_ATTRIBUTE).
fn is_template_directive_attr(n: &str) -> bool {
    matches!(n, "if:true" | "if:false" | "for:each" | "for:item" | "for:index" | "lwc:if" | "lwc:elseif" | "lwc:else" | "lwc:slot-data" | "lwc:preserve-comments" | "lwc:render-mode")
        || n.starts_with("iterator:")
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

/// Error diagnostic; `None` = uncoded (TS plain `Error`). `Some(0)` is rejected by `CompilerDiagnostic::new`.
fn diag_opt(code: Option<u32>, message: String, l: SourceLocation) -> CompilerDiagnostic {
    CompilerDiagnostic::new(code, message, DiagnosticLevel::Error, None, Some(l))
}

const ROOT_DIRECTIVES: &[&str] = &["lwc:render-mode", "lwc:preserve-comments"];
/// `ElementDirectiveName` + lwc:if/elseif/else (parsed before applyLwcDirectives in TS).
const LWC_ELEMENT_DIRECTIVES: &[&str] = &[
    "lwc:dom", "lwc:dynamic", "lwc:is", "lwc:external", "lwc:inner-html", "lwc:ref", "lwc:slot-bind",
    "lwc:slot-data", "lwc:spread", "lwc:on", "lwc:if", "lwc:elseif", "lwc:else",
];
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
