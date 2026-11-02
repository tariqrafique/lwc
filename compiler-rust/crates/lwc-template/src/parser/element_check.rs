//! TS `parseElement` error checks that abort the element (`ctx.throw*` inside `withErrorRecovery`):
//! the first error is reported and the element, with its subtree, is dropped.
//!
//! Order follows parser/index.ts: parseAttributes (normalizeAttributeValue) → parseElementDirectives
//! (lwc:if/elseif/else, for:each, iterator, if:*, lwc:slot-data) → validateTemplate | (parseBaseElement
//! → parseSlot / lwc:component) → applyHandlers → applyKey → applyLwcDirectives → applyAttributes →
//! validateElement → validateAttributes / validateProperties.
//! Expression errors stay with `validate::attribute_expressions` (its owner); this module only
//! decides whether any error-level diagnostic aborts the element.

use super::*;
use super::messages::lwc;
use parse5::common::html::{NS_HTML, NS_MATHML, NS_SVG};

/// TS `ast.*Literal` / expression value after normalizeAttributeValue.
#[derive(Clone, PartialEq)]
pub(super) enum Av { Expr(String), Str(String), Bool }

pub(super) struct Pa { pub(super) name: String, pub(super) v: Av, pub(super) loc: SourceLocation }

impl Pa { fn expr(&self) -> bool { matches!(self.v, Av::Expr(_)) } }

#[derive(Clone, Copy, PartialEq)]
enum Kind { Slot, Component, External, LwcComponent, Element }

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr", "param", "keygen", "menuitem"];
const ELEMENT_DIRECTIVES: &[&str] = &["lwc:dom", "lwc:dynamic", "lwc:is", "lwc:external", "lwc:inner-html", "lwc:ref", "lwc:slot-bind", "lwc:slot-data", "lwc:spread", "lwc:on", "key"];
const DISALLOWED_MATHML_TAGS: &[&str] = &["script", "link", "base", "object"];
const VALID_IF_MODIFIERS: &[&str] = &["true", "false", "strict-true"];

/// TS `isBooleanAttribute(name, tag)` (@lwc/shared).
fn is_boolean_attribute(attr: &str, tag: &str) -> bool {
    let tags: &[&str] = match attr {
        "autofocus" => &["button", "input", "keygen", "select", "textarea"],
        "autoplay" => &["audio", "video"],
        "checked" => &["command", "input"],
        "disabled" => &["button", "command", "fieldset", "input", "keygen", "optgroup", "select", "textarea"],
        "formnovalidate" => &["button"],
        "hidden" | "inert" => return true,
        "loop" => &["audio", "bgsound", "marquee", "video"],
        "multiple" => &["input", "select"],
        "muted" => &["audio", "video"],
        "novalidate" => &["form"],
        "open" => &["details"],
        "readonly" => &["input", "textarea"],
        "required" => &["input", "select", "textarea"],
        "reversed" => &["ol"],
        "selected" => &["option"],
        _ => return false,
    };
    tags.contains(&tag)
}

/// TS `isPotentialExpression`: `/^.?{.+}.*$/` (`.` excludes line terminators).
fn is_potential_expression(s: &str) -> bool {
    if s.chars().any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')) { return false; }
    let cs: Vec<char> = s.chars().collect();
    let close_after = |p: usize| cs.iter().skip(p + 2).any(|&c| c == '}');
    (cs.first() == Some(&'{') && close_after(0)) || (cs.get(1) == Some(&'{') && close_after(1))
}

/// TS `parseIdentifier`: 1059 not an identifier, 1211 reserved word.
fn identifier(s: &str, l: &SourceLocation) -> Result<(), CompilerDiagnostic> {
    if !is_ident(s) { return Err(lwc(1059, &[s], l.clone())); }
    if expr_check::is_reserved(s) { return Err(lwc(1211, &[s], l.clone())); }
    Ok(())
}

