//! Rust port of `@lwc/template-compiler`.
//!
//! - HTML parse: `tariqrafique/parse5-rust` (pending GitHub)
//! - Emit: ESM only (no AMD)
//! - Parity: behavioral / new goldens (AST + warnings + render semantics), not astring bytes
//!
//! Phase 0 harness entrypoints: [`compile`] and [`parse`].

mod codegen;
pub mod config;
mod parser;
mod scope_tokens;

pub use scope_tokens::{generate_scope_tokens, ScopeTokens};

pub use lwc_shared::template_ir::*;
/// Alias used by callers.
pub type TemplateConfig = Config;
pub use config::{Config, ConfigError, CustomRendererConfig, CustomRendererElementConfig, NormalizedConfig};
pub use lwc_shared::{CompilerDiagnostic, DiagnosticLevel};
pub use parser::TemplateParseResult;

/// Result of [`compile`].
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateCompileResult {
    pub code: String,
    pub root: Option<Root>,
    pub warnings: Vec<CompilerDiagnostic>,
    pub css_scope_tokens: Vec<String>,
}

/// Parse HTML markup into an AST (no codegen).
pub fn parse(source: &str, config: Config) -> Result<TemplateParseResult, ConfigError> {
    let options = config.normalize()?;
    Ok(parser::parse_template(source, &options))
}

