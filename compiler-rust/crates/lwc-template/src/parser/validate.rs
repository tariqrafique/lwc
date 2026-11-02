//! Parser diagnostics (TS `ParserCtx.throw*` / `warn*` call sites), split out of lowering.
//!
//! Every function here only pushes diagnostics onto `Ctx`. Where lowering depends on the
//! outcome, the function returns it (documented per function) and lowering branches on that.

use super::*;

use super::messages::lwc;
use lwc_shared::api_version::{is_api_feature_enabled, ApiFeature};

/// Text node: 1083/1196/1060/1074 for `{...}` segments that are not simple bindings.
/// Returns `true` when the text node must be dropped (TS `withErrorRecovery` around
/// parseTextNode: any error-level diagnostic discards the whole node).
pub(super) fn text_expressions(segments: &[(String, bool)], l: &SourceLocation, cfg: &NormalizedConfig, d: &mut Ctx) -> bool {
    let before = d.diags.len();
    let o = expr_opts(cfg, false);
    // TS parseTextNode parses the raw (entity-undecoded) source text, not parse5's decoded value.
    let raw = raw_source(l, d);
    let raw_segments = clean_text(&raw, &raw).map(|r| split_mustache(&r));
    let segments = match &raw_segments { Some(r) if r.iter().any(|(_, e)| *e) => r.as_slice(), _ => segments };
    if o.complex && is_api_feature_enabled(ApiFeature::EnableComplexTemplateExpressions, cfg.api_version) {
        // TS parseTextComplex: the acorn source (and the message's `{source}`) is the whole cleaned text.
        let full: String = segments.iter().map(|(s, _)| s.as_str()).collect();
        let mut at = 0;
        for (seg, is_expr) in segments {
            let n = seg.chars().count();
            if *is_expr && !is_binding(seg) {
                if let Some(e) = expr_check::check_complex(&full, at, at + n, &o) { d.push(e.into_diagnostic(l.clone())); }
            }
            at += n;
        }
    } else {
        for (seg, is_expr) in segments {
            if *is_expr && !is_binding(seg) && expr_check::is_expression(seg) {
                if let Some(e) = expr_check::check_opts(seg, &o) { d.push(e.into_diagnostic(l.clone())); }
            }
        }
    }
    d.diags[before..].iter().any(|x| x.level == DiagnosticLevel::Error)
}

/// Every attribute value that is not a simple binding goes through the expression checker.
pub(super) fn attribute_expressions(attrs: &[parse5::Attribute], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx) {
    for a in attrs {
        let ra = raw_attr(n, a, d);
        let l = ra.location.clone();
        let v = &a.value;
        // TS normalizeAttributeValue: `isEscapedAttribute(rawAttrVal)` / `isQuotedAttribute(rawAttrVal)`.
        let raw_escaped = ra.raw_value.as_deref().is_none_or(|r| !(r.contains('{') && r.contains('}')));
        let is_expr = !raw_escaped && expr_check::is_expression(v);
        if is_expr && ra.quoted && !cfg.experimental_complex_expressions {
            let raw = &ra.raw;
            d.push(lwc(1034, &[raw, &raw.replace('"', ""), &raw.replacen("\"{", "\"\\{", 1)], l));
            continue;
        }
        let potential = !raw_escaped && !is_expr && is_potential_expression(v);
        // `<input value={x}/>`: parse5 keeps the `/` in the unquoted value; TS strips it.
        let sliced = potential && !ra.quoted && v.starts_with('{') && v.ends_with("}/");
        let v = if sliced { &v[..v.len() - 1] } else { v.as_str() };
        let escaped_expr = potential && v.starts_with("\\{");
        let is_expr = is_expr || (sliced && expr_check::is_expression(v));
        if escaped_expr || (is_expr && is_binding(v)) { continue; }
        let o = expr_opts(cfg, !ra.quoted);
        if ra.quoted && o.complex && is_api_feature_enabled(ApiFeature::EnableComplexTemplateExpressions, cfg.api_version) && v.starts_with('{') {
            let err = expr_check::check_complex(v, 0, v.chars().count(), &o).or_else(|| {
                // TS validateComplexExpression's second acorn pass over the raw template source.
                let rv = ra.raw_value.as_deref().unwrap_or_default();
                let rv = &rv[1..rv.len() - 1];
                (rv != v).then(|| expr_check::check_raw_source(v, rv)).flatten()
            });
            if let Some(e) = err { d.push(e.into_diagnostic(l)); }
        } else if is_expr {
            if let Some(e) = expr_check::check_opts(v, &o) { d.push(e.into_diagnostic(l)); }
        }
    }
}

/// A simple Identifier / non-computed MemberExpression binding (TS validateExpression accepts these at
/// any API version). `true` / `false` / `null` parse as acorn `Literal`s, not identifiers, so they go
/// through validateExpression (1060 / 1210 / 1208).
fn is_binding(v: &str) -> bool {
    let head = v.strip_prefix('{').map(|s| s.trim_start().split(|c: char| c == '.' || c == '}' || c.is_whitespace()).next().unwrap_or(""));
    parse_binding(v).is_some() && !matches!(head, Some("true" | "false" | "null"))
}

/// TS `isPotentialExpression`: `/^.?{.+}.*$/` (`.` excludes line terminators).
fn is_potential_expression(v: &str) -> bool {
    let cs: Vec<char> = v.chars().collect();
    !cs.iter().any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        && (0..2).any(|k| cs.get(k) == Some(&'{') && cs.get(k + 2..).is_some_and(|r| r.contains(&'}')))
}

