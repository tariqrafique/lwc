//! Rust port of `@lwc/template-compiler`.
//!
//! - HTML parse: `tariqrafique/parse5-rust` (pending GitHub)
//! - Emit: ESM only (no AMD)
//! - Parity: behavioral / new goldens (AST + warnings + render semantics), not astring bytes
//!
//! Phase 0 harness entrypoints: [`compile`] and [`parse`].

mod codegen;
/// Public binder for complex template expressions (D-0221 / AB-D0004 S3).
/// The implementation lives at `codegen/expression.rs` (Arch layout: no `expr/` directory).
pub mod expr {
    pub use crate::codegen::expression::{bind, BindError, BindPlan};
}
/// Public printer for complex template expressions (D-0221 / AB-D0004 S4).
/// The implementation lives at `codegen/printer.rs` (Arch layout: no `expr/` directory).
pub mod printer {
    pub use crate::codegen::printer::{print_expr, Js, PrintError};
}
pub mod config;
mod parser;
mod scope_tokens;
pub mod shared;

pub use scope_tokens::{generate_scope_tokens, ScopeTokens};

pub use lwc_shared::template_ir::*;
/// Alias used by callers.
pub type TemplateConfig = Config;
pub use config::{Config, ConfigError, CustomRendererConfig, CustomRendererElementConfig, NormalizedConfig};
pub use lwc_shared::{CompilerDiagnostic, DiagnosticLevel};
pub use parser::TemplateParseResult;
/// The arena a [`parse`] result's complex expressions live in (AB-D0004).
pub use oxc_allocator::Allocator;

/// Result of [`compile`]. Unlike TS, no `root`: the IR borrows `compile`'s own arena (AB-D0004); use
/// [`parse`] for the IR.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateCompileResult {
    pub code: String,
    pub warnings: Vec<CompilerDiagnostic>,
    pub css_scope_tokens: Vec<String>,
}

/// Parse HTML markup into an AST (no codegen). Complex expression ASTs are allocated in `alloc`.
pub fn parse<'a>(alloc: &'a Allocator, source: &str, config: Config) -> Result<TemplateParseResult<'a>, ConfigError> {
    let options = config.normalize()?;
    Ok(parser::parse_template(alloc, source, &options))
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
    // The compile entry owns the arena; the IR never outlives this call or leaves its thread.
    let alloc = Allocator::default();
    let parsing = parser::parse_template(&alloc, source, &options);

    let has_error = parsing
        .warnings
        .iter()
        .any(|w| matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal));

    let (code, warnings) = match &parsing.root {
        Some(root) if !has_error => match codegen::generate(root, &options, filename, &tokens) {
            Ok(code) => (code, parsing.warnings),
            Err(e) => {
                let mut warnings = parsing.warnings;
                warnings.push(codegen_catch(e));
                (String::new(), warnings)
            }
        },
        _ => (String::new(), parsing.warnings),
    };

    Ok(TemplateCompileResult {
        code,
        warnings,
        css_scope_tokens: tokens.css_scope_tokens,
    })
}

