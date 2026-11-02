//! Port of transformers/component (all modes; sync/async are the transmogrified `__lwcYield +=` shapes).
//! Slotted content follows slotted-content.ts getSlottedContent: shadow fn (hoisted), light map, scoped map.
use super::{add, element, emit_children, expr, is_concat, js_str, Cx, TemplateOptions};
use crate::runtime_abi::{ADD_SLOTTED_CONTENT, LIGHTNING_ELEMENT, NORMALIZE_CLASS, NORMALIZE_TAB_INDEX, SYMBOL__GENERATE_MARKUP};
use lwc_shared::template_ir::{Attribute, AttributeValue, ChildNode, Component, ElementDirective, Expression, ScopedSlotFragment, LiteralValue, LwcComponent, Property, SourceLocation};

/// "x-foo-bar" -> "x/fooBar": first '-' becomes '/', later '-' uppercases the next char.
pub(crate) fn kebabcase_to_camelcase_path(name: &str) -> String {
    let mut out = String::new();
    let (mut seen, mut up) = (false, false);
    for ch in name.chars() {
        if ch == '-' { if seen { up = true; } else { seen = true; out.push('/'); } continue; }
        if up { out.extend(ch.to_uppercase()); up = false; } else { out.push(ch); }
    }
    out
}

/// "x-foo" -> "xFoo".
pub(crate) fn to_property_name(name: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for ch in name.chars() {
        if ch == '-' { up = true; continue; }
        if up { out.extend(ch.to_uppercase()); up = false; } else { out.push(ch); }
    }
    out
}

/// shared.ts getChildAttrsOrProps value: class/style normalized, spellcheck -> bool, tabindex -> normalizeTabIndex;
/// bool -> `''` for attrs, the literal for props; class with empty/bool value is omitted (`None`).
fn child_value(name: &str, v: &AttributeValue, is_prop: bool, cx: &mut Cx, sc: &[String]) -> Option<String> {
    let lower = name.to_lowercase();
    match v {
        AttributeValue::Literal(l) => match &l.value {
            LiteralValue::String(s) => Some(match name {
                "style" => js_str(&element::normalize_style_attribute_value(s)),
                "class" => { let c = element::normalize_class_attribute_value(s); if c.is_empty() { return None; } js_str(&c) }
                "spellcheck" => (s.to_lowercase() != "false").to_string(),
                _ if lower == "tabindex" => element::normalize_tab_index_literal(s),
                _ => js_str(s),
            }),
            LiteralValue::Bool(b) => if name == "class" { None } else if is_prop { Some(b.to_string()) } else { Some("''".into()) },
        },
        AttributeValue::Expression(e) => {
            let x = expr(e, sc);
            Some(if name == "class" { add(&mut cx.imports, NORMALIZE_CLASS); format!("normalizeClass({x})") }
                 else if lower == "tabindex" { add(&mut cx.imports, NORMALIZE_TAB_INDEX); format!("normalizeTabIndex({x})") }
                 else { x })
        }
    }
}

fn child_obj(entries: Vec<(&str, &AttributeValue)>, is_prop: bool, cx: &mut Cx, sc: &[String]) -> String {
    let parts: Vec<String> = entries.into_iter().filter_map(|(k, v)| child_value(k, v, is_prop, cx, sc).map(|v| format!("{}: {v}", js_str(k)))).collect();
    if parts.is_empty() { "{}".into() } else { format!("{{ {} }}", parts.join(", ")) }
}

/// The `childProps` / `childAttrs` / dangling-slot lines shared by component.ts and lwc-component.ts.
fn child_attrs_lines(attributes: &[Attribute], properties: &[Property], out: &mut Vec<String>, cx: &mut Cx, sc: &[String]) {
    let p = child_obj(properties.iter().map(|p| (p.name.as_str(), &p.value)).collect(), true, cx, sc);
    out.push(format!("const childProps = {p};"));
    let a = child_obj(attributes.iter().map(|a| (a.name.as_str(), &a.value)).collect(), false, cx, sc);
    out.push(format!("const childAttrs = {a};"));
    out.push("if (slotAttributeValue) { childAttrs.slot = slotAttributeValue; }".into());
}

