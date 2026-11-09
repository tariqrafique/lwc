//! Port of transformers/element.ts (Element / ExternalComponent / the `<slot>` element of Slot), plus the
//! @lwc/shared helpers it uses (isBooleanAttribute, isVoidElement, normalizeStyleAttributeValue,
//! normalizeTabIndex) and shared.ts normalizeClassAttributeValue.
use super::{add, emit_children, expr, js_str, Cx, Emit, TemplateOptions};
use crate::runtime_abi as abi;
use lwc_shared::template_ir::{Attribute, AttributeValue, ChildNode, ElementDirective, LiteralValue, Property};

pub(crate) const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";

/// The fields element.ts reads off IrElement | IrExternalComponent | IrSlot.
pub(crate) struct ElView<'a> {
    pub name: &'a str,
    pub namespace: &'a str,
    pub attributes: &'a [Attribute<'a>],
    pub properties: &'a [Property<'a>],
    pub directives: &'a [ElementDirective<'a>],
    pub children: &'a [ChildNode<'a>],
    /// `node.type === 'Element'` (only plain elements honor lwc:inner-html).
    pub is_element: bool,
}

/// @lwc/shared isBooleanAttribute.
pub(crate) fn is_boolean_attribute(attr: &str, tag: &str) -> bool {
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

/// @lwc/shared isVoidElement(name, HTML_NAMESPACE) (element.ts always passes the HTML namespace).
pub(crate) fn is_void_element(name: &str) -> bool {
    const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr", "param", "keygen", "menuitem"];
    VOID.contains(&name.to_lowercase().as_str())
}

use lwc_shared::js::{js_trim, js_ws};

/// shared.ts normalizeClassAttributeValue: trim, then collapse `\s+` to one space.
pub(crate) fn normalize_class_attribute_value(v: &str) -> String {
    let mut out = String::new();
    let mut ws = false;
    for c in js_trim(v).chars() {
        if js_ws(c) { ws = true; continue; }
        if ws { out.push(' '); ws = false; }
        out.push(c);
    }
    out
}

pub(crate) use lwc_shared::style::normalize_style_attribute_value;

/// JS `Number(str)` (enough for normalizeTabIndex's `value > 0`).
fn js_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() { return 0.0; }
    let radix = |p: &str, r: u32| u64::from_str_radix(p, r).map(|n| n as f64).unwrap_or(f64::NAN);
    let l = t.to_ascii_lowercase();
    if let Some(p) = l.strip_prefix("0x") { return radix(p, 16); }
    if let Some(p) = l.strip_prefix("0o") { return radix(p, 8); }
    if let Some(p) = l.strip_prefix("0b") { return radix(p, 2); }
    match t { "Infinity" | "+Infinity" => return f64::INFINITY, "-Infinity" => return f64::NEG_INFINITY, _ => {} }
    if t.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E') { return f64::NAN; }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// normalizeTabIndex on a string literal, rendered as a JS literal.
pub(crate) fn normalize_tab_index_literal(s: &str) -> String {
    if js_number(s) > 0.0 { "0".into() } else { js_str(s) }
}

enum AttrOrProp<'a> { Attr(&'a Attribute<'a>), Prop(&'a Property<'a>) }
impl AttrOrProp<'_> {
    fn name(&self) -> &str { match self { Self::Attr(a) => &a.name, Self::Prop(p) => &p.name } }
    fn value(&self) -> &AttributeValue<'_> { match self { Self::Attr(a) => &a.value, Self::Prop(p) => &p.value } }
}

