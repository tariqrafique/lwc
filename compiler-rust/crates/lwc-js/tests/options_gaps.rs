//! Option gaps #4-#9 vs shipped @lwc/compiler 9.5.3 (`transformSync`). Expected strings were captured from the
//! shipped npm tarballs (oracle script /tmp/lwc953/run/oracle.mjs); comparisons reprint both sides through oxc.
use lwc_shared::{CompileOptions, Sourcemap};
use oxc::{allocator::Allocator, codegen::Codegen, parser::{ParseOptions, Parser}, span::SourceType};

fn opts() -> CompileOptions { CompileOptions { name: "foo".into(), namespace: "x".into(), ..Default::default() } }
fn lws() -> CompileOptions { CompileOptions { enable_lightning_web_security_transforms: true, ..opts() } }
fn t(src: &str, o: &CompileOptions) -> String { lwc_js::transform(src, "foo.js", o).unwrap_or_else(|e| panic!("{e:?}")).code }
fn norm(src: &str) -> String {
    let alloc = Allocator::default();
    let r = Parser::new(&alloc, src, SourceType::mjs()).with_options(ParseOptions { preserve_parens: false, ..Default::default() }).parse();
    assert!(!r.diagnostics.has_errors(), "parse error in:\n{src}");
    Codegen::new().build(&r.program).code
}
fn codes(e: &lwc_shared::CompileError) -> Vec<u32> { e.diagnostics.iter().filter_map(|d| d.code).collect() }
const CMP: &str = "import { LightningElement, api } from 'lwc';\nexport default class Foo extends LightningElement {\n  @api value = 1;\n}\n";

// ---------- #4 LWS ----------
#[test]
fn lws_unforgeables_match_shipped() {
    let cases = [
        ("var v = a.top.b.top;", "var v = (a.top.b === globalThis ? top : a.top.b.top);"),
        ("let _location = 1; function f(location) { return window.location; }",
         "var _location2 = location;\nlet _location = 1;\nfunction f(location) {\n  return (window === globalThis || window === document ? _location2 : window.location);\n}"),
        ("function f(location, document, globalThis) { return window.location; }",
         "var _document = document,\n  _globalThis = globalThis,\n  _location = location;\nfunction f(location, document, globalThis) {\n  return (window === _globalThis || window === _document ? _location : window.location);\n}"),
        ("window.top.top.location = 1;",
         "((window === globalThis ? top : window.top.top) === globalThis || (window === globalThis ? top : window.top.top) === document ? location.assign(1) : (window === globalThis ? top : window.top.top).location = 1);"),
    ];
    for (src, shipped) in cases {
        assert_eq!(norm(&t(src, &lws())), norm(shipped), "input: {src}");
    }
}

fn lws_err(src: &str) -> lwc_shared::CompileError { lwc_js::transform(src, "foo.js", &lws()).unwrap_err() }

/// Shipped 9.5.3 throws (Babel validator TypeError, wrapped as LWC1007) for unforgeable for-in/of member targets.
#[test]
fn lws_for_in_of_member_target_throws_like_shipped() {
    for (src, kind) in [("for (x.location in o) {}", "ForInStatement"), ("for (x.location of o) {}", "ForOfStatement"), ("async function f() { for (window.top of o) { await 1; } }", "ForOfStatement")] {
        let e = lws_err(src);
        assert_eq!(codes(&e), vec![1007], "{src}");
        assert!(e.message.contains(&format!("foo.js: Property left of {kind} expected node to be of a type [\"VariableDeclaration\",\"LVal\"] but instead got \"ParenthesizedExpression\"")), "{}", e.message);
    }
}

/// Shipped emits invalid JS for these (conditional as an assignment target / `super === globalThis`); we fail closed.
#[test]
fn lws_invalid_shipped_shapes_fail_closed() {
    for (src, needle) in [
        ("({ a: x.location } = o);", "`x.location` used as a destructuring assignment target"),
        ("[window.top] = o;", "`window.top` used as a destructuring assignment target"),
        ("async function f() { for ({ a: x.top } of await o) {} }", "`x.top` used as a destructuring assignment target"),
        ("class A extends B { m() { return super.location; } }", "`super.location` on `super`"),
        ("class A extends B { async m() { return super.top; } }", "`super.top` on `super`"),
        ("class A extends B { m() { super.location = 1; } }", "`super.location` on `super`"),
    ] {
        let e = lws_err(src);
        assert_eq!(codes(&e), vec![1007], "{src}");
        assert!(e.message.contains(needle), "{src}: {}", e.message);
    }
    // the same inputs compile untouched with LWS off
    assert_eq!(norm(&t("({ a: x.location } = o);", &opts())), norm("({ a: x.location } = o);"));
}

