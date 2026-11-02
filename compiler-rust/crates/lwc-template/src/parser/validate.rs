//! Parser diagnostics (TS `ParserCtx.throw*` / `warn*` call sites), split out of lowering.
//!
//! Every function here only pushes diagnostics onto `Ctx`. Where lowering depends on the
//! outcome, the function returns it (documented per function) and lowering branches on that.

use super::*;

// Not declared in mod.rs yet (Template Rust owns it); proposed: `mod messages;` there, drop this.
#[path = "messages.rs"]
pub(super) mod messages;
use messages::lwc;

/// Text node: 1083/1196/1060/1074 for `{...}` segments that are not simple bindings.
/// Returns `true` when the text node must be dropped (TS `withErrorRecovery` around
/// parseTextNode: any error-level diagnostic discards the whole node).
pub(super) fn text_expressions(segments: &[(String, bool)], l: &SourceLocation, cfg: &NormalizedConfig, d: &mut Ctx) -> bool {
    let before = d.diags.len();
    let o = expr_opts(cfg, false);
    if o.complex && cfg.api_version >= expr_check::MIN_CTE_API_VERSION {
        // TS parseTextComplex: the acorn source (and the message's `{source}`) is the whole cleaned text.
        let full: String = segments.iter().map(|(s, _)| s.as_str()).collect();
        let mut at = 0;
        for (seg, is_expr) in segments {
            let n = seg.chars().count();
            if *is_expr && parse_binding(seg).is_none() {
                if let Some(e) = expr_check::check_complex(&full, at, at + n, &o) { d.push(e.into_diagnostic(l.clone())); }
            }
            at += n;
        }
    } else {
        for (seg, is_expr) in segments {
            if *is_expr && parse_binding(seg).is_none() && expr_check::is_expression(seg) {
                if let Some(e) = expr_check::check_opts(seg, &o) { d.push(e.into_diagnostic(l.clone())); }
            }
        }
    }
    d.diags[before..].iter().any(|x| x.level == DiagnosticLevel::Error)
}

/// Every attribute value that is not a simple binding goes through the expression checker.
pub(super) fn attribute_expressions(attrs: &[parse5::Attribute], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx) {
    // parse5-rust stretches the source-last attribute's end over trailing whitespace and `/` before `>`
    // (its emit_current_tag_token re-runs leave_attr_value; JS parse5 does not), so that attribute's
    // length cannot tell `a="{x}">` from `a={x} />`.
    let last_start = n.borrow().source_code_location().and_then(|l| l.attrs.values().map(|a| a.start_offset).max());
    for a in attrs {
        let l = attr_loc(n, &a.name);
        let v = &a.value;
        // TS normalizeAttributeValue: only `"`-quoted values count as quoted. The raw attribute text is not
        // available here, so quotedness comes from the source length (entity-free, double-quoted values):
        // `name="v"` is n+v+3, `name=v` is n+v+1. Ambiguous lengths (source-last attribute, entities) are
        // read as quoted only under CTE (where quoting is required) and never raise 1034.
        let raw_len = (l.end as usize).saturating_sub(l.start as usize);
        let exact = a.name.chars().count() + v.chars().count() + 3;
        let is_last = last_start.is_some_and(|s| s as u32 == l.start as u32);
        let surely_quoted = !is_last && raw_len == exact;
        let quoted = surely_quoted || (raw_len >= exact && cfg.experimental_complex_expressions);
        if surely_quoted && !cfg.experimental_complex_expressions && expr_check::is_expression(v) {
            let raw = format!("{}=\"{v}\"", a.name);
            let (unquoted, escaped) = (raw.replace('"', ""), raw.replacen("\"{", "\"\\{", 1));
            d.push(lwc(1034, &[&raw, &unquoted, &escaped], l));
            continue;
        }
        if parse_binding(v).is_some() { continue; }
        let o = expr_opts(cfg, !quoted);
        if quoted && o.complex && cfg.api_version >= expr_check::MIN_CTE_API_VERSION && v.starts_with('{') {
            if let Some(e) = expr_check::check_complex(v, 0, v.chars().count(), &o) { d.push(e.into_diagnostic(l)); }
        } else if expr_check::is_expression(v) {
            if let Some(e) = expr_check::check_opts(v, &o) { d.push(e.into_diagnostic(l)); }
        }
    }
}

fn expr_opts(cfg: &NormalizedConfig, unquoted: bool) -> expr_check::Opts {
    expr_check::Opts { complex: cfg.experimental_complex_expressions, computed_ok: cfg.experimental_computed_member_expression, api_version: cfg.api_version, unquoted }
}

/// Non-template element checks, in TS order: 1176 (lwc:slot-data off <template>),
/// element validation (1127/1051/1123/1187/1057), then 1171 (lwc:slot-bind off <slot>).
pub(super) fn non_template_element(tag: &str, ns: &str, attrs: &[parse5::Attribute], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx) {
    if let Some(sd) = attrs.iter().find(|a| a.name == "lwc:slot-data") {
        d.push(lwc(1176, &[], attr_loc(n, &sd.name)));
    }
    element(tag, ns, attrs, n, d, cfg.experimental_dynamic_directive);
    // TS applyLwcSlotBindDirective runs after the lwc:dynamic/lwc:is directives.
    if tag != "slot" && attrs.iter().any(|a| a.name == "lwc:slot-bind") {
        d.push(lwc(1171, &[&format!("<{tag}>")], elem_loc(n).location));
    }
}

