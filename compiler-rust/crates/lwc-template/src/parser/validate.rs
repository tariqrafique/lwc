//! Parser diagnostics (TS `ParserCtx.throw*` / `warn*` call sites), split out of lowering.
//!
//! Every function here only pushes diagnostics onto `Ctx`. Where lowering depends on the
//! outcome, the function returns it (documented per function) and lowering branches on that.

use super::*;

use lwc_shared::errors::ParserDiagnostics;
use lwc_shared::api_version::{is_api_feature_enabled, ApiFeature};

/// Text node: 1083/1196/1060/1074 for `{...}` segments that are not simple bindings.
/// Returns `true` when the text node must be dropped (TS `withErrorRecovery` around
/// parseTextNode: any error-level diagnostic discards the whole node).
pub(super) fn text_expressions(segments: &[(String, bool)], l: &SourceLocation, cfg: &NormalizedConfig, d: &mut Ctx) -> Located<bool> {
    let o = expr_opts(cfg, false);
    // TS parseTextNode parses the raw (entity-undecoded) source text, not parse5's decoded value.
    let raw = raw_source(l, d);
    let raw_segments = clean_text(&raw, &raw).map(|r| split_mustache(&r));
    let segments = match &raw_segments { Some(r) if r.iter().any(|(_, e)| *e) => r.as_slice(), _ => segments };
    // TS parseChildren wraps the whole parseTextNode in one withErrorRecovery: the first expression that
    // throws (an Error-level diagnostic) ends the text node, so later expressions in it are not checked.
    let report = |d: &mut Ctx, e: CompilerDiagnostic| { let threw = e.level == DiagnosticLevel::Error; d.push(e); threw };
    if o.complex && is_api_feature_enabled(ApiFeature::EnableComplexTemplateExpressions, cfg.api_version) {
        // TS parseTextComplex: the acorn source (and the message's `{source}`) is the whole cleaned text.
        let full: String = segments.iter().map(|(s, _)| s.as_str()).collect();
        let mut at = 0;
        for (seg, is_expr) in segments {
            let n = seg.chars().count();
            if *is_expr && !is_binding(seg) {
                if let Some(e) = expression_complex::validate::validate_text_cached(&full, at, at + n, d.alloc, seg, &mut d.exprs)? {
                    if report(d, e.into_diagnostic(l.clone())) { return Ok(true); }
                }
            }
            at += n;
        }
    } else {
        for (seg, is_expr) in segments {
            if *is_expr && !is_binding(seg) && expr_check::is_expression(seg) {
                if let Some(e) = expr_check::check_opts(seg, &o) {
                    if report(d, e.into_diagnostic(l.clone())) { return Ok(true); }
                }
            }
        }
    }
    Ok(false)
}

/// The attribute's value, unless a simple binding or escaped, goes through the expression checker (TS
/// getTemplateAttribute after normalizeAttributeValue); its diagnostics go to `out` (the caller decides
/// which one TS throws).
pub(super) fn attribute_expressions(nz: &element_check::Normalized, cfg: &NormalizedConfig, out: &mut Vec<CompilerDiagnostic>, d: &mut Ctx) -> Located<()> {
    let (ra, na) = (&nz.ra, &nz.na);
    let v = na.value.as_str();
    if na.escaped_expression { return Ok(()); }
    let o = expr_opts(cfg, !na.quoted_expression);
    // TS isPotentialComplexExpression is tested before isExpression, so a binding still gets the raw-source pass.
    let complex = na.quoted_expression && o.complex && is_api_feature_enabled(ApiFeature::EnableComplexTemplateExpressions, cfg.api_version) && v.starts_with('{');
    if complex {
        // The second acorn pass (validate.ts:296) reads the raw template source of the value.
        let rv = ra.raw_value.as_deref().unwrap_or_default();
        let rv = &rv[1..rv.len() - 1];
        if let Some(e) = expression_complex::validate::validate_attribute_cached(v, rv, d.alloc, &mut d.exprs)? { out.push(e.into_diagnostic(ra.location.clone())); }
    } else if expr_check::is_expression(v) && !is_binding(v) {
        if let Some(e) = expr_check::check_opts(v, &o) { out.push(e.into_diagnostic(ra.location.clone())); }
    }
    Ok(())
}