fn expr_opts(cfg: &NormalizedConfig, unquoted: bool) -> expr_check::Opts {
    expr_check::Opts { complex: cfg.experimental_complex_expressions, computed_ok: cfg.experimental_computed_member_expression, api_version: cfg.api_version, unquoted }
}

/// Non-template element checks, in TS order: 1176 (lwc:slot-data off <template>),
/// element validation (1127/1051/1123/1187/1057), then 1171 (lwc:slot-bind off <slot>).
pub(super) fn non_template_element(tag: &str, _ns: &str, attrs: &[parse5::Attribute], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx) {
    if let Some(sd) = attrs.iter().find(|a| a.name == "lwc:slot-data") {
        d.push(lwc(1176, &[], attr_loc(n, &sd.name)));
    }
    element(tag, attrs, n, d, cfg.experimental_dynamic_directive);
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
/// TS applyKey: kept iff a for:each / for:of parent exists (through templates, up to the nearest element).
pub(super) fn key_in_iteration(tag: &str, has_key: bool, has_loop: bool, _in_loop: bool, n: &NodeRef, d: &mut Ctx) -> bool {
    if tag != "template" && has_key && !has_loop && d.for_of.is_none() && d.for_each.is_none() {
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
    else if !scoped && d.iter_depth > 0 {
        // TS parseSlot: a (non-scoped) slot inside an iterator.
        d.push(lwc(1138, &[&label], l.clone()));
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
        // TS getLwcErrorFromParse5Error: ErrorOn → 1058; the older-API set → 1058 from API 59
        // (TREAT_ALL_PARSE5_ERRORS_AS_ERRORS), else the 1148 warning; unknown codes → 1148.
        let error = PARSE5_ERROR_ON.contains(&code) || (PARSE5_WARN_OLDER.contains(&code) && is_api_feature_enabled(ApiFeature::TreatAllParse5ErrorsAsErrors, cfg.api_version));
        out.push(lwc(if error { 1058 } else { 1148 }, &[code], loc(&e.location)));
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

/// Element-level validation: UNKNOWN_HTML_TAG (1123), lwc:dynamic deprecation (1187). Location = element
/// location, like TS `throwOnNode(…, element)`. 1184 / 1057 are emitted by element_check via
/// `unknown_lwc_tag` / `attr_name_warnings` (creation stage / applyAttributes order).
fn element(tag: &str, attrs: &[parse5::Attribute], n: &NodeRef, d: &mut Ctx, dynamic_ok: bool) {
    let l = elem_loc(n).location;
    // 1127 / 1051 are thrown by element_check (applyLwcDirectives / validateElement).
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
}

/// TS `EVENT_HANDLER_NAME_RE`: `/^on[a-z][a-z0-9_]*$/` (applyHandlers takes these before applyAttributes).
fn is_event_handler_name(name: &str) -> bool {
    let mut cs = name.strip_prefix("on").map(str::chars).into_iter().flatten();
    cs.next().is_some_and(|c| c.is_ascii_lowercase()) && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// TS normalizeLocation(undefined): `{ line: 0, column: 0, start: 0, length: 0 }`.
const NO_LOCATION: SourceLocation = SourceLocation { start_line: 0, start_column: 0, end_line: 0, end_column: 0, start: 0, end: 0 };

/// TS parseElementLocation: 1144 (location-less warning) for an element parse5 inserted while recovering.
/// TS names `ctx.findAncestor(isBaseElement, () => false)?.name ?? 'template'`: only the last node of the enclosing
/// element scope is tested, i.e. the parent's `element ?? directive`. That is a base element exactly when the parse5
/// parent is a non-`<template>` element (its own For/If directives sit *above* it); a `<template>` parent yields a
/// Root / If / For / scoped-slot node, and children of `<template>` content have the content fragment as parse5
/// parent. A parent whose parseElement threw never parses children, so it never reaches here.
pub(super) fn html_recovery(n: &NodeRef, d: &mut Ctx) {
    let b = n.borrow();
    if b.source_code_location_ref().is_some() { return; }
    let Some(tag) = b.tag_name() else { return };
    let parent = b.parent().and_then(|p| p.borrow().tag_name().filter(|t| *t != "template").map(str::to_string));
    d.push(lwc(1144, &[tag, parent.as_deref().unwrap_or("template")], NO_LOCATION));
}

/// TS applyAttributes 1057 (INVALID_HTML_ATTRIBUTE) for one attribute name. Names that TS applyHandlers /
/// directive parsers consume first (event handlers, `key`, lwc/if/for/iterator prefixes) never warn.
/// Returns the warning; the caller pushes it (callable with a shared `Ctx` borrow, as in element_check).
pub(super) fn attr_name_warnings(tag: &str, attr_name: &str, loc: &SourceLocation, _d: &Ctx) -> Option<CompilerDiagnostic> {
    let directive = attr_name == "key" || ["lwc", "if", "for", "iterator"].iter().any(|p| attr_name.starts_with(p));
    (!directive && !is_event_handler_name(attr_name) && !is_valid_html_attribute(tag, attr_name))
        .then(|| lwc(1057, &[attr_name, tag], loc.clone()))
}

/// TS parseLwcElementAsBuiltIn: 1184 (UNSUPPORTED_LWC_TAG_NAME) for an unrecognized `lwc:*` tag, at creation.
/// Returns the warning; the caller pushes it.
pub(super) fn unknown_lwc_tag(tag: &str, loc: &SourceLocation, _d: &Ctx) -> Option<CompilerDiagnostic> {
    (tag.starts_with("lwc:") && tag != "lwc:component").then(|| lwc(1184, &[tag], loc.clone()))
}