/// Matched as shipped: update expressions leave the access native (shipped output is valid there).
#[test]
fn lws_update_expression_matches_shipped() {
    assert_eq!(norm(&t("x.location++; class A extends B { m() { super.location++; } }", &lws())), norm("x.location++; class A extends B { m() { super.location++; } }"));
}

#[test]
fn lws_off_leaves_unforgeables() {
    assert_eq!(norm(&t("var v = window.location;", &opts())), norm("var v = window.location;"));
}

#[test]
fn lws_lowers_async_with_module_local_helpers() {
    let out = t("export async function f() { await 1; return window.location; }\nexport async function* g() { yield globalThis.top; }\n", &lws());
    // shipped 9.5.3 inlines module-local helpers (`_asyncToGenerator`, `_wrapAsyncGenerator`, ...) - no `babelHelpers` global
    assert!(!out.contains("babelHelpers"), "{out}");
    assert!(out.contains("function _asyncToGenerator("), "{out}");
    assert!(out.contains("_asyncToGenerator(function* () {"), "{out}");
    assert!(out.contains("_wrapAsyncGenerator(function* () {"), "{out}");
    assert!(out.contains("return window === globalThis || window === document ? location : window.location;"), "{out}");
    assert!(out.contains("yield globalThis === globalThis ? top : globalThis.top;"), "{out}");
    assert!(!out.contains("async "), "{out}");
    // without LWS, async stays native and no helpers
    let plain = t("async function f() { return window.location; }\n", &opts());
    assert!(plain.contains("async function f") && plain.contains("return window.location") && !plain.contains("_asyncToGenerator"), "{plain}");
}

#[test]
fn lws_on_component_compiles() {
    let out = t("import { LightningElement } from 'lwc';\nexport default class Foo extends LightningElement { async m() { await 0; return window.top; } }\n", &lws());
    assert!(out.contains("return _asyncToGenerator(function* () {"), "{out}");
    assert!(out.contains("function _asyncToGenerator(") && !out.contains("babelHelpers"), "{out}");
    assert!(out.contains("return window === globalThis ? top : window.top;"), "{out}");
    assert!(out.contains("_registerComponent(Foo"), "{out}");
}

// ---------- #6 sourcemap ----------
fn check_map(json: &str, src: &str, out: &str) {
    let m = oxc_sourcemap::SourceMap::from_json_string(json).expect("valid map json");
    assert_eq!(m.get_sources().map(|s| s.to_string()).collect::<Vec<_>>(), vec!["foo.js".to_string()]);
    assert_eq!(m.get_source_contents().map(|s| s.map(|s| s.to_string())).collect::<Vec<_>>(), vec![Some(src.to_string())]);
    let (src_lines, out_lines): (Vec<&str>, Vec<&str>) = (src.split('\n').collect(), out.split('\n').collect());
    let mut n = 0;
    for tok in m.get_tokens() {
        n += 1;
        let (dl, dc) = (tok.get_dst_line() as usize, tok.get_dst_col() as usize);
        assert!(dl < out_lines.len() && dc <= out_lines[dl].encode_utf16().count(), "dst out of range {dl}:{dc}");
        let (sl, sc) = (tok.get_src_line() as usize, tok.get_src_col() as usize);
        assert!(sl < src_lines.len() && sc <= src_lines[sl].encode_utf16().count(), "src out of range {sl}:{sc}");
    }
    assert!(n > 0, "empty mappings");
}

#[test]
fn sourcemap_true_returns_valid_map() {
    let o = CompileOptions { sourcemap: Sourcemap::On, ..opts() };
    let r = lwc_js::transform(CMP, "foo.js", &o).unwrap();
    check_map(r.map.as_deref().expect("map"), CMP, &r.code);
    assert!(!r.code.contains("sourceMappingURL"));
    // `export default class Foo` maps `class Foo` back to line 2
    let m = oxc_sourcemap::SourceMap::from_json_string(r.map.as_deref().unwrap()).unwrap();
    let cls_line = r.code.split('\n').position(|l| l.starts_with("class Foo")).unwrap() as u32;
    assert!(m.get_tokens().any(|t| t.get_dst_line() == cls_line && t.get_src_line() == 1), "class line not mapped");
}