/// A simple Identifier / non-computed MemberExpression binding (TS validateExpression accepts these at
/// any API version). `true` / `false` / `null` parse as acorn `Literal`s, not identifiers, so they go
/// through validateExpression (1060 / 1210 / 1208).
fn is_binding(v: &str) -> bool {
    let head = v.strip_prefix('{').map(|s| s.trim_start_matches(js_ws).split(|c: char| c == '.' || c == '}' || js_ws(c)).next().unwrap_or(""));
    parse_binding(v).is_some() && !matches!(head, Some("true" | "false" | "null"))
}

fn expr_opts(cfg: &NormalizedConfig, unquoted: bool) -> expr_check::Opts {
    expr_check::Opts { complex: cfg.experimental_complex_expressions, computed_ok: cfg.experimental_computed_member_expression, api_version: cfg.api_version, unquoted }
}

/// Warnings on a created (non-template) element, in TS parseElement order: parseIf 1182, applyKey 1149,
/// applyLwcDirectives 1187 / 1171, applyAttributes 1041, validateSlotAttribute 1201, validateElement 1123.
/// (1176 is thrown by element_check before creation; kept for the unchecked path.)
pub(super) fn non_template_element(tag: &str, _ns: &str, attrs: &[parse5::Attribute], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx) -> Located<()> {
    if let Some(sd) = attrs.iter().find(|a| a.name == "lwc:slot-data") {
        d.push(ParserDiagnostics::SCOPED_SLOT_DATA_ON_TEMPLATE_ONLY.generate(&[], attr_loc(n, &sd.name, d)?));
    }
    if_warnings(tag, attrs, n, d);
    // TS applyKey: any expression key (simple or not) without a for:each / for:of parent warns.
    if attrs.iter().any(|a| a.name == "key" && expr_check::is_expression(&a.value)) && d.for_of_parent().is_none() && d.for_each_parent().is_none() {
        d.push(ParserDiagnostics::KEY_SHOULD_BE_IN_ITERATION.generate(&[tag], attr_loc(n, "key", d)?));
    }
    dynamic_directive(tag, attrs, n, d, cfg.experimental_dynamic_directive);
    // TS applyLwcSlotBindDirective runs after the lwc:dynamic/lwc:is directives.
    if tag != "slot" && attrs.iter().any(|a| a.name == "lwc:slot-bind") {
        d.push(ParserDiagnostics::INVALID_LWC_SLOT_BIND_NON_SLOT_ELEMENT.generate(&[&format!("<{tag}>")], elem_span(n, d)));
    }
    // TS isInIteration: any For ancestor, including the element's own for:each / iterator.
    if d.in_iteration() {
        // TS applyAttributes: string-literal ids only (a valueless `id` is the boolean `true`).
        for a in attrs.iter().filter(|a| a.name == "id" && !(a.value.starts_with('{') && a.value.ends_with('}'))) {
            let ra = raw_attr(n, a, d)?;
            if ra.raw_value.is_some() {
                d.push(ParserDiagnostics::INVALID_STATIC_ID_IN_ITERATION.generate(&[], ra.location));
                break;
            }
        }
    }
    if attrs.iter().any(|a| a.name == "slot") {
        if let Some(anc) = d.slot_ancestor.clone() {
            d.push(ParserDiagnostics::IGNORED_SLOT_ATTRIBUTE_IN_CHILD.generate(&[&format!("<{tag}>"), &format!("<{anc}>")], attr_loc(n, "slot", d)?));
        }
    }
    if let Some(w) = unknown_html_tag(tag, &elem_span(n, d)) { d.push(w); }
    Ok(())
}

/// 1182 on `<template>` (non-template elements get it from `non_template_element`, in TS order).
pub(super) fn multiple_if(tag: &str, attrs: &[parse5::Attribute], n: &NodeRef, d: &mut Ctx) {
    if tag == "template" { if_warnings(tag, attrs, n, d); }
}

/// 1182: TS `parseIf` keeps the first if:* attribute and warns once per extra one.
fn if_warnings(tag: &str, attrs: &[parse5::Attribute], n: &NodeRef, d: &mut Ctx) {
    for _ in attrs.iter().filter(|a| a.name.starts_with("if:")).skip(1) {
        d.push(ParserDiagnostics::SINGLE_IF_DIRECTIVE_ALLOWED.generate(&[tag], elem_span(n, d)));
    }
}

/// TS applyKey: `true` when the key is kept (lowering drops it otherwise): kept iff a for:each / for:of
/// parent exists (through templates, up to the nearest element). The 1149 warning for a dropped key is
/// reported by `non_template_element`.
pub(super) fn key_in_iteration(tag: &str, has_key: bool, has_loop: bool, _in_loop: bool, _n: &NodeRef, d: &mut Ctx) -> bool {
    has_key && (tag == "template" || has_loop || d.for_of_parent().is_some() || d.for_each_parent().is_some())
}