/// `/-([a-z])/g` camel-casing (shared kebabCaseToCamelCase).
fn kebab_case_to_camel_case(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '-' && i + 1 < cs.len() && cs[i + 1].is_ascii_lowercase() { out.push(cs[i + 1].to_ascii_uppercase()); i += 2; } else { out.push(cs[i]); i += 1; }
    }
    out
}

/// slotted-content.ts getSlottedContent: shadow fn (hoisted + curried over local vars, deduped by node id),
/// light map, scoped map. Returns the statements inserted into the component block.
fn slotted_content(name: &str, loc: &SourceLocation, children_all: &[ChildNode], o: TemplateOptions, cx: &mut Cx, sc: &mut Vec<String>) -> Vec<String> {
    let prev = cx.slotted;
    cx.slotted = true;
    let slottable: Vec<ChildNode> = children_all.iter().filter(|c| !matches!(c, ChildNode::ScopedSlotFragment(_))).cloned().collect();
    let mut shadow_body = Vec::new();
    cx.reset_top = true;
    emit_children(&slottable, o, &mut shadow_body, cx, sc);
    cx.reset_top = false;
    let light = light_slotted_content(&slottable, o, cx, sc);
    let scoped = scoped_slotted_content(&original_children_scoped(children_all), o, cx, sc);
    cx.slotted = prev;
    let has_shadow = !shadow_body.is_empty();
    if has_shadow || !light.is_empty() || !scoped.is_empty() { add(&mut cx.imports, ADD_SLOTTED_CONTENT); }
    let uid = format!("{name}:{}:{}", loc.start, loc.end);
    let mut locals: Vec<String> = Vec::new();
    for v in sc.iter() { if !locals.contains(v) { locals.push(v.clone()); } }
    let params = locals.join(", ");
    let mut res = Vec::new();
    if has_shadow {
        let f = match cx.shadow_fns.iter().find(|(k, _)| *k == uid) {
            Some((_, f)) => f.clone(),
            None => {
                let f = format!("__lwcGenerateShadowSlottedContent_{}_{}", kebab_case_to_camel_case(name).replacen(':', "_", 1), cx.fn_id);
                cx.fn_id += 1;
                cx.shadow_fns.push((uid, f.clone()));
                let mut h = format!("const {f} = ({params}) => {} {f}(contextfulParent) {{ {}", o.func(), o.prologue());
                for l in shadow_body { h.push_str(&l); h.push(' '); }
                h.push_str(o.epilogue()); h.push_str("};");
                cx.hoisted.push(h);
                f
            }
        };
        res.push(format!("const shadowSlottedContent = {f}({params});"));
    } else {
        res.push("const shadowSlottedContent = null;".into());
    }
    res.push(format!("const lightSlottedContentMap = {};", if light.is_empty() { "null" } else { "Object.create(null)" }));
    res.push(format!("const scopedSlottedContentMap = {};", if scoped.is_empty() { "null" } else { "Object.create(null)" }));
    res.extend(light);
    res.extend(scoped);
    res
}

fn original_children_scoped(all: &[ChildNode]) -> Vec<&ScopedSlotFragment> {
    all.iter().filter_map(|c| if let ChildNode::ScopedSlotFragment(f) = c { Some(f) } else { None }).collect()
}

/// Sync form of bAddSlottedContent; a null scoped-data param is omitted (isNullableOf), as in transmogrified JS.
fn add_slotted(o: TemplateOptions, slot_name: &str, data: Option<&str>, body: Vec<String>, map: &str) -> String {
    let params = match data { Some(d) => format!("contextfulParent, {d}, slotAttributeValue"), None => "contextfulParent, slotAttributeValue".into() };
    let mut s = format!("addSlottedContent({slot_name} ?? \"\", {} __lwcGenerateSlottedContent({params}) {{ {}", o.func(), o.prologue());
    for l in body { s.push_str(&l); s.push(' '); }
    s.push_str(&format!("{}}}, {map});", o.epilogue()));
    s
}

fn kids(n: &mut ChildNode) -> Option<&mut Vec<ChildNode>> {
    match n {
        ChildNode::If(x) => Some(&mut x.children), ChildNode::IfBlock(x) => Some(&mut x.children),
        ChildNode::ElseifBlock(x) => Some(&mut x.children), ChildNode::ElseBlock(x) => Some(&mut x.children),
        _ => None,
    }
}

