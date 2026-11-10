//! Every error the binding returns: `transform`'s `{ error }` result is serialized here, nowhere else.
//! The lwc-compiler facades already surface the error `@lwc/compiler` 9.5.3 throws (`TransformError`);
//! this file only converts it to the JS object. No level, url, code or shape is derived here.
//!
//! What upstream throws (packages/@lwc/compiler/src/transformers, /workspace/lwc @ b9e8007):
//! - one `CompilerError` (`normalizeToCompilerError` / `CompilerError.from`): javascript.ts:121,
//!   template.ts:73 and :85, style.ts:60 -> `TransformError::Compiler`, own keys
//!   `message, code, filename, location, level, url` (`location` own even when `undefined`);
//! - a `CompilerAggregateError` only where upstream throws several: javascript.ts:129-133 and
//!   style.ts:47-58 (recovery mode), and template.ts:79-82 (recovery mode, no message: `""`) ->
//!   `TransformError::Aggregate`, own keys `message, errors`;
//! - the SSR compiler's own error, which no `transformSync` facade normalizes: compile-js `generateError`
//!   (a `CompilerError`) -> `TransformError::Compiler`; a plain `Error` -> `TransformError::Unnormalized`.
//! Byte-level spec: tests/fixtures/{single-error-shipped,aggregate-shipped}.json.
use crate::{JsAggregateEntry, JsCompileError, JsCompileResult};
use lwc_shared::{CompileError, CompileOptions, CompileResult, CompilerAggregateError, CompilerError, TransformError, UnnormalizedError};
use napi::bindgen_prelude::Either;

/// `lwc_compiler::dispatch`, its error surfaced as the binding's `{ error }` result.
pub(crate) fn compile(src: &str, filename: &str, o: &CompileOptions) -> Result<CompileResult, JsCompileResult> {
    lwc_compiler::dispatch(src, filename, o).map_err(|e| result(serialize(e)))
}

/// An options validation error (`to_compile_options`), surfaced by `lwc_compiler::options_error`.
pub(crate) fn options_error(e: CompileError) -> JsCompileResult {
    result(serialize(lwc_compiler::options_error(e)))
}

fn result(error: JsCompileError) -> JsCompileResult {
    JsCompileResult { code: String::new(), map: serde_json::Value::Null, css_scope_tokens: None, warnings: None, error: Some(error) }
}

fn serialize(e: TransformError) -> JsCompileError {
    match e {
        TransformError::Compiler(e) => compiler_error(e),
        TransformError::Aggregate(a) => aggregate(a),
        TransformError::Unnormalized(e) => unnormalized(e),
    }
}

/// `CompilerError`. `name` is prototype-inherited 'Error' upstream (not own). Position is only
/// under `location` (D-0239 / D-0241); no mirrored own `line` / `column`.
fn compiler_error(e: CompilerError) -> JsCompileError {
    JsCompileError {
        message: e.message,
        code: e.code,
        // Own even when `undefined` (an options error has no origin), like `location`.
        filename: Some(e.filename.map_or(Either::B(()), Either::A)),
        location: Some(e.location.map_or(Either::B(()), |l| Either::A(l.into()))),
        errors: None,
        level: Some(e.level.js_value()),
        url: Some(e.url),
    }
}

/// `CompilerAggregateError`: own `message` and `errors` (each a `CompilerError`). Upstream
/// `name` is prototype-inherited 'AggregateError' (not own); harness builds AggregateError itself.
fn aggregate(a: CompilerAggregateError) -> JsCompileError {
    let errors = a
        .errors
        .into_iter()
        .map(|e| JsAggregateEntry {
            name: "Error".into(),
            message: e.message,
            code: e.code,
            filename: e.filename,
            location: e.location.map(Into::into),
            level: e.level.js_value(),
            url: e.url,
        })
        .collect();
    JsCompileError {
        message: a.message, code: None, filename: None,
        location: None, errors: Some(errors), level: None, url: None,
    }
}