/// TS getTemplateAttribute + normalizeAttributeValue errors other than 1034 / expression errors
/// (those are `validate::attribute_expressions`): 1055, 1037/1036, 1035.
pub(super) fn normalize(tag: &str, a: &parse5::Attribute, n: &NodeRef, d: &Ctx) -> Result<Pa, CompilerDiagnostic> {
    let ra = raw_attr(n, a, d);
    let l = ra.location.clone();
    if !ra.raw.starts_with(&a.name) { return Err(lwc(1055, &[&ra.raw, tag], l)); }
    let is_bool_form = !ra.raw.contains('=');
    let mut value = a.value.clone();
    if is_boolean_attribute(&a.name, tag) && (value == "true" || value == "false") {
        let code = if value == "true" { 1037 } else { 1036 };
        return Err(lwc(code, &[tag, &a.name, &value], l));
    }
    let rav = ra.raw_value.clone().unwrap_or_default();
    let escaped = rav.is_empty() || !(rav.contains('{') && rav.contains('}'));
    let mut esc = escaped;
    if !escaped && !expr_check::is_expression(&value) && is_potential_expression(&value) {
        if value.starts_with('{') && value.ends_with("}/") && !ra.quoted {
            value.pop();
        } else if let Some(rest) = value.strip_prefix("\\{") {
            value = format!("{{{rest}");
            esc = true;
        } else {
            let mut e = match ra.raw.find('=') {
                Some(i) => {
                    let after = &ra.raw[i + 1..];
                    format!("{}=\"\\{}", &ra.raw[..i], after.strip_prefix('"').unwrap_or(after))
                }
                None => ra.raw.clone(),
            };
            if !e.ends_with('"') { e.push('"'); }
            return Err(lwc(1035, &[&ra.raw, &e], l));
        }
    }
    let v = if !esc && expr_check::is_expression(&value) { Av::Expr(value) } else if is_bool_form { Av::Bool } else { Av::Str(value) };
    Ok(Pa { name: a.name.clone(), v, loc: l })
}

fn pick(pas: &mut Vec<Pa>, f: impl Fn(&str) -> bool) -> Option<Pa> {
    let i = pas.iter().position(|p| f(&p.name))?;
    Some(pas.remove(i))
}

/// Outcome of the pre-pass for one element.
pub(super) struct ElementCheck {
    /// The element (and subtree) is dropped: an error was thrown before TS parseBaseElement created it.
    pub dropped: bool,
    /// TS pushes the element into its parent when it is created; a later throw (withErrorRecovery)
    /// leaves it in the tree without children. `bare`: thrown before applyAttributes finished, so it
    /// carries no attributes/listeners/directives either.
    pub childless: bool,
    pub bare: bool,
    stage: Stage,
    /// Own `iterator:<name>` name / `for:each` index (Some(None) = for:each without index).
    pub for_of: Option<String>,
    pub for_each: Option<Option<String>>,
    /// A valid `lwc:dynamic` was applied (TS warns 1187 there). On success `validate::element` reports it;
    /// when a later check throws, the warning is emitted here, ahead of the error.
    dynamic_ok: bool,
}

/// Runs the JS-ordered element checks. Pushes at most one error. `template` elements get
/// validateTemplate; others the base-element checks.
#[derive(Clone, Copy, PartialEq)]
enum Stage { NotCreated, Created, Applied }

pub(super) fn element_errors(tag: &str, ns: &str, attrs: &[parse5::Attribute], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx) -> ElementCheck {
    let mut out = ElementCheck { dropped: false, childless: false, bare: false, stage: Stage::NotCreated, for_of: None, for_each: None, dynamic_ok: false };
    let before = d.diags.len();
    let r = check(tag, ns, attrs, n, cfg, d, &mut out);
    match r {
        Err(Some(e)) => {
            if out.dynamic_ok {
                d.push(lwc(1187, &[], elem_loc(n).location));
                d.count(lwc_shared::CompilerMetric::LwcDynamicDirective);
            }
            d.push(e);
            match out.stage {
                Stage::NotCreated => out.dropped = true,
                Stage::Created => { out.childless = true; out.bare = true; }
                Stage::Applied => out.childless = true,
            }
        }
        Err(None) => out.dropped = true,
        Ok(()) => out.dropped = d.diags[before..].iter().any(|x| x.level == DiagnosticLevel::Error),
    }
    out
}

type R = Result<(), Option<CompilerDiagnostic>>;

