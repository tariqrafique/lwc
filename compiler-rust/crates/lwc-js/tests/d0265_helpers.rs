//! AB-L0075 / D-0265: localize always runs; one prelude is emitted from the union of helper sets.
use lwc_shared::CompileOptions;

fn lws() -> CompileOptions {
    CompileOptions {
        enable_lightning_web_security_transforms: true,
        name: "foo".into(),
        namespace: "x".into(),
        ..Default::default()
    }
}

fn compile(src: &str) -> String {
    lwc_js::transform(src, "foo.js", &lws()).unwrap_or_else(|e| panic!("{e:?}")).code
}

/// Arch's repro: LWS on, `async` appears only in comments that codegen drops.
const COMMENT_ASYNC: &str = "export default class A extends LightningElement { #p = 1 /* async */ + 2; get x(){ return this.#p /* async */; } }";

/// LWS plus a real async method plus a private field.
const REAL_ASYNC: &str = "export default class A extends LightningElement { #p = 1; async m() { return this.#p; } }";

/// Helper names in the order `helpers.rs` emits them.
const HELPERS_ORDER: &[&str] = &[
    "defineProperty",
    "toPrimitive",
    "toPropertyKey",
    "objectSpread2",
    "objectWithoutPropertiesLoose",
    "objectWithoutProperties",
    "objectDestructuringEmpty",
    "extends",
    "asyncToGenerator",
    "OverloadYield",
    "awaitAsyncGenerator",
    "wrapAsyncGenerator",
    "asyncGeneratorDelegate",
    "classPrivateFieldLooseBase",
    "classPrivateFieldLooseKey",
    "checkInRHS",
    "asyncIterator",
];

fn prelude_helper_order(code: &str) -> Vec<&'static str> {
    // Map emitted local names back to helper table names by looking for
    // `function _<base>` / `var _<base>` in source order. generateUid strips a
    // trailing digit-run (`objectSpread2` -> `_objectSpread`).
    let mut found = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let rest = &code[pos..];
        let next = ["function _", "var _"]
            .iter()
            .filter_map(|p| rest.find(p).map(|i| (i, *p)))
            .min_by_key(|(i, _)| *i);
        let Some((i, pref)) = next else { break };
        let start = pos + i + pref.len();
        let name_end = code[start..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(code.len() - start);
        let local = format!("_{}", &code[start..start + name_end]);
        pos = start + name_end;
        // Skip `var id = 0`.
        if local == "_id" { continue; }
        if let Some(h) = HELPERS_ORDER.iter().copied().find(|h| {
            let base = h.trim_start_matches('_').trim_end_matches(|c: char| c.is_ascii_digit());
            local == format!("_{base}") || local.starts_with(&format!("_{base}"))
        }) {
            if !found.contains(&h) { found.push(h); }
        }
    }
    found
}

#[test]
fn lws_comment_only_async_matches_main_and_has_no_babelhelpers() {
    let out = compile(COMMENT_ASYNC);
    assert!(!out.contains("babelHelpers."), "comment-only async left bare helpers:\n{out}");
    assert!(out.contains("_classPrivateFieldLooseKey"), "{out}");
    assert!(out.contains("_classPrivateFieldLooseBase"), "{out}");
    let main = include_str!("goldens/ab-l0075-comment-async.js");
    assert_eq!(out, main, "differs from main 0b903dd output");
}

#[test]
fn lws_real_async_and_private_field_one_prelude_matches_main() {
    let out = compile(REAL_ASYNC);
    assert!(!out.contains("babelHelpers."), "{out}");
    let order = prelude_helper_order(&out);
    let mut sorted = order.clone();
    sorted.sort_by_key(|h| HELPERS_ORDER.iter().position(|x| x == h).unwrap());
    assert_eq!(order, sorted, "prelude not in HELPERS order: {order:?}\n{out}");
    // Each helper appears once as a declaration.
    for h in &order {
        let base = h.trim_start_matches('_').trim_end_matches(|c: char| c.is_ascii_digit());
        let fn_hits = out.matches(&format!("function _{base}")).count();
        let var_hits = out.matches(&format!("var _{base}")).count();
        assert_eq!(fn_hits + var_hits, 1, "{h} declared {} times\n{out}", fn_hits + var_hits);
    }
    let main = include_str!("goldens/ab-l0075-real-async.js");
    assert_eq!(out, main, "differs from main 0b903dd output");
}

#[test]
fn prelude_is_a_pure_union_and_does_not_generate_uids() {
    let src = include_str!("../src/helpers.rs");
    let start = src.find("pub fn prelude(").expect("prelude");
    let after = &src[start + "pub fn prelude(".len()..];
    let end = after.find("\npub fn ").expect("next pub fn after prelude");
    let body = &after[..end];
    for needle in ["Uids", "from_reserved", "remember", "close_deps", ".generate(", ".reserve("] {
        assert!(!body.contains(needle), "prelude must not {needle:?}:\n{body}");
    }
}
