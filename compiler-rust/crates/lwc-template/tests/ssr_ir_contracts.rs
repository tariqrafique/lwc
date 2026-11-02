#![allow(unused)]
use lwc_template::*;
fn ssr_parse(src: &str, f: impl FnOnce(&mut Config)) -> TemplateParseResult {
    // Same Config shape lwc-ssr::transform_html passes (index.ts:83 of @lwc/ssr-compiler).
    let mut c = Config { name: Some("c".into()), namespace: Some("x".into()), ssr: Some(true), ..Default::default() };
    f(&mut c);
    lwc_template::parse(src, c).unwrap()
}
fn root(src: &str, f: impl FnOnce(&mut Config)) -> Root { ssr_parse(src, f).root.unwrap() }
fn lit(s: &str) -> AttributeValue { AttributeValue::Literal(Literal { value: LiteralValue::String(s.into()) }) }
fn id(s: &str) -> Expression { Expression::Identifier(Identifier { name: s.into() }) }
macro_rules! el { ($n:expr, $v:ident) => { match $n { ChildNode::$v(e) => e, other => panic!("expected {}, got {:?}", stringify!($v), other) } } }

#[test]
fn preserve_comments_directive() {
    let r = root("<template lwc:preserve-comments><!-- c --><p>x</p></template>", |_| {});
    assert_eq!(r.directives, vec![RootDirective::PreserveComments(true)]);
    assert!(matches!(&r.children[0], ChildNode::Comment(c) if c.value == " c "));
    let r = root("<template lwc:render-mode=\"light\"><slot></slot></template>", |_| {});
    assert_eq!(r.directives, vec![RootDirective::RenderMode(RenderMode::Light)]);
}

#[test]
fn lwc_directive_attrs() {
    let r = root("<template><div lwc:inner-html={h}></div><div lwc:dom=\"manual\"></div><x-c lwc:spread={s}></x-c><div lwc:on={o}></div></template>", |c| c.enable_lwc_on = Some(true));
    let d0 = el!(&r.children[0], Element); assert!(d0.attributes.is_empty()); assert_eq!(d0.directives, vec![ElementDirective::InnerHtml(AttributeValue::Expression(id("h")))]);
    let d1 = el!(&r.children[1], Element); assert!(d1.attributes.is_empty()); assert_eq!(d1.directives, vec![ElementDirective::Dom(LiteralValue::String("manual".into()))]);
    let d2 = el!(&r.children[2], Component); assert!(d2.attributes.is_empty()); assert_eq!(d2.directives, vec![ElementDirective::Spread(id("s"))]);
    let d3 = el!(&r.children[3], Element); assert!(d3.attributes.is_empty()); assert_eq!(d3.directives, vec![ElementDirective::OnDirective(id("o"))]);
    let r = root("<template lwc:render-mode=\"light\"><slot lwc:slot-bind={d}></slot></template>", |_| {});
    let s = el!(&r.children[0], Slot); assert!(s.attributes.is_empty()); assert_eq!(s.directives, vec![ElementDirective::SlotBind(id("d"))]);
}

#[test]
fn lwc_component_node() {
    let r = root("<template><lwc:component lwc:is={ctor} foo-bar=\"x\"></lwc:component></template>", |c| c.enable_dynamic_components = Some(true));
    let l = el!(&r.children[0], LwcComponent);
    assert_eq!(l.name, "lwc:component"); assert!(l.attributes.is_empty());
    assert_eq!(l.directives, vec![ElementDirective::Is(id("ctor"))]);
    assert_eq!(l.properties.iter().map(|p| (p.name.as_str(), p.attribute_name.as_str())).collect::<Vec<_>>(), vec![("fooBar", "foo-bar")]);
}

#[test]
fn component_prop_split() {
    let r = root("<template><x-c role=\"r\" id=\"i\" aria-label=\"a\" tabindex=\"0\" exportparts=\"p\" data-x=\"d\" foo-bar=\"f\" class=\"c\"></x-c></template>", |_| {});
    let c = el!(&r.children[0], Component);
    assert_eq!(c.attributes.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), vec!["exportparts", "data-x", "class"]);
    assert_eq!(c.properties.iter().map(|p| (p.name.as_str(), p.attribute_name.as_str())).collect::<Vec<_>>(),
      vec![("role", "role"), ("id", "id"), ("ariaLabel", "aria-label"), ("tabIndex", "tabindex"), ("fooBar", "foo-bar")]);
}

#[test]
fn clean_text_node() {
    let r = root("<template><p>Checked: <input></p><p>\n    a:\n    {b}  c\n</p></template>", |_| {});
    let p0 = el!(&r.children[0], Element);
    assert_eq!(el!(&p0.children[0], Text).value, TextValue::Literal("Checked: ".into()));
    let p1 = el!(&r.children[1], Element);
    let v: Vec<_> = p1.children.iter().map(|c| el!(c, Text).value.clone()).collect();
    assert_eq!(v, vec![TextValue::Literal("a: ".into()), TextValue::Expression(id("b")), TextValue::Literal("  c".into())]);
}

#[test]
fn valueless_attr_bool() {
    let r = root("<template><input hidden spellcheck title=\"\"><x-c accesskey></x-c></template>", |_| {});
    let i = el!(&r.children[0], Element);
    let t = AttributeValue::Literal(Literal { value: LiteralValue::Bool(true) });
    assert_eq!(i.attributes[0].value, t); assert_eq!(i.attributes[1].value, t); assert_eq!(i.attributes[2].value, lit(""));
    let c = el!(&r.children[1], Component);
    assert_eq!((c.properties[0].name.as_str(), &c.properties[0].value), ("accessKey", &t));
}

#[test]
fn empty_ns_prefix() {
    let r = root("<template><svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"></svg></template>", |_| {});
    let s = el!(&r.children[0], Element);
    assert_eq!(s.attributes.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), vec!["xmlns", "xmlns:xlink"]);
}

#[test]
fn complex_expr_literal() {
    let r = root("<template><div class=\"{bar() + foo()}\"></div>{bar() + \' \' + foo()}</template>", |c| c.experimental_complex_expressions = Some(true));
    let d = el!(&r.children[0], Element);
    assert_eq!(d.attributes[0].value, AttributeValue::Expression(Expression::Complex("bar() + foo()".into())));
    assert!(matches!(&el!(&r.children[1], Text).value,
      TextValue::Expression(Expression::Complex(s)) | TextValue::Complex(s) if s == "bar() + \' \' + foo()"));
}

#[test]
fn complex_for_each() {
    let p = ssr_parse("<template><ul><template for:each=\"{list()}\" for:item=\"item\" for:index=\"index\"><li key={item}>{item}</li></template></ul></template>", |c| c.experimental_complex_expressions = Some(true));
    assert!(p.warnings.iter().all(|w| w.code != Some(1153)), "{:?}", p.warnings);
    let r = p.root.unwrap(); let ul = el!(&r.children[0], Element);
    let f = el!(&ul.children[0], ForEach);
    assert_eq!(f.expression, Expression::Complex("list()".into()));
    assert_eq!(f.item.name, "item"); assert_eq!(f.index.as_ref().unwrap().name, "index");
}
