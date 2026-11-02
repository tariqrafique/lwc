//! `level` / `url` on single (non-aggregate) errors, matching shipped 9.5.3.
//!
//! Rule, from `@lwc/compiler` 9.5.3 dist (`transformSync` per file kind) and `@lwc/errors`
//! (`normalizeToCompilerError` -> `convertErrorToDiagnostic`: `level = 'level' in e ? e.level : fallback.level`,
//! `url = e.url ?? fallback.url`). Evidence: notes/lwc-rust-migrate/proofs/single-error-shape/shipped.json.
//! - JS (Babel): every throw is normalized against JS_TRANSFORMER_ERROR (1007). Babel's error carries no
//!   `url`, so url is LWC1007's even for LWC plugin codes (1107, 1097, 1121, 1103). The one exception is the
//!   "Decorators are not enabled" + track/api/wire sniff, normalized against 1198, whose url is ''.
//! - Template: `CompilerError.from(diagnostic)` -> the diagnostic's own (info) url, '' for most template codes;
//!   a thrown template failure falls back to HTML_TRANSFORMER_ERROR (1008, url '').
//! - CSS: normalized against CSS_TRANSFORMER_ERROR (1009, url LWC1009).
//! - Options validation: `generateCompilerError(info)` -> the info's level and url.
//! - level is 1 (Error) in every case observed.
//! - SSR: not derived. Shipped SSR template errors throw a bare `Error` with only `message`; left unset.

use lwc_shared::CompileOptions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Options,
    Js,
    Template,
    Style,
    Ssr,
    Other,
}

impl Origin {
    /// Mirrors `lwc_compiler::dispatch` routing.
    pub(crate) fn of(filename: &str, o: &CompileOptions) -> Self {
        match filename.rsplit('.').next().unwrap_or("") {
            "css" => Origin::Style,
            "html" | "js" | "ts" | "mjs" | "jsx" | "tsx" if o.target_ssr => Origin::Ssr,
            "html" => Origin::Template,
            "js" | "ts" | "mjs" | "jsx" | "tsx" => Origin::Js,
            _ => Origin::Other,
        }
    }
}

const JS_TRANSFORMER_ERROR: u32 = 1007;
const JS_TRANSFORMER_DECORATOR_ERROR: u32 = 1198;
const CSS_TRANSFORMER_ERROR: u32 = 1009;
const LEVEL_ERROR: u32 = 1;

