//! AB-X0013 / AB-L0056 seam: the LWC1121 `transformSync` throws carries the Babel location of the
//! `import(...)` node, in UTF-16 units, with non-BMP text before it, plus the code, level and url 9.5.3 gives it.
//!
//! Expected values are not typed here: each test runs shipped @lwc/compiler 9.5.3 live in node
//! (`$LWC_ORACLE`, which must be at b9e8007) and compares. Missing node or a
//! moved oracle is a failure, not a skip.
use lwc_shared::{CompileOptions, SsrMode, TransformError};
use std::process::Command;

const SRC: &str = "const s = 'é😀'; import(foo)";
const ORACLE_SHA: &str = "b9e8007";

fn opts(target_ssr: bool) -> CompileOptions {
    CompileOptions {
        name: "foo".into(),
        namespace: "x".into(),
        target_ssr,
        ssr_mode: SsrMode::Sync,
        dynamic_import_strict_specifier: true,
        ..Default::default()
    }
}

/// `[code, level, url, line, column, start, length]` of what 9.5.3 `transformSync` throws for SRC.
fn oracle(target_ssr: bool) -> String {
    let root = lwc_shared::oracle_root().to_str().expect("LWC_ORACLE must be UTF-8").to_string();
    let head = Command::new("git").args(["-C", &root, "rev-parse", "HEAD"]).output().expect("git is required");
    let head = String::from_utf8(head.stdout).unwrap();
    assert!(head.starts_with(ORACLE_SHA), "oracle {root} is at {head:?}, want {ORACLE_SHA}");
    let script = format!(
        "const {{ transformSync }} = require({root:?} + '/packages/@lwc/compiler');
         try {{ transformSync({SRC:?}, 'foo.js', {{ name: 'foo', namespace: 'x', targetSSR: {target_ssr}, ssrMode: 'sync',
             dynamicImports: {{ strictSpecifier: true }} }}); console.log('no throw'); }}
         catch (e) {{ const l = e.location; console.log(JSON.stringify([e.code, e.level, e.url, l.line, l.column, l.start, l.length])); }}"
    );
    let out = Command::new("node").arg("-e").arg(script).output().expect("node is required");
    assert!(out.status.success(), "oracle failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The facade's error (what napi serializes field for field) in the oracle's shape.
fn ours(target_ssr: bool) -> String {
    let TransformError::Compiler(e) = lwc_compiler::dispatch(SRC, "foo.js", &opts(target_ssr)).unwrap_err() else {
        panic!("not a CompilerError")
    };
    let l = e.location.expect("location");
    format!(
        "[{},{},\"{}\",{},{},{},{}]",
        e.code.unwrap(), e.level.js_value(), e.url, l.line, l.column, l.start.unwrap(), l.length.unwrap()
    )
}

/// SSR (@lwc/ssr-compiler `generateError`): the whole `import(foo)` node, the LWC1121 info's level and url.
#[test]
fn ssr_lwc1121_matches_9_5_3() {
    assert_eq!(ours(true), oracle(true));
}

/// CSR (babel-plugin-component, normalized by javascript.ts): the `foo` argument.
#[test]
fn csr_lwc1121_matches_9_5_3() {
    assert_eq!(ours(false), oracle(false));
}