/// getLightSlottedContent: one addSlottedContent per slottable leaf, wrapped in a clone of its If/IfBlock
/// ancestor chain with siblings pruned. Ancestor `else_` branches are kept as-is (JS only prunes `children`).
fn light_slotted_content(roots: &[ChildNode], o: TemplateOptions, cx: &mut Cx, sc: &mut Vec<String>) -> Vec<String> {
    let mut res = Vec::new();
    fn walk(nodes: &[ChildNode], path: &mut Vec<usize>, roots: &[ChildNode], o: TemplateOptions, cx: &mut Cx, sc: &mut Vec<String>, res: &mut Vec<String>) {
        for (i, n) in nodes.iter().enumerate() {
            match n {
                ChildNode::If(_) | ChildNode::IfBlock(_) | ChildNode::ElseifBlock(_) | ChildNode::ElseBlock(_) => {
                    let mut c = n.clone(); path.push(i);
                    walk(kids(&mut c).unwrap(), path, roots, o, cx, sc, res); path.pop();
                }
                ChildNode::Slot(_) | ChildNode::Element(_) | ChildNode::Text(_) | ChildNode::Component(_) | ChildNode::ExternalComponent(_) => {
                    // Concatenated text: only the last node renders, carrying its preceding concat run.
                    if matches!(n, ChildNode::Text(_)) && nodes.get(i + 1).is_some_and(|s| is_concat(s, o)) { continue; }
                    // Top-level text is transformed with the original siblings (renders its whole concat run);
                    // nested under an ancestor clone, the pruned children hold only the leaf.
                    let mut leaf: Vec<ChildNode> = if matches!(n, ChildNode::Text(_)) && path.is_empty() {
                        let start = (0..=i).rev().take_while(|&j| is_concat(&nodes[j], o)).last().unwrap_or(i);
                        nodes[start..=i].to_vec()
                    } else { vec![n.clone()] };
                    let slot_name = match n {
                        ChildNode::Text(_) => "''".to_string(),
                        ChildNode::Element(e) => super::slot::attr_value(&e.attributes, "slot", sc),
                        ChildNode::Component(e) => super::slot::attr_value(&e.attributes, "slot", sc),
                        ChildNode::ExternalComponent(e) => super::slot::attr_value(&e.attributes, "slot", sc),
                        ChildNode::Slot(e) => super::slot::attr_value(&e.attributes, "slot", sc),
                        _ => unreachable!(),
                    };
                    // Light DOM slots do not render the `slot` attribute.
                    if let Some(l) = leaf.last_mut() { match l {
                        ChildNode::Element(e) => e.attributes.retain(|a| a.name != "slot"),
                        ChildNode::Component(e) => e.attributes.retain(|a| a.name != "slot"),
                        ChildNode::ExternalComponent(e) => e.attributes.retain(|a| a.name != "slot"),
                        ChildNode::Slot(e) => e.attributes.retain(|a| a.name != "slot"),
                        _ => {}
                    } }
                    // Rebuild ancestor chain with pruned children.
                    let mut tree = leaf;
                    let mut cur: &[ChildNode] = roots; let mut chain = Vec::new();
                    for &k in path.iter() { chain.push(cur[k].clone()); cur = kids_ref(&cur[k]); }
                    for mut a in chain.into_iter().rev() { *kids(&mut a).unwrap() = tree; tree = vec![a]; }
                    let prev = cx.slotted;
                    cx.slotted = !path.is_empty() || matches!(n, ChildNode::Slot(_));
                    let mut body = Vec::new();
                    emit_children(&tree, o, &mut body, cx, sc);
                    cx.slotted = prev;
                    res.push(add_slotted(o, &slot_name, None, body, "lightSlottedContentMap"));
                }
                _ => {}
            }
        }
    }
    walk(roots, &mut Vec::new(), roots, o, cx, sc, &mut res);
    res
}

fn kids_ref(n: &ChildNode) -> &[ChildNode] {
    match n { ChildNode::If(x) => &x.children, ChildNode::IfBlock(x) => &x.children, ChildNode::ElseifBlock(x) => &x.children, ChildNode::ElseBlock(x) => &x.children, _ => &[] }
}