/// `index.ts:76-79`: `normalizeToDiagnostic(GENERIC_PARSING_ERROR, error)` then prefix
/// `Unexpected compilation error: `. A thrown `CompilerError` (1189 from bind) keeps its code,
/// level and (absent) location via `toDiagnostic()`; `GENERIC_PARSING_ERROR` is only the fallback
/// for a non-CompilerError, which PrintError maps to [`InternalError`].
fn codegen_catch(e: codegen::CodegenError) -> CompilerDiagnostic {
    match e {
        codegen::CodegenError::Bind(e) => {
            let ce = e.compiler_error();
            CompilerDiagnostic::new(
                ce.code,
                format!("Unexpected compilation error: {}", ce.message),
                ce.level,
                ce.filename,
                ce.location,
            )
        }
        codegen::CodegenError::Internal(e) => e.diagnostic(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`super::parse`] in a leaked per-test arena.
    fn parse(source: &str, config: Config) -> Result<TemplateParseResult<'static>, ConfigError> {
        super::parse(Box::leak(Box::default()), source, config)
    }

    const SRC: &str = "<template>\n    foo\n</template>\n";

    #[test]
    fn compile_empty_source() {
        let r = compile("", "x-foo.html", Config::default()).unwrap();
        assert!(r.code.is_empty());
        assert!(parse("", Config::default()).unwrap().root.is_none());
    }

    #[test]
    fn parse_static_text() {
        let r = parse(SRC, Config::default()).unwrap();
        let root = r.root.expect("root");
        assert_eq!(root.children.len(), 1);
        match &root.children[0] {
            ChildNode::Text(Text { value: TextValue::Literal(s), raw, .. }) => {
                assert_eq!(s, "foo");
                assert_eq!(raw, "foo"); // TS Text.raw = cleaned source token
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
        for n in ["api_element(\"section\"", "classMap: {", "\"foo\": true", "\"bar\": true", "\"baz-fiz\": true", "key: 0", "h: api_element"] {
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
        assert!(c.contains("\"href\": $cmp.narita") && c.contains("api_text(\"KIX\")"), "{c}");
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
    /// Inline the module-level `const stcN = ...;` hoists back into `tmpl`, for tests that pin shapes.
    fn unhoist(code: &str) -> String {
        let mut decls = Vec::new();
        let mut rest = String::new();
        for line in code.split_inclusive('\n') {
            if let Some(d) = line.strip_prefix("const stc") {
                let (n, v) = d.split_once(" = ").unwrap();
                decls.push((format!("stc{n}"), String::new()));
                decls.last_mut().unwrap().1.push_str(v);
            } else if decls.last().is_some_and(|(_, v)| !v.trim_end().ends_with(';')) {
                decls.last_mut().unwrap().1.push_str(line);
            } else { rest.push_str(line); }
        }
        for (n, v) in decls.iter().rev() {
            let v = v.trim_end().trim_end_matches(';');
            let mut out = String::new();
            let mut last = 0;
            for (i, _) in rest.match_indices(n.as_str()) {
                if rest[i + n.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric()) { continue; }
                out.push_str(&rest[last..i]);
                out.push_str(v);
                last = i + n.len();
            }
            out.push_str(&rest[last..]);
            rest = out;
        }
        rest
    }

    fn cn(src: &str) -> String { compile(src, "x.html", Config { enable_static_content_optimization: Some(false), ..Default::default() }).unwrap().code }

    const MIXED: &str = "<template>\n  <div>\n    <section class=\"s\"><p>hi</p></section>\n    {msg}\n  </div>\n</template>\n";

    #[test]
    fn static_fragment_dynamic_text_part() {
        // JS oracle: `<p>{x}</p>` → static fragment with a text part (partId 1 = the text run).
        let code = c("<template><p>{x}</p></template>");
        for n in ["import {freezeTemplate, parseFragment, registerTemplate} from \"lwc\";", "const $fragment1 = parseFragment`<p${3}>${\"t1\"}</p>`;",
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
        for n in ["import {freezeTemplate, parseFragment, registerTemplate} from \"lwc\";", "const $fragment1 = parseFragment`<div${3}><section class=\"s${0}\"${2}><p${3}>hi</p></section>${\"t4\"}</div>`;", "api_static_part(4, null, api_dynamic_text($cmp.msg))", "st: api_static_fragment"] {
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
        assert!(cn(src).contains("\"title\": $cmp.foo.bar"), "{}", cn(src));
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
        for n in ["api_iterator($cmp.items, function (item)", "api_key(", "item.id", "api_dynamic_text(item)", "\"my-list\": true", "i: api_iterator", "k: api_key"] {
            assert!(code.contains(n), "{n}\n{code}");
        }
        assert!(!code.contains("$cmp.item.") && !code.contains("$cmp.item)") && !code.contains("for:"), "{code}");
    }

    #[test]
    fn compile_for_index() {
        let code = c("<template>\n  <div for:each={items} for:item=\"item\" for:index=\"i\" key={item.id}>\n    <span>{item}{i}</span>\n  </div>\n</template>\n");
        // `key={i}` (the for:index) is LWC1065 upstream (KEY_SHOULD_NOT_USE_INDEX).
        assert!(code.contains("function (item, i)") && code.contains("api_key(1, item.id)"), "{code}");
    }

    #[test]
    fn nested_template_for_each_and_iterator() {
        let root = parse("<template><ul><template for:each={xs} for:item=\"x\"><li key={x.k}>{x.v}</li></template></ul><template iterator:it={xs}><p key={it.value.id}>{it.index}</p></template></template>", Config::default()).unwrap().root.unwrap();
        let ChildNode::Element(ul) = &root.children[0] else { panic!() };
        assert!(matches!(&ul.children[0], ChildNode::ForEach(f) if matches!(&f.children[..], [ChildNode::Element(li)] if li.name == "li")));
        let ChildNode::ForOf(o) = &root.children[1] else { panic!("{:?}", root.children) };
        assert_eq!(o.iterator.name, "it");
        let code = c("<template><template iterator:it={xs}><p key={it.value.id}>{it.index}</p></template></template>");
        assert!(code.contains("function (itValue, itIndex, itFirst, itLast)") && code.contains("api_key(1, it.value.id)") && code.contains("api_dynamic_text(it.index)") && !code.contains("$cmp.it"), "{code}");
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
        // TS transformConditionalParentBlock: every branch is `api_fragment(ifBlockKey, children, 0)`, the
        // if-block key allocated before its children's keys.
        for n in ["$cmp.visible ? api_fragment(0, [api_element", " : api_fragment(0, [api_element", "api_text(\"Visible Header\")", "api_text(\"Alternative Header\")", "key: 1", "key: 2"] {
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

    fn first_slot(src: &str) -> lwc_shared::template_ir::Slot<'static> {
        use lwc_shared::template_ir::ChildNode;
        fn find<'a>(cs: &[ChildNode<'a>]) -> Option<lwc_shared::template_ir::Slot<'a>> {
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
        assert!(code.contains("const {_m0} = $ctx;\n") && code.contains("on: _m0 || ($ctx._m0 = { ") && code.contains("\"click\": api_bind($cmp.handleClick)") && code.contains("api_text(\"x\")") && code.contains("b: api_bind") && !code.contains("attrs"), "{code}");
        // TS genEventListeners: with a local-rooted handler only the non-local ones are memoized.
        let code = cn("<template><template for:each={list} for:item=\"task\"><button key={task.id} onclick={task.del} ontouchstart={foo}>x</button></template><p onclick={a} onkeyup={b}></p></template>");
        let flat: String = code.split_whitespace().collect();
        assert!(flat.contains("const{_m0,_m1}=$ctx;") && flat.contains("on:{\"click\":api_bind(task.del),\"touchstart\":_m0||($ctx._m0=api_bind($cmp.foo))}"), "{code}");
        assert!(flat.contains("on:_m1||($ctx._m1={\"click\":api_bind($cmp.a),\"keyup\":api_bind($cmp.b)})"), "{code}");
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
        assert!(code.contains("props: {\n") && code.contains("\"myProp\": $cmp.val\n") && code.contains("classMap: stc0") && code.contains("const stc0 = {\n  \"c\": true\n};"), "{code}");
    }

    #[test]
    fn element_keeps_attributes() {
        let code = cn("<template><div my-prop={val} title=\"t\"></div></template>");
        assert!(!code.contains("props") && code.contains("\"my-prop\": $cmp.val") && code.contains("\"title\": \"t\""), "{code}");
    }

    /// TS elementDataBag (index.ts:511-561): string class/style/slot become classMap/styleDecls/
    /// slotAssignment ahead of attrs; boolean class/style emit nothing; a boolean slot is `""`.
    /// Expected text is the JS compiler's output.
    #[test]
    fn databag_class_style_slot() {
        let code = cn("<template><div class=\"a&nbsp;b  c a\" style=\"background: url(a;b); x:; color:red;color: blue ! IMPORTANT ;1: q; w: !important\" title=\"t\" onclick={h}></div><div class style></div><x-y slot></x-y><p class={c} style={s} slot={z}></p></template>");
        for n in [
            "const stc0 = {\n  \"a\": true,\n  \"b\": true,\n  \"c\": true,\n  \"a\": true\n};",
            "const stc1 = [[\"1\", \"q\", false], [\"background\", \"url(a;b)\", false], [\"color\", \"blue\", true], [\"w\", \"\", true]];",
            "classMap: stc0,\n      styleDecls: stc1,\n      attrs: stc2,",
            "const stc3 = {\n  key: 1\n};",
            "const stc4 = {\n  slotAssignment: \"\",\n  key: 2\n};",
            "className: api_normalize_class_name($cmp.c),\n      style: $cmp.s,\n      slotAssignment: $cmp.z,\n      key: 3",
        ] {
            assert!(code.contains(n), "{n}\n{code}");
        }
        let code = cn("<template><div class=\"\" style=\"\" onclick={h}></div><slot name=\"n\" onslotchange={h}></slot></template>");
        // Empty classMap/styleDecls hoist like any static literal; `[]` shares a const with the slot's children.
        for n in ["const stc0 = {};\nconst stc1 = [];\nconst stc2 = {\n  \"name\": \"n\"\n};", "classMap: stc0,\n      styleDecls: stc1,\n      key: 0", "attrs: stc2,", "}, stc1, $slotset)"] {
            assert!(code.contains(n), "{n}\n{code}");
        }
    }

    #[test]
    fn slot_attr_on_child() {
        let code = c("<template>\n  <x-foo>\n    <p slot=\"footer\">f</p>\n  </x-foo>\n</template>");
        assert!(code.contains("slotAssignment: \"footer\"") && code.contains("api_element(\"p\""), "{code}");
    }

    #[test]
    fn compile_rich_smoke() {
        let code = c("<template>\n  <section class=\"wrap\">\n    <x-foo onclick={onFoo} title={msg}>\n      <slot></slot>\n    </x-foo>\n  </section>\n</template>");
        for n in ["api_element(\"section\"", "api_custom_element(\"x-foo\"", "import _xFoo from \"x/foo\"", "api_bind($cmp.onFoo)", "api_slot(\"\"", "props: {", "\"title\": $cmp.msg", "tmpl.slots = [\"\"];"] {
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
        for n in ["import {freezeTemplate, parseFragment, registerTemplate} from \"lwc\";", "const $fragment1 = parseFragment`<section class=\"foo bar baz-fiz${0}\"${2}></section>`;", "const {st: api_static_fragment} = $api;", "return [api_static_fragment($fragment1, 1)];", "\"./class.css\"", "\"./class.scoped.css?scoped=true\""] {
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
        assert_eq!((d.code, d.location.unwrap().line), (Some(1127), 2));
        assert!(d.message.contains("lwc:foo") && d.message.contains("<div>"));
    }

    #[test]
    fn diag_1080_root_unknown_attrs() {
        let d = first_err("<template foo=\"bar\">\n  <div></div>\n</template>");
        assert_eq!((d.code, d.location.unwrap().line), (Some(1080), 1));
        assert!(compile("<template lwc:render-mode=\"light\"><div></div></template>", "x.html", Config::default()).unwrap().warnings.is_empty());
    }

    #[test]
    fn diag_1051_forbidden_tag() {
        let d = first_err("<template>\n\n  <meta charset=\"utf-8\">\n</template>");
        assert_eq!((d.code, d.location.unwrap().line), (Some(1051), 3));
    }

    #[test]
    fn diag_1058_invalid_html() {
        let d = first_err("<template>\n  <div a=\"1\" a=\"2\"></div>\n</template>");
        assert_eq!((d.code, d.location.unwrap().line), (Some(1058), 2));
        assert!(d.message.contains("duplicate-attribute"));
    }

    #[test]
    fn diag_1165_orphan_else() {
        let d = first_err("<template>\n  <div lwc:elseif={x}></div>\n</template>");
        assert_eq!((d.code, d.location.unwrap().line), (Some(1165), 2));
        let d = first_err("<template>\n  <p></p>\n  <div lwc:else></div>\n</template>");
        assert_eq!((d.code, d.location.unwrap().line), (Some(1165), 3));
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
    fn warn_template_leftovers_after_picks() {
        // JS oracle: 1145 "class, key" (parseScopedSlotFragment picks lwc:slot-data and slot, then validateTemplate).
        let msgs = |src: &str| compile(src, "x.html", Config::default()).unwrap().warnings.iter().map(|w| (w.code.unwrap(), w.message.clone())).collect::<Vec<_>>();
        let w = msgs("<template><x-foo><template lwc:slot-data=\"d\" slot=\"s\" class=\"c\" key=\"k\"></template></x-foo></template>");
        assert_eq!(w.iter().map(|w| w.0).collect::<Vec<_>>(), vec![1145]);
        assert!(w[0].1.contains("in LWC: class, key."), "{}", w[0].1);
        // JS oracle: [1182, 1145 "iterator:jt, class"]: parseForOf picks only the first iterator:*.
        let w = msgs("<template><template if:true={a} if:false={b} iterator:it={xs} iterator:jt={ys} class=\"c\"></template></template>");
        assert_eq!(w.iter().map(|w| w.0).collect::<Vec<_>>(), vec![1182, 1145]);
        assert!(w[1].1.contains("in LWC: iterator:jt, class."), "{}", w[1].1);
    }

    #[test]
    fn warn_invalid_template_without_directive() {
        // JS oracle (template-compiler dist): [1145, 1153] — validateTemplate reports the bad attribute first.
        assert_eq!(warns("<template><template class=\"c\"><p></p></template></template>", Config::default()), vec![1145, 1153]);
    }

    #[test]
    fn svg_use_href_sanitized_static() {
        let code = c("<template>\n    <svg viewBox=\"0 0 5 5\" class=\"x\">\n        <use xlink:href=\"/x\"></use>\n    </svg>\n</template>\n");
        assert!(code.contains("import {freezeTemplate, parseFragment, registerTemplate, sanitizeAttribute} from \"lwc\";"), "{code}");
        assert!(code.contains("<use${\"a1:xlink:href\"}${3}/>"), "{code}");
        assert!(code.contains("api_static_part(1, { attrs: { \"xlink:href\": sanitizeAttribute(\"use\", \"http://www.w3.org/2000/svg\", \"xlink:href\", \"/x\") } }, null)"), "{code}");
    }

    #[test]
    fn svg_use_href_sanitized_dynamic() {
        let code = cn("<template><svg><use href={h}></use></svg></template>");
        assert!(code.contains("sanitizeAttribute") && code.contains("\"href\": sanitizeAttribute(\"use\", \"http://www.w3.org/2000/svg\", \"href\", api_scoped_frag_id($cmp.h))"), "{code}");
    }

    #[test]
    fn svg_fragment_in_iteration() {
        let code = c("<template>\n    <svg>\n        <template for:each={lines} for:item=\"line\">\n            <line key={line.key} x1={line.x1}></line>\n        </template>\n    </svg>\n</template>\n");
        assert!(code.contains("import {freezeTemplate, parseSVGFragment, registerTemplate} from \"lwc\";"), "{code}");
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

    /// The SSR-only inner/outer-h-t-m-l reject lives in lwc-ssr (ssr-compiler element.ts:234);
    /// the template compiler never errors on them (TS template-compiler parity).
    #[test]
    fn inner_outer_html_attributes_compile() {
        for src in ["<template><div inner-h-t-m-l=\"x\">o</div></template>", "<template><omg-whatever lwc:external outer-h-t-m-l={x}></omg-whatever></template>"] {
            let out = compile(src, "foo.html", Config::default()).unwrap();
            assert!(!out.code.is_empty(), "{src}");
            assert!(out.warnings.iter().all(|w| !w.message.contains("Cannot set attribute")), "{src}: {:?}", out.warnings);
        }
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
        std::fs::read_to_string(lwc_shared::oracle_root().join("packages/@lwc").join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }
    fn cv(src: &str, api: u32, sco: bool, complex: bool) -> String {
        compile(src, "x.html", Config {
            api_version: Some(api), namespace: Some("ns".into()), name: Some("x".into()),
            enable_static_content_optimization: Some(sco), experimental_complex_expressions: Some(complex), ..Default::default()
        }).unwrap().code
    }

    /// TS transformChildren/shouldFlatten: an iterator among siblings is flattened, a lone one is returned as is.
    #[test]
    fn for_each_children_are_flattened() {
        let code = cn("<template><section>text<template for:each={items} for:item=\"i\"><p key={i.id}>x</p></template></section></template>");
        assert!(code.contains("api_flatten([api_text(\"text\"), api_iterator($cmp.items") && code.contains("f: api_flatten"), "{code}");
        let code = cn("<template><template for:each={items} for:item=\"i\"><p key={i.id}>x</p></template></template>");
        assert!(code.contains("return api_iterator($cmp.items") && !code.contains("api_flatten"), "{code}");
    }

    /// TS transformIf: array children get one ternary each; a flattened body becomes `test ? expr : []`.
    #[test]
    fn if_children_spread_or_flatten() {
        let code = cn("<template><div><template if:true={a}><p>1</p><p>2</p></template></div></template>");
        assert_eq!(code.matches("$cmp.a ? api_element(\"p\"").count(), 2, "{code}");
        assert!(!code.contains("[api_element(\"p\""), "{code}");
        let code = unhoist(&cn("<template><div><template if:false={a}><template for:each={items} for:item=\"i\"><p key={i.id}>x</p></template></template></div></template>"));
        assert!(code.contains("!$cmp.a ? api_iterator($cmp.items") && code.contains(" : []"), "{code}");
    }

    /// USE_FRAGMENTS_FOR_LIGHT_DOM_SLOTS @60: below it a light-DOM `<slot>` child forces flattening.
    #[test]
    fn light_slot_flatten_gate_59_60() {
        let src = "<template lwc:render-mode=\"light\"><div><slot></slot><p>x</p></div></template>";
        assert!(cv(src, 59, false, false).contains("api_flatten(["), "{}", cv(src, 59, false, false));
        assert!(!cv(src, 60, false, false).contains("api_flatten"), "{}", cv(src, 60, false, false));
    }

    /// LOWERCASE_SCOPE_TOKENS @59: legacy `{ns}-{name}_{stem}` token below, lowercase token at/above.
    #[test]
    fn stylesheet_token_gate_58_59() {
        let t58 = cv("<template><p></p></template>", 58, true, false);
        let t59 = cv("<template><p></p></template>", 59, true, false);
        assert!(t58.contains("tmpl.stylesheetToken = \"ns-x_x\";"), "{t58}");
        assert!(t59.contains("tmpl.stylesheetToken = \"lwc-") && !t59.contains("ns-x_x"), "{t59}");
        assert_eq!(t58.matches("stylesheetToken =").count(), 1);
        // Clamp: 57 behaves like 58, 1000 like 66.
        assert_eq!(cv("<template><p></p></template>", 57, true, false), t58);
        assert_eq!(cv("<template><p></p></template>", 1000, true, false), cv("<template><p></p></template>", 66, true, false));
    }

    /// LOWERCASE_SCOPE_TOKENS also gates static foreign-namespace (SVG) content.
    #[test]
    fn static_svg_gate_58_59() {
        let src = "<template><div><svg><path></path></svg></div></template>";
        assert!(!cv(src, 58, true, false).contains("api_static_fragment"), "{}", cv(src, 58, true, false));
        assert!(cv(src, 59, true, false).contains("api_static_fragment"), "{}", cv(src, 59, true, false));
    }

    /// TEMPLATE_CLASS_NAME_OBJECT_BINDING @62.
    #[test]
    fn class_object_binding_gate_61_62() {
        let src = "<template><div class={c}></div></template>";
        assert!(!cv(src, 61, false, false).contains("api_normalize_class_name"));
        assert!(cv(src, 62, false, false).contains("api_normalize_class_name($cmp.c)"), "{}", cv(src, 62, false, false));
    }

    /// ENABLE_COMPLEX_TEMPLATE_EXPRESSIONS @66: complex sources are bound only at/above it; plain member access
    /// (computed included) so access on `undefined` throws at render like JS.
    #[test]
    fn complex_expression_gate_65_66() {
        let src = "<template><p>{a + b.c[d]}</p></template>";
        assert!(cv(src, 66, false, true).contains("api_dynamic_text($cmp.a + $cmp.b.c[$cmp.d])"), "{}", cv(src, 66, false, true));
        assert!(!cv(src, 65, false, true).contains("$cmp.a + "));
    }

    /// expression/parenthesis: parentheses are transparent (ESTree), so `((foo).bar)` is `$cmp.foo.bar`.
    #[test]
    fn parenthesized_member_is_plain_member() {
        let code = cn("<template><p>{ ((foo).bar) }</p><p>{ (foo) }</p></template>");
        assert!(code.contains("api_dynamic_text($cmp.foo.bar)") && code.contains("api_dynamic_text($cmp.foo)") && !code.contains("undefined"), "{code}");
    }

    /// TS generateTemplateMetadata: `tmpl.renderMode = "light"` after slots, before hasRefs; nothing for shadow.
    #[test]
    fn render_mode_light_metadata() {
        let code = c("<template lwc:render-mode=\"light\"><slot></slot><p lwc:ref=\"p\"></p></template>");
        let (s, m, r) = (code.find("tmpl.slots =").unwrap(), code.find("tmpl.renderMode = \"light\";").expect(&code), code.find("tmpl.hasRefs").unwrap());
        assert!(s < m && m < r, "{code}");
        assert!(!c("<template lwc:render-mode=\"shadow\"><p></p></template>").contains("renderMode"));
    }

    /// TS genStaticParts/computeAttrValue: under synthetic shadow, static ids/idrefs and `<a href="#x">`
    /// (when the template has an id) are parts through gid/fid; light DOM serializes them inline.
    #[test]
    fn static_ids_are_scoped_parts() {
        let code = c("<template><div><label for=\"i\">l</label><input id=\"i\"><a href=\"#i\">x</a></div></template>");
        assert!(code.contains("<label${\"a1:for\"}${3}>") && code.contains("<input${\"a3:id\"}${3}>") && code.contains("<a${\"a4:href\"}${3}>"), "{code}");
        assert!(code.contains("api_static_part(1, { attrs: { \"for\": api_scoped_id(\"i\") } }, null)"), "{code}");
        assert!(code.contains("api_static_part(4, { attrs: { \"href\": api_scoped_frag_id(\"#i\") } }, null)"), "{code}");
        assert!(code.contains("gid: api_scoped_id") && code.contains("fid: api_scoped_frag_id"), "{code}");
        // No id anywhere: `#frag` hrefs stay static.
        assert!(c("<template><div><a href=\"#i\">x</a></div></template>").contains("<a href=\"#i\""));
        // Light DOM is not synthetic shadow: inline, no parts.
        let light = c("<template lwc:render-mode=\"light\"><div><input id=\"i\"></div></template>");
        assert!(light.contains("<input id=\"i\"${3}>") && !light.contains("api_scoped_id"), "{light}");
        // Dynamic path: literal id and computed tabindex.
        let dynamic = cn("<template><div id=\"x\" tabindex={t}></div></template>");
        assert!(dynamic.contains("\"id\": api_scoped_id(\"x\")") && dynamic.contains("\"tabindex\": api_tab_index($cmp.t)") && dynamic.contains("ti: api_tab_index"), "{dynamic}");
    }

    /// TS isAttribute: on a custom element, `class`/`data-*` are attributes, so a valueless one is `''`.
    #[test]
    fn component_boolean_attribute_is_empty_string() {
        let code = cn("<template><x-child data-attr class></x-child></template>");
        assert!(code.contains("\"data-attr\": \"\"") && !code.contains(": true"), "{code}");
    }

    /// Slice b: root `lwc:preserve-comments` keeps comments in static fragments and emits `api_comment` outside.
    #[test]
    fn preserve_comments_root_directive() {
        let st = c("<template lwc:preserve-comments><span><!-- yolo -->{foo}<!-- yolo --></span></template>");
        assert!(st.contains("<span${3}><!-- yolo -->${\"t2\"}<!-- yolo --></span>"), "{st}");
        assert!(st.contains("api_static_part(2, null, api_dynamic_text($cmp.foo))"), "{st}");
        let dy = cn("<template lwc:preserve-comments><div><!-- a -->x {foo} y<x-c></x-c></div></template>");
        assert!(dy.contains("api_comment(\" a \")") && dy.contains("co: api_comment"), "{dy}");
        assert!(dy.contains("api_text(\"x \" + api_dynamic_text($cmp.foo) + \" y\")"), "{dy}");
        // Not preserved: the comment is skipped and the text run continues across it.
        let off = cn("<template><div>{foo}<!-- a -->{bar}<x-c></x-c></div></template>");
        assert!(off.contains("api_text(api_dynamic_text($cmp.foo) + api_dynamic_text($cmp.bar))") && !off.contains("api_comment"), "{off}");
    }

    /// Slice d: `{'str'}` is a string literal, so it is static text with no `d`/`sp`.
    #[test]
    fn string_literal_text_is_static() {
        let code = c("<template><p>{'a&b'}</p></template>");
        assert!(!code.contains("api_dynamic_text") && !code.contains("api_static_part"), "{code}");
    }

    /// TS transformElement runs elementDataBag (a key) before genStaticElement (another key).
    #[test]
    fn static_fragments_burn_a_key() {
        let code = c("<template><p>a</p><p>b</p></template>");
        assert!(code.contains("api_static_fragment($fragment1, 1)") && code.contains("api_static_fragment($fragment2, 3)"), "{code}");
        let code = c("<template><section><p if:strict-true={x}>1</p></section></template>");
        assert!(code.contains("$cmp.x === true ? api_static_fragment($fragment1, 2) : null") && !code.contains("api_static_part"), "{code}");
    }

    /// TS parseText splits the undecoded source: `&#123; foo &#125;` is literal text; comments are decoded.
    #[test]
    fn entity_braces_are_literal_text() {
        let code = c("<template><p>const &#123; foo &#125; = bar;</p></template>");
        assert!(code.contains("<p${3}>const { foo } = bar;</p>") && !code.contains("api_dynamic_text"), "{code}");
        let code = c("<template lwc:preserve-comments><h1><!-- a&lt;b --></h1></template>");
        assert!(code.contains("<!-- a&lt;b -->"), "{code}");
    }

    /// Deprecated void elements serialize without a closing tag; the dropped `</input>` stays as text.
    #[test]
    fn deprecated_void_elements() {
        let code = c("<template>\n    <input type=\"text\">\n        <keygen name=\"n\">\n    </input>\n</template>");
        assert!(code.contains("<keygen name=\"n\"${3}>`") && code.contains("api_text(\"</input>\")"), "{code}");
    }

    /// Slice c: under complex expressions an on* attribute with any expression is a listener, and arrow
    /// params stay local.
    #[test]
    fn complex_on_handlers_are_listeners() {
        let code = cv("<template><section><button onclick=\"{() => myField = 'foo'}\"></button><a onclick=\"{(e) => go(e, x)}\"></a></section></template>", 66, true, true);
        let scoped = cv("<template><x-p foo=\"{[foo, (foo) => foo, [(foo) => [foo, (foo) => foo, foo], foo]]}\"></x-p></template>", 66, true, true);
        assert!(scoped.contains("[$cmp.foo, foo => foo, [foo => [foo, foo => foo, foo], $cmp.foo]]"), "{scoped}");
        assert!(code.contains("api_bind(() => $cmp.myField = 'foo')") && code.contains("api_bind(e => $cmp.go(e, $cmp.x))") && !code.contains("attrs"), "{code}");
    }

    /// Default arrow params: bind throws 1189 and compile() applies index.ts:76-79.
    #[test]
    fn default_arrow_params_are_1189_at_compile() {
        let cfg = Config { experimental_complex_expressions: Some(true), namespace: Some("ns".into()), name: Some("x".into()), ..Default::default() };
        let expected = "Unexpected compilation error: LWC1189: Template expression doesn't allow AssignmentPattern in arrow function params.";
        for src in [
            "<template>{(x = 1) => x}</template>",
            r#"<template><button onclick="{(x = 1) => x}"></button></template>"#,
            "<template>{({a = 1}) => a}</template>",
            "<template>{([a = 1]) => a}</template>",
        ] {
            let out = compile(src, "x.html", cfg.clone()).unwrap();
            assert!(out.code.is_empty(), "{src}\n{}", out.code);
            let w = out.warnings.iter().find(|w| w.code == Some(1189)).unwrap_or_else(|| panic!("{src}: {:?}", out.warnings));
            assert_eq!(w.code, Some(1189), "{src}");
            assert_eq!(w.message, expected, "{src}");
            assert_eq!(w.level, DiagnosticLevel::Error, "{src}");
            assert_eq!(w.location, None, "{src}: {:?}", w.location);
        }
    }

    /// Grouping vs call parens after static hoist, matching oracle JS compile()
    /// (`typeof stc0`, `!stc0`, `($cmp.x in stc0)`, `stc0`, `$cmp.foo(stc0)`).
    #[test]
    fn grouping_wrap_matches_oracle_compile() {
        let cfg = Config { experimental_complex_expressions: Some(true), namespace: Some("ns".into()), name: Some("x".into()), ..Default::default() };
        let cases = [
            ("<template>{typeof ({a: 1})}</template>", "api_dynamic_text(typeof stc0)"),
            ("<template>{!({a: 1})}</template>", "api_dynamic_text(!stc0)"),
            ("<template>{x in ({a: 1})}</template>", "api_dynamic_text(($cmp.x in stc0))"),
            ("<template>{(({a: 1}))}</template>", "api_dynamic_text(stc0)"),
            ("<template>{foo({a: 1})}</template>", "api_dynamic_text($cmp.foo(stc0))"),
        ];
        for (src, want) in cases {
            let out = compile(src, "x.html", cfg.clone()).unwrap();
            assert!(out.warnings.iter().all(|w| !matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal)), "{src}: {:?}", out.warnings);
            assert!(out.code.contains("const stc0 = {
  a: 1
};"), "{src}
{}", out.code);
            assert!(out.code.contains(want), "{src} missing {want}
{}", out.code);
        }
    }

    /// TS CodeGen.usedApis: the `$api` destructure lists helpers in first-use order (elementDataBag
    /// before static parts, `sp` before `k`/`st`, children before their parent's helper).
    #[test]
    fn api_helpers_in_first_use_order() {
        let code = cv("<template><template for:each={l} for:item=\"i\"><p key={i} class={i.c} tabindex={i.t}>{i}</p></template><div><span id=\"a\" class={c}>hi</span></div></template>", 66, true, true);
        assert!(code.contains("const {ncls: api_normalize_class_name, ti: api_tab_index, k: api_key, d: api_dynamic_text, sp: api_static_part, st: api_static_fragment, i: api_iterator, gid: api_scoped_id, f: api_flatten} = $api;"), "{code}");
    }

    /// TS shouldFlatten / transformChildren (codegen/helpers.ts L47-64, codegen/index.ts L204-211): the children
    /// argument is the lone child's expression, `api_flatten([...])`, or a plain array. Expected shapes taken from
    /// JS compile(), whitespace-stripped, with data objects (JS `stcN`) written as `{}`.
    #[test]
    fn flatten_rule_table() {
        const FOR: &str = "<template for:each={l} for:item=\"i\"><li key={i}></li></template>";
        let cases: &[(&str, String, u32, &str)] = &[
            ("light slot, API 59: flattened", "<template lwc:render-mode=\"light\"><div><slot></slot></div></template>".into(), 59, "api_element(\"div\",{},api_slot(\"\","),
            ("light slot, API 60: fragments, no flatten", "<template lwc:render-mode=\"light\"><div><slot></slot></div></template>".into(), 60, "api_element(\"div\",{},[api_slot(\"\","),
            ("shadow slot, API 59: no flatten", "<template><div><slot></slot></div></template>".into(), 59, "api_element(\"div\",{},[api_slot(\"\","),
            ("lone for-each", format!("<template><ul>{FOR}</ul></template>"), 66, "api_element(\"ul\",{},api_iterator($cmp.l,function(i){"),
            ("for-each with a sibling", format!("<template><ul><li></li>{FOR}</ul></template>"), 66, "api_element(\"ul\",{},api_flatten([api_element(\"li\",{}"),
            ("lone iterator", "<template><ul><template iterator:it={l}><li key={it.value}></li></template></ul></template>".into(), 66, "api_element(\"ul\",{},api_iterator($cmp.l,function(itValue,itIndex,itFirst,itLast){"),
            ("if:* over a flattening child", format!("<template><ul><template if:true={{x}}>{FOR}</template></ul></template>"), 66, "api_element(\"ul\",{},$cmp.x?api_iterator($cmp.l,"),
            ("if:* over a plain child", "<template><ul><template if:true={x}><li></li></template></ul></template>".into(), 66, "api_element(\"ul\",{},[$cmp.x?api_element(\"li\",{}"),
            ("lone plain child", "<template><ul><li></li></ul></template>".into(), 66, "api_element(\"ul\",{},[api_element(\"li\",{}"),
        ];
        for (name, src, api, want) in cases {
            let mut code: String = unhoist(&cv(src, *api, false, false)).split_whitespace().collect();
            // `{key:N}` data bags → `{}`, so the table pins only the children shape.
            let mut from = 0;
            while let Some(i) = code[from..].find("{key:").map(|i| from + i) {
                let digits = code[i + 5..].bytes().take_while(u8::is_ascii_digit).count();
                if digits > 0 && code[i + 5 + digits..].starts_with('}') { code.replace_range(i..i + 6 + digits, "{}"); }
                from = i + 1;
            }
            assert!(code.contains(want), "{name}: missing {want}\n{code}");
        }
    }

    fn codes(src: &str, cfg: Config) -> Vec<u32> {
        parse(src, cfg).unwrap().warnings.iter().map(|w| w.code.expect("coded diagnostic")).collect()
    }

    /// directive-lwc-if-else/error-multiple-directives/if-else: `lwc:else lwc:if` throws 1162 after TS has
    /// pushed the lwc:if block, so there is no orphaned lwc:else (no extra 1165).
    #[test]
    fn else_plus_if_is_1162_only() {
        let src = "<template>\n    <template lwc:else lwc:if={visible}>Conditional Text</template>\n</template>";
        assert_eq!(codes(src, Config::default()), vec![1162]);
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
        fn find<'a>(cs: &'a [ChildNode<'a>], out: &mut Vec<&'a ScopedSlotFragment<'a>>) {
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
        // TS validateChildren: a comment is non-element content (1177) and makes lwc:dom / lwc:inner-html
        // non-empty (1086 / 1142) only when comments are preserved. Oracle at b9e8007.
        for (src, code) in [
            ("<template><x-a><template lwc:slot-data=\"d\"><!-- c --><p></p></template></x-a></template>", 1177),
            ("<template><div lwc:dom=\"manual\"><!-- c --></div></template>", 1086),
            ("<template><div lwc:inner-html={h}><!-- c --></div></template>", 1142),
        ] {
            assert_eq!(codes(src, Config::default()), Vec::<u32>::new(), "{src}");
            assert_eq!(codes(src, Config { preserve_html_comments: Some(true), ..Config::default() }), vec![code], "{src}");
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

    // Wave 2 slice 1: lowering/diagnostic parity fixes (template-compiler fixtures named per test).

    /// parsing-errors/empty-template: blank source → 1072 at line 0 / column 0 (TS normalizeLocation(undefined)).
    #[test]
    fn missing_root_1072_at_0_0() {
        for src in ["", "  \n "] {
            let r = parse(src, Config::default()).unwrap();
            assert!(r.root.is_none());
            let [w] = &r.warnings[..] else { panic!("{:?}", r.warnings) };
            assert_eq!((w.code, w.level), (Some(1072), DiagnosticLevel::Error));
            let l = w.location.clone().expect("1072 carries a location");
            assert_eq!((l.line, l.column, l.start, l.length), (0, 0, Some(0), Some(0)));
        }
    }

    /// directive-lwc-if-else/interop/mixed-if-true-else(-elseif): if:* with lwc:else / lwc:elseif → 1166.
    #[test]
    fn mixed_if_true_with_lwc_else_1166() {
        assert_eq!(codes("<template><template lwc:if={a}>A</template><template if:true={a} lwc:else>B</template></template>", Config::default()), vec![1166]);
        assert_eq!(codes("<template><template lwc:if={a}>A</template><template if:true={a} lwc:elseif={b}>B</template></template>", Config::default()), vec![1166]);
    }

    /// no-directives-in-slots: the first slot throws 1082 after its lwc:if block began the chain, so the
    /// sibling lwc:else still has a chain (1082 again, not 1165).
    #[test]
    fn dropped_if_slot_keeps_chain_1082() {
        let src = "<template><slot lwc:if={c} name=\"x\"></slot><slot lwc:else name=\"x\"></slot></template>";
        assert_eq!(codes(src, Config::default()), vec![1082, 1082]);
        // A valid lwc:else after a dropped lwc:if element attaches to the (empty) if block: no 1165.
        let src = "<template><slot lwc:if={c}></slot><template lwc:else><p></p></template></template>";
        assert_eq!(codes(src, Config::default()), vec![1082]);
    }

    /// TS parseElementLocation: the parse5-implied `<tbody>` takes the `<table>` location (end tag included), so
    /// no 1078; 1144 is warned for the recovery.
    #[test]
    fn implied_tbody_uses_parent_location() {
        let c = codes("<template><table><tr><td>x</td></tr></table></template>", Config::default());
        assert!(!c.contains(&1078), "{c:?}");
        assert!(c.contains(&1144), "{c:?}");
    }

    /// AB-X0014, TS parseElementLocation + parseAttributes: an element parse5's adoption agency recreated
    /// without a location reads its attribute locations from the ancestor location it falls back to
    /// (here `div.z`, which has a `class`), so only 1144 is warned; an attribute that ancestor lacks
    /// (`title`) is TS's internal parsing error, reported alone and without a location.
    #[test]
    fn recreated_element_reads_attributes_from_fallback_location() {
        let c = codes("<template><div><b class=\"x\"><div class=\"z\">t</b>u</div></div></template>", Config::default());
        assert_eq!(c, vec![1144]);
        let d = diags("<template><div><b class=\"x\" title=\"q\"><div class=\"z\">t</b>u</div></div></template>");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].0, 1052);
        assert!(d[0].2.ends_with("Error parsing attribute: An internal parsing error occurred while parsing attributes; attributes were found without a location."), "{d:?}");
        assert_eq!(d[0].3, None);
        // TS reads each location just before normalizing that attribute: a throw on an earlier attribute
        // (1037, at the fallback `hidden`) ends parseAttributes before the unlocated `title` is read.
        let c = codes("<template><div><b hidden=\"true\" title=\"q\"><div hidden class=\"z\">t</b>u</div></div></template>", Config::default());
        assert_eq!(c, vec![1037, 1144, 1037]);
        let c = codes("<template><div><b title=\"q\" hidden=\"true\"><div hidden class=\"z\">t</b>u</div></div></template>", Config::default());
        assert_eq!(c, vec![1052]);
    }

    /// TS normalizeAttributeValue: isEscapedAttribute reads the raw (undecoded) value and only gates 1034/1035;
    /// getTemplateAttribute then parses the parse5-decoded value whenever `isExpression(value) && !escapedExpression`.
    /// So `&#123;a b&#125;` is the expression `{a b}` (1083), and so is a recreated element's attribute whose
    /// fallback raw value (`class="z"`) has no braces. Expected values from the oracle (b9e8007, `x.html`, `{}`).
    #[test]
    fn entity_encoded_attribute_value_is_an_expression() {
        let invalid = "Invalid expression {a b} - LWC1083: Error parsing template expression: Unexpected end of expression";
        assert_eq!(diags("<template><p class=\"&#123;a b&#125;\"></p></template>"),
            vec![(1083, DiagnosticLevel::Error, invalid.to_string(), Some((1, 14, Some(13), Some(23))))]);
        let d = diags("<template><b class=\"{a b}\" title=\"q\"><div class=\"z\">x</b>y</div></template>");
        assert_eq!(d.iter().map(|d| (d.0, d.1, d.3)).collect::<Vec<_>>(), vec![
            (1034, DiagnosticLevel::Error, Some((1, 14, Some(13), Some(13)))),
            (1144, DiagnosticLevel::Warning, Some((0, 0, Some(0), Some(0)))),
            (1083, DiagnosticLevel::Error, Some((1, 43, Some(42), Some(9)))),
        ]);
        assert_eq!(d[2].2, invalid);
        // `\{` is still the only escapedExpression: the decoded value stays a string.
        assert!(codes("<template><p title=\"\\&#123;a b&#125;\"></p></template>", Config::default()).is_empty());
        // Complex mode tests isPotentialComplexExpression first: the raw-source pass sees `&#123;`.
        let cfg = Config { experimental_complex_expressions: Some(true), ..Config::default() };
        let w = parse("<template><p title=\"&#123;a&#125;\"></p></template>", cfg).unwrap().warnings;
        assert_eq!(w.iter().map(|w| (w.code, w.message.as_str())).collect::<Vec<_>>(),
            vec![(Some(1083), "LWC1083: Error parsing template expression: Invalid expression {a} - Unexpected character '1' (1:2)")]);
    }

    /// slots/dynamic-components/usage: `<lwc:component>` is transparent for the slot-ancestor check.
    #[test]
    fn lwc_component_transparent_for_slot_ancestor() {
        let cfg = Config { enable_dynamic_components: Some(true), ..Config::default() };
        let src = "<template><lwc:component lwc:is={ctor}><p slot=\"header\">H</p><p slot>D</p></lwc:component></template>";
        let r = compile(src, "x.html", cfg).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    /// blacklisted-svg shape: 1057 (applyAttributes) is warned before the 1049 throw of the same element.
    #[test]
    fn attr_warning_before_later_throw() {
        let c = codes("<template><svg><filter id=\"f\" primitiveUnits=\"objectBoundingBox\"><feImage xlink:href=\"s.png\" preserveAspectRatio=\"none\"/></filter></svg></template>", Config::default());
        let i57 = c.iter().position(|x| *x == 1057);
        let i49 = c.iter().position(|x| *x == 1049);
        assert!(i57.is_some() && i49.is_some() && i57 < i49, "{c:?}");
    }

    /// Every diagnostic as (code, level, message, @lwc/errors location) from the template parser.
    fn diags(src: &str) -> Vec<(u32, DiagnosticLevel, String, Option<(u32, u32, Option<u32>, Option<u32>)>)> {
        parse(src, Config::default()).unwrap().warnings.into_iter()
            .map(|w| (w.code.expect("coded"), w.level, w.message.clone(), w.location.map(|l| (l.line, l.column, l.start, l.length))))
            .collect()
    }

    /// TS attributeExpressionReferencesForOfIndex (parser/index.ts:109-126) reads the parsed key: a computed
    /// `it[index]` is a MemberExpression with Identifier object and property, so it is 1066 wherever computed
    /// members parse (quoted under complex expressions, braced under experimentalComputedMemberExpression).
    /// Expected values from the oracle (@lwc/template-compiler b9e8007).
    #[test]
    fn computed_key_referencing_iterator_index_is_1066() {
        let msg = "LWC1066: Invalid key value for element <p>. Key cannot reference iterator index".to_string();
        let rows = [
            ("<template><template iterator:it={l}><p key=\"{it[index]}\"></p></template></template>", Config { experimental_complex_expressions: Some(true), ..Config::default() }, 17),
            ("<template><template iterator:it={l}><p key={it[index]}></p></template></template>", Config { experimental_computed_member_expression: Some(true), ..Config::default() }, 15),
        ];
        for (src, cfg, len) in rows {
            let got: Vec<_> = parse(src, cfg).unwrap().warnings.into_iter()
                .map(|w| (w.code, w.level, w.message.clone(), w.location.map(|l| (l.line, l.column, l.start, l.length))))
                .collect();
            assert_eq!(got, vec![(Some(1066), DiagnosticLevel::Error, msg.clone(), Some((1, 40, Some(39), Some(len))))], "{src}");
        }
    }

    /// TS `isDataAttribute` (DATA_RE, per UTF-16 unit) gates the applyAttributes 1057 warning; a `data-*` name with
    /// a char outside ATTRIBUTE_NAME_CHAR is not a data attribute. Expected values from the oracle
    /// (@lwc/template-compiler b9e8007 `compile(src, 'x.html', {})`).
    #[test]
    fn data_attribute_is_data_re() {
        let w57 = |n: &str, len: u32| (1057, DiagnosticLevel::Warning, format!("LWC1057: {n} is not valid attribute for div. For more information refer to https://developer.mozilla.org/en-US/docs/Web/HTML/Element/div"), Some((1, 16, Some(15), Some(len))));
        let e25 = |n: &str, len: u32| (1125, DiagnosticLevel::Error, format!("LWC1125: {n} is not valid attribute for div. Attribute name must end with alpha-numeric character."), Some((1, 16, Some(15), Some(len))));
        assert_eq!(diags("<template><div data-a!b=\"x\"></div></template>"), vec![w57("data-a!b", 12)]);
        assert_eq!(diags("<template><div data-a\u{1F600}></div></template>"), vec![w57("data-a\u{1F600}", 8), e25("data-a\u{1F600}", 8)]);
        assert_eq!(diags("<template><div data-\u{e9}></div></template>"), vec![e25("data-\u{e9}", 6)]);
        assert_eq!(diags("<template><div data-></div></template>"), vec![e25("data-", 5)]);
        assert!(diags("<template><div data-a:b data-x.y data-a_b></div></template>").is_empty());
    }

    /// D-0197: the HTML_ATTRIBUTE_ELEMENT_MAP lookup is own-key only, deliberately NOT upstream. TS indexes a plain
    /// object, so an Object.prototype key resolves to an inherited member: `<div constructor>` crashes with
    /// "Unexpected compilation error: LWC1052: Error parsing attribute: validElements.includes is not a function",
    /// `<div __proto__>` skips the 1057 warning (only 1125). Rust treats every such name like any other unknown attribute.
    /// The names come from node at test time; a missing node fails the test.
    #[test]
    fn prototype_keys_are_plain_unknown_attributes() {
        let out = std::process::Command::new("node")
            .args(["-e", "console.log([...new Set([...Object.getOwnPropertyNames(Object.prototype), '__proto__'])].join('\\n'))"])
            .output()
            .expect("node is required for this test (D-0197 prototype-key pin)");
        assert!(out.status.success(), "node failed: {}", String::from_utf8_lossy(&out.stderr));
        let names: Vec<String> = String::from_utf8(out.stdout).unwrap().lines().map(str::to_string).collect();
        assert!(names.iter().any(|n| n == "constructor") && names.iter().any(|n| n == "__proto__"), "{names:?}");
        let render = |name: &str| {
            let src = format!("<template><div {name}></div></template>");
            (format!("{:?}", parse(&src, Config::default()).unwrap().warnings), format!("{:?}", compile(&src, "x.html", Config::default()).unwrap()))
        };
        for name in &names {
            // Control: same length and letter case, not an Object.prototype key, not a known attribute.
            let ctl: String = name.chars().map(|c| if c.is_ascii_uppercase() { 'Z' } else if c.is_ascii_lowercase() { 'z' } else { c }).collect();
            let (diag, code) = render(name);
            assert!(!diag.contains("1052") && !code.contains("1052"), "{name}: {diag}");
            let sub = |s: String| s.replace(name.as_str(), &ctl).replace(&name.to_lowercase(), &ctl.to_lowercase());
            let (cdiag, ccode) = render(&ctl);
            assert_eq!(sub(diag), cdiag, "{name} vs {ctl}: diagnostics");
            assert_eq!(sub(code), ccode, "{name} vs {ctl}: compile result");
        }
    }

    /// TS `isAttribute` on Component / LwcComponent uses DATA_RE: `data-a!b` is not a data attribute, so it becomes
    /// a property (`dataA!b`) with no diagnostic; `data-ab` stays an attribute. Expected diagnostics and `stc0`
    /// blocks are the oracle's (@lwc/template-compiler b9e8007 `compile(src, 'x.html', opts)`; lwc:component with
    /// `enableDynamicComponents: true`).
    #[test]
    fn component_is_attribute_uses_data_re() {
        let stc0 = |kind: &str, name: &str, value: &str| format!("const stc0 = {{\n  {kind}: {{\n    \"{name}\": {value}\n  }},\n  key: 0\n}};");
        let dc = Config { enable_dynamic_components: Some(true), ..Config::default() };
        let cases = [
            ("<template><x-foo data-a!b=\"x\"></x-foo></template>", Config::default(), stc0("props", "dataA!b", "\"x\"")),
            ("<template><x-foo data-a!b></x-foo></template>", Config::default(), stc0("props", "dataA!b", "true")),
            ("<template><x-foo data-ab=\"x\"></x-foo></template>", Config::default(), stc0("attrs", "data-ab", "\"x\"")),
            ("<template><lwc:component lwc:is={c} data-a!b=\"x\"></lwc:component></template>", dc.clone(), stc0("props", "dataA!b", "\"x\"")),
            ("<template><lwc:component lwc:is={c} data-a!b></lwc:component></template>", dc.clone(), stc0("props", "dataA!b", "true")),
            ("<template><lwc:component lwc:is={c} data-ab=\"x\"></lwc:component></template>", dc, stc0("attrs", "data-ab", "\"x\"")),
        ];
        for (src, cfg, want) in cases {
            let r = compile(src, "x.html", cfg).unwrap();
            assert!(r.warnings.is_empty(), "{src}: {:?}", r.warnings);
            assert!(r.code.contains(&want), "{src}: want\n{want}\ngot\n{}", r.code);
        }
    }
}
