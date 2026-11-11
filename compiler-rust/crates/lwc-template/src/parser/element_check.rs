//! TS `parseElement` error checks that abort the element (`ctx.throw*` inside `withErrorRecovery`):
//! the first error is reported and the element, with its subtree, is dropped.
//!
//! Order follows parser/index.ts: parseAttributes (normalizeAttributeValue) → parseElementDirectives
//! (lwc:if/elseif/else, for:each, iterator, if:*, lwc:slot-data) → validateTemplate | (parseBaseElement
//! → parseSlot / lwc:component) → applyHandlers → applyKey → applyLwcDirectives → applyAttributes →
//! validateElement → validateAttributes / validateProperties.
//! Expression diagnostics come from `validate::attribute_expressions` (its owner); this module picks
//! the first error-level one as the throw.

use super::*;
use lwc_shared::LwcErrorInfo;
use lwc_shared::errors::ParserDiagnostics;
use parse5::common::html::{NS_HTML, NS_MATHML, NS_SVG};

/// TS parser/attribute.ts. A child of this module so its normalizer is reachable from the pre-pass only.
#[path = "attribute.rs"]
pub(super) mod attribute;
use attribute::{NormalizedAttr, normalize_attribute_value};

/// TS `ast.*Literal` / expression value after normalizeAttributeValue.
#[derive(Clone, PartialEq)]
pub(super) enum Av {
    Expr(String),
    Str(String),
    Bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Slot,
    Component,
    External,
    LwcComponent,
    Element,
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr", "param", "keygen", "menuitem",
];
const ELEMENT_DIRECTIVES: &[&str] = &[
    "lwc:dom",
    "lwc:dynamic",
    "lwc:is",
    "lwc:external",
    "lwc:inner-html",
    "lwc:ref",
    "lwc:slot-bind",
    "lwc:slot-data",
    "lwc:spread",
    "lwc:on",
    "key",
];
const DISALLOWED_MATHML_TAGS: &[&str] = &["script", "link", "base", "object"];
const VALID_IF_MODIFIERS: &[&str] = &["true", "false", "strict-true"];

/// TS `isBooleanAttribute(name, tag)` (@lwc/shared).
pub(crate) fn is_boolean_attribute(attr: &str, tag: &str) -> bool {
    let tags: &[&str] = match attr {
        "autofocus" => &["button", "input", "keygen", "select", "textarea"],
        "autoplay" => &["audio", "video"],
        "checked" => &["command", "input"],
        "disabled" => &[
            "button", "command", "fieldset", "input", "keygen", "optgroup", "select", "textarea",
        ],
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

/// TS `parseIdentifier`: 1059 not an identifier, 1211 reserved word.
fn identifier(s: &str, l: &SourceLocation) -> Result<(), CompilerDiagnostic> {
    if !is_ident(s) {
        return Err(ParserDiagnostics::INVALID_IDENTIFIER.generate(&[s], l.clone()));
    }
    if expr_check::is_reserved(s) {
        return Err(ParserDiagnostics::RESERVED_KEYWORD_AS_IDENTIFIER.generate(&[s], l.clone()));
    }
    Ok(())
}

/// One attribute after TS getTemplateAttribute's normalizeAttributeValue.
pub(super) struct Normalized {
    pub(super) ra: RawAttr,
    pub(super) na: NormalizedAttr,
}

impl Normalized {
    /// TS getTemplateAttribute's value: an expression unless escaped, `true` for a valueless attribute
    /// (no `=` in the raw source), else the string.
    fn v(&self) -> Av {
        let na = &self.na;
        if !na.escaped_expression && expr_check::is_expression(&na.value) {
            Av::Expr(na.value.clone())
        } else if !self.ra.raw.contains('=') {
            Av::Bool
        } else {
            Av::Str(na.value.clone())
        }
    }
}

/// TS getTemplateAttribute up to the value parse: the attribute's location (read first, TS
/// parseAttributes, so a missing one is an internal error only for attributes TS reached before any
/// throw), 1055 INVALID_ATTRIBUTE_CASE, then [`normalize_attribute_value`]. Private: the element and root
/// pre-passes are its only callers, once per attribute; lowering reads [`ElementCheck::attrs`].
fn normalize(
    tag: &str,
    a: &parse5::Attribute,
    n: &NodeRef,
    cfg: &NormalizedConfig,
    d: &Ctx,
) -> Located<Result<Normalized, CompilerDiagnostic>> {
    let ra = raw_attr(n, a, d)?;
    if !ra.raw.starts_with(&a.name) {
        return Ok(Err(
            ParserDiagnostics::INVALID_ATTRIBUTE_CASE.generate(&[&ra.raw, tag], ra.location)
        ));
    }
    Ok(normalize_attribute_value(
        cfg.experimental_complex_expressions,
        &ra.raw,
        tag,
        a,
        &ra.location,
    )
    .map(|na| Normalized { ra, na }))
}

fn pick<'n>(pas: &mut Vec<CheckedAttr<'n>>, f: impl Fn(&str) -> bool) -> Option<CheckedAttr<'n>> {
    let i = pas.iter().position(|p| f(p.attr().name.as_str()))?;
    Some(pas.remove(i))
}