/// Scoped slots: `<template lwc:slot-data="d">` -> addSlottedContent(name, fn(contextfulParent, d, slotAttributeValue), scopedSlottedContentMap).
fn scoped_slotted_content(frags: &[&ScopedSlotFragment], o: TemplateOptions, cx: &mut Cx, sc: &mut Vec<String>) -> Vec<String> {
    frags.iter().map(|f| {
        let var = match &f.slot_data { Expression::Identifier(i) => i.name.clone(), e => expr(e, &[]) };
        sc.push(var.clone());
        let name = match &f.slot_name {
            AttributeValue::Literal(l) => match &l.value { LiteralValue::String(s) => js_str(s), LiteralValue::Bool(b) => b.to_string() },
            AttributeValue::Expression(e) => expr(e, sc),
        };
        let prev = cx.slotted; cx.slotted = true;
        let mut body = Vec::new();
        emit_children(&f.children, o, &mut body, cx, sc);
        cx.slotted = prev; sc.pop();
        add_slotted(o, &name, Some(&var), body, "scopedSlottedContentMap")
    }).collect()
}

pub(crate) fn emit_component(c: &Component, o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) {
    let local = format!("ChildComponentCtor_{}", to_property_name(&c.name));
    if !cx.modules.iter().any(|(l, _)| *l == local) { cx.modules.push((local.clone(), kebabcase_to_camelcase_path(&c.name))); }
    add(&mut cx.imports, SYMBOL__GENERATE_MARKUP);
    add(&mut cx.imports, o.fallback_tmpl());
    out.push("{".into());
    child_attrs_lines(&c.attributes, &c.properties, out, cx, sc);
    let slotted = slotted_content(&c.name, &c.location.location, &c.children, o, cx, sc);
    out.extend(slotted);
    out.push("const scopeToken = hasScopedStylesheets ? stylesheetScopeToken : undefined;".into());
    out.push(format!("const generateMarkup = {local}[SYMBOL__GENERATE_MARKUP];"));
    out.push(format!("const tagName = {};", js_str(&c.name)));
    out.push("if (generateMarkup) {".into());
    out.push(o.yd("generateMarkup(tagName, childProps, childAttrs, scopeToken, contextfulParent, renderContext, shadowSlottedContent, lightSlottedContentMap, scopedSlottedContentMap)"));
    out.push("} else {".into());
    out.push(o.y("`<${tagName}>`"));
    out.push(o.yd(&format!("{}(shadowSlottedContent, lightSlottedContentMap, scopedSlottedContentMap, {local}, instance, renderContext)", o.fallback_tmpl())));
    out.push(o.y("`</${tagName}>`"));
    out.push("}".into());
    out.push("}".into());
}