/// Nested `<template>`: 1145 (unsupported attributes) before 1153 (no directive), as in TS
/// validateTemplate. Returns `true` when the template must be dropped (1153).
pub(super) fn template(attrs: &[&element_check::CheckedAttr], has_directive: bool, l: &SourceLocation, d: &mut Ctx) -> bool {
    // `attrs`: whatever the directive parsers did not pick (`ParsedAttribute.getAttributes()`).
    if !attrs.is_empty() {
        let bad: Vec<&str> = attrs.iter().map(|c| c.attr().name.as_str()).collect();
        d.push(ParserDiagnostics::INVALID_TEMPLATE_ATTRIBUTE.generate(&[&bad.join(", ")], l.clone()));
    }
    if !has_directive {
        d.push(ParserDiagnostics::INVALID_TEMPLATE_WARNING.generate(&[], l.clone()));
        return true;
    }
    false
}

/// TS parseScopedSlotFragment, same order: 1179, 1178, 1174, identifier 1059/1211.
/// Returns the slot-data identifier when valid; `None` means the fragment is dropped, and the one
/// diagnostic TS throws is in `out`.
pub(super) fn scoped_slot_data(sd: &parse5::Attribute, other_directive: bool, l: &SourceLocation, sd_loc: &SourceLocation, d: &Ctx, out: &mut Vec<CompilerDiagnostic>) -> Option<Expression<'static>> {
    if other_directive {
        out.push(ParserDiagnostics::SCOPED_SLOTDATA_CANNOT_BE_COMBINED_WITH_OTHER_DIRECTIVE.generate(&[], l.clone()));
        return None;
    }
    if !d.slot_data_parent_ok {
        out.push(ParserDiagnostics::INVALID_PARENT_OF_LWC_SLOT_DATA.generate(&[], l.clone()));
        return None;
    }
    // TS: a boolean `lwc:slot-data` (no `=`; its source span is just the name) is the literal `true`.
    let boolean_form = sd.value.is_empty() && sd_loc.end.saturating_sub(sd_loc.start) as usize == sd.name.len();
    if boolean_form || (sd.value.starts_with('{') && sd.value.ends_with('}')) {
        out.push(ParserDiagnostics::SLOT_DATA_VALUE_SHOULD_BE_STRING.generate(&[], sd_loc.clone()));
        return None;
    }
    if !is_ident(&sd.value) {
        out.push(ParserDiagnostics::INVALID_IDENTIFIER.generate(&[&sd.value], sd_loc.clone()));
        return None;
    }
    if expr_check::is_reserved(&sd.value) {
        out.push(ParserDiagnostics::RESERVED_KEYWORD_AS_IDENTIFIER.generate(&[&sd.value], sd_loc.clone()));
        return None;
    }
    Some(Expression::Identifier(Identifier { name: sd.value.clone() }))
}

/// 1177 on direct Text children, and Comment children when comments are preserved, of a template
/// directive within a scoped slot fragment, into `out` in child order (TS validateChildren throws the first).
pub(super) fn non_element_slot_content(children: &[ChildNode], preserve_comments: bool, out: &mut Vec<CompilerDiagnostic>) {
    for c in children {
        let l = match c { ChildNode::Text(t) => Some(&t.location), ChildNode::Comment(c) if preserve_comments => Some(&c.location), _ => None };
        if let Some(l) = l {
            out.push(ParserDiagnostics::NON_ELEMENT_SCOPED_SLOT_CONTENT.generate(&[], l.clone()));
        }
    }
}


/// parse5 errors → 1058 (parse5Errors.ts: errorCodesToErrorOn always; the "older API" set from API 59 on).
pub(super) fn parse5_errors(errors: &[parse5::ParserError], cfg: &NormalizedConfig, out: &mut Vec<CompilerDiagnostic>) {
    for e in errors {
        let code = e.code.as_str();
        // TS getLwcErrorFromParse5Error: ErrorOn → 1058; the older-API set → 1058 from API 59
        // (TREAT_ALL_PARSE5_ERRORS_AS_ERRORS), else the 1148 warning; unknown codes → 1148.
        let error = PARSE5_ERROR_ON.contains(&code) || (PARSE5_WARN_OLDER.contains(&code) && is_api_feature_enabled(ApiFeature::TreatAllParse5ErrorsAsErrors, cfg.api_version));
        out.push((if error { ParserDiagnostics::INVALID_HTML_SYNTAX } else { ParserDiagnostics::INVALID_HTML_SYNTAX_WARNING }).generate(&[code], loc(&e.location)));
    }
}

