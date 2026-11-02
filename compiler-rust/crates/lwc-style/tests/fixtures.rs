//! Fixture harness over @lwc/style-compiler's JS fixtures.
//! Env: LWC_FIXTURES=dir override, LWC_FIXTURE=name filter, LWC_STYLE_STRICT=1 to fail on any mismatch,
//! LWC_STYLE_DIFF=1 to print expected/actual for failures.
use lwc_style::{transform_with_config, Config, StyleError};
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
        StyleError::Syntax { name, reason, line, column } => json!({"name": name, "reason": reason, "column": column, "line": line}),
        StyleError::Aggregate(v) => Value::Array(v.iter().map(err_json).collect()),
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("LWC_FIXTURES").unwrap_or_else(|_| "/workspace/lwc/packages/@lwc/style-compiler/src/__tests__/fixtures".into()));
    let filter = std::env::var("LWC_FIXTURE").ok();
    let diff = std::env::var("LWC_STYLE_DIFF").is_ok();
    let mut dirs: Vec<_> = fs::read_dir(&root).expect("fixtures dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("actual.css").exists()).collect();
    dirs.sort();
    let (mut pass, mut fail) = (0, Vec::new());
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if filter.as_ref().is_some_and(|f| !name.contains(f.as_str())) { continue; }
        let src = fs::read_to_string(dir.join("actual.css")).unwrap();
        let cfg: Value = fs::read_to_string(dir.join("config.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}));
        let b = |k: &str| cfg.get(k).and_then(Value::as_bool).unwrap_or(false);
        let config = Config { scoped: b("scoped"), disable_synthetic_shadow_support: b("disableSyntheticShadowSupport"), experimental_error_recovery_mode: b("experimentalErrorRecoveryMode"), api_version: cfg.get("apiVersion").and_then(Value::as_u64).map(|v| v as u32) };
        let exp_js = fs::read_to_string(dir.join("expected.js")).unwrap_or_default();
        let exp_err = fs::read_to_string(dir.join("error.json")).unwrap_or_default();
        let res = std::panic::catch_unwind(|| transform_with_config(&src, &format!("{}/actual.css", dir.display()), config));
        let (ok, got) = match res {
            Err(_) => (false, "PANIC".to_string()),
            Ok(Ok(code)) => { let c = normalize_version(&code); (exp_err.trim().is_empty() && c == exp_js, c) }
            Ok(Err(e)) => {
                let got = err_json(&e);
                let want: Option<Value> = serde_json::from_str(&exp_err).ok();
                (exp_js.is_empty() && want.as_ref() == Some(&got), serde_json::to_string_pretty(&got).unwrap())
            }
        };
        if ok { pass += 1; println!("PASS {name}"); } else {
            println!("FAIL {name}");
            if diff { println!("--- expected\n{}{}\n--- actual\n{}\n", exp_js, exp_err, got); }
            fail.push(name);
        }
    }
    let total = pass + fail.len();
    println!("\nlwc-style fixtures: {pass}/{total} passed, {} failed", fail.len());
    if std::env::var("LWC_STYLE_STRICT").is_ok() && !fail.is_empty() { std::process::exit(1); }
}