/// lwc-component.ts: only `lwc:is` renders (no wrapping block, as in JS); without it, nothing.
pub(crate) fn emit_lwc_component(c: &LwcComponent, o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) {
    let Some(is) = c.directives.iter().find_map(|d| if let ElementDirective::Is(e) = d { Some(expr(e, sc)) } else { None }) else { return };
    add(&mut cx.imports, LIGHTNING_ELEMENT);
    add(&mut cx.imports, SYMBOL__GENERATE_MARKUP);
    out.push(format!("const Ctor = {is};"));
    out.push("if (Ctor) {".into());
    out.push("if (typeof Ctor !== 'function' || !(Ctor.prototype instanceof LightningElement)) { throw new Error(`Invalid constructor: \"${String(Ctor)}\" is not a LightningElement constructor.`); }".into());
    child_attrs_lines(&c.attributes, &c.properties, out, cx, sc);
    let slotted = slotted_content(&c.name, &c.location.location, &c.children, o, cx, sc);
    out.extend(slotted);
    out.push("const scopeToken = hasScopedStylesheets ? stylesheetScopeToken : undefined;".into());
    out.push(o.yd("Ctor[SYMBOL__GENERATE_MARKUP](null, childProps, childAttrs, scopeToken, contextfulParent, renderContext, shadowSlottedContent, lightSlottedContentMap, scopedSlottedContentMap)"));
    out.push("}".into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use lwc_shared::template_ir::{Literal, Root, SourceLocation, Text, TextValue};
    #[test]
    fn naming() {
        assert_eq!(kebabcase_to_camelcase_path("x-foo-bar"), "x/fooBar");
        assert_eq!(to_property_name("x-foo"), "xFoo");
    }
    #[test]
    fn component_sync_emit() {
        let lit = |s: &str| AttributeValue::Literal(Literal { value: LiteralValue::String(s.into()) });
        let c = Component {
            name: "x-foo".into(),
            properties: vec![Property { name: "myProp".into(), attribute_name: "my-prop".into(), value: lit("v"), location: SourceLocation::default() }],
            attributes: vec![Attribute { name: "class".into(), value: lit("a"), location: SourceLocation::default() }],
            children: vec![ChildNode::Text(Text { value: TextValue::Literal("hi".into()), raw: String::new(), location: SourceLocation::default() })],
            ..Default::default()
        };
        let root = Root { children: vec![ChildNode::Component(c)], ..Default::default() };
        let code = super::super::compile_module(&root, TemplateOptions::default(), &super::super::ModuleMeta::default());
        for n in ["ChildComponentCtor_xFoo", "from 'x/foo'", "childProps", "myProp", "class", "SYMBOL__GENERATE_MARKUP", "generateMarkup", "__lwcGenerateShadowSlottedContent_xFoo_0", "fallbackTmplNoYield"] {
            assert!(code.contains(n), "missing {n}: {code}");
        }
        assert!(!code.contains("yield ") && !code.contains("async"), "{code}");
        println!("{code}");
    }
    fn lit(s: &str) -> AttributeValue { AttributeValue::Literal(Literal { value: LiteralValue::String(s.into()) }) }
    fn txt(s: &str) -> ChildNode { ChildNode::Text(Text { value: TextValue::Literal(s.into()), raw: String::new(), location: SourceLocation::default() }) }
    fn cmp(children: Vec<ChildNode>) -> String {
        let c = Component { name: "x-foo".into(), children, ..Default::default() };
        super::super::compile_module(&Root { children: vec![ChildNode::Component(c)], ..Default::default() }, TemplateOptions::default(), &super::super::ModuleMeta::default())
    }
    #[test]
    fn light_map_named_slot() {
        let div = ChildNode::Element(lwc_shared::template_ir::Element { name: "div".into(), namespace: element::HTML_NAMESPACE.into(),
            attributes: vec![Attribute { name: "slot".into(), value: lit("footer"), location: SourceLocation::default() }],
            children: vec![txt("f")], ..Default::default() });
        let code = cmp(vec![div, txt("d")]);
        println!("{code}");
        assert!(code.contains("const lightSlottedContentMap = Object.create(null);"), "{code}");
        assert!(code.contains("const scopedSlottedContentMap = null;"), "{code}");
        assert!(code.contains("addSlottedContent('footer' ?? \"\", function __lwcGenerateSlottedContent(contextfulParent, slotAttributeValue) { let __lwcYield = ''; __lwcYield += '<div';"), "{code}");
        // Default light slot: text goes to '' slot. Shadow fn still gets all non-scoped children (with slot attr kept).
        assert!(code.contains("addSlottedContent('' ?? \"\""), "{code}");
        assert!(code.contains("<div") && code.contains("__lwcGenerateShadowSlottedContent_xFoo_0"), "{code}");
        assert!(code.contains("addSlottedContent") && !code.contains("yield ") && !code.contains("async"), "{code}");
    }
    #[test]
    fn scoped_fragment_map() {
        use lwc_shared::template_ir::{Expression, Identifier, ScopedSlotFragment};
        let f = ChildNode::ScopedSlotFragment(ScopedSlotFragment { slot_data: Expression::Identifier(Identifier { name: "item".into() }), slot_name: lit(""),
            children: vec![ChildNode::Text(Text { value: TextValue::Expression(Expression::Member(lwc_shared::template_ir::MemberExpression {
                object: Box::new(Expression::Identifier(Identifier { name: "item".into() })), property: Box::new(Expression::Identifier(Identifier { name: "v".into() })), computed: false })),
                raw: String::new(), location: SourceLocation::default() })],
            location: Default::default(), directive_location: Default::default() });
        let code = cmp(vec![f]);
        assert!(code.contains("const shadowSlottedContent = null;") && code.contains("const lightSlottedContentMap = null;"), "{code}");
        assert!(code.contains("const scopedSlottedContentMap = Object.create(null);"), "{code}");
        assert!(code.contains("function __lwcGenerateSlottedContent(contextfulParent, item, slotAttributeValue)") && code.contains("normalizeTextContent(item.v)") && code.contains("}, scopedSlottedContentMap);"), "{code}");
    }
}