#[test]
fn sourcemap_true_through_lws_and_rest_spread() {
    let src = "export async function f(a) { const o = { ...a }; return window.location; }\n";
    for api in [Some(59), None] {
        let o = CompileOptions { sourcemap: Sourcemap::On, api_version: api, ..lws() };
        let r = lwc_js::transform(src, "foo.js", &o).unwrap();
        check_map(r.map.as_deref().unwrap(), src, &r.code);
    }
}

#[test]
fn sourcemap_inline_appends_data_url() {
    let o = CompileOptions { sourcemap: Sourcemap::Inline, ..opts() };
    let r = lwc_js::transform(CMP, "foo.js", &o).unwrap();
    assert!(r.map.is_none());
    let last = r.code.trim_end().rsplit('\n').next().unwrap();
    assert!(last.starts_with("//# sourceMappingURL=data:application/json;charset=utf-8;base64,"), "{last}");
    assert!(!r.code.contains("\n\n//# sourceMappingURL"), "double newline before comment");
}

#[test]
fn sourcemap_off_by_default() {
    let r = lwc_js::transform(CMP, "foo.js", &opts()).unwrap();
    assert!(r.map.is_none() && !r.code.contains("sourceMappingURL"));
}

#[test]
fn sourcemap_invalid_string_is_lwc1021() {
    let e = Sourcemap::from_js_str("foo", "foo.js").unwrap_err();
    assert_eq!(codes(&e), vec![1021]);
}

// ---------- #5 isExplicitImport ----------
#[test]
fn explicit_import_skips_register_component() {
    let o = CompileOptions { is_explicit_import: true, ..opts() };
    let out = t(CMP, &o);
    assert!(!out.contains("registerComponent") && !out.contains(".html"), "{out}");
    // shipped still emits registerDecorators for the @api field
    assert!(out.contains("_registerDecorators(Foo, {"), "{out}");
    assert!(out.contains("publicProps: { value: { config: 0 } }") || out.contains("publicProps: {\n"), "{out}");
    let implicit = t(CMP, &opts());
    assert!(implicit.contains("_registerComponent(Foo"), "{implicit}");
}

// ---------- #7 enablePrivateMethods ----------
const PRIV: &str = "import { LightningElement } from 'lwc';\nexport default class Foo extends LightningElement { #m() { return 1; } go() { return this.#m(); } }\n";
#[test]
fn private_methods_default_and_false_reject_like_shipped() {
    for o in [opts(), CompileOptions { enable_private_methods: false, ..opts() }] {
        let e = lwc_js::transform(PRIV, "foo.js", &o).unwrap_err();
        assert_eq!(codes(&e), vec![1007]);
        assert!(e.message.contains("Class private methods are not enabled. Please add `@babel/plugin-transform-private-methods` to your configuration."), "{}", e.message);
    }
}

#[test]
fn private_methods_true_stays_native_and_flags_component() {
    let out = t(PRIV, &CompileOptions { enable_private_methods: true, ..opts() });
    assert!(out.contains("#m() {") && out.contains("this.#m()"), "{out}");
    assert!(out.contains("enablePrivateMethods: true"), "{out}");
    assert!(!t("import { LightningElement } from 'lwc';\nexport default class Foo extends LightningElement {}\n", &opts()).contains("enablePrivateMethods"));
}

// ---------- #8 dynamicImports.strictSpecifier ----------
const DYN: &str = "export function f(x) { return import(x); }\n";
#[test]
fn strict_specifier_defaults_true() {
    assert!(opts().dynamic_import_strict_specifier);
    for loader in ["", "myLoader"] {
        let e = lwc_js::transform(DYN, "foo.js", &CompileOptions { dynamic_import_loader: loader.into(), ..opts() }).unwrap_err();
        assert_eq!(codes(&e), vec![1121], "loader={loader:?}");
        assert!(e.message.contains("Invalid import. The argument \"x\" must be a stringLiteral for dynamic imports when strict mode is enabled."), "{}", e.message);
    }
}

#[test]
fn strict_specifier_false_with_loader() {
    let o = CompileOptions { dynamic_import_loader: "myLoader".into(), dynamic_import_strict_specifier: false, ..opts() };
    let out = t(DYN, &o);
    assert!(out.contains("import { load as _load } from \"myLoader\""), "{out}");
    assert!(out.contains("return _load(x);"), "{out}");
}

// ---------- #9 apiVersion ----------
const API: &str = "import { LightningElement } from 'lwc';\nexport default class Foo extends LightningElement { plain = 1; m(a) { return { ...a }; } }\n";
#[test]
fn api_59_lowers_object_spread() {
    let out = t(API, &CompileOptions { api_version: Some(59), ..opts() });
    assert!(!out.contains("{ ...a }"), "{out}");
    assert!(out.contains("_objectSpread({}, a)") && out.contains("function _objectSpread(") && !out.contains("babelHelpers"), "{out}");
    assert!(out.contains("_registerDecorators(Foo, { fields: [\"plain\"] })"), "{out}");
    assert!(out.contains("apiVersion: 59"), "{out}");
}