/// `(level, url)` for a single error, or `None` where shipped's shape is not derived (SSR / unknown kind).
/// `diag_level`: the first diagnostic's JS level (used for options errors, which keep the info's level).
pub(crate) fn level_url(origin: Origin, code: Option<u32>, diag_level: Option<u32>) -> Option<(u32, String)> {
    use crate::error_urls::error_url;
    Some(match origin {
        Origin::Js if code == Some(JS_TRANSFORMER_DECORATOR_ERROR) => (LEVEL_ERROR, error_url(Some(JS_TRANSFORMER_DECORATOR_ERROR))),
        Origin::Js => (LEVEL_ERROR, error_url(Some(JS_TRANSFORMER_ERROR))),
        Origin::Template => (LEVEL_ERROR, error_url(code)),
        Origin::Style => (LEVEL_ERROR, error_url(Some(CSS_TRANSFORMER_ERROR))),
        Origin::Options => (diag_level.unwrap_or(LEVEL_ERROR), error_url(code)),
        Origin::Ssr | Origin::Other => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SHIPPED: &str = include_str!("../tests/fixtures/single-error-shipped.json");

    /// Cases where the Rust error itself (not napi) differs from shipped, so level/url can't be pinned
    /// end-to-end yet. Each needs a change outside lwc-compiler-napi.
    const UPSTREAM_GAPS: &[(&str, &str)] = &[];

    /// Rust's code differs from shipped (upstream), but level/url/own keys still match and are checked.
    const CODE_GAPS: &[(&str, &str)] = &[];

    fn rust_error(c: &Value) -> (lwc_shared::CompileError, Origin) {
        let filename = c["filename"].as_str().unwrap();
        let mut opts = json!({"name": "foo", "namespace": "x"});
        if let Some(extra) = c.get("opts").and_then(Value::as_object) {
            for (k, v) in extra {
                opts[k] = v.clone();
            }
        }
        match crate::to_compile_options(opts, filename).unwrap() {
            Err(e) => (e, Origin::Options),
            Ok(o) => (lwc_compiler::dispatch(c["src"].as_str().unwrap(), filename, &o).expect_err("case must fail"), Origin::of(filename, &o)),
        }
    }

    /// Seam: `error_result` gives every shipped single error the same level, url and own keys
    /// (message, code, filename, location, level, url), except documented upstream gaps.
    #[test]
    fn single_errors_match_shipped_level_url_and_own_keys() {
        let cases: Vec<Value> = serde_json::from_str(SHIPPED).unwrap();
        assert!(cases.len() >= 10);
        for c in &cases {
            let id = c["id"].as_str().unwrap();
            let want = &c["error"];
            assert_eq!(want["ownKeys"], json!(["message", "code", "filename", "location", "level", "url"]), "{id}");
            if UPSTREAM_GAPS.iter().any(|(g, _)| *g == id) {
                continue;
            }
            let (e, origin) = rust_error(c);
            let j = crate::error_result(e, origin).error.unwrap();
            assert_eq!(j.level, want["level"].as_u64().map(|l| l as u32), "{id} level");
            assert_eq!(j.url.as_deref(), want["url"].as_str(), "{id} url");
            let keys = j.own_keys();
            for k in ["message", "code", "filename", "location", "level", "url"] {
                assert!(keys.contains(&k), "{id}: own key {k} missing from {keys:?}");
            }
            if origin == Origin::Style {
                // CSS: message (cwd-normalized) and `{line, column}` location are pinned too.
                let cwd = std::env::current_dir().unwrap().to_string_lossy().into_owned();
                assert_eq!(j.message.replacen(&format!("{cwd}/"), "<cwd>/", 1), want["message"].as_str().unwrap(), "{id} message");
                let loc = match &j.location { Some(napi::Either::A(l)) => json!({"line": l.line, "column": l.column, "start": l.start, "length": l.length}), _ => Value::Null };
                let mut want_loc = want["location"].clone();
                for k in ["start", "length"] { if want_loc.get(k).is_none() { want_loc[k] = Value::Null; } }
                assert_eq!(loc, want_loc, "{id} location");
            }
            if origin == Origin::Js && matches!(want["code"].as_u64(), Some(1007 | 1198)) {
                // Babel-internal errors: message (Rust keeps the filename as given; shipped resolves it
                // against cwd) and location (parse: Babel `Position {line, column, index}`; class-features
                // throw: `undefined`) are pinned too.
                assert_eq!(j.message, want["message"].as_str().unwrap().replacen("<cwd>/", "", 1), "{id} message");
                let loc = match &j.location {
                    Some(napi::Either::A(l)) => json!({"line": l.line, "column": l.column, "start": l.start, "length": l.length, "index": l.index}),
                    _ => Value::Null,
                };
                let mut want_loc = want["location"].clone();
                if want_loc.is_object() { for k in ["start", "length", "index"] { if want_loc.get(k).is_none() { want_loc[k] = Value::Null; } } }
                assert_eq!(loc, want_loc, "{id} location");
            }
            if !CODE_GAPS.iter().any(|(g, _)| *g == id) {
                assert_eq!(j.code, want["code"].as_u64().map(|x| x as u32), "{id} code");
            }
            assert!(!keys.contains(&"errors"), "{id}");
        }
    }

    /// The rule itself, independent of what upstream emits today.
    #[test]
    fn url_rule() {
        let l1007 = "https://lwc.dev/guide/error_codes#lwc1007".to_string();
        assert_eq!(level_url(Origin::Js, Some(1107), Some(1)), Some((1, l1007.clone())));
        assert_eq!(level_url(Origin::Js, None, None), Some((1, l1007)));
        assert_eq!(level_url(Origin::Js, Some(1198), Some(1)), Some((1, String::new())));
        assert_eq!(level_url(Origin::Template, Some(1058), Some(1)), Some((1, String::new())));
        assert_eq!(level_url(Origin::Template, None, None), Some((1, String::new())));
        assert_eq!(level_url(Origin::Style, None, None), Some((1, "https://lwc.dev/guide/error_codes#lwc1009".into())));
        assert_eq!(level_url(Origin::Options, Some(1021), Some(1)), Some((1, "https://lwc.dev/guide/error_codes#lwc1021".into())));
        assert_eq!(level_url(Origin::Ssr, Some(1107), Some(1)), None);
    }

    /// location is an own key even when unknown (shipped: own, value `undefined`).
    #[test]
    fn missing_location_is_present_undefined() {
        let c = json!({"filename": "foo.js", "src": "", "opts": {"outputConfig": {"sourcemap": "foo"}}});
        let (e, origin) = rust_error(&c);
        let j = crate::error_result(e, origin).error.unwrap();
        assert!(matches!(j.location, Some(napi::Either::B(()))));
        assert_eq!(j.own_keys(), ["message", "code", "filename", "location", "level", "url"]);
    }
}