/// One attribute of the checked element with TS getTemplateAttribute's normalized value and location,
/// paired in the pre-pass so lowering never looks a value up or normalizes again. Also the working
/// list for directive picks in [`check_parsed`] (one vector, not a parallel `Pa`).
/// Fields are private to this module (AB-L0058-FU): only the two construction sites below build it;
/// other parser modules read through [`Self::attr`] / [`Self::value`] / [`Self::loc`].
#[derive(Clone)]
pub(super) struct CheckedAttr<'n> {
    attr: &'n parse5::Attribute,
    value: Av,
    loc: SourceLocation,
}

impl<'n> CheckedAttr<'n> {
    pub(super) fn attr(&self) -> &'n parse5::Attribute {
        self.attr
    }
    pub(super) fn value(&self) -> &Av {
        &self.value
    }
    pub(super) fn loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn expr(&self) -> bool {
        matches!(self.value, Av::Expr(_))
    }
}

/// Outcome of the pre-pass for an element TS created (parseBaseElement ran, or a `<template>`).
pub(super) struct ElementCheck<'n> {
    /// A throw after TS parseBaseElement created the element: TS already pushed it into its parent, so it
    /// stays in the tree without children and lowering returns it with this diagnostic as the throw.
    pub thrown: Option<CompilerDiagnostic>,
    /// `thrown` before applyAttributes finished: the element carries no attributes/listeners/directives.
    pub bare: bool,
    stage: Stage,
    /// Own `iterator:<name>` name / `for:each` index (Some(None) = for:each without index).
    pub for_of: Option<String>,
    pub for_each: Option<Option<String>>,
    /// A valid `lwc:dynamic` was applied (TS warns 1187 there). On success `validate::element` reports it;
    /// when a later check throws, the warning is emitted here, ahead of the error.
    dynamic_ok: bool,
    /// An lwc:if / lwc:elseif / lwc:else block was parsed (and the sibling if-chain updated) before any throw.
    lwc_parsed: bool,
    /// Warnings TS emits before a later throw: 1184 at creation, 1057 per attribute in applyAttributes.
    created_warn: Option<CompilerDiagnostic>,
    attr_warns: Vec<CompilerDiagnostic>,
    /// Warnings before a later throw: 1182 per extra if:* (parseIf, before creation) and 1149 for a key
    /// outside an iteration (applyKey). On success lowering reports them (`validate::non_template_element`
    /// / `validate::multiple_if`).
    if_warns: Vec<CompilerDiagnostic>,
    key_warn: Option<CompilerDiagnostic>,
    /// Every checked attribute with its normalized value (a normalize throw drops the element, so an
    /// `ElementCheck` always has them all).
    pub attrs: Vec<CheckedAttr<'n>>,
}

/// A throw before TS parseBaseElement created the element: the element and its subtree are dropped.
pub(super) struct Dropped<'n> {
    pub diag: CompilerDiagnostic,
    /// Some: TS parseIfBlock / parseElseifBlock / parseElseBlock had already pushed the (empty) block into
    /// the parent and updated the sibling if-chain. Directives are parsed after every attribute was
    /// normalized, so these are all the element's attributes, for picking its lwc role.
    pub lwc_parsed: Option<Vec<CheckedAttr<'n>>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    NotCreated,
    Created,
    Applied,
}

/// Runs the JS-ordered element checks. `template` elements get validateTemplate; others the
/// base-element checks. Warnings TS emits before a throw are pushed here; the throw itself is returned
/// (`Err`, or `ElementCheck::thrown` once the element exists) for parseChildren's withErrorRecovery.
pub(super) fn element_errors<'n>(
    tag: &str,
    ns: &str,
    attrs: &'n [parse5::Attribute],
    n: &NodeRef,
    cfg: &NormalizedConfig,
    d: &mut Ctx,
) -> Located<Result<ElementCheck<'n>, Dropped<'n>>> {
    let mut out = ElementCheck {
        thrown: None,
        bare: false,
        stage: Stage::NotCreated,
        for_of: None,
        for_each: None,
        dynamic_ok: false,
        lwc_parsed: false,
        created_warn: None,
        attr_warns: Vec::new(),
        if_warns: Vec::new(),
        key_warn: None,
        attrs: Vec::new(),
    };
    let r = check(tag, ns, attrs, n, cfg, d, &mut out)?;
    if r.is_err() {
        for w in std::mem::take(&mut out.if_warns) {
            d.push(w);
        }
    }
    if let Some(w) = out.created_warn.take() {
        d.push(w);
    }
    match r {
        Err(e) => {
            if let Some(w) = out.key_warn.take() {
                d.push(w);
            }
            if out.dynamic_ok {
                d.push(
                    ParserDiagnostics::DEPRECATED_LWC_DYNAMIC_ATTRIBUTE
                        .generate(&[], elem_span(n, d)),
                );
                d.count(lwc_shared::CompilerMetric::LwcDynamicDirective);
            }
            for w in std::mem::take(&mut out.attr_warns) {
                d.push(w);
            }
            match out.stage {
                Stage::NotCreated => {
                    return Ok(Err(Dropped {
                        diag: e,
                        lwc_parsed: out.lwc_parsed.then_some(out.attrs),
                    }));
                }
                Stage::Created => {
                    out.thrown = Some(e);
                    out.bare = true;
                }
                Stage::Applied => out.thrown = Some(e),
            }
        }
        Ok(()) => {
            for w in std::mem::take(&mut out.attr_warns) {
                d.push(w);
            }
        }
    }
    Ok(Ok(out))
}