#[test]
fn api_60_and_omitted_keep_object_spread() {
    for (api, stamp) in [(Some(60), "apiVersion: 60"), (None, "apiVersion: 66")] {
        let out = t(API, &CompileOptions { api_version: api, ..opts() });
        assert!(out.contains("{ ...a }"), "{out}");
        assert!(!out.contains("babelHelpers"), "{out}");
        assert!(out.contains("_registerDecorators(Foo, { fields: [\"plain\"] })"), "{out}");
        assert!(out.contains(stamp), "{out}");
    }
}

// ---------- #6 sourcemap identifier fidelity ----------
fn ident_at(line: &str, col_utf16: usize) -> Option<String> {
    let u: Vec<u16> = line.encode_utf16().collect();
    if col_utf16 >= u.len() { return None; }
    let rest = String::from_utf16_lossy(&u[col_utf16..]);
    let is_start = |c: char| c == '_' || c == '$' || c.is_alphabetic();
    let prev_ok = col_utf16 == 0 || !String::from_utf16_lossy(&u[col_utf16 - 1..col_utf16]).chars().all(|c| c == '_' || c == '$' || c.is_alphanumeric());
    let first = rest.chars().next()?;
    if !is_start(first) || !prev_ok { return None; }
    Some(rest.chars().take_while(|c| *c == '_' || *c == '$' || c.is_alphanumeric()).collect())
}

/// Every mapping that carries a `name`, and every mapping whose generated position starts an identifier, must point
/// at the same identifier in the original source.
fn check_identifiers(json: &str, src: &str, out: &str) -> usize {
    let m = oxc_sourcemap::SourceMap::from_json_string(json).unwrap();
    let (sl, ol): (Vec<&str>, Vec<&str>) = (src.split('\n').collect(), out.split('\n').collect());
    let mut checked = 0;
    for tok in m.get_tokens() {
        let gid = ident_at(ol[tok.get_dst_line() as usize], tok.get_dst_col() as usize);
        let orig = ident_at(sl[tok.get_src_line() as usize], tok.get_src_col() as usize);
        if let Some(id) = tok.get_name_id() {
            let name = m.get_name(id).unwrap();
            assert_eq!(orig.as_deref(), Some(&*name), "name {name} maps to {orig:?} at {}:{}\n{out}", tok.get_src_line() + 1, tok.get_src_col());
            checked += 1;
        }
        if let Some(g) = gid {
            assert_eq!(orig.as_deref(), Some(g.as_str()), "generated `{g}` at {}:{} maps to {orig:?} at {}:{}\n{out}",
                tok.get_dst_line() + 1, tok.get_dst_col(), tok.get_src_line() + 1, tok.get_src_col());
            checked += 1;
        }
    }
    checked
}

#[test]
fn sourcemap_identifiers_resolve_to_same_name_and_line() {
    let plain = "export function add(a, b) {\n  const sum = a + b;\n  return sum;\n}\n";
    let deco = "import { LightningElement, api, track, wire } from 'lwc';\nimport getThing from '@salesforce/apex/Ctl.getThing';\nexport default class Foo extends LightningElement {\n  @api value = 1;\n  @track state = { n: 0 };\n  @wire(getThing, { id: '$value' }) thing;\n  @api get label() { return this.value; }\n  set label(v) { this.value = v; }\n  handle(evt) { this.state = { n: evt.detail }; }\n}\n";
    let lws_src = "export async function load(url) {\n  const res = await fetch(url);\n  const where = window.location;\n  return { res, where, top: window.top };\n}\n";
    let spread = "export function merge(a, b) {\n  const { x, ...rest } = a;\n  return { ...rest, ...b, x };\n}\n";
    let cases: [(&str, CompileOptions); 4] = [
        (plain, opts()),
        (deco, opts()),
        (lws_src, lws()),
        (spread, CompileOptions { api_version: Some(59), ..opts() }),
    ];
    for (src, o) in cases {
        let o = CompileOptions { sourcemap: Sourcemap::On, ..o };
        let r = lwc_js::transform(src, "foo.js", &o).unwrap();
        let n = check_identifiers(r.map.as_deref().unwrap(), src, &r.code);
        assert!(n > 3, "too few identifier mappings ({n}) for:\n{}", r.code);
    }
}