/// reorderAttributes: class, style, other attrs, props, slot.
fn reorder<'a>(attrs: &'a [Attribute], props: &'a [Property]) -> Vec<AttrOrProp<'a>> {
    let (mut class, mut style, mut slot) = (None, None, None);
    let mut boring = Vec::new();
    for a in attrs {
        match a.name.as_str() { "class" => class = Some(a), "style" => style = Some(a), "slot" => slot = Some(a), _ => boring.push(a) }
    }
    let mut v: Vec<AttrOrProp> = Vec::new();
    v.extend(class.map(AttrOrProp::Attr));
    v.extend(style.map(AttrOrProp::Attr));
    v.extend(boring.into_iter().map(AttrOrProp::Attr));
    v.extend(props.iter().map(AttrOrProp::Prop));
    v.extend(slot.map(AttrOrProp::Attr));
    v
}

const SCOPE_TEST: &str = "hasScopedStylesheets || hasScopedStaticStylesheets(Cmp)";

/// yieldAttrOrPropLiteralValue (bStringLiteralYield is folded at compile time except for `class`,
/// whose scope-token suffix is a runtime decision).
fn literal_value(name: &str, v: &LiteralValue, o: TemplateOptions) -> Vec<String> {
    match v {
        LiteralValue::String(s) => {
            let val = match name {
                "style" => normalize_style_attribute_value(s),
                "class" => { let c = normalize_class_attribute_value(s); if c.is_empty() { return vec![]; } c }
                "spellcheck" => (s.to_lowercase() != "false").to_string(),
                _ => s.clone(),
            };
            if name == "class" {
                vec![format!("{{ const shouldRenderScopeToken = {SCOPE_TEST}; const suffix = shouldRenderScopeToken ? ' ' + stylesheetScopeToken : ''; {} {} }}",
                    o.y(&js_str(" class")), o.y(&format!("'=\"' + {} + suffix + '\"'", js_str(&val))))]
            } else if val.is_empty() {
                vec![o.y(&js_str(&format!(" {name}")))]
            } else {
                vec![o.y(&js_str(&format!(" {name}=\"{val}\"")))]
            }
        }
        LiteralValue::Bool(_) => if name == "class" { vec![] } else { vec![o.y(&js_str(&format!(" {name}")))] },
    }
}

/// yieldAttrOrPropDynamicValue.
fn dynamic_value(el: &str, name: &str, e: &lwc_shared::template_ir::Expression, o: TemplateOptions, cx: &mut Cx, sc: &[String]) -> String {
    add(&mut cx.imports, abi::HTML_ESCAPE);
    let v = expr(e, sc);
    if name == "class" {
        add(&mut cx.imports, abi::NORMALIZE_CLASS);
        return format!("{{ const attrValue = normalizeClass({v}); const shouldRenderScopeToken = {SCOPE_TEST}; let combinedValue = shouldRenderScopeToken ? stylesheetScopeToken : ''; if (attrValue) {{ if (combinedValue) {{ combinedValue += ' '; }} combinedValue += htmlEscape(String(attrValue), true); }} if (combinedValue) {{ {} }} }}",
            o.y("` class=\"${combinedValue}\"`"));
    }
    format!("{{ const attrName = {}; let attrValue = {v}; const isHtmlBooleanAttr = {}; if (isHtmlBooleanAttr) {{ attrValue = attrValue ? '' : undefined; }} if (attrName === 'tabindex') {{ const shouldNormalize = attrValue > 0 && typeof attrValue !== 'boolean'; attrValue = shouldNormalize ? 0 : attrValue; }} if (attrName === 'style' && (typeof attrValue !== 'string' || attrValue === '')) {{ attrValue = undefined; }} if (attrValue !== undefined && attrValue !== null) {{ {} if (attrValue !== '') {{ {} }} }} }}",
        js_str(name), is_boolean_attribute(name, el), o.y("' ' + attrName"), o.y("`=\"${htmlEscape(String(attrValue), true)}\"`"))
}

