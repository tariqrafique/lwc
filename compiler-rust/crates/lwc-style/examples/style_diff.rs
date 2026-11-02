//! Differential-corpus driver: reads JSONL jobs on stdin
//! `{"id","src","filename","scoped","dss","erm"}` and writes JSONL results
//! `{"id","ok":true,"code"}` | `{"id","ok":false,"err"}` | `{"id","panic":true}`.
//! Used by tools/style-diff/run.mjs.
use lwc_style::{transform_with_config, Config, StyleError};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

fn err_json(e: &StyleError) -> Value {
    match e {
        StyleError::Syntax { name, reason, line, column } => json!({"name": name, "reason": reason, "column": column, "line": line}),
        StyleError::Aggregate(v) => Value::Array(v.iter().map(err_json).collect()),
    }
}

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout());
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        if line.trim().is_empty() { continue; }
        let j: Value = serde_json::from_str(&line).unwrap();
        let b = |k: &str| j[k].as_bool().unwrap_or(false);
        let cfg = Config { scoped: b("scoped"), disable_synthetic_shadow_support: b("dss"), experimental_error_recovery_mode: b("erm"), api_version: None };
        let src = j["src"].as_str().unwrap().to_string();
        let fname = j["filename"].as_str().unwrap_or("x.css").to_string();
        let r = std::panic::catch_unwind(move || transform_with_config(&src, &fname, cfg));
        let v = match r {
            Err(_) => json!({"id": j["id"], "panic": true}),
            Ok(Ok(code)) => json!({"id": j["id"], "ok": true, "code": code}),
            Ok(Err(e)) => json!({"id": j["id"], "ok": false, "err": err_json(&e)}),
        };
        writeln!(out, "{v}").unwrap();
    }
}
