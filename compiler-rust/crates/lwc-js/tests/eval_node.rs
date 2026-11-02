//! Runs compiled output under node (ESM, no `babelHelpers` global) to check runtime behaviour shipped 9.5.3 has:
//! loose private fields (work through a Proxy) and module-local helpers (no global reference, no global leak).
use lwc_shared::CompileOptions;
use std::process::Command;

fn opts() -> CompileOptions { CompileOptions { name: "foo".into(), namespace: "x".into(), ..Default::default() } }

/// Evaluates `module` followed by `driver` as one ES module; returns stdout. Fails on a non-zero exit.
fn run(module: &str, driver: &str) -> String {
    let src = format!("{module}\n;{driver}\n");
    let out = Command::new("node").args(["--input-type=module", "-e", &src]).output().expect("node on PATH");
    assert!(out.status.success(), "node failed:\n{}\n--- module ---\n{src}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

const PRIV: &str = "export class C {\n  #x = 41;\n  static #s = 1;\n  get x() { return this.#x + C.#s; }\n  set x(v) { this.#x = v; }\n  has(o) { return #x in o; }\n}\n";
const DRIVER: &str = "if (typeof babelHelpers !== 'undefined') throw new Error('babelHelpers global');\nconst p = new Proxy(new C(), {});\np.x = p.x;\nconsole.log(p.x, new C().has(p), Object.keys(new C()).length);";

#[test]
fn private_fields_work_through_proxy() {
    for flag in [false, true] {
        let o = CompileOptions { enable_private_methods: flag, ..opts() };
        let code = lwc_js::transform(PRIV, "foo.js", &o).unwrap().code;
        assert!(code.contains("_classPrivateFieldLooseKey(\"x\")") && code.contains("Object.defineProperty(this, _x, {"), "{code}");
        assert!(!code.contains("this.#x") && !code.contains("babelHelpers"), "{code}");
        // loose keys are non-enumerable: `Object.keys` stays empty, the Proxy forwards `get`/`set`/`has`
        assert_eq!(run(&code, DRIVER).trim(), "43 true 0", "flag={flag}");
    }
}

#[test]
fn native_private_fields_would_fail_through_proxy() {
    // sanity: the driver really distinguishes loose from native semantics
    let out = Command::new("node").args(["--input-type=module", "-e", &format!("{PRIV}\n;{DRIVER}")]).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("TypeError"));
}

#[test]
fn helpers_are_module_local_without_global() {
    let lws = CompileOptions { enable_lightning_web_security_transforms: true, api_version: Some(59), ..opts() };
    let src = "export async function f(a) { await null; return { ...a, loc: typeof window === 'undefined' ? 1 : window.location }; }\nexport async function* g() { yield* [1, 2]; }\nexport class K { #k = 5; get k() { return this.#k; } }\n";
    let code = lwc_js::transform(src, "foo.js", &lws).unwrap().code;
    assert!(!code.contains("babelHelpers"), "{code}");
    assert!(code.contains("function _asyncToGenerator(") && code.contains("function _objectSpread(") && code.contains("_classPrivateFieldLooseKey"), "{code}");
    let driver = "const r = await f({ a: 1 });\nconst ys = []; for await (const y of g()) ys.push(y);\n\
        const leaked = ['babelHelpers', '_asyncToGenerator', '_objectSpread', '_classPrivateFieldLooseKey', 'id'].filter((n) => n in globalThis);\n\
        console.log(JSON.stringify([r, ys, new K().k, leaked]));";
    assert_eq!(run(&code, driver).trim(), r#"[{"a":1,"loc":1},[1,2],5,[]]"#);
}