/// element.ts:234-235: `if (type === 'Attribute' && (name === 'inner-h-t-m-l' || name === 'outer-h-t-m-l'))
/// throw new Error(`Cannot set attribute "${name}" on <${node.name}>.`)`. A plain `Error` (no LWC code, no
/// filename/location) that @lwc/compiler transformer.ts:99 returns uncaught, so every field but `message` is `None`.
pub(crate) fn reject_inner_outer_html(tag: &str, name: &str) -> Emit {
    if name == "inner-h-t-m-l" || name == "outer-h-t-m-l" {
        return Err(lwc_shared::CompileError {
            message: format!("Cannot set attribute \"{name}\" on <{tag}>."),
            filename: None, diagnostics: vec![], name: None, line: None, column: None,
        });
    }
    Ok(())
}

pub(crate) fn emit_element(n: &ElView, o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) -> Emit {
    let inner_html = if n.is_element { n.directives.iter().find_map(|d| if let ElementDirective::InnerHtml(v) = d { Some(v) } else { None }) } else { None };
    let mut has_class = false;
    let mut attr_stmts = Vec::new();
    for a in reorder(n.attributes, n.properties) {
        let name = a.name();
        if n.name == "input" && (name == "value" || name == "checked") { continue; }
        // element.ts:234: Attributes only (props, e.g. a component's innerHTML, never throw).
        if let AttrOrProp::Attr(_) = a { reject_inner_outer_html(n.name, name)?; }
        let r = match a.value() {
            AttributeValue::Literal(l) => literal_value(name, &l.value, o),
            AttributeValue::Expression(e) => vec![dynamic_value(n.name, name, e, o, cx, sc)],
        };
        if !r.is_empty() && name == "class" { has_class = true; }
        attr_stmts.extend(r);
    }
    let mut child = Vec::new();
    if !n.children.is_empty() {
        emit_children(n.children, o, &mut child, cx, sc)?;
    } else if let Some(v) = inner_html {
        let e = match v {
            AttributeValue::Literal(l) => match &l.value { LiteralValue::String(s) => js_str(s), LiteralValue::Bool(b) => b.to_string() },
            AttributeValue::Expression(e) => expr(e, sc),
        };
        child.push(o.y(&format!("sanitizeHtmlContent({e})")));
        add(&mut cx.imports, abi::SANITIZE_HTML_CONTENT);
    }
    let foreign_self_closing = n.namespace != HTML_NAMESPACE && child.is_empty();
    let self_closing = is_void_element(n.name) || foreign_self_closing;
    add(&mut cx.imports, abi::HAS_SCOPED_STATIC_STYLESHEETS);
    out.push(o.y(&js_str(&format!("<{}", n.name))));
    out.push(format!("if (slotAttributeValue) {{ {} }}", o.y("` slot=\"${slotAttributeValue}\"`")));
    if !has_class { out.push(format!("if ({SCOPE_TEST}) {{ {} }}", o.y("` class=\"${stylesheetScopeToken}\"`"))); }
    out.extend(attr_stmts);
    out.push(o.y(&js_str(if foreign_self_closing { "/>" } else { ">" })));
    if !self_closing {
        out.extend(child);
        out.push(o.y(&js_str(&format!("</{}>", n.name))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn style_normalization() {
        assert_eq!(normalize_style_attribute_value("color: red    !important ;background:blue"), "color: red !important; background: blue;");
        assert_eq!(normalize_style_attribute_value("background: url(a;b); x"), "background: url(a;b);");
        assert_eq!(normalize_style_attribute_value(""), "");
    }
    #[test]
    fn class_and_tabindex() {
        assert_eq!(normalize_class_attribute_value("  a \n b  "), "a b");
        assert_eq!(normalize_tab_index_literal("-1"), "'-1'");
        assert_eq!(normalize_tab_index_literal("3"), "0");
        assert_eq!(normalize_tab_index_literal("0"), "'0'");
    }
    #[test]
    fn booleans() {
        assert!(is_boolean_attribute("hidden", "div") && is_boolean_attribute("checked", "input") && !is_boolean_attribute("checked", "div"));
        assert!(is_void_element("BR") && !is_void_element("div"));
    }
}