fn check(tag: &str, ns: &str, attrs: &[parse5::Attribute], n: &NodeRef, cfg: &NormalizedConfig, d: &mut Ctx, out: &mut ElementCheck) -> R {
    let el = elem_loc(n);
    let l = el.location.clone();
    let tagb = format!("<{tag}>");
    let e = |code: u32, args: &[&str], at: &SourceLocation| Err(Some(lwc(code, args, at.clone())));

    // parseAttributes: per-attribute normalization, then the expression checker.
    let mut pas = Vec::new();
    for a in attrs { pas.push(normalize(tag, a, n, d).map_err(Some)?); }
    let before = d.diags.len();
    validate::attribute_expressions(attrs, n, cfg, d);
    if d.diags[before..].iter().any(|x| x.level == DiagnosticLevel::Error) { return Err(None); }

    // parseElementDirectives.
    let has = |pas: &Vec<Pa>, name: &str| pas.iter().any(|p| p.name == name);
    let lwc_if = pick(&mut pas, |x| x == "lwc:if");
    if let Some(p) = &lwc_if { if !p.expr() { return e(1159, &[], &p.loc); } }
    let elseif = pick(&mut pas, |x| x == "lwc:elseif");
    if let Some(p) = &elseif {
        if lwc_if.is_some() { return e(1162, &["lwc:elseif"], &l); }
        if !p.expr() { return e(1160, &[], &p.loc); }
        if !d.chain_open { return e(1165, &["lwc:elseif"], &l); }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:else") {
        if lwc_if.is_some() { return e(1162, &["lwc:else"], &l); }
        if elseif.is_some() { return e(1163, &["lwc:else"], &l); }
        if !d.chain_open { return e(1165, &["lwc:else"], &l); }
        if p.v != Av::Bool { return e(1161, &[], &l); }
    }
    let lwc_cond = lwc_if.is_some() || elseif.is_some() || has(&pas, "lwc:else");
    let (fe, fi, fx) = (pick(&mut pas, |x| x == "for:each"), pick(&mut pas, |x| x == "for:item"), pick(&mut pas, |x| x == "for:index"));
    match (&fe, &fi) {
        (Some(fe), Some(fi)) => {
            if !fe.expr() { return e(1045, &[], &fe.loc); }
            let Av::Str(item) = &fi.v else { return e(1047, &[], &fi.loc); };
            identifier(item, &fi.loc).map_err(Some)?;
            let mut index = None;
            if let Some(fx) = &fx {
                let Av::Str(ix) = &fx.v else { return e(1046, &[], &fx.loc); };
                identifier(ix, &fx.loc).map_err(Some)?;
                index = Some(ix.clone());
            }
            out.for_each = Some(index);
        }
        (None, None) => {}
        _ => return e(1044, &[], &l),
    }
    if let Some(it) = pick(&mut pas, |x| x.starts_with("iterator:")) {
        if out.for_each.is_some() { return e(1136, &[&it.name], &l); }
        if !it.expr() { return e(1039, &[&it.name], &it.loc); }
        let name = it.name.split(':').nth(1).unwrap_or("");
        identifier(name, &it.loc).map_err(Some)?;
        out.for_of = Some(name.to_string());
    }
    let mut ifs = Vec::new();
    while let Some(p) = pick(&mut pas, |x| x.starts_with("if:")) { ifs.push(p); }
    if let Some(p) = ifs.first() {
        if lwc_cond { return e(1166, &[&p.name], &l); }
        if !p.expr() { return e(1054, &[], &p.loc); }
        let m = &p.name["if:".len()..];
        if !VALID_IF_MODIFIERS.contains(&m) { return e(1084, &[m], &p.loc); }
    }
    let sd = pas.iter().find(|p| p.name == "lwc:slot-data").map(|p| p.loc.clone());
    if let Some(sl) = &sd { if tag != "template" { return e(1176, &[], sl); } }
    let has_dir = lwc_cond || out.for_each.is_some() || out.for_of.is_some() || !ifs.is_empty();

    if tag == "template" {
        // validateTemplate (scoped-slot fragments keep their own ordered checks in lowering).
        if attrs.is_empty() { return e(1077, &[], &l); }
        if sd.is_some() { return Ok(()); }
        let t = "<template>";
        if has(&pas, "lwc:external") { return e(1180, &[t], &l); }
        if has(&pas, "lwc:inner-html") { return e(1141, &[t], &l); }
        if has(&pas, "lwc:ref") { return e(1156, &[t], &l); }
        if has(&pas, "lwc:is") { return e(1186, &[t], &l); }
        if has(&pas, "lwc:on") { return if cfg.enable_lwc_on { e(1203, &[t], &l) } else { e(1206, &[t], &l) }; }
        return Ok(());
    }

    // parseBaseElement.
    let kind = if tag == "slot" { Kind::Slot }
        else if is_custom_element_tag(tag) { if has(&pas, "lwc:external") { Kind::External } else { Kind::Component } }
        else if tag == "lwc:component" { Kind::LwcComponent }
        else { Kind::Element };
    if kind == Kind::LwcComponent {
        if !cfg.enable_dynamic_components { return e(1188, &[], &l); }
        if !has(&pas, "lwc:is") { return e(1183, &[], &l); }
    }
    if kind == Kind::Slot {
        if has(&pas, "lwc:slot-bind") && !d.light { return e(1169, &[], &l); }
        if has_dir { return e(1082, &["if:true, lwc:if, lwc:elseif, lwc:else, for:each, for:of, lwc:slot-data"], &l); }
        if d.light {
            let bad: Vec<&str> = pas.iter().map(|p| p.name.as_str()).filter(|x| !matches!(*x, "key" | "lwc:slot-bind" | "name" | "slot")).collect();
            if let Some(h) = bad.iter().find(|x| is_event_handler_name(x)) { return e(1139, &[h], &l); }
            if !bad.is_empty() { return e(1134, &[&bad.join(","), "key, lwc:slot-bind, name, slot"], &l); }
        }
        if let Some(p) = pas.iter().find(|p| p.name == "name") { if p.expr() { return e(1076, &[], &p.loc); } }
    }

    // parseBaseElement: the element now exists in its parent.
    out.stage = Stage::Created;
    // applyHandlers.
    let mut handlers = 0;
    while let Some(h) = pick(&mut pas, |x| x.starts_with("on")) {
        if !h.expr() { return e(1043, &[], &h.loc); }
        if !is_event_handler_name(&h.name) { return e(1056, &[&h.name], &h.loc); }
        handlers += 1;
    }

    // applyKey (nearest iteration through templates; base elements reset the scope for children).
    let for_of = out.for_of.clone().or_else(|| d.for_of.clone());
    let for_each = out.for_each.clone().or_else(|| d.for_each.clone());
    if let Some(k) = pick(&mut pas, |x| x == "key") {
        let Av::Expr(src) = &k.v else { return e(1064, &[], &k.loc); };
        let b = parse_binding(src);
        if let Some(it) = &for_of {
            if let Some(Expression::Member(m)) = &b {
                if matches!((&*m.object, &*m.property), (Expression::Identifier(o), Expression::Identifier(p)) if &o.name == it && p.name == "index") {
                    return e(1066, &[tag], &k.loc);
                }
            }
        } else if let Some(Some(ix)) = &for_each {
            if matches!(&b, Some(Expression::Identifier(i)) if &i.name == ix) { return e(1065, &[tag, ix], &k.loc); }
        }
    } else if for_of.is_some() || for_each.is_some() {
        return e(1071, &[tag], &l);
    }

    // applyLwcDirectives.
    if let Some(p) = pas.iter().find(|p| p.name.starts_with("lwc:")) {
        if !ELEMENT_DIRECTIVES.contains(&p.name.as_str()) { return e(1127, &[&p.name, &tagb], &l); }
    }
    for root_only in ["lwc:render-mode", "lwc:preserve-comments"] {
        if has(&pas, root_only) { return e(1127, &[root_only, &tagb], &l); }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:external") {
        if kind != Kind::External { return e(1180, &[&tagb], &l); }
        if p.v != Av::Bool { return e(1181, &[&tagb], &l); }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:dynamic") {
        if !cfg.experimental_dynamic_directive { return e(1128, &[], &l); }
        if kind != Kind::Component { return e(1129, &[&tagb], &l); }
        if !p.expr() { return e(1130, &[&tagb], &l); }
        out.dynamic_ok = true;
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:is") {
        if kind != Kind::LwcComponent { return e(1186, &[&tagb], &l); }
        match &p.v { Av::Expr(_) => {} Av::Str(s) => return e(1185, &[s], &l), Av::Bool => return e(1185, &["true"], &l) }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:dom") {
        if d.light { return e(1135, &[&tagb], &l); }
        if kind == Kind::Component { return e(1087, &[&tagb], &l); }
        if kind == Kind::Slot { return e(1088, &[], &l); }
        if p.v != Av::Str("manual".into()) { return e(1085, &["\"manual\""], &l); }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:inner-html") {
        if matches!(kind, Kind::Component | Kind::LwcComponent) { return e(1140, &[&tagb], &l); }
        if kind == Kind::Slot { return e(1141, &[&tagb], &l); }
        if p.v == Av::Bool { return e(1143, &[&tagb], &l); }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:ref") {
        if kind == Kind::Slot { return e(1156, &[&tagb], &l); }
        if d.iter_depth > 0 || out.for_each.is_some() || out.for_of.is_some() { return e(1158, &[&tagb], &l); }
        if !matches!(&p.v, Av::Str(s) if !s.is_empty()) { return e(1157, &[&tagb], &l); }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:spread") { if !p.expr() { return e(1155, &[&tagb], &l); } }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:on") {
        if !cfg.enable_lwc_on { return e(1206, &[], &l); }
        if !p.expr() { return e(1204, &[&tagb], &l); }
        if handlers > 0 { return e(1205, &[&tagb], &l); }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:slot-bind") {
        if kind != Kind::Slot { return e(1171, &[&tagb], &l); }
        if !p.expr() { return e(1170, &[&tagb], &l); }
    }
    pick(&mut pas, |x| x == "lwc:slot-data");

    // applyAttributes.
    for p in &pas {
        let last = p.name.chars().last().unwrap_or(' ');
        if !(last.is_ascii_lowercase() || last.is_ascii_digit()) { return e(1125, &[&p.name, tag], &p.loc); }
        // `/^-*[a-z]|^[_$]/`
        let start_ok = p.name.trim_start_matches('-').starts_with(|c: char| c.is_ascii_lowercase()) || p.name.starts_with(['_', '$']);
        if !start_ok { return e(1124, &[&p.name, tag], &p.loc); }
        if p.name == "id" {
            if let Av::Str(v) = &p.v {
                if v.chars().any(char::is_whitespace) { return e(1040, &[v], &p.loc); }
                if !d.seen_ids.insert(v.clone()) { return e(1042, &[v], &p.loc); }
            }
        }
    }

    // validateElement (handlers, key, lwc directives and attributes are applied).
    out.stage = Stage::Applied;
    if ns == NS_HTML && !VOID.contains(&tag) && el.end_tag.is_none() && tag == tag.to_lowercase() {
        return e(1078, &[tag], &l);
    }
    if tag == "style" && ns == NS_HTML { return e(1122, &[], &l); }
    if ns == NS_HTML && DISALLOWED_HTML_TAGS.contains(&tag) { return e(1051, &[tag], &l); }
    if ns == NS_SVG && !tables::SUPPORTED_SVG_TAGS.contains(&tag) { return e(1049, &[tag], &l); }
    if ns == NS_MATHML && DISALLOWED_MATHML_TAGS.contains(&tag) { return e(1050, &[tag], &l); }

    // validateAttributes / validateProperties.
    for p in &pas {
        if p.name == "is" { return e(1063, &[], &l); }
        if p.name == "tabindex" && !p.expr() && !matches!(&p.v, Av::Str(s) if s == "0" || s == "-1") { return e(1061, &[], &l); }
        if tag == "iframe" && p.name == "srcdoc" { return e(1048, &[], &l); }
    }
    Ok(())
}