/// 1080: root `<template>` attributes other than the root directives.
pub(super) fn root_attributes(attrs: &[parse5::Attribute], l: &SourceLocation, out: &mut Vec<CompilerDiagnostic>) {
    let bad: Vec<_> = attrs.iter().map(|a| a.name.as_str()).filter(|n| !ROOT_DIRECTIVES.contains(n)).collect();
    if !bad.is_empty() {
        out.push(ParserDiagnostics::ROOT_TEMPLATE_HAS_UNKNOWN_ATTRIBUTES.generate(&[&bad.join(",")], l.clone()));
    }
}

/// TS validateElement 1123 (UNKNOWN_HTML_TAG_IN_TEMPLATE) at the element location. Returns the warning;
/// the caller pushes it (element_check can push it ahead of a validateAttributes throw).
/// 1184 / 1057 come from `unknown_lwc_tag` / `attr_name_warnings`.
pub(super) fn unknown_html_tag(tag: &str, l: &SourceLocation) -> Option<CompilerDiagnostic> {
    use tables::*;
    let known = tag.contains('-') || tag == "lwc:component" || KNOWN_HTML_AND_SVG_ELEMENTS.contains(&tag)
        || SUPPORTED_SVG_TAGS.contains(&tag) || DASHED_TAGNAME_ELEMENT_SET.contains(&tag);
    (!known).then(|| ParserDiagnostics::UNKNOWN_HTML_TAG_IN_TEMPLATE.generate(&[tag], l.clone()))
}

/// lwc:dynamic deprecation 1187 (only the valid shape warns; error shapes are element_check throws).
fn dynamic_directive(tag: &str, attrs: &[parse5::Attribute], n: &NodeRef, d: &mut Ctx, dynamic_ok: bool) {
    let l = elem_span(n, d);
    if let Some(a) = attrs.iter().find(|a| a.name == "lwc:dynamic") {
        if dynamic_ok && tag.contains('-') && parse_binding(&a.value).is_some() {
            d.push(ParserDiagnostics::DEPRECATED_LWC_DYNAMIC_ATTRIBUTE.generate(&[], l.clone()));
            d.count(lwc_shared::CompilerMetric::LwcDynamicDirective);
        }
    }
}

/// TS `EVENT_HANDLER_NAME_RE`: `/^on[a-z][a-z0-9_]*$/` (applyHandlers takes these before applyAttributes).
fn is_event_handler_name(name: &str) -> bool {
    let mut cs = name.strip_prefix("on").map(str::chars).into_iter().flatten();
    cs.next().is_some_and(|c| c.is_ascii_lowercase()) && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

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
    d.push(ParserDiagnostics::INVALID_HTML_RECOVERY.generate_normalized(&[tag, parent.as_deref().unwrap_or("template")], None));
}

/// TS applyAttributes 1057 (INVALID_HTML_ATTRIBUTE) for one attribute name. Names that TS applyHandlers /
/// directive parsers consume first (event handlers, `key`, lwc/if/for/iterator prefixes) never warn.
/// Returns the warning; the caller pushes it (callable with a shared `Ctx` borrow, as in element_check).
pub(super) fn attr_name_warnings(tag: &str, attr_name: &str, loc: &SourceLocation, _d: &Ctx) -> Option<CompilerDiagnostic> {
    let directive = attr_name == "key" || ["lwc", "if", "for", "iterator"].iter().any(|p| attr_name.starts_with(p));
    (!directive && !is_event_handler_name(attr_name) && !is_valid_html_attribute(tag, attr_name))
        .then(|| ParserDiagnostics::INVALID_HTML_ATTRIBUTE.generate(&[attr_name, tag], loc.clone()))
}

/// TS parseLwcElementAsBuiltIn: 1184 (UNSUPPORTED_LWC_TAG_NAME) for an unrecognized `lwc:*` tag, at creation.
/// Returns the warning; the caller pushes it.
pub(super) fn unknown_lwc_tag(tag: &str, loc: &SourceLocation, _d: &Ctx) -> Option<CompilerDiagnostic> {
    (tag.starts_with("lwc:") && tag != "lwc:component").then(|| ParserDiagnostics::UNSUPPORTED_LWC_TAG_NAME.generate(&[tag], loc.clone()))
}