/// `Err`: the diagnostic TS throws (`ctx.throw*`); parseElement stops there.
type R = Result<(), CompilerDiagnostic>;

/// The parse5 location reads (TS parseAttributes) can fail with an internal error; everything after them
/// is [`check_parsed`].
fn check<'n>(
    tag: &str,
    ns: &str,
    attrs: &'n [parse5::Attribute],
    n: &NodeRef,
    cfg: &NormalizedConfig,
    d: &mut Ctx,
    out: &mut ElementCheck<'n>,
) -> Located<R> {
    // TS parseAttributes → getTemplateAttribute, per attribute: its location, normalizeAttributeValue, then
    // the expression parse. The first error is the throw; later attributes are never read.
    let mut checked = Vec::new();
    for a in attrs {
        let nz = match normalize(tag, a, n, cfg, d)? {
            Ok(nz) => nz,
            Err(e) => return Ok(Err(e)),
        };
        let mut found = Vec::new();
        validate::attribute_expressions(&nz, cfg, &mut found, d)?;
        checked.push(CheckedAttr {
            attr: a,
            value: nz.v(),
            loc: nz.ra.location.clone(),
        });
        for x in found {
            if x.level == DiagnosticLevel::Error {
                return Ok(Err(x));
            }
            d.push(x);
        }
    }
    out.attrs = checked.clone();
    Ok(check_parsed(tag, ns, attrs, checked, n, cfg, d, out))
}