/// TS `EVENT_HANDLER_NAME_RE`: `/^on[a-z][a-z0-9_]*$/`.
fn is_event_handler_name(s: &str) -> bool {
    s.strip_prefix("on").is_some_and(|r| {
        let mut cs = r.chars();
        cs.next().is_some_and(|c| c.is_ascii_lowercase()) && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    })
}

/// TS parseRoot after the tag check: parseAttributes (1055/1037/1036/1035), applyRootLwcDirectives
/// (1133 render-mode, 1146 preserve-comments), validateRoot (1080 unknown attributes, 1078 no end tag).
/// Returns `true` when an error was reported (no root).
pub(super) fn root_errors(attrs: &[parse5::Attribute], n: &NodeRef, el: &ElementSourceLocation, d: &mut Ctx) -> bool {
    let l = el.location.clone();
    let mut pas = Vec::new();
    for a in attrs {
        match normalize("template", a, n, d) { Ok(p) => pas.push(p), Err(e) => { d.push(e); return true; } }
    }
    if pas.iter().any(|p| p.name.starts_with("lwc:")) {
        if let Some(p) = pick(&mut pas, |x| x == "lwc:render-mode") {
            if !matches!(&p.v, Av::Str(s) if s == "shadow" || s == "light") { d.push(lwc(1133, &[], l)); return true; }
        }
        if let Some(p) = pick(&mut pas, |x| x == "lwc:preserve-comments") {
            if p.v != Av::Bool { d.push(lwc(1146, &[], l)); return true; }
        }
    }
    let before = d.diags.len();
    validate::root_attributes(attrs, &l, &mut d.diags);
    if d.diags.len() > before { return true; }
    if el.end_tag.is_none() { d.push(lwc(1078, &["template"], l)); return true; }
    false
}