/// 1182: TS `parseIf` keeps the first if:* attribute and warns once per extra one.
pub(super) fn multiple_if(tag: &str, attrs: &[parse5::Attribute], n: &NodeRef, d: &mut Ctx) {
    for _ in attrs.iter().filter(|a| a.name.starts_with("if:")).skip(1) {
        d.push(lwc(1182, &[tag], elem_loc(n).location));
    }
}

/// 1041 static id inside an iteration, then 1201 slot attribute under a non-component ancestor.
pub(super) fn element_attributes(tag: &str, attrs: &[parse5::Attribute], has_loop: bool, n: &NodeRef, d: &mut Ctx) {
    if tag == "template" { return; }
    if d.iter_depth > 0 || has_loop {
        if let Some(a) = attrs.iter().find(|a| a.name == "id" && !(a.value.starts_with('{') && a.value.ends_with('}'))) {
            d.push(lwc(1041, &[], attr_loc(n, &a.name)));
        }
    }
    if attrs.iter().any(|a| a.name == "slot") {
        if let Some(anc) = d.slot_ancestor.clone() {
            d.push(lwc(1201, &[&format!("<{tag}>"), &format!("<{anc}>")], attr_loc(n, "slot")));
        }
    }
}

/// 1149: `key` outside an iteration. Returns `true` when the key is kept (lowering drops it otherwise).
pub(super) fn key_in_iteration(tag: &str, has_key: bool, has_loop: bool, in_loop: bool, n: &NodeRef, d: &mut Ctx) -> bool {
    if tag != "template" && has_key && !has_loop && !in_loop {
        d.push(lwc(1149, &[tag], attr_loc(n, "key")));
        return false;
    }
    has_key
}

/// Nested `<template>`: 1145 (unsupported attributes) before 1153 (no directive), as in TS
/// validateTemplate. Returns `true` when the template must be dropped (1153).
pub(super) fn template(attrs: &[&parse5::Attribute], has_directive: bool, l: &SourceLocation, d: &mut Ctx) -> bool {
    let bad: Vec<&str> = attrs.iter().map(|a| a.name.as_str()).filter(|n| !is_template_directive_attr(n)).collect();
    if !bad.is_empty() {
        d.push(lwc(1145, &[&bad.join(", ")], l.clone()));
    }
    if !has_directive {
        d.push(lwc(1153, &[], l.clone()));
        return true;
    }
    false
}

/// TS parseSlot: 1169 (scoped slot outside light DOM), duplicate/mixed slots 1172/1173/1137,
/// then 1170 (non-expression lwc:slot-bind). Records the slot in the seen sets.
pub(super) fn slot(slot_name: &str, bind: Option<&parse5::Attribute>, l: &SourceLocation, d: &mut Ctx) {
    let scoped = bind.is_some();
    if scoped && !d.light {
        d.push(lwc(1169, &[], l.clone()));
    }
    let label = if slot_name.is_empty() { "default".to_string() } else { format!("name=\"{slot_name}\"") };
    if !d.seen_slots.insert(slot_name.to_string()) {
        if d.seen_scoped_slots.contains(slot_name) {
            d.push(lwc(if scoped { 1172 } else { 1173 }, &[&label], l.clone()));
        } else if scoped {
            d.push(lwc(1173, &[&label], l.clone()));
        } else {
            d.push(lwc(1137, &[&label], l.clone()));
        }
    }
    if scoped { d.seen_scoped_slots.insert(slot_name.to_string()); }
    if bind.is_some_and(|b| parse_binding(&b.value).is_none() && !(b.value.starts_with('{') && b.value.ends_with('}'))) {
        d.push(lwc(1170, &["<slot>"], l.clone()));
    }
}

/// TS parseScopedSlotFragment, same order: 1179, 1178, 1174, identifier 1059/1211.
/// Returns the slot-data identifier when valid; `None` means the fragment is dropped.
pub(super) fn scoped_slot_data(sd: &parse5::Attribute, other_directive: bool, l: &SourceLocation, sd_loc: &SourceLocation, d: &mut Ctx) -> Option<Expression> {
    if other_directive {
        d.push(lwc(1179, &[], l.clone()));
        return None;
    }
    if !d.slot_data_parent_ok {
        d.push(lwc(1178, &[], l.clone()));
        return None;
    }
    if sd.value.starts_with('{') && sd.value.ends_with('}') {
        d.push(lwc(1174, &[], sd_loc.clone()));
        return None;
    }
    if !is_ident(&sd.value) {
        d.push(lwc(1059, &[&sd.value], sd_loc.clone()));
        return None;
    }
    if expr_check::is_reserved(&sd.value) {
        d.push(lwc(1211, &[&sd.value], sd_loc.clone()));
        return None;
    }
    Some(Expression::Identifier(Identifier { name: sd.value.clone() }))
}

