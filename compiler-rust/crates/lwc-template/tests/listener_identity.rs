//! AB-0014 behaviour lock: TS genEventListeners memoizes listeners on `$ctx` (`_mN`), so a re-render
//! with the same `$ctx` hands the engine the same `on` object / handler functions and no listener
//! is re-attached. Runs the compiled `tmpl` twice under node with a stub `$api` whose `api_bind`
//! returns a fresh function on every call, so only memoization can make them identical.

use std::process::Command;

const STUBS: &str = r#"
const _implicitStylesheets = undefined, _implicitScopedStylesheets = undefined;
const freezeTemplate = () => {}, registerTemplate = (t) => t, parseFragment = () => null, parseSVGFragment = () => null;
const $api = new Proxy({}, { get: (_, k) => ({
  h: (sel, data, children) => ({ sel, data, children }),
  b: (fn) => function bound(e) { return fn.call(this, e); },
  i: (list, fn) => list.map((x, i) => fn(x, i)),
  f: (a) => a.flat(),
  k: (a, b) => `${a}:${b}`,
  st: (frag, key, parts) => ({ sel: "static", key, data: parts[0].data }),
  sp: (id, data, text) => ({ id, data, text }),
})[k] ?? ((...a) => a) });
"#;

const DRIVER: &str = r#"
const vnodes = (r) => r.flat(Infinity).filter((v) => v && v.sel);
const cmp = { list: [{ id: 1, del() {} }, { id: 2, del() {} }], handleClick() {}, foo() {} };
const ctx = {};
const [b1, l1, m1] = vnodes(tmpl($api, cmp, {}, ctx));
const [b2, l2, m2] = vnodes(tmpl($api, cmp, {}, ctx));
const [, l4] = vnodes(tmpl($api, cmp, {}, ctx));
const [b3] = vnodes(tmpl($api, cmp, {}, {}));
console.log(JSON.stringify({
  onObject: b1.data.on === b2.data.on,
  // `_mN` is read once per render, so the first render's items each bind; from the second render
  // on every item shares the memoized handler.
  nonLocalHandler: l2.data.on.touchstart === m2.data.on.touchstart && l2.data.on.touchstart === l4.data.on.touchstart,
  localHandlerPerItem: l1.data.on.click !== m1.data.on.click,
  freshCtx: b1.data.on !== b3.data.on,
}));
"#;

/// Element databags (static content optimization off) and static parts (on) both memoize.
#[test]
fn listeners_keep_identity_across_rerender() {
    for static_opt in [false, true] {
        run(static_opt);
    }
}

fn run(static_opt: bool) {
    let src = "<template><button onclick={handleClick}>x</button><template for:each={list} for:item=\"task\"><li key={task.id} onclick={task.del} ontouchstart={foo}></li></template></template>";
    let cfg = lwc_template::Config { enable_static_content_optimization: Some(static_opt), ..Default::default() };
    let code = lwc_template::compile(src, "x.html", cfg).unwrap().code;
    assert_eq!(code.contains("api_static_part"), static_opt, "{code}");
    assert!(code.contains("$ctx._m"), "{code}");
    let body: String = code.lines()
        .filter(|l| !l.starts_with("import ") && !l.starts_with("export default "))
        .map(|l| format!("{l}\n")).collect();
    let out = Command::new("node").args(["--input-type=module", "-e", &format!("{STUBS}\n{body}\n{DRIVER}")]).output().expect("node on PATH");
    assert!(out.status.success(), "{}\n{code}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        r#"{"onObject":true,"nonLocalHandler":true,"localHandlerPerItem":true,"freshCtx":true}"#,
        "{code}"
    );
}