#[allow(clippy::too_many_arguments)]
fn check_parsed(
    tag: &str,
    ns: &str,
    attrs: &[parse5::Attribute],
    mut pas: Vec<CheckedAttr<'_>>,
    n: &NodeRef,
    cfg: &NormalizedConfig,
    d: &mut Ctx,
    out: &mut ElementCheck<'_>,
) -> R {
    let el = elem_parse5_loc(n, d);
    let l = loc(&el.location);
    let tagb = format!("<{tag}>");
    let e = |info: &LwcErrorInfo, args: &[&str], at: &SourceLocation| {
        Err(info.generate(args, at.clone()))
    };

    // parseElementDirectives.
    let has = |pas: &Vec<CheckedAttr<'_>>, name: &str| pas.iter().any(|p| p.attr().name == name);
    let lwc_if = pick(&mut pas, |x| x == "lwc:if");
    if let Some(p) = &lwc_if {
        if !p.expr() {
            return e(
                &ParserDiagnostics::IF_BLOCK_DIRECTIVE_SHOULD_BE_EXPRESSION,
                &[],
                p.loc(),
            );
        }
        out.lwc_parsed = true;
        // TS parseIfBlock: endIfChain (if open), addNodeCurrentElementScope, beginIfChain.
        d.add_scope_node(ScopeNode::IfBlock);
        d.chain = Some(Chain::If);
    }
    let elseif = pick(&mut pas, |x| x == "lwc:elseif");
    if let Some(p) = &elseif {
        if lwc_if.is_some() {
            return e(
                &ParserDiagnostics::INVALID_IF_BLOCK_DIRECTIVE_WITH_CONDITIONAL,
                &["lwc:elseif"],
                &l,
            );
        }
        if !p.expr() {
            return e(
                &ParserDiagnostics::ELSEIF_BLOCK_DIRECTIVE_SHOULD_BE_EXPRESSION,
                &[],
                p.loc(),
            );
        }
        if !matches!(d.chain, Some(Chain::If | Chain::Elseif)) {
            return e(
                &ParserDiagnostics::LWC_IF_SCOPE_NOT_FOUND,
                &["lwc:elseif"],
                &l,
            );
        }
        out.lwc_parsed = true;
        d.add_scope_node(ScopeNode::ElseifBlock);
        d.chain = Some(Chain::Elseif);
    }
    let else_ = pick(&mut pas, |x| x == "lwc:else");
    if let Some(p) = &else_ {
        if lwc_if.is_some() {
            return e(
                &ParserDiagnostics::INVALID_IF_BLOCK_DIRECTIVE_WITH_CONDITIONAL,
                &["lwc:else"],
                &l,
            );
        }
        if elseif.is_some() {
            return e(
                &ParserDiagnostics::INVALID_ELSEIF_BLOCK_DIRECTIVE_WITH_CONDITIONAL,
                &["lwc:else"],
                &l,
            );
        }
        if !matches!(d.chain, Some(Chain::If | Chain::Elseif)) {
            return e(
                &ParserDiagnostics::LWC_IF_SCOPE_NOT_FOUND,
                &["lwc:else"],
                &l,
            );
        }
        if *p.value() != Av::Bool {
            return e(
                &ParserDiagnostics::ELSE_BLOCK_DIRECTIVE_CANNOT_HAVE_VALUE,
                &[],
                &l,
            );
        }
        out.lwc_parsed = true;
        d.add_scope_node(ScopeNode::ElseBlock);
        d.chain = Some(Chain::Else);
    }
    let lwc_cond = lwc_if.is_some() || elseif.is_some() || else_.is_some();
    let (fe, fi, fx) = (
        pick(&mut pas, |x| x == "for:each"),
        pick(&mut pas, |x| x == "for:item"),
        pick(&mut pas, |x| x == "for:index"),
    );
    match (&fe, &fi) {
        (Some(fe), Some(fi)) => {
            if !fe.expr() {
                return e(
                    &ParserDiagnostics::FOR_EACH_DIRECTIVE_SHOULD_BE_EXPRESSION,
                    &[],
                    fe.loc(),
                );
            }
            let Av::Str(item) = fi.value() else {
                return e(
                    &ParserDiagnostics::FOR_ITEM_DIRECTIVE_SHOULD_BE_STRING,
                    &[],
                    fi.loc(),
                );
            };
            identifier(item, fi.loc())?;
            let mut index = None;
            if let Some(fx) = &fx {
                let Av::Str(ix) = fx.value() else {
                    return e(
                        &ParserDiagnostics::FOR_INDEX_DIRECTIVE_SHOULD_BE_STRING,
                        &[],
                        fx.loc(),
                    );
                };
                identifier(ix, fx.loc())?;
                index = Some(ix.clone());
            }
            d.add_scope_node(ScopeNode::ForEach(index.clone()));
            out.for_each = Some(index);
        }
        (None, None) => {}
        _ => {
            return e(
                &ParserDiagnostics::FOR_EACH_AND_FOR_ITEM_DIRECTIVES_SHOULD_BE_TOGETHER,
                &[],
                &l,
            );
        }
    }
    if let Some(it) = pick(&mut pas, |x| x.starts_with("iterator:")) {
        if out.for_each.is_some() {
            return e(
                &ParserDiagnostics::INVALID_FOR_EACH_WITH_ITERATOR,
                &[&it.attr().name],
                &l,
            );
        }
        if !it.expr() {
            return e(
                &ParserDiagnostics::DIRECTIVE_SHOULD_BE_EXPRESSION,
                &[&it.attr().name],
                it.loc(),
            );
        }
        let name = it.attr().name.split(':').nth(1).unwrap_or("");
        identifier(name, it.loc())?;
        out.for_of = Some(name.to_string());
        d.add_scope_node(ScopeNode::ForOf(name.to_string()));
    }
    let mut ifs = Vec::new();
    while let Some(p) = pick(&mut pas, |x| x.starts_with("if:")) {
        ifs.push(p);
    }
    for _ in ifs.iter().skip(1) {
        out.if_warns
            .push(ParserDiagnostics::SINGLE_IF_DIRECTIVE_ALLOWED.generate(&[tag], l.clone()));
    }
    if let Some(p) = ifs.first() {
        if lwc_cond {
            return e(
                &ParserDiagnostics::LWC_IF_CANNOT_BE_USED_WITH_IF_DIRECTIVE,
                &[&p.attr().name],
                &l,
            );
        }
        if !p.expr() {
            return e(
                &ParserDiagnostics::IF_DIRECTIVE_SHOULD_BE_EXPRESSION,
                &[],
                p.loc(),
            );
        }
        let m = &p.attr().name["if:".len()..];
        if !VALID_IF_MODIFIERS.contains(&m) {
            return e(&ParserDiagnostics::UNEXPECTED_IF_MODIFIER, &[m], p.loc());
        }
        d.add_scope_node(ScopeNode::If);
    }
    let sd = pas
        .iter()
        .find(|p| p.attr().name == "lwc:slot-data")
        .map(|p| p.loc().clone());
    if let Some(sl) = &sd {
        if tag != "template" {
            return e(
                &ParserDiagnostics::SCOPED_SLOT_DATA_ON_TEMPLATE_ONLY,
                &[],
                sl,
            );
        }
    }
    let has_dir = lwc_cond || out.for_each.is_some() || out.for_of.is_some() || !ifs.is_empty();

    if tag == "template" {
        // validateTemplate (scoped-slot fragments keep their own ordered checks in lowering).
        if attrs.is_empty() {
            return e(&ParserDiagnostics::NO_DIRECTIVE_FOUND_ON_TEMPLATE, &[], &l);
        }
        if sd.is_some() {
            return Ok(());
        }
        let t = "<template>";
        if has(&pas, "lwc:external") {
            return e(
                &ParserDiagnostics::INVALID_LWC_EXTERNAL_ON_NON_CUSTOM_ELEMENT,
                &[t],
                &l,
            );
        }
        if has(&pas, "lwc:inner-html") {
            return e(&ParserDiagnostics::LWC_INNER_HTML_INVALID_ELEMENT, &[t], &l);
        }
        if has(&pas, "lwc:ref") {
            return e(&ParserDiagnostics::LWC_REF_INVALID_ELEMENT, &[t], &l);
        }
        if has(&pas, "lwc:is") {
            return e(&ParserDiagnostics::LWC_IS_INVALID_ELEMENT, &[t], &l);
        }
        if has(&pas, "lwc:on") {
            return if cfg.enable_lwc_on {
                e(&ParserDiagnostics::INVALID_LWC_ON_ELEMENT, &[t], &l)
            } else {
                e(&ParserDiagnostics::INVALID_LWC_ON_OPTS, &[t], &l)
            };
        }
        return Ok(());
    }

    // parseBaseElement.
    let kind = if tag == "slot" {
        Kind::Slot
    } else if is_custom_element_tag(tag) {
        if has(&pas, "lwc:external") {
            Kind::External
        } else {
            Kind::Component
        }
    } else if tag == "lwc:component" {
        Kind::LwcComponent
    } else {
        Kind::Element
    };
    if kind == Kind::LwcComponent {
        if !cfg.enable_dynamic_components {
            return e(
                &ParserDiagnostics::INVALID_OPTS_LWC_ENABLE_DYNAMIC_COMPONENTS,
                &[],
                &l,
            );
        }
        if !has(&pas, "lwc:is") {
            return e(
                &ParserDiagnostics::LWC_COMPONENT_TAG_WITHOUT_IS_DIRECTIVE,
                &[],
                &l,
            );
        }
    }
    if kind == Kind::Slot {
        if has(&pas, "lwc:slot-bind") && !d.light {
            return e(
                &ParserDiagnostics::SCOPED_SLOT_BIND_IN_LIGHT_DOM_ONLY,
                &[],
                &l,
            );
        }
        if has_dir {
            return e(
                &ParserDiagnostics::SLOT_TAG_CANNOT_HAVE_DIRECTIVES,
                &["if:true, lwc:if, lwc:elseif, lwc:else, for:each, for:of, lwc:slot-data"],
                &l,
            );
        }
        if d.light {
            let bad: Vec<&str> = pas
                .iter()
                .map(|p| p.attr().name.as_str())
                .filter(|x| !matches!(*x, "key" | "lwc:slot-bind" | "name" | "slot"))
                .collect();
            if let Some(h) = bad.iter().find(|x| is_event_handler_name(x)) {
                return e(
                    &ParserDiagnostics::LWC_LIGHT_SLOT_INVALID_EVENT_LISTENER,
                    &[h],
                    &l,
                );
            }
            if !bad.is_empty() {
                return e(
                    &ParserDiagnostics::LWC_LIGHT_SLOT_INVALID_ATTRIBUTES,
                    &[&bad.join(","), "key, lwc:slot-bind, name, slot"],
                    &l,
                );
            }
        }
        if let Some(p) = pas.iter().find(|p| p.attr().name == "name") {
            if p.expr() {
                return e(
                    &ParserDiagnostics::NAME_ON_SLOT_CANNOT_BE_EXPRESSION,
                    &[],
                    p.loc(),
                );
            }
        }
        // parseSlot: seen-slot bookkeeping (duplicate / mixed slots), then 1138 inside an iteration.
        let name = pas
            .iter()
            .find(|p| p.attr().name == "name")
            .and_then(|p| match p.value() {
                Av::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let scoped = has(&pas, "lwc:slot-bind");
        let label = if name.is_empty() {
            "default".to_string()
        } else {
            format!("name=\"{name}\"")
        };
        if !d.seen_slots.insert(name.clone()) {
            if d.seen_scoped_slots.contains(&name) {
                return e(
                    &if scoped {
                        ParserDiagnostics::NO_DUPLICATE_SCOPED_SLOT
                    } else {
                        ParserDiagnostics::NO_MIXED_SLOT_TYPES
                    },
                    &[&label],
                    &l,
                );
            }
            // warnAtLocation: 1173 keeps its error level but does not throw.
            if scoped {
                d.push(ParserDiagnostics::NO_MIXED_SLOT_TYPES.generate(&[&label], l.clone()));
            } else {
                d.push(ParserDiagnostics::NO_DUPLICATE_SLOTS.generate(&[&label], l.clone()));
            }
        } else if !scoped && d.in_iteration() {
            d.push(ParserDiagnostics::NO_SLOTS_IN_ITERATOR.generate(&[&label], l.clone()));
        }
        if scoped {
            d.seen_scoped_slots.insert(name);
        }
    }

    // parseBaseElement: the element now exists in its parent.
    out.stage = Stage::Created;
    d.add_scope_node(ScopeNode::Base);
    out.created_warn = validate::unknown_lwc_tag(tag, &l, d);
    // applyHandlers.
    let mut handlers = 0;
    while let Some(h) = pick(&mut pas, |x| x.starts_with("on")) {
        if !h.expr() {
            return e(
                &ParserDiagnostics::EVENT_HANDLER_SHOULD_BE_EXPRESSION,
                &[],
                h.loc(),
            );
        }
        if !is_event_handler_name(&h.attr().name) {
            return e(
                &ParserDiagnostics::INVALID_EVENT_NAME,
                &[&h.attr().name],
                h.loc(),
            );
        }
        handlers += 1;
    }

    // applyKey (nearest iteration through templates; base elements reset the scope for children).
    let for_of = d.for_of_parent();
    let for_each = d.for_each_parent();
    if let Some(k) = pick(&mut pas, |x| x == "key") {
        let Av::Expr(src) = k.value() else {
            return e(
                &ParserDiagnostics::KEY_ATTRIBUTE_SHOULD_BE_EXPRESSION,
                &[],
                k.loc(),
            );
        };
        // TS checks the parsed key's shape (t.isMemberExpression / t.isIdentifier), computed or not:
        // `it[index]` references the iterator index wherever computed members parse (legacy computed or
        // complex expressions; elsewhere normalize already rejected them).
        let b = braced_chain(src, true);
        if let Some(it) = &for_of {
            if let Some(Expression::Member(m)) = &b {
                if matches!((&*m.object, &*m.property), (Expression::Identifier(o), Expression::Identifier(p)) if &o.name == it && p.name == "index")
                {
                    return e(
                        &ParserDiagnostics::KEY_SHOULDNT_REFERENCE_ITERATOR_INDEX,
                        &[tag],
                        k.loc(),
                    );
                }
            }
        } else if let Some(Some(ix)) = &for_each {
            if matches!(&b, Some(Expression::Identifier(i)) if &i.name == ix) {
                return e(
                    &ParserDiagnostics::KEY_SHOULDNT_REFERENCE_FOR_EACH_INDEX,
                    &[tag, ix],
                    k.loc(),
                );
            }
        }
        if for_of.is_none() && for_each.is_none() {
            out.key_warn = Some(
                ParserDiagnostics::KEY_SHOULD_BE_IN_ITERATION.generate(&[tag], k.loc().clone()),
            );
        }
    } else if for_of.is_some() || for_each.is_some() {
        return e(&ParserDiagnostics::MISSING_KEY_IN_ITERATOR, &[tag], &l);
    }

    // applyLwcDirectives.
    if let Some(p) = pas.iter().find(|p| p.attr().name.starts_with("lwc:")) {
        if !ELEMENT_DIRECTIVES.contains(&p.attr().name.as_str()) {
            return e(
                &ParserDiagnostics::UNKNOWN_LWC_DIRECTIVE,
                &[&p.attr().name, &tagb],
                &l,
            );
        }
    }
    for root_only in ["lwc:render-mode", "lwc:preserve-comments"] {
        if has(&pas, root_only) {
            return e(
                &ParserDiagnostics::UNKNOWN_LWC_DIRECTIVE,
                &[root_only, &tagb],
                &l,
            );
        }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:external") {
        if kind != Kind::External {
            return e(
                &ParserDiagnostics::INVALID_LWC_EXTERNAL_ON_NON_CUSTOM_ELEMENT,
                &[&tagb],
                &l,
            );
        }
        if *p.value() != Av::Bool {
            return e(&ParserDiagnostics::INVALID_LWC_EXTERNAL_VALUE, &[&tagb], &l);
        }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:dynamic") {
        if !cfg.experimental_dynamic_directive {
            return e(&ParserDiagnostics::INVALID_OPTS_LWC_DYNAMIC, &[], &l);
        }
        if kind != Kind::Component {
            return e(
                &ParserDiagnostics::INVALID_LWC_DYNAMIC_ON_NATIVE_ELEMENT,
                &[&tagb],
                &l,
            );
        }
        if !p.expr() {
            return e(
                &ParserDiagnostics::INVALID_LWC_DYNAMIC_LITERAL_PROP,
                &[&tagb],
                &l,
            );
        }
        out.dynamic_ok = true;
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:is") {
        if kind != Kind::LwcComponent {
            return e(&ParserDiagnostics::LWC_IS_INVALID_ELEMENT, &[&tagb], &l);
        }
        match p.value() {
            Av::Expr(_) => {}
            Av::Str(s) => return e(&ParserDiagnostics::INVALID_LWC_IS_DIRECTIVE_VALUE, &[s], &l),
            Av::Bool => {
                return e(
                    &ParserDiagnostics::INVALID_LWC_IS_DIRECTIVE_VALUE,
                    &["true"],
                    &l,
                );
            }
        }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:dom") {
        if d.light {
            return e(
                &ParserDiagnostics::LWC_DOM_INVALID_IN_LIGHT_DOM,
                &[&tagb],
                &l,
            );
        }
        if kind == Kind::Component {
            return e(
                &ParserDiagnostics::LWC_DOM_INVALID_CUSTOM_ELEMENT,
                &[&tagb],
                &l,
            );
        }
        if kind == Kind::Slot {
            return e(&ParserDiagnostics::LWC_DOM_INVALID_SLOT_ELEMENT, &[], &l);
        }
        if *p.value() != Av::Str("manual".into()) {
            return e(
                &ParserDiagnostics::LWC_DOM_INVALID_VALUE,
                &["\"manual\""],
                &l,
            );
        }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:inner-html") {
        if matches!(kind, Kind::Component | Kind::LwcComponent) {
            return e(
                &ParserDiagnostics::LWC_INNER_HTML_INVALID_CUSTOM_ELEMENT,
                &[&tagb],
                &l,
            );
        }
        if kind == Kind::Slot {
            return e(
                &ParserDiagnostics::LWC_INNER_HTML_INVALID_ELEMENT,
                &[&tagb],
                &l,
            );
        }
        if *p.value() == Av::Bool {
            return e(
                &ParserDiagnostics::LWC_INNER_HTML_INVALID_VALUE,
                &[&tagb],
                &l,
            );
        }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:ref") {
        if kind == Kind::Slot {
            return e(&ParserDiagnostics::LWC_REF_INVALID_ELEMENT, &[&tagb], &l);
        }
        if d.in_iteration() {
            return e(
                &ParserDiagnostics::LWC_REF_INVALID_LOCATION_INSIDE_ITERATION,
                &[&tagb],
                &l,
            );
        }
        if !matches!(p.value(), Av::Str(s) if !s.is_empty()) {
            return e(&ParserDiagnostics::LWC_REF_INVALID_VALUE, &[&tagb], &l);
        }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:spread") {
        if !p.expr() {
            return e(
                &ParserDiagnostics::INVALID_LWC_SPREAD_LITERAL_PROP,
                &[&tagb],
                &l,
            );
        }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:on") {
        if !cfg.enable_lwc_on {
            return e(&ParserDiagnostics::INVALID_LWC_ON_OPTS, &[], &l);
        }
        if !p.expr() {
            return e(
                &ParserDiagnostics::INVALID_LWC_ON_LITERAL_PROP,
                &[&tagb],
                &l,
            );
        }
        if handlers > 0 {
            return e(
                &ParserDiagnostics::INVALID_LWC_ON_WITH_DECLARATIVE_LISTENERS,
                &[&tagb],
                &l,
            );
        }
    }
    if let Some(p) = pick(&mut pas, |x| x == "lwc:slot-bind") {
        if kind != Kind::Slot {
            return e(
                &ParserDiagnostics::INVALID_LWC_SLOT_BIND_NON_SLOT_ELEMENT,
                &[&tagb],
                &l,
            );
        }
        if !p.expr() {
            return e(
                &ParserDiagnostics::INVALID_LWC_SLOT_BIND_LITERAL_PROP,
                &[&tagb],
                &l,
            );
        }
    }
    pick(&mut pas, |x| x == "lwc:slot-data");

    // applyAttributes.
    for p in &pas {
        if let Some(w) = validate::attr_name_warnings(tag, &p.attr().name, p.loc(), d) {
            out.attr_warns.push(w);
        }
        let last = p.attr().name.chars().last().unwrap_or(' ');
        if !(last.is_ascii_lowercase() || last.is_ascii_digit()) {
            return e(
                &ParserDiagnostics::ATTRIBUTE_NAME_MUST_END_WITH_ALPHA_NUMERIC_CHARACTER,
                &[&p.attr().name, tag],
                p.loc(),
            );
        }
        // `/^-*[a-z]|^[_$]/`
        let start_ok = p
            .attr()
            .name
            .trim_start_matches('-')
            .starts_with(|c: char| c.is_ascii_lowercase())
            || p.attr().name.starts_with(['_', '$']);
        if !start_ok {
            return e(
                &ParserDiagnostics::ATTRIBUTE_NAME_STARTS_WITH_INVALID_CHARACTER,
                &[&p.attr().name, tag],
                p.loc(),
            );
        }
        if p.attr().name == "id" {
            if let Av::Str(v) = p.value() {
                if v.chars().any(js_ws) {
                    return e(&ParserDiagnostics::INVALID_ID_ATTRIBUTE, &[v], p.loc());
                }
                if !d.seen_ids.insert(v.clone()) {
                    return e(&ParserDiagnostics::DUPLICATE_ID_FOUND, &[v], p.loc());
                }
            }
        }
    }

    // validateElement (handlers, key, lwc directives and attributes are applied).
    out.stage = Stage::Applied;
    if ns == NS_HTML && !VOID.contains(&tag) && el.end_tag.is_none() && tag == tag.to_lowercase() {
        return e(&ParserDiagnostics::NO_MATCHING_CLOSING_TAGS, &[tag], &l);
    }
    if tag == "style" && ns == NS_HTML {
        return e(
            &ParserDiagnostics::STYLE_TAG_NOT_ALLOWED_IN_TEMPLATE,
            &[],
            &l,
        );
    }
    if ns == NS_HTML && DISALLOWED_HTML_TAGS.contains(&tag) {
        return e(&ParserDiagnostics::FORBIDDEN_TAG_ON_TEMPLATE, &[tag], &l);
    }
    if ns == NS_SVG && !tables::SUPPORTED_SVG_TAGS.contains(&tag) {
        return e(
            &ParserDiagnostics::FORBIDDEN_SVG_NAMESPACE_IN_TEMPLATE,
            &[tag],
            &l,
        );
    }
    if ns == NS_MATHML && DISALLOWED_MATHML_TAGS.contains(&tag) {
        return e(
            &ParserDiagnostics::FORBIDDEN_MATHML_NAMESPACE_IN_TEMPLATE,
            &[tag],
            &l,
        );
    }

    // validateAttributes / validateProperties.
    for p in &pas {
        if p.attr().name == "is" {
            return e(&ParserDiagnostics::IS_ATTRIBUTE_NOT_SUPPORTED, &[], &l);
        }
        if p.attr().name == "tabindex"
            && !p.expr()
            && !matches!(p.value(), Av::Str(s) if s == "0" || s == "-1")
        {
            return e(&ParserDiagnostics::INVALID_TABINDEX_ATTRIBUTE, &[], &l);
        }
        if tag == "iframe" && p.attr().name == "srcdoc" {
            return e(
                &ParserDiagnostics::FORBIDDEN_IFRAME_SRCDOC_ATTRIBUTE,
                &[],
                &l,
            );
        }
    }
    Ok(())
}

/// TS `EVENT_HANDLER_NAME_RE`: `/^on[a-z][a-z0-9_]*$/`.
fn is_event_handler_name(s: &str) -> bool {
    s.strip_prefix("on").is_some_and(|r| {
        let mut cs = r.chars();
        cs.next().is_some_and(|c| c.is_ascii_lowercase())
            && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    })
}

/// TS parseRoot after the tag check: parseAttributes (1055/1037/1036/1035), applyRootLwcDirectives
/// (1133 render-mode, 1146 preserve-comments), validateRoot (1080 unknown attributes, 1078 no end tag).
/// Returns the root's location (TS `ast.root`, read between parseAttributes and the directives), or
/// `None` when an error was reported (no root).
pub(super) fn root_errors(
    attrs: &[parse5::Attribute],
    n: &NodeRef,
    el: &parse5::ElementLocation,
    cfg: &NormalizedConfig,
    d: &mut Ctx,
) -> Located<Option<ElementSourceLocation>> {
    let l = loc(&el.location);
    let mut pas = Vec::new();
    // TS parseAttributes: per attribute, its location, normalizeAttributeValue (1055/1037/1036/1034/1035)
    // then the expression parse; the first error aborts the root.
    for a in attrs {
        let nz = match normalize("template", a, n, cfg, d)? {
            Ok(nz) => nz,
            Err(e) => {
                d.push(e);
                return Ok(None);
            }
        };
        let mut found = Vec::new();
        validate::attribute_expressions(&nz, cfg, &mut found, d)?;
        pas.push(CheckedAttr {
            attr: a,
            value: nz.v(),
            loc: nz.ra.location.clone(),
        });
        for x in found {
            let threw = x.level == DiagnosticLevel::Error;
            d.push(x);
            if threw {
                return Ok(None);
            }
        }
    }
    let location = element_location(el)?;
    if pas.iter().any(|p| p.attr().name.starts_with("lwc:")) {
        if let Some(p) = pick(&mut pas, |x| x == "lwc:render-mode") {
            if !matches!(p.value(), Av::Str(s) if s == "shadow" || s == "light") {
                d.push(ParserDiagnostics::LWC_RENDER_MODE_INVALID_VALUE.generate(&[], l));
                return Ok(None);
            }
        }
        if let Some(p) = pick(&mut pas, |x| x == "lwc:preserve-comments") {
            if *p.value() != Av::Bool {
                d.push(ParserDiagnostics::PRESERVE_COMMENTS_MUST_BE_BOOLEAN.generate(&[], l));
                return Ok(None);
            }
        }
    }
    let mut bad = Vec::new();
    validate::root_attributes(attrs, &l, &mut bad);
    if let Some(x) = bad.pop() {
        d.push(x);
        return Ok(None);
    }
    if el.end_tag.is_none() {
        d.push(ParserDiagnostics::NO_MATCHING_CLOSING_TAGS.generate(&["template"], l));
        return Ok(None);
    }
    Ok(Some(location))
}