/// Compile an LWC template to ESM source consumable by the engine.
///
/// `filename` is required for implicit CSS imports and style tokens (same as TS).
pub fn compile(
    source: &str,
    filename: &str,
    config: Config,
) -> Result<TemplateCompileResult, ConfigError> {
    let options = config.normalize()?;
    let tokens = generate_scope_tokens(filename, options.namespace.as_deref(), options.name.as_deref());
    let parsing = parser::parse_template(source, &options);

    let has_error = parsing
        .warnings
        .iter()
        .any(|w| matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal));

    let (code, root) = if !has_error {
        if let Some(ref root) = parsing.root {
            (codegen::generate(root, &options, filename, &tokens.scope_token), parsing.root)
        } else {
            (String::new(), None)
        }
    } else {
        (String::new(), parsing.root)
    };

    Ok(TemplateCompileResult {
        code,
        root,
        warnings: parsing.warnings,
        css_scope_tokens: tokens.css_scope_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "<template>\n    foo\n</template>\n";

    #[test]
    fn compile_empty_source() {
        let r = compile("", "x-foo.html", Config::default()).unwrap();
        assert!(r.code.is_empty());
        assert!(r.root.is_none());
    }

    #[test]
    fn parse_static_text() {
        let r = parse(SRC, Config::default()).unwrap();
        let root = r.root.expect("root");
        assert_eq!(root.children.len(), 1);
        match &root.children[0] {
            ChildNode::Text(Text { value: TextValue::Literal(s), raw, .. }) => {
                assert_eq!(s, "foo");
                assert_eq!(raw, "\n    foo\n");
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn compile_static_text() {
        let r = compile(SRC, "x-foo.html", Config::default()).unwrap();
        assert!(r.code.contains("api_text(\"foo\")"), "{}", r.code);
        assert!(r.code.contains("registerTemplate(tmpl)"));
        assert!(r.code.contains("freezeTemplate(tmpl);"));
    }

    #[test]
    fn js_escape() {
        assert_eq!(codegen::js_string("a\"b\\\n\u{2028}"), "\"a\\\"b\\\\\\n\\u2028\"");
    }

    const SECTION: &str = "<template>\n    <section class=\"foo bar   baz-fiz\"></section>\n</template>\n";

    #[test]
    fn parse_section_class() {
        let root = parse(SECTION, Config::default()).unwrap().root.unwrap();
        assert_eq!(root.children.len(), 1);
        let ChildNode::Element(e) = &root.children[0] else { panic!("{:?}", root.children) };
        assert_eq!(e.name, "section");
        assert_eq!(e.attributes.len(), 1);
        assert_eq!(e.attributes[0].name, "class");
        assert_eq!(e.attributes[0].value, AttributeValue::Literal(Literal { value: LiteralValue::String("foo bar baz-fiz".into()) }));
    }

    #[test]
    fn compile_section_class() {
        let c = compile(SECTION, "x.html", Config { enable_static_content_optimization: Some(false), ..Default::default() }).unwrap().code;
        for n in ["api_element(\"section\"", "class: \"foo bar baz-fiz\"", "key: 0", "h: api_element"] {
            assert!(c.contains(n), "{n}\n{c}");
        }
        if std::env::var("SHOW").is_ok() { eprintln!("{c}");
        }
    }

    #[test]
    fn compile_section_no_static_opt() {
        let cfg = Config { enable_static_content_optimization: Some(false), ..Default::default() };
        let c = compile(SECTION, "x.html", cfg).unwrap().code;
        assert_ne!(c, compile(SECTION, "x.html", Config::default()).unwrap().code);
        assert!(c.contains("api_element(\"section\"") && !c.contains("parseFragment"));
    }

    #[test]
    fn compile_nested_text() {
        let c = compile("<template>\n    <p>hello</p>\n</template>\n", "x.html", Config::default()).unwrap().code;
        assert!(c.contains("parseFragment`<p${3}>hello</p>`"), "{c}");
        let c = compile("<template>\n    <p>hello</p>\n</template>\n", "x.html", Config { enable_static_content_optimization: Some(false), ..Default::default() }).unwrap().code;
        assert!(c.contains("api_element(\"p\"") && c.contains("api_text(\"hello\")"), "{c}");
    }

    #[test]
    fn compile_binding_attr() {
        let c = cn("<template>\n    <a href={narita}>KIX</a>\n</template>\n");
        assert!(c.contains("href: $cmp.narita") && c.contains("api_text(\"KIX\")"), "{c}");
    }

    #[test]
    fn compile_dynamic_text_and_keys() {
        // Static opt off: `<div><span>{x}</span></div>` is a static fragment with a text part under JS rules.
        let c = cn("<template><div><span>{x}</span></div><x-foo></x-foo></template>");
        assert!(c.contains("api_text(api_dynamic_text($cmp.x))") && c.contains("d: api_dynamic_text"), "{c}");
        assert!(c.contains("key: 0") && c.contains("key: 1") && c.contains("key: 2"), "{c}");
    }

    fn c(src: &str) -> String { compile(src, "x.html", Config::default()).unwrap().code }
    /// Static content optimization off: nested static elements stay `api_element`.
    fn cn(src: &str) -> String { compile(src, "x.html", Config { enable_static_content_optimization: Some(false), ..Default::default() }).unwrap().code }

    const MIXED: &str = "<template>\n  <div>\n    <section class=\"s\"><p>hi</p></section>\n    {msg}\n  </div>\n</template>\n";

    #[test]
    fn static_fragment_dynamic_text_part() {
        // JS oracle: `<p>{x}</p>` → static fragment with a text part (partId 1 = the text run).
        let code = c("<template><p>{x}</p></template>");
        for n in ["import { freezeTemplate, parseFragment, registerTemplate } from \"lwc\";", "const $fragment1 = parseFragment`<p${3}>${\"t1\"}</p>`;",
                  "api_static_fragment($fragment1, 1, [api_static_part(1, null, api_dynamic_text($cmp.x))])", "d: api_dynamic_text", "sp: api_static_part"] {
            assert!(code.contains(n), "{n}\n{code}");
        }
    }

    #[test]
    fn static_fragment_mixed_text_runs() {
        // Contiguous text groups into one part; element/text pre-order part ids (div0 p1 t2 span3 c4 t5).
        let code = c("<template><div><p>a {x} b<span>c</span>{y}</p></div></template>");
        for n in ["parseFragment`<div${3}><p${3}>${\"t2\"}<span${3}>c</span>${\"t5\"}</p></div>`",
                  "[api_static_part(2, null, \"a \" + api_dynamic_text($cmp.x) + \" b\"), api_static_part(5, null, api_dynamic_text($cmp.y))]"] {
            assert!(code.contains(n), "{n}\n{code}");
        }
    }

    #[test]
    fn static_fragment_attr_event_ref_parts_and_serializer() {
        let code = c("<template><div class={c} style={s} data-x={dx} onclick={h} lwc:ref=\"r\"><b>t</b></div></template>");
        for n in ["parseFragment`<div${\"c0\"}${\"s0\"}${\"a0:data-x\"}${2}><b${3}>t</b></div>`", "ref: \"r\"", "className: api_normalize_class_name($cmp.c)", "style: $cmp.s", "\"data-x\": $cmp.dx", "tmpl.hasRefs = true;"] {
            assert!(code.contains(n), "{n}\n{code}");
        }
        let code = c("<template><p class=\"  a  b \" style=\"color: red ; ;x:y !important\" spellcheck=\"FALSE\">it's \"q\" &amp; &lt;</p></template>");
        assert!(code.contains("parseFragment`<p class=\"a b${0}\" style=\"color: red; x: y !important;\" spellcheck=\"false\"${2}>it&#x27;s &quot;q&quot; &amp; &lt;</p>`"), "{code}");
        // iframe / properties-free rule: iframe is never static.
        assert!(!c("<template><iframe></iframe></template>").contains("parseFragment"));
    }

    #[test]
    fn dashed_builtin_tag_is_not_a_component() {
        // TS isCustomElementTag excludes DASHED_TAGNAME_ELEMENT_SET (fixture regression/dashed-html-tag).
        let code = c("<template><section><color-profile local=\"x\"></color-profile></section></template>");
        assert!(code.contains("parseFragment`<section${3}><color-profile local=\"x\"${3}></color-profile></section>`") && !code.contains("color/profile"), "{code}");
    }

    #[test]
    fn static_fragment_in_scoped_slot_uses_locals() {
        let code = c("<template><x-child><template lwc:slot-data=\"d\"><p>{d.name}</p></template></x-child></template>");
        assert!(code.contains("const $fragment1 = parseFragment`<p${3}>${\"t1\"}</p>`;"), "{code}");
        assert!(code.contains("[api_static_part(1, null, api_dynamic_text(d.name))]"), "{code}");
    }

    #[test]
    fn nested_static_under_dynamic_parent() {
        let code = c(MIXED);
        // JS oracle: dynamic text no longer breaks the fragment — the whole `<div>` is one static fragment.
        for n in ["import { freezeTemplate, parseFragment, registerTemplate } from \"lwc\";", "const $fragment1 = parseFragment`<div${3}><section class=\"s${0}\"${2}><p${3}>hi</p></section>${\"t4\"}</div>`;", "api_static_part(4, null, api_dynamic_text($cmp.msg))", "st: api_static_fragment"] {
            assert!(code.contains(n), "{n}\n{code}");
        }
        assert!(!code.contains("api_element(") && !code.contains("$fragment2"), "{code}");
        // A component sibling keeps the parent dynamic; the static subtree under it is still a fragment.
        let code = c("<template><div><section class=\"s\"><p>hi</p></section><x-foo></x-foo></div></template>");
        assert!(code.contains("api_element(\"div\"") && code.contains("const $fragment1 = parseFragment`<section class=\"s${0}\"${2}><p${3}>hi</p></section>`;"), "{code}");
        println!("{code}");
        let off = cn(MIXED);
        assert!(!off.contains("parseFragment") && off.contains("api_element(\"section\""), "{off}");
    }

    #[test]
    fn nested_all_static_is_one_root_fragment() {
        let code = c("<template><div><section><p>a</p></section></div></template>");
        assert_eq!(code.matches("parseFragment`").count(), 1, "{code}");
        assert!(code.contains("return [api_static_fragment($fragment1, 1)];"), "{code}");
    }

    #[test]
    fn static_child_under_if_uses_fragment() {
        let src = "<template><h1 if:true={x}><b>a</b></h1><template if:true={y}><p>hi</p><i>x</i></template></template>";
        let code = c(src);
        assert!(code.contains("$cmp.x ?") && code.contains("$cmp.y ?") && code.contains("const $fragment1 = parseFragment`<h1${3}><b${3}>a</b></h1>`;") && code.contains("const $fragment2 = parseFragment`<p${3}>hi</p>`;") && code.contains("$fragment3"), "{code}");
        assert!(!cn(src).contains("parseFragment"));
    }


    #[test]
    fn compile_if_true() {
        let code = cn("<template>\n    <h1 if:true={ifTrue}></h1>\n</template>\n");
        assert!(code.contains("$cmp.ifTrue ?") && code.contains("api_element(\"h1\"") && code.contains(": null"), "{code}");
        assert!(!code.contains("if:true"), "{code}");
    }

    #[test]
    fn compile_if_false() {
        let code = c("<template>\n    <h1 if:false={hide}></h1>\n</template>\n");
        assert!(code.contains("!$cmp.hide ?"), "{code}");
    }

    #[test]
    fn compile_if_both_keeps_true() {
        let code = c("<template>\n    <h1 if:true={ifTrue} if:false={ifFalse}></h1>\n</template>\n");
        assert!(code.contains("$cmp.ifTrue") && !code.contains("ifFalse"), "{code}");
    }

    const NESTED_IF: &str = "<template>\n    <section>\n        <template if:true={isTrue}>\n            <p>hi</p>\n        </template>\n    </section>\n</template>\n";

    #[test]
    fn parse_nested_template_if() {
        let root = parse(NESTED_IF, Config::default()).unwrap().root.unwrap();
        let ChildNode::Element(s) = &root.children[0] else { panic!() };
        assert_eq!(s.name, "section");
        let ChildNode::If(f) = &s.children[0] else { panic!("{:?}", s.children) };
        assert_eq!(f.modifier, "true");
        assert!(matches!(&f.children[..], [ChildNode::Element(p)] if p.name == "p"), "{:?}", f.children);
    }

    #[test]
    fn compile_nested_template_if() {
        let code = cn(NESTED_IF);
        assert!(code.contains("$cmp.isTrue ?") && code.contains("api_element(\"p\"") && code.contains("api_text(\"hi\")"), "{code}");
    }

    #[test]
    fn member_attr() {
        let src = "<template>\n  <p title={foo.bar}></p>\n</template>\n";
        let root = parse(src, Config::default()).unwrap().root.unwrap();
        let ChildNode::Element(p) = &root.children[0] else { panic!() };
        assert!(matches!(&p.attributes[0].value, AttributeValue::Expression(Expression::Member(m)) if !m.computed), "{:?}", p.attributes);
        assert!(cn(src).contains("title: $cmp.foo.bar"), "{}", cn(src));
    }

    const FOR_EACH: &str = "<template>\n  <section>\n    <div key={item.id} class=\"my-list\" for:each={items} for:item=\"item\">\n      <p>{item}</p>\n    </div>\n  </section>\n</template>\n";

    #[test]
    fn parse_for_each() {
        let root = parse(FOR_EACH, Config::default()).unwrap().root.unwrap();
        let ChildNode::Element(s) = &root.children[0] else { panic!() };
        let ChildNode::ForEach(f) = &s.children[0] else { panic!("{:?}", s.children) };
        assert_eq!(f.item.name, "item");
        assert!(f.index.is_none());
        assert_eq!(f.expression, Expression::Identifier(Identifier { name: "items".into() }));
        let [ChildNode::Element(d)] = &f.children[..] else { panic!() };
        assert_eq!(d.name, "div");
        assert!(matches!(&d.directives[..], [ElementDirective::Key(Expression::Member(_))]));
        assert_eq!(d.attributes.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["class"]);
    }

    #[test]
    fn compile_for_each() {
        // Static opt off (with it on, JS emits the `<div key>` row as a static fragment; see below).
        let code = cn(FOR_EACH);
        for n in ["api_iterator($cmp.items, function (item)", "api_key(", "item.id", "api_dynamic_text(item)", "class: \"my-list\"", "i: api_iterator", "k: api_key"] {
            assert!(code.contains(n), "{n}\n{code}");
        }
        assert!(!code.contains("$cmp.item.") && !code.contains("$cmp.item)") && !code.contains("for:"), "{code}");
    }

    #[test]
    fn compile_for_index() {
        let code = c("<template>\n  <div for:each={items} for:item=\"item\" for:index=\"i\" key={item.id}>\n    <span>{item}{i}</span>\n  </div>\n</template>\n");
        // `key={i}` (the for:index) is LWC1065 upstream (KEY_SHOULD_NOT_USE_INDEX).
        assert!(code.contains("function (item, i)") && code.contains("api_key(0, item.id)"), "{code}");
    }

    #[test]
    fn nested_template_for_each_and_iterator() {
        let root = parse("<template><ul><template for:each={xs} for:item=\"x\"><li key={x.k}>{x.v}</li></template></ul><template iterator:it={xs}><p key={it.value.id}>{it.index}</p></template></template>", Config::default()).unwrap().root.unwrap();
        let ChildNode::Element(ul) = &root.children[0] else { panic!() };
        assert!(matches!(&ul.children[0], ChildNode::ForEach(f) if matches!(&f.children[..], [ChildNode::Element(li)] if li.name == "li")));
        let ChildNode::ForOf(o) = &root.children[1] else { panic!("{:?}", root.children) };
        assert_eq!(o.iterator.name, "it");
        let code = c("<template><template iterator:it={xs}><p key={it.value.id}>{it.index}</p></template></template>");
        assert!(code.contains("function (itValue, itIndex, itFirst, itLast)") && code.contains("api_key(0, it.value.id)") && code.contains("api_dynamic_text(it.index)") && !code.contains("$cmp.it"), "{code}");
    }

    const LWC_IF_ELSE: &str = "<template>\n    <h1 lwc:if={visible}>Visible Header</h1>\n    <h1 lwc:else>Alternative Header</h1>\n</template>\n";

    #[test]
    fn lwc_if_else_elements() {
        let root = parse(LWC_IF_ELSE, Config::default()).unwrap().root.unwrap();
        let [ChildNode::IfBlock(b)] = &root.children[..] else { panic!("{:?}", root.children) };
        assert_eq!(b.condition, Expression::Identifier(Identifier { name: "visible".into() }));
        let [ChildNode::Element(h)] = &b.children[..] else { panic!() };
        assert!(h.attributes.is_empty(), "{:?}", h.attributes);
        assert!(matches!(b.else_.as_deref(), Some(ElseBranch::Else(e)) if matches!(&e.children[..], [ChildNode::Element(h)] if h.attributes.is_empty())));
        let code = cn(LWC_IF_ELSE);
        for n in ["$cmp.visible ?", " : api_element", "api_text(\"Visible Header\")", "api_text(\"Alternative Header\")", "key: 0", "key: 1"] {
            assert!(code.contains(n), "{n}\n{code}");
        }
        assert!(!code.contains("lwc:"), "{code}");
    }

    #[test]
    fn lwc_if_elseif_else_chain() {
        let src = "<template>\n    <h1 lwc:if={visible}>A</h1>\n    <h1 lwc:elseif={other}>B</h1>\n    <h1 lwc:else>C</h1>\n</template>\n";
        let root = parse(src, Config::default()).unwrap().root.unwrap();
        let [ChildNode::IfBlock(b)] = &root.children[..] else { panic!("{:?}", root.children) };
        let Some(ElseBranch::Elseif(ei)) = b.else_.as_deref() else { panic!() };
        assert_eq!(ei.condition, Expression::Identifier(Identifier { name: "other".into() }));
        assert!(matches!(ei.else_.as_deref(), Some(ElseBranch::Else(_))));
        let code = cn(src);
        assert!(code.contains("$cmp.visible ?") && code.contains(": $cmp.other ?") && code.contains("key: 2") && code.contains("api_text(\"C\")"), "{code}");
    }

    #[test]
    fn lwc_if_else_nested_templates() {
        let src = "<template>\n    <template lwc:if={visible}><p>yes</p></template>\n    <template lwc:else><p>no</p></template>\n</template>\n";
        let root = parse(src, Config::default()).unwrap().root.unwrap();
        let [ChildNode::IfBlock(b)] = &root.children[..] else { panic!("{:?}", root.children) };
        assert!(matches!(&b.children[..], [ChildNode::Element(p)] if p.name == "p"));
        assert!(matches!(b.else_.as_deref(), Some(ElseBranch::Else(e)) if matches!(&e.children[..], [ChildNode::Element(p)] if p.name == "p")));
        let code = cn(src);
        assert!(code.contains("$cmp.visible ?") && code.contains("api_text(\"yes\")") && code.contains("api_text(\"no\")"), "{code}");
    }

    #[test]
    fn lwc_if_without_else_is_null() {
        let code = c("<template><p lwc:if={x}>a</p></template>");
        assert!(code.contains("$cmp.x ?") && code.contains(": null"), "{code}");
    }

    fn first_slot(src: &str) -> lwc_shared::template_ir::Slot {
        use lwc_shared::template_ir::ChildNode;
        fn find(cs: &[ChildNode]) -> Option<lwc_shared::template_ir::Slot> {
            cs.iter().find_map(|c| match c {
                ChildNode::Slot(s) => Some(s.clone()),
                ChildNode::Element(e) => find(&e.children),
                ChildNode::Component(e) => find(&e.children),
                _ => None,
            })
        }
        find(&parse(src, Config::default()).unwrap().root.unwrap().children).expect("slot")
    }

    #[test]
    fn slot_default() {
        let src = "<template>\n  <section>\n    <slot><p>Default slot content</p></slot>\n  </section>\n</template>";
        assert_eq!(first_slot(src).slot_name, "");
        let code = cn(src);
        assert!(code.contains("api_slot(\"\"") && code.contains("tmpl.slots = [\"\"];") && code.contains("api_text(\"Default slot content\")") && code.contains("s: api_slot") && code.contains("$slotset)"), "{code}");
    }

    #[test]
    fn slot_named() {
        let src = "<template>\n  <slot name=\"footer\"></slot>\n</template>";
        assert_eq!(first_slot(src).slot_name, "footer");
        let code = c(src);
        assert!(code.contains("api_slot(\"footer\"") && code.contains("tmpl.slots = [\"footer\"];"), "{code}");
    }

    #[test]
    fn event_listener() {
        let code = cn("<template>\n  <div onclick={handleClick}>x</div>\n</template>");
        assert!(code.contains("on: {") && code.contains("click: api_bind($cmp.handleClick)") && code.contains("api_text(\"x\")") && code.contains("b: api_bind") && !code.contains("attrs"), "{code}");
    }

    #[test]
    fn custom_element() {
        let code = c("<template>\n  <x-foo></x-foo>\n</template>");
        assert!(code.contains("import _xFoo from \"x/foo\";") && code.contains("api_custom_element(\"x-foo\", _xFoo") && code.contains("c: api_custom_element") && !code.contains("h: api_element"), "{code}");
    }

    #[test]
    fn custom_element_import_camel_case() {
        let code = c("<template>\n  <c-custom-else></c-custom-else><x-foo-bar></x-foo-bar>\n</template>");
        assert!(code.contains("import _cCustomElse from \"c/customElse\";") && code.contains("import _xFooBar from \"x/fooBar\";"), "{code}");
    }

    #[test]
    fn diag_1083_expression_parse_error() {
        let e = first_err("<template>\n    <div>{ (foo }</div>\n</template>");
        assert_eq!(e.code, Some(1083));
        assert_eq!(first_err("<template>\n    { }\n</template>").code, Some(1083));
    }

    #[test]
    fn diag_1196_prohibited_node() {
        let cfg = Config { experimental_complex_expressions: Some(true), ..Default::default() };
        let r = parse("<template><x-pert foo=\"{this.bar}\"></x-pert></template>", cfg).unwrap();
        assert_eq!(r.warnings.iter().find(|d| d.level == DiagnosticLevel::Error).and_then(|d| d.code), Some(1196));
    }

    #[test]
    fn custom_element_dedup_and_slot_child() {
        let code = c("<template><x-foo><slot></slot></x-foo><x-foo></x-foo></template>");
        assert_eq!(code.matches("import _xFoo").count(), 1, "{code}");
        assert!(code.contains("api_slot(\"\"") && code.contains("tmpl.slots = [\"\"];"), "{code}");
    }

    #[test]
    fn component_property_camel_case() {
        use lwc_shared::template_ir::ChildNode;
        let src = "<template>\n  <x-foo my-prop={val} class=\"c\"></x-foo>\n</template>";
        let root = parse(src, Config::default()).unwrap().root.unwrap();
        let ChildNode::Component(cmp) = &root.children[0] else { panic!("{:?}", root.children) };
        assert_eq!(cmp.properties.len(), 1);
        assert_eq!((cmp.properties[0].name.as_str(), cmp.properties[0].attribute_name.as_str()), ("myProp", "my-prop"));
        assert_eq!(cmp.attributes.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["class"]);
        let code = c(src);
        assert!(code.contains("props: {\n") && code.contains("myProp: $cmp.val,") && code.contains("attrs: {\n") && code.contains("class: \"c\","), "{code}");
    }

    #[test]
    fn element_keeps_attributes() {
        let code = cn("<template><div my-prop={val} title=\"t\"></div></template>");
        assert!(!code.contains("props") && code.contains("\"my-prop\": $cmp.val") && code.contains("title: \"t\""), "{code}");
    }

    #[test]
    fn slot_attr_on_child() {
        let code = c("<template>\n  <x-foo>\n    <p slot=\"footer\">f</p>\n  </x-foo>\n</template>");
        assert!(code.contains("slot: \"footer\"") && code.contains("api_element(\"p\""), "{code}");
    }

    #[test]
    fn compile_rich_smoke() {
        let code = c("<template>\n  <section class=\"wrap\">\n    <x-foo onclick={onFoo} title={msg}>\n      <slot></slot>\n    </x-foo>\n  </section>\n</template>");
        for n in ["api_element(\"section\"", "api_custom_element(\"x-foo\"", "import _xFoo from \"x/foo\"", "api_bind($cmp.onFoo)", "api_slot(\"\"", "props: {", "title: $cmp.msg", "tmpl.slots = [\"\"];"] {
            assert!(code.contains(n), "missing {n}\n{code}");
        }
        println!("{code}");
    }

    #[test]
    fn lwc_ref_on_div() {
        let src = "<template>\n  <div lwc:ref=\"foo\">Foo</div>\n</template>";
        let root = parse(src, Config::default()).unwrap().root.unwrap();
        let ChildNode::Element(d) = &root.children[0] else { panic!() };
        assert!(d.attributes.is_empty() && d.directives == [ElementDirective::Ref("foo".into())], "{d:?}");
        let code = cn(src);
        assert!(code.contains("ref: \"foo\",") && code.contains("tmpl.hasRefs = true;") && !code.contains("lwc:ref"), "{code}");
        let bad = c("<template><div lwc:ref></div><p lwc:ref={x}></p></template>");
        assert!(!bad.contains("ref:") && !bad.contains("hasRefs") && !bad.contains("lwc:ref"), "{bad}");
        println!("{code}");
    }

    #[test]
    fn compile_scope_tokens() {
        let cfg = Config { namespace: Some("x".into()), name: Some("valid".into()), ..Default::default() };
        let r = compile("<template><p>a</p></template>", "valid.html", cfg).unwrap();
        let t = generate_scope_tokens("valid.html", Some("x"), Some("valid"));
        assert_eq!(r.css_scope_tokens, t.css_scope_tokens);
        assert_eq!(r.css_scope_tokens.len(), 4);
        assert!(r.code.contains(&format!("tmpl.stylesheetToken = \"{}\";", t.scope_token)), "{}", r.code);
        assert_eq!(t.legacy_scope_token, "x-valid_valid");
    }
    #[test]
    fn implicit_css_imports() {
        let r = compile("<template>\n    foo\n</template>", "text.html", Config { namespace: Some("x".into()), name: Some("text".into()), ..Default::default() }).unwrap();
        assert_eq!(r.css_scope_tokens[0], "lwc-3q20iingih8");
        assert!(r.code.contains("\"./text.css\"") && r.code.contains("\"./text.scoped.css?scoped=true\""), "{}", r.code);
        assert!(r.code.contains("tmpl.stylesheets.push.apply(tmpl.stylesheets, _implicitScopedStylesheets);"));
    }

    #[test]
    fn static_fragment_base_class() {
        let r = compile(SECTION, "class.html", Config { namespace: Some("x".into()), name: Some("class".into()), ..Default::default() }).unwrap();
        let c = &r.code;
        for n in ["import { freezeTemplate, parseFragment, registerTemplate } from \"lwc\";", "const $fragment1 = parseFragment`<section class=\"foo bar baz-fiz${0}\"${2}></section>`;", "const { st: api_static_fragment } = $api;", "return [api_static_fragment($fragment1, 1)];", "\"./class.css\"", "\"./class.scoped.css?scoped=true\""] {
            assert!(c.contains(n), "missing {n}\n{c}");
        }
        assert!(!c.contains("api_element"));
        assert_eq!(r.css_scope_tokens[0], "lwc-44l3bs9cc8q");
        assert_eq!(generate_scope_tokens("class.html", Some("x"), Some("class")).css_scope_tokens[0], "lwc-44l3bs9cc8q");
        println!("{c}");
    }

    #[test]
    fn dynamic_forces_api_element() {
        // JS `isStaticNode`: components, iframes, `slot` attrs, props and non-Key/Ref directives force api_element.
        // TODO: lwc:dom is not lowered to ElementDirective::Dom by the parser yet, so it is not covered here.
        for src in ["<template><div><x-foo></x-foo></div></template>", "<template><iframe></iframe></template>", "<template><lwc:component lwc:is={c} lwc:ref=\"r\">Foo</lwc:component></template>"] {
            let code = c(src);
            assert!(!code.contains("parseFragment"), "{src}\n{code}");
        }
        // ...while listeners, lwc:ref and dynamic text become static parts (JS oracle).
        for src in ["<template><div onclick={f}></div></template>", "<template><div lwc:ref=\"r\"></div></template>", "<template><p>{x}</p></template>"] {
            let code = c(src);
            assert!(code.contains("parseFragment") && code.contains("api_static_part(0") || code.contains("api_static_part(1"), "{src}\n{code}");
        }
    }

    fn first_err(src: &str) -> CompilerDiagnostic {
        let r = compile(src, "x.html", Config::default()).unwrap();
        assert!(r.code.is_empty(), "no codegen on error");
        r.warnings.into_iter().find(|w| w.level == DiagnosticLevel::Error).expect("error diag")
    }

    #[test]
    fn diag_1127_unknown_lwc_directive() {
        let d = first_err("<template>\n  <div lwc:foo=\"bar\"></div>\n</template>");
        println!("{d:?}");
        assert_eq!((d.code, d.location.unwrap().start_line), (Some(1127), 2));
        assert!(d.message.contains("lwc:foo") && d.message.contains("<div>"));
    }

    #[test]
    fn diag_1080_root_unknown_attrs() {
        let d = first_err("<template foo=\"bar\">\n  <div></div>\n</template>");
        assert_eq!((d.code, d.location.unwrap().start_line), (Some(1080), 1));
        assert!(compile("<template lwc:render-mode=\"light\"><div></div></template>", "x.html", Config::default()).unwrap().warnings.is_empty());
    }

    #[test]
    fn diag_1051_forbidden_tag() {
        let d = first_err("<template>\n\n  <meta charset=\"utf-8\">\n</template>");
        assert_eq!((d.code, d.location.unwrap().start_line), (Some(1051), 3));
    }

    #[test]
    fn diag_1058_invalid_html() {
        let d = first_err("<template>\n  <div a=\"1\" a=\"2\"></div>\n</template>");
        assert_eq!((d.code, d.location.unwrap().start_line), (Some(1058), 2));
        assert!(d.message.contains("duplicate-attribute"));
    }

    #[test]
    fn diag_1165_orphan_else() {
        let d = first_err("<template>\n  <div lwc:elseif={x}></div>\n</template>");
        assert_eq!((d.code, d.location.unwrap().start_line), (Some(1165), 2));
        let d = first_err("<template>\n  <p></p>\n  <div lwc:else></div>\n</template>");
        assert_eq!((d.code, d.location.unwrap().start_line), (Some(1165), 3));
    }

    fn warns(src: &str, cfg: Config) -> Vec<u32> {
        let r = compile(src, "x.html", cfg).unwrap();
        assert!(!r.code.is_empty(), "warnings must not fail compile: {:?}", r.warnings);
        r.warnings.iter().filter(|w| w.level == DiagnosticLevel::Warning).map(|w| w.code.expect("coded warning")).collect()
    }

    #[test]
    fn warn_invalid_html_attribute() {
        assert_eq!(warns("<template><div href=\"x\" title=\"t\" data-x=\"1\" aria-label=\"a\"></div></template>", Config::default()), vec![1057]);
        assert!(warns("<template><a href=\"x\"></a><x-foo bar=\"1\"></x-foo></template>", Config::default()).is_empty());
    }

    #[test]
    fn warn_unknown_tag() {
        assert_eq!(warns("<template><foo></foo><div></div><x-bar></x-bar></template>", Config::default()), vec![1123]);
    }

    /// `@lwc/template-compiler` exposes `experimentalComputedMemberExpression` (the facade does not).
    #[test]
    fn computed_member_expression_flag() {
        let src = "<template>{val[state.foo]}</template>";
        let on = compile(src, "x.html", Config { experimental_computed_member_expression: Some(true), ..Default::default() }).unwrap();
        assert!(on.warnings.iter().all(|w| !matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal)), "{:?}", on.warnings);
        assert!(!on.code.is_empty());
        let off = compile(src, "x.html", Config { experimental_computed_member_expression: Some(false), ..Default::default() }).unwrap();
        let err = off.warnings.iter().find(|w| matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal)).expect("LWC1038 error");
        assert_eq!(err.code, Some(1038));
    }

    #[test]
    fn instrumentation_counts_directives() {
        use lwc_shared::{CompilerMetric, Instrumentation, InstrumentationHandle};
        use std::sync::Mutex;
        #[derive(Default)]
        struct Rec(Mutex<Vec<CompilerMetric>>);
        impl Instrumentation for Rec { fn increment_counter(&self, m: CompilerMetric) { self.0.lock().unwrap().push(m); } }
        let rec = std::sync::Arc::new(Rec::default());
        let h = InstrumentationHandle(rec.clone());
        let cfg = Config { experimental_dynamic_directive: Some(true), instrumentation: Some(h), ..Default::default() };
        compile("<template lwc:render-mode=\"light\"><x-foo lwc:dynamic={ctor}></x-foo></template>", "x.html", cfg).unwrap();
        assert_eq!(*rec.0.lock().unwrap(), vec![CompilerMetric::LwcRenderModeDirective, CompilerMetric::LwcDynamicDirective]);
    }

    #[test]
    fn warn_lwc_dynamic() {
        let cfg = Config { experimental_dynamic_directive: Some(true), ..Default::default() };
        assert_eq!(warns("<template><x-foo lwc:dynamic={ctor}></x-foo></template>", cfg), vec![1187]);
    }

    #[test]
    fn warn_key_outside_iteration() {
        assert_eq!(warns("<template><div key={k}></div></template>", Config::default()), vec![1149]);
        assert!(warns("<template><template for:each={xs} for:item=\"x\"><div key={x.id}></div></template></template>", Config::default()).is_empty());
        assert!(warns("<template><div for:each={xs} for:item=\"x\" key={x.id}></div></template>", Config::default()).is_empty());
    }

    #[test]
    fn warn_duplicate_slots() {
        assert_eq!(warns("<template><slot></slot><slot></slot><slot name=\"a\"></slot><slot name=\"a\"></slot></template>", Config::default()), vec![1137, 1137]);
        // lwc:if / lwc:else branches are separate scopes.
        assert!(warns("<template><template lwc:if={a}><slot></slot></template><template lwc:else><slot></slot></template></template>", Config::default()).is_empty());
    }

    #[test]
    fn warn_invalid_template_attribute() {
        assert_eq!(warns("<template><template if:true={a} class=\"c\"><p></p></template></template>", Config::default()), vec![1145]);
    }

    #[test]
    fn warn_static_id_in_iteration() {
        assert_eq!(warns("<template><template for:each={xs} for:item=\"x\"><div key={x.id}><p id=\"a\"></p></div></template></template>", Config::default()), vec![1041]);
        assert_eq!(warns("<template><div for:each={xs} for:item=\"x\" key={x.id} id=\"b\"></div></template>", Config::default()), vec![1041]);
        assert!(warns("<template><p id=\"a\"></p><template for:each={xs} for:item=\"x\"><div key={x.id} id={x.id}></div></template></template>", Config::default()).is_empty());
    }

    #[test]
    fn warn_ignored_slot_attribute_in_child() {
        assert_eq!(warns("<template><x-foo><div><span slot=\"bar\"></span></div></x-foo></template>", Config::default()), vec![1201]);
        assert!(warns("<template><div><x-foo><span slot=\"bar\"></span></x-foo></div></template>", Config::default()).is_empty());
        assert!(warns("<template><x-foo><template if:true={a}><span slot=\"bar\"></span></template></x-foo></template>", Config::default()).is_empty());
    }

    #[test]
    fn warn_single_if_directive() {
        assert_eq!(warns("<template><div if:true={a} if:false={b}></div></template>", Config::default()), vec![1182]);
    }

    #[test]
    fn warn_invalid_template_without_directive() {
        // JS oracle (template-compiler dist): [1145, 1153] — validateTemplate reports the bad attribute first.
        assert_eq!(warns("<template><template class=\"c\"><p></p></template></template>", Config::default()), vec![1145, 1153]);
    }

    #[test]
    fn svg_use_href_sanitized_static() {
        let code = c("<template>\n    <svg viewBox=\"0 0 5 5\" class=\"x\">\n        <use xlink:href=\"/x\"></use>\n    </svg>\n</template>\n");
        assert!(code.contains("import { freezeTemplate, parseFragment, registerTemplate, sanitizeAttribute } from \"lwc\";"), "{code}");
        assert!(code.contains("<use${\"a1:xlink:href\"}${3}/>"), "{code}");
        assert!(code.contains("api_static_part(1, { attrs: { \"xlink:href\": sanitizeAttribute(\"use\", \"http://www.w3.org/2000/svg\", \"xlink:href\", \"/x\") } }, null)"), "{code}");
    }

    #[test]
    fn svg_use_href_sanitized_dynamic() {
        let code = cn("<template><svg><use href={h}></use></svg></template>");
        assert!(code.contains("sanitizeAttribute") && code.contains("href: sanitizeAttribute(\"use\", \"http://www.w3.org/2000/svg\", \"href\", $cmp.h)"), "{code}");
    }

    #[test]
    fn svg_fragment_in_iteration() {
        let code = c("<template>\n    <svg>\n        <template for:each={lines} for:item=\"line\">\n            <line key={line.key} x1={line.x1}></line>\n        </template>\n    </svg>\n</template>\n");
        assert!(code.contains("import { freezeTemplate, parseSVGFragment, registerTemplate } from \"lwc\";"), "{code}");
        assert!(code.contains("parseSVGFragment`<line${\"a0:x1\"}${3}/>`"), "{code}");
        assert!(code.contains("api_static_fragment($fragment1, api_key(") && code.contains("\"x1\": line.x1"), "{code}");
    }

    #[test]
    fn lwc_external_lowers_to_external_component_without_import() {
        let src = "<template><x-foo lwc:external data-id=\"a\"></x-foo></template>";
        let root = parse(src, Config::default()).unwrap().root.unwrap();
        let ChildNode::ExternalComponent(ext) = &root.children[0] else { panic!("{:?}", root.children) };
        assert_eq!(ext.name, "x-foo");
        assert!(ext.attributes.iter().all(|a| a.name != "lwc:external"));
        assert_eq!(ext.attributes[0].name, "data-id");
        let out = compile(src, "foo.html", Config::default()).unwrap();
        assert!(!out.code.contains("from \"x/foo\""), "{}", out.code);
        assert!(out.code.contains("api_element(\"x-foo\""), "{}", out.code);
        assert!(out.code.contains("external: true"), "{}", out.code);
        assert!(!out.code.contains("api_custom_element"), "{}", out.code);
    }

    #[test]
    fn without_lwc_external_stays_component_with_import() {
        let src = "<template><x-foo></x-foo></template>";
        let root = parse(src, Config::default()).unwrap().root.unwrap();
        assert!(matches!(root.children[0], ChildNode::Component(_)));
        let out = compile(src, "foo.html", Config::default()).unwrap();
        assert!(out.code.contains("from \"x/foo\""), "{}", out.code);
    }

    #[test]
    fn ssr_inner_h_t_m_l_attribute_errors() {
        let ssr = || Config { ssr: Some(true), ..Default::default() };
        let src = "<template><div inner-h-t-m-l=\"x\">o</div></template>";
        let out = compile(src, "foo.html", ssr()).unwrap();
        let e = out.warnings.iter().find(|w| w.code.is_none()).expect("diag");
        assert!(matches!(e.level, DiagnosticLevel::Error));
        assert_eq!(e.message, "Cannot set attribute \"inner-h-t-m-l\" on <div>.");
        assert!(out.code.is_empty());
        let ext = compile("<template><omg-whatever lwc:external outer-h-t-m-l={x}></omg-whatever></template>", "foo.html", ssr()).unwrap();
        assert!(ext.warnings.iter().any(|w| w.message == "Cannot set attribute \"outer-h-t-m-l\" on <omg-whatever>."));
        // Components: becomes innerHTML prop, no error (TS inner-lwc fixture).
        let cmp = compile("<template><x-c inner-h-t-m-l=\"x\"></x-c></template>", "foo.html", ssr()).unwrap();
        assert!(!cmp.code.is_empty());
        // CSR: TS template-compiler does not error.
        let csr = compile(src, "foo.html", Config::default()).unwrap();
        assert!(!csr.code.is_empty());
    }

    #[test]
    fn adjacent_mustache_text_splits() {
        let src = "<template><span>{foo}yolo{bar}toto{baz}</span></template>";
        let r = parse(src, Config::default()).unwrap();
        assert!(r.warnings.iter().all(|w| w.code != Some(1083)), "{:?}", r.warnings);
        let root = r.root.unwrap();
        let ChildNode::Element(e) = &root.children[0] else { panic!("{:?}", root.children) };
        let kinds: Vec<String> = e.children.iter().map(|c| match c {
            ChildNode::Text(Text { value: TextValue::Expression(Expression::Identifier(i)), .. }) => format!("expr:{}", i.name),
            ChildNode::Text(Text { value: TextValue::Literal(s), .. }) => format!("lit:{s}"),
            o => panic!("{o:?}"),
        }).collect();
        eprintln!("IR span children: {kinds:?}");
        assert_eq!(kinds, ["expr:foo", "lit:yolo", "expr:bar", "lit:toto", "expr:baz"]);
        let out = compile(src, "x.html", Config::default()).unwrap();
        assert!(out.warnings.iter().all(|w| w.code != Some(1083)), "{:?}", out.warnings);
        eprintln!("{}", out.code);
        for n in ["$cmp.foo", "$cmp.bar", "$cmp.baz", "yolo", "toto"] { assert!(out.code.contains(n), "{n}\n{}", out.code); }
    }

    fn fixture(rel: &str) -> String {
        std::fs::read_to_string(format!("/workspace/lwc/packages/@lwc/{rel}")).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }
    fn codes(src: &str, cfg: Config) -> Vec<u32> {
        parse(src, cfg).unwrap().warnings.iter().map(|w| w.code.expect("coded diagnostic")).collect()
    }

    /// ssr-compiler fixture lwc-if-else-complex: complex `lwc:if` conditions (CTE) keep their
    /// chain role, so each `lwc:else` attaches to its IfBlock (was 1153 + 1165 "No such element found").
    #[test]
    fn lwc_if_else_complex_chain() {
        let src = fixture("ssr-compiler/src/__tests__/fixtures/lwc-if-else-complex/modules/x/component/component.html");
        let r = parse(&src, Config { experimental_complex_expressions: Some(true), ..Config::default() }).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let root = r.root.unwrap();
        let blocks: Vec<_> = root.children.iter().filter_map(|c| match c { ChildNode::IfBlock(b) => Some(b), _ => None }).collect();
        assert_eq!(blocks.len(), 2, "{:?}", root.children);
        for b in blocks {
            assert!(matches!(b.condition, Expression::Complex(_)), "{:?}", b.condition);
            assert!(matches!(b.else_.as_deref(), Some(ElseBranch::Else(_))), "{:?}", b.else_);
        }
        // Whitespace + comments (not preserved) between members still chain.
        let r = parse("<template><template lwc:if=\"{a.b()}\">x</template>\n <!-- c -->\n<template lwc:else>y</template></template>",
            Config { experimental_complex_expressions: Some(true), ..Config::default() }).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        // Non-whitespace text breaks the chain → orphan else.
        assert_eq!(codes("<template><template lwc:if={a}>x</template>t<template lwc:else>y</template></template>", Config::default()), vec![1165]);
    }

    /// ssr-compiler fixture context-slotted/scoped: `<template lwc:slot-data>` children are kept as a
    /// ScopedSlotFragment, so `x-consumer` reaches the IR (and its `x/consumer` import).
    #[test]
    fn scoped_slot_fragment_ir_and_import() {
        let src = fixture("ssr-compiler/src/__tests__/fixtures/context-slotted/scoped/modules/x/root/root.html");
        let r = parse(&src, Config::default()).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        fn find<'a>(cs: &'a [ChildNode], out: &mut Vec<&'a ScopedSlotFragment>) {
            for c in cs { match c {
                ChildNode::ScopedSlotFragment(f) => { out.push(f); find(&f.children, out) }
                ChildNode::Component(e) => find(&e.children, out),
                ChildNode::Element(e) => find(&e.children, out),
                _ => {}
            } }
        }
        let mut frags = vec![]; find(&r.root.as_ref().unwrap().children, &mut frags);
        assert!(!frags.is_empty());
        assert!(frags.iter().any(|f| f.children.iter().any(|c| matches!(c, ChildNode::Component(e) if e.name == "x-consumer"))), "{frags:?}");
        let c = compile(&src, "root.html", Config::default()).unwrap();
        assert!(c.code.contains("import _xConsumer from \"x/consumer\";"), "{}", c.code);
        assert!(c.code.contains("api_scoped_slot_factory("), "{}", c.code);
    }

    /// template-compiler fixture scoped-slots/parent-named-slot: codegen shape.
    #[test]
    fn scoped_slot_codegen() {
        let src = fixture("template-compiler/src/__tests__/fixtures/scoped-slots/parent-named-slot/actual.html");
        let r = compile(&src, "parent-named-slot.html", Config::default()).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        for needle in [
            "fr: api_fragment", "ssf: api_scoped_slot_factory",
            "api_scoped_slot_factory(\"slotname1\", function (slot1data, key) {",
            "api_scoped_slot_factory(\"\", function (defaultdata, key) {",
            "return api_fragment(", "api_dynamic_text(slot1data.name)", "api_dynamic_text(defaultdata.title)",
        ] { assert!(r.code.contains(needle), "missing {needle}\n{}", r.code); }
        assert!(!r.code.contains("$cmp.slot1data"), "{}", r.code);
        // Bound slot name.
        let r = compile("<template><x-c><template slot={n} lwc:slot-data=\"d\"><p>{d.a}</p></template></x-c></template>", "a.html", Config::default()).unwrap();
        assert!(r.code.contains("api_scoped_slot_factory($cmp.n, function (d, key) {"), "{}", r.code);
    }

    /// TS parseScopedSlotFragment / validateChildren diagnostics (scoped-slots/invalid/slot-data fixtures).
    #[test]
    fn scoped_slot_bind_diagnostics() {
        let base = "template-compiler/src/__tests__/fixtures/scoped-slots/invalid/slot-bind";
        let cases: &[(&str, &[u32])] = &[
            ("binding-not-expression", &[1170]),
            ("duplicate-slot-default", &[1172, 1172]),
            ("duplicate-slot-named", &[1172]),
            ("invalid-on-non-slot-content", &[1171, 1171, 1145, 1153, 1187, 1171, 1171]),
            ("mixed-slot-alt", &[1173]),
            ("mixed-slot-default", &[1173]),
            ("mixed-slot-named", &[1173]),
            ("non-light-dom", &[1169]),
        ];
        for (name, want) in cases {
            let src = fixture(&format!("{base}/{name}/actual.html"));
            let got = codes(&src, Config { enable_dynamic_components: Some(true), experimental_dynamic_directive: Some(true), ..Config::default() });
            assert_eq!(&got, want, "{name}");
        }
    }

    #[test]
    fn custom_element_imports_are_post_order() {
        let r = crate::compile("<template><x-outer><x-inner></x-inner></x-outer></template>", "x.html", Config::default()).unwrap();
        let (i, o) = (r.code.find("x/inner").unwrap(), r.code.find("x/outer").unwrap());
        assert!(i < o, "{}", r.code);
    }

    #[test]
    fn scoped_slot_data_diagnostics() {
        let base = "template-compiler/src/__tests__/fixtures/scoped-slots/invalid/slot-data";
        let cases: &[(&str, &[u32], bool)] = &[
            ("comment-direct-child", &[1177], true),
            ("invalid-on-non-slot-content", &[1176, 1176, 1176, 1176, 1176], false),
            ("text-direct-child", &[1177, 1177], false),
            ("value-non-string", &[1174], false),
            ("with-for-each-parent", &[1178, 1178], false),
            ("with-for-each-sibling", &[1179], false),
            ("with-lwc-if-sibling", &[1179], false),
        ];
        for (name, want, comments) in cases {
            let src = fixture(&format!("{base}/{name}/actual.html"));
            let got: Vec<u32> = codes(&src, Config { preserve_html_comments: Some(*comments), enable_dynamic_components: Some(true), experimental_dynamic_directive: Some(true), ..Config::default() })
                .into_iter().filter(|c| want.contains(c)).collect();
            assert_eq!(&got, want, "{name}");
        }
        // 1177 also applies to template directives nested inside the fragment (TS validateChildren).
        let dirs = "template-compiler/src/__tests__/fixtures/scoped-slots/slot-content-with-directives/invalid";
        assert_eq!(codes(&fixture(&format!("{dirs}/if-block/actual.html")), Config { preserve_html_comments: Some(true), ..Config::default() }).iter().filter(|c| **c == 1177).count(), 3);
        assert_eq!(codes(&fixture(&format!("{dirs}/if-true/actual.html")), Config { preserve_html_comments: Some(true), ..Config::default() }).iter().filter(|c| **c == 1177).count(), 2);
        // Parent may be reached through lwc:if/else blocks; reserved/invalid identifiers.
        assert_eq!(codes("<template><x-c><template lwc:if={a}><template lwc:slot-data=\"d\"><p></p></template></template></x-c></template>", Config::default()), Vec::<u32>::new());
        assert_eq!(codes("<template><div><template lwc:slot-data=\"d\"><p></p></template></div></template>", Config::default()), vec![1178]);
        assert_eq!(codes("<template><x-c><template lwc:slot-data=\"class\"><p></p></template></x-c></template>", Config::default()), vec![1211]);
        assert_eq!(codes("<template><x-c><template lwc:slot-data=\"a-b\"><p></p></template></x-c></template>", Config::default()), vec![1059]);
        // Text below an element child is fine (text-non-direct-child).
        assert_eq!(codes("<template><x-c><template lwc:slot-data=\"d\"><p>{d.x} t</p></template></x-c></template>", Config::default()), Vec::<u32>::new());
    }

    /// ssr-compiler fixture exports/component-as-default: TS acorn `parseExpressionAt` stops after
    /// `Component`; the leftover ` as default` is 1083 "Unexpected end of expression" (not 1060), and
    /// error recovery drops the text node.
    #[test]
    fn mustache_unexpected_end_of_expression() {
        for f in ["light/light.html", "shadow/shadow.html"] {
            let src = fixture(&format!("ssr-compiler/src/__tests__/fixtures/exports/component-as-default/modules/x/{f}"));
            let r = parse(&src, Config::default()).unwrap();
            let w: Vec<_> = r.warnings.iter().map(|w| (w.code, w.level)).collect();
            assert_eq!(w, vec![(Some(1083), DiagnosticLevel::Error)], "{f}");
            assert!(r.warnings[0].message.contains("Unexpected end of expression"), "{}", r.warnings[0].message);
            assert!(r.root.unwrap().children.is_empty(), "{f}");
        }
    }

    /// TS throws a plain uncoded `Error` for SSR inner/outer-h-t-m-l attributes → `code: None`
    /// (napi `code: null`), never a sentinel 0.
    #[test]
    fn ssr_inner_outer_html_is_uncoded() {
        for attr in ["inner-h-t-m-l", "outer-h-t-m-l"] {
            let r = parse(&format!("<template><div {attr}=\"x\"></div></template>"), Config { ssr: Some(true), ..Config::default() }).unwrap();
            let e = r.warnings.iter().find(|w| w.level == DiagnosticLevel::Error).expect("error");
            assert_eq!(e.code, None, "{e:?}");
            assert!(e.message.contains(attr), "{}", e.message);
        }
    }
}