/// 1177 on direct Text/Comment children of a template directive within a scoped slot fragment.
pub(super) fn non_element_slot_content(children: &[ChildNode], d: &mut Ctx) {
    for c in children {
        let l = match c { ChildNode::Text(t) => Some(&t.location), ChildNode::Comment(c) => Some(&c.location), _ => None };
        if let Some(l) = l {
            d.push(lwc(1177, &[], l.clone()));
        }
    }
}

/// 1165: an `lwc:elseif` / `lwc:else` sibling with no open chain. The node is dropped.
pub(super) fn orphan_else(directive: &str, l: SourceLocation, d: &mut Ctx) {
    d.push(lwc(1165, &[directive], l));
}

/// parse5 errors → 1058 (parse5Errors.ts: errorCodesToErrorOn always; the "older API" set from API 59 on).
pub(super) fn parse5_errors(errors: &[parse5::ParserError], cfg: &NormalizedConfig, out: &mut Vec<CompilerDiagnostic>) {
    for e in errors {
        let code = e.code.as_str();
        // TODO: INVALID_HTML_SYNTAX_WARNING for the older-API set below v59 (warning work, deferred).
        if PARSE5_ERROR_ON.contains(&code) || (PARSE5_WARN_OLDER.contains(&code) && cfg.api_version >= 59) {
            out.push(lwc(1058, &[code], loc(&e.location)));
        }
    }
}

/// 1080: root `<template>` attributes other than the root directives.
pub(super) fn root_attributes(attrs: &[parse5::Attribute], l: &SourceLocation, out: &mut Vec<CompilerDiagnostic>) {
    let bad: Vec<_> = attrs.iter().map(|a| a.name.as_str()).filter(|n| !ROOT_DIRECTIVES.contains(n)).collect();
    if !bad.is_empty() {
        out.push(lwc(1080, &[&bad.join(",")], l.clone()));
    }
}

/// SSR-only: TS `@lwc/ssr-compiler` element.ts throws a plain `Error` (no LWC error code) for
/// `inner-h-t-m-l` / `outer-h-t-m-l` *attributes* on elements and external components. Components
/// are unaffected (those become `innerHTML` props). TS throws a plain (uncoded) `Error`, so `code: None`.
pub(super) fn ssr_inner_outer_html(attrs: &[Attribute], tag: &str, cfg: &NormalizedConfig, d: &mut Ctx) {
    if !cfg.ssr { return; }
    if let Some(a) = attrs.iter().find(|a| a.name == "inner-h-t-m-l" || a.name == "outer-h-t-m-l") {
        d.push(diag_opt(None, format!("Cannot set attribute \"{}\" on <{tag}>.", a.name), a.location.clone()));
    }
}

/// Element-level validation: UNKNOWN_LWC_DIRECTIVE (1127), FORBIDDEN_TAG_ON_TEMPLATE (1051),
/// UNKNOWN_HTML_TAG (1123), lwc:dynamic deprecation (1187), INVALID_HTML_ATTRIBUTE (1057).
/// Location = element location, like TS `throwOnNode(…, element)`.
fn element(tag: &str, ns: &str, attrs: &[parse5::Attribute], n: &NodeRef, d: &mut Ctx, dynamic_ok: bool) {
    let l = elem_loc(n).location;
    if let Some(a) = attrs.iter().find(|a| a.name.starts_with("lwc:") && !LWC_ELEMENT_DIRECTIVES.contains(&a.name.as_str())) {
        d.push(lwc(1127, &[&a.name, &format!("<{tag}>")], l.clone()));
    }
    if ns == parse5::common::html::NS_HTML && DISALLOWED_HTML_TAGS.contains(&tag) {
        d.push(lwc(1051, &[tag], l.clone()));
    }
    use tables::*;
    let known = tag.contains('-') || tag == "lwc:component" || KNOWN_HTML_AND_SVG_ELEMENTS.contains(&tag)
        || SUPPORTED_SVG_TAGS.contains(&tag) || DASHED_TAGNAME_ELEMENT_SET.contains(&tag);
    if !known {
        d.push(lwc(1123, &[tag], l.clone()));
    }
    // lwc:dynamic (only the valid shape warns; error shapes are not modeled yet).
    if let Some(a) = attrs.iter().find(|a| a.name == "lwc:dynamic") {
        if dynamic_ok && tag.contains('-') && parse_binding(&a.value).is_some() {
            d.push(lwc(1187, &[], l.clone()));
            d.count(lwc_shared::CompilerMetric::LwcDynamicDirective);
        }
    }
    for a in attrs {
        let handler = a.name.starts_with("on") && parse_binding(&a.value).is_some();
        if !handler && !is_valid_html_attribute(tag, &a.name) {
            d.push(lwc(1057, &[&a.name, tag], attr_loc(n, &a.name)));
        }
    }
}