/// The producer's error as it threw it: no `level` / `url`; `location` only when it has one.
fn unnormalized(e: UnnormalizedError) -> JsCompileError {
    JsCompileError {
        message: e.message, code: e.code, filename: e.filename.map(Either::A),
        location: e.location.map(|l| Either::A(l.into())), errors: None, level: None, url: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JsDiagnosticLocation;
    use serde_json::{Value, json};

    const SINGLE: &str = include_str!("../tests/fixtures/single-error-shipped.json");
    const AGGREGATE: &str = include_str!("../tests/fixtures/aggregate-shipped.json");

    fn opts(extra: Option<&Value>, filename: &str) -> Result<CompileOptions, CompileError> {
        let mut o = json!({"name": "foo", "namespace": "x"});
        for (k, v) in extra.and_then(Value::as_object).into_iter().flatten() {
            o[k] = v.clone();
        }
        crate::to_compile_options(o, filename).unwrap()
    }

    fn js_error(src: &str, filename: &str, extra: Option<&Value>) -> JsCompileError {
        match opts(extra, filename) {
            Err(e) => options_error(e),
            Ok(o) => compile(src, filename, &o).expect_err("case must fail"),
        }
        .error
        .unwrap()
    }

    fn loc_json(l: &JsDiagnosticLocation) -> Value {
        let mut v = json!({"line": l.line, "column": l.column});
        for (k, x) in [("start", l.start), ("length", l.length), ("index", l.index)] {
            if let Some(x) = x { v[k] = json!(x); }
        }
        v
    }

    fn cwd() -> String { std::env::current_dir().unwrap().to_string_lossy().into_owned() }

    /// D-0248: oxc parser/semantic errors keep oxc's text and first-label position. Per case: our pinned
    /// `(line, column, index)`; the message is compared up to the class + code + filename prefix only.
    const OXC_SYNTAX_ERRORS: &[(&str, (u32, u32, u32))] = &[
        // 9.5.3 throws "Unexpected reserved word 'let'." at (1:0); oxc's parser error (1:6) precedes the semantic check.
        ("js-parse-1007", (1, 6, 6)),
    ];

    /// Every shipped single error: own keys, code, level, url, location; message where Rust and shipped
    /// spell the path alike (CSS resolves it against cwd; Babel-internal messages keep it as given).
    #[test]
    fn single_errors_match_shipped() {
        let cases: Vec<Value> = serde_json::from_str(SINGLE).unwrap();
        assert!(cases.len() >= 10);
        for c in &cases {
            let id = c["id"].as_str().unwrap();
            let (want, filename) = (&c["error"], c["filename"].as_str().unwrap());
            let j = js_error(c["src"].as_str().unwrap(), filename, c.get("opts"));
            let shipped: Vec<&str> = want["ownKeys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
            assert_eq!(j.own_keys(), shipped, "{id} own keys");
            assert_eq!(j.code, want["code"].as_u64().map(|x| x as u32), "{id} code");
            assert_eq!(j.level, want["level"].as_u64().map(|l| l as u32), "{id} level");
            assert_eq!(j.url.as_deref(), want["url"].as_str(), "{id} url");
            let got = match &j.filename { Some(Either::A(f)) => Value::from(f.as_str()), _ => Value::Null };
            assert_eq!(got, want["filename"], "{id} filename");
            let loc = match &j.location { Some(Either::A(l)) => loc_json(l), _ => Value::Null };
            if let Some((_, (line, column, index))) = OXC_SYNTAX_ERRORS.iter().find(|(x, _)| *x == id) {
                assert_ne!(json!({"line": line, "column": column, "index": index}), want["location"], "{id}: matches shipped now; drop the pin");
                assert_eq!(loc, json!({"line": line, "column": column, "index": index}), "{id} D-0248 pinned location");
                let prefix = format!("SyntaxError: LWC1007: {filename}: ");
                assert!(j.message.starts_with(&prefix) && want["message"].as_str().unwrap().replacen("<cwd>/", "", 1).starts_with(&prefix), "{id} message prefix");
                continue;
            }
            assert_eq!(loc, want["location"], "{id} location");
            if filename.ends_with(".css") {
                assert_eq!(j.message.replacen(&format!("{}/", cwd()), "<cwd>/", 1), want["message"].as_str().unwrap(), "{id} message");
            } else if matches!(want["code"].as_u64(), Some(1007 | 1198)) {
                assert_eq!(j.message, want["message"].as_str().unwrap().replacen("<cwd>/", "", 1), "{id} message");
            }
        }
    }

    /// Every shipped recovery-mode aggregate: message, own keys (no Aggregate `name`), and each `errors[]` entry.
    #[test]
    fn aggregates_match_shipped() {
        for f in serde_json::from_str::<Vec<Value>>(AGGREGATE).unwrap() {
            let id = &f["fixture"];
            let j = js_error(f["src"].as_str().unwrap(), "foo.js", Some(&json!({"experimentalErrorRecoveryMode": true})));
            assert_eq!(j.message, f["message"].as_str().unwrap(), "{id}");
            let shipped: Vec<&str> = f["ownKeys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
            assert_eq!(j.own_keys(), shipped, "{id}");
            let cwd = cwd();
            let got: Vec<Value> = j.errors.unwrap().iter().map(|x| json!({
                "name": x.name, "message": x.message, "code": x.code,
                "filename": x.filename.as_deref().map(|f| f.replacen(&format!("{cwd}/"), "<cwd>/", 1)),
                "location": x.location.as_ref().map(|l| json!({"line": l.line, "column": l.column, "start": l.start, "length": l.length})),
                "level": x.level, "url": x.url,
            })).collect();
            let want: Vec<Value> = f["errors"].as_array().unwrap().iter().map(|x| {
                assert_eq!(x["ownKeys"], json!(["message", "code", "filename", "location", "level", "url"]));
                let mut x = x.clone();
                x.as_object_mut().unwrap().remove("ownKeys");
                x
            }).collect();
            assert_eq!(got, want, "{id}");
        }
    }

    /// A shipped single error owns `location`, `undefined` when unknown.
    #[test]
    fn missing_location_is_present_undefined() {
        let j = js_error("", "foo.js", Some(&json!({"outputConfig": {"sourcemap": "foo"}})));
        assert!(matches!(j.location, Some(Either::B(()))));
        assert_eq!(j.own_keys(), ["message", "code", "filename", "location", "level", "url"]);
    }

    /// AB-R001: the JS location is the stored `DiagnosticLocation` converted as is (CSS `{line, column}`,
    /// Babel parse `{line, column, index}`, `{line, column, start, length}`). Expected: 9.5.3
    /// `transformSync` error.location for each source.
    #[test]
    fn error_location_is_the_producers() {
        let cases = [
            ("export default class Foo {\n  x = ;\n}\n", "a.js", json!({"line": 2, "column": 6, "index": 33})),
            ("a {", "a.css", json!({"line": 1, "column": 1})),
            ("<template><div></div></template>\n<template></template>", "a.html", json!({"line": 2, "column": 1, "start": 33, "length": 21})),
        ];
        for (src, filename, want) in cases {
            let Some(Either::A(l)) = js_error(src, filename, None).location else { panic!("{filename}: no location") };
            assert_eq!(loc_json(&l), want, "{filename}");
        }
    }

    /// AB-L0056: LWC1121 through the napi seam, SSR and CSR, against shipped 9.5.3 run live in node
    /// (`$LWC_ORACLE`, default /workspace/lwc, pinned at b9e8007). No node or a moved oracle fails.
    #[test]
    fn lwc1121_seam_matches_live_oracle() {
        let src = "const s = 'é😀'; import(foo)";
        let root = std::env::var("LWC_ORACLE").unwrap_or_else(|_| "/workspace/lwc".into());
        let head = std::process::Command::new("git").args(["-C", &root, "rev-parse", "HEAD"]).output().expect("git is required");
        let head = String::from_utf8(head.stdout).unwrap();
        assert!(head.starts_with("b9e8007"), "oracle {root} is at {head:?}, want b9e8007");
        for ssr in [true, false] {
            let extra = json!({"targetSSR": ssr, "ssrMode": "sync", "dynamicImports": {"strictSpecifier": true}});
            let script = format!(
                "const {{ transformSync }} = require({root:?} + '/packages/@lwc/compiler');
                 try {{ transformSync({src:?}, 'foo.js', Object.assign({{ name: 'foo', namespace: 'x' }}, {extra})); }}
                 catch (e) {{ const {{ line, column, start, length }} = e.location;
                   console.log(JSON.stringify({{ code: e.code, level: e.level, url: e.url, filename: e.filename,
                     location: {{ line, column, start, length }} }})); }}"
            );
            let out = std::process::Command::new("node").arg("-e").arg(script).output().expect("node is required");
            assert!(out.status.success(), "oracle failed: {}", String::from_utf8_lossy(&out.stderr));
            let want: Value = serde_json::from_slice(&out.stdout).expect("oracle threw no error");
            let j = js_error(src, "foo.js", Some(&extra));
            let Some(Either::A(l)) = &j.location else { panic!("ssr={ssr}: no location") };
            let Some(Either::A(f)) = &j.filename else { panic!("ssr={ssr}: no filename") };
            let got = json!({"code": j.code, "level": j.level, "url": j.url, "filename": f, "location": loc_json(l)});
            assert_eq!(got, want, "ssr={ssr}");
        }
    }
}
