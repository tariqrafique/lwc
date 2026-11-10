//! Fixture harness over @lwc/style-compiler's JS fixtures, then the crate-local
//! `tests/parse-errors` (generated from the JS oracle by tools/style-diff/gen-parse-errors.mjs), whose
//! `compiler-error.json` also pins the `@lwc/compiler` transformSync error (message/code/location) via [`transform`].
//! Env: LWC_FIXTURES=dir override, LWC_FIXTURE=name filter, LWC_STYLE_STRICT=1 to fail on any mismatch,
//! LWC_STYLE_DIFF=1 to print expected/actual for failures.
use lwc_shared::{CompilerError, TransformError};
use lwc_style::{transform_with_config, CompileOptions, Config, StyleError};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn normalize_version(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("LWC compiler v") {
        out.push_str(&rest[..i]);
        out.push_str("LWC compiler vX.X.X");
        rest = &rest[i + "LWC compiler v".len()..];
        let end = rest.find("*/").unwrap_or(rest.len());
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn err_json(e: &StyleError) -> Value {
    match e {
        // jsErr / normalizeError: a non-CssSyntaxError is `{name, message}`.
        StyleError::Syntax { name, reason, .. } if name != "CssSyntaxError" => json!({"name": name, "message": reason}),
        StyleError::Syntax { name, reason, line, column } => {
            // JSON.stringify drops `undefined` line/column (position-less PostCSS errors).
            let mut o = json!({"name": name, "reason": reason});
            if let Some(c) = column { o["column"] = json!(c); }
            if let Some(l) = line { o["line"] = json!(l); }
            o
        }
        StyleError::Aggregate(v) => Value::Array(v.iter().map(err_json).collect()),
    }
}

/// `@lwc/compiler` transformSync error as JS shows it (see gen-parse-errors.mjs `compilerError`).
fn compiler_err_json(e: &TransformError) -> Value {
    let cwd = format!("{}/", std::env::current_dir().unwrap().display());
    let one = |e: &CompilerError| {
        let mut o = json!({
            "name": "Error", "message": e.message.replace(&cwd, "<cwd>/"), "code": e.code, "filename": e.filename,
            "level": e.level.js_value(), "url": e.url,
        });
        if let Some(l) = &e.location { o["location"] = json!({"line": l.line, "column": l.column}); }
        o
    };
    match e {
        TransformError::Aggregate(a) => json!({"name": "AggregateError", "message": a.message, "errors": a.errors.iter().map(one).collect::<Vec<_>>()}),
        TransformError::Compiler(e) => one(e),
        TransformError::Unnormalized(e) => panic!("style errors are normalized: {e:?}"),
    }
}

fn run_dir(root: &PathBuf, filter: &Option<String>, diff: bool, pass: &mut usize, fail: &mut Vec<String>) {
    let mut dirs: Vec<_> = fs::read_dir(root).expect("fixtures dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("actual.css").exists()).collect();
    dirs.sort();
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if filter.as_ref().is_some_and(|f| !name.contains(f.as_str())) { continue; }
        let src = fs::read_to_string(dir.join("actual.css")).unwrap();
        let cfg: Value = fs::read_to_string(dir.join("config.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}));
        let b = |k: &str| cfg.get(k).and_then(Value::as_bool).unwrap_or(false);
        let config = Config { scoped: b("scoped"), disable_synthetic_shadow_support: b("disableSyntheticShadowSupport"), experimental_error_recovery_mode: b("experimentalErrorRecoveryMode"), api_version: cfg.get("apiVersion").and_then(Value::as_u64).map(|v| v as u32) };
        let exp_js = fs::read_to_string(dir.join("expected.js")).unwrap_or_default();
        let exp_err = fs::read_to_string(dir.join("error.json")).unwrap_or_default();
        let cfg2 = config.clone();
        let res = std::panic::catch_unwind(|| transform_with_config(&src, &format!("{}/actual.css", dir.display()), cfg2));
        let (mut ok, mut got) = match res {
            Err(_) => (false, "PANIC".to_string()),
            Ok(Ok(code)) => { let c = normalize_version(&code); (exp_err.trim().is_empty() && c == exp_js, c) }
            Ok(Err(e)) => {
                let got = err_json(&e);
                let want: Option<Value> = serde_json::from_str(&exp_err).ok();
                (exp_js.is_empty() && want.as_ref() == Some(&got), serde_json::to_string_pretty(&got).unwrap())
            }
        };
        // Facade parity with @lwc/compiler transformSync, where the oracle pinned it.
        if let Ok(want) = fs::read_to_string(dir.join("compiler-error.json")) {
            let mut want: Value = serde_json::from_str(&want).unwrap();
            want.as_object_mut().unwrap().remove("ownKeys");
            let opts = CompileOptions {
                name: "x".into(), namespace: "x".into(), scoped_styles: Some(config.scoped),
                disable_synthetic_shadow_support: config.disable_synthetic_shadow_support,
                experimental_error_recovery_mode: config.experimental_error_recovery_mode,
                ..Default::default()
            };
            let facade = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| lwc_compiler::dispatch(&src, "x.css", &opts))) {
                Ok(Err(e)) => compiler_err_json(&e),
                Ok(Ok(_)) => json!("compiled"),
                Err(_) => json!("PANIC"),
            };
            if facade != want {
                ok = false;
                got = format!("{got}\n--- facade\n{}\n--- facade expected\n{}", serde_json::to_string_pretty(&facade).unwrap(), serde_json::to_string_pretty(&want).unwrap());
            }
        }
        if ok { *pass += 1; println!("PASS {name}"); } else {
            println!("FAIL {name}");
            if diff { println!("--- expected\n{}{}\n--- actual\n{}\n", exp_js, exp_err, got); }
            fail.push(name);
        }
    }
}

fn main() {
    let root = std::env::var("LWC_FIXTURES").map(PathBuf::from).unwrap_or_else(|_| lwc_shared::oracle_root().join("packages/@lwc/style-compiler/src/__tests__/fixtures"));
    let parse_errors = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parse-errors");
    let filter = std::env::var("LWC_FIXTURE").ok();
    let diff = std::env::var("LWC_STYLE_DIFF").is_ok();
    let (mut pass, mut fail) = (0, Vec::new());
    run_dir(&root, &filter, diff, &mut pass, &mut fail);
    let upstream = (pass, pass + fail.len());
    run_dir(&parse_errors, &filter, diff, &mut pass, &mut fail);
    let total = pass + fail.len();
    println!("\nupstream fixtures: {}/{}; parse-errors: {}/{}", upstream.0, upstream.1, pass - upstream.0, total - upstream.1);
    println!("lwc-style fixtures: {pass}/{total} passed, {} failed", fail.len());
    if std::env::var("LWC_STYLE_STRICT").is_ok() && !fail.is_empty() { std::process::exit(1); }
}
