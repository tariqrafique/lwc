//! Recovery-mode `CompilerAggregateError` over the napi seam, pinned to shipped 9.5.3
//! (`tests/fixtures/aggregate-shipped.json`, made by `proofs/aggregate-shape/make-fixture.mjs`).
//!
//! Shipped aggregate: `name` 'AggregateError' (prototype), own `message` exactly
//! "Multiple errors occurred during compilation.", own `errors`; no own `code` / `filename` / location.
//! Each `errors[]` entry is a `CompilerError.from(diagnostic)`: `name` 'Error' (prototype), own
//! `message` (`LWC####: ...`, unframed), `code`, `filename` (Babel's cwd-resolved path), `location`
//! `{line, column, start, length}`, `level` (always 1: `from()` passes no level), `url` (error info).
use crate::{JsAggregateEntry, error_urls::error_url};

/// Phase B switch (flipped to `true` 2026-11-01 after Harness landed errors[] forwarding). `false` (Phase A): message = header + every sub-message, `code` = first sub-code,
/// `filename` = the compiled file (what the harness parses today). `true`: shipped's exact shape,
/// message exactly [`AGGREGATE_MESSAGE`], `code` and `filename` absent (JS `undefined`).
pub(crate) const AGGREGATE_EXACT_SHAPE: bool = true;

pub(crate) const AGGREGATE_MESSAGE: &str = "Multiple errors occurred during compilation.";

/// A `CompilerAggregateError` (lwc-js recovery). lwc-style's CSS `AggregateError` carries no diagnostics
/// and is left alone.
pub(crate) fn is_compiler_aggregate(e: &lwc_shared::CompileError) -> bool {
    e.name.as_deref() == Some("AggregateError") && !e.diagnostics.is_empty()
}

/// The aggregate's own `(message, code, filename)` in the given switch state.
pub(crate) fn surface(e: &lwc_shared::CompileError, exact: bool) -> (String, Option<u32>, Option<String>) {
    if exact {
        (AGGREGATE_MESSAGE.to_string(), None, None)
    } else {
        (e.message.clone(), e.diagnostics.first().and_then(|d| d.code), e.filename.clone())
    }
}

/// `errors[]`, in diagnostic order. `babel_paths`: the file went through Babel, which resolves
/// `filename` against `cwd` (`path.resolve(process.cwd(), filename)`) before the plugin sees it.
pub(crate) fn entries(e: &lwc_shared::CompileError, babel_paths: bool, cwd: &std::path::Path) -> Vec<JsAggregateEntry> {
    e.diagnostics
        .iter()
        .map(|d| JsAggregateEntry {
            name: "Error".into(),
            message: d.message.clone(),
            code: d.code,
            filename: d.filename.clone().map(|f| if babel_paths { resolve_like_node(cwd, &f) } else { f }),
            location: d.js_location().map(Into::into),
            level: 1,
            url: error_url(d.code),
        })
        .collect()
}

/// Node `path.resolve(cwd, f)` for POSIX paths: absolute `f` wins; `.`/`..`/empty segments collapse.
pub(crate) fn resolve_like_node(cwd: &std::path::Path, f: &str) -> String {
    let joined = if f.starts_with('/') { f.to_string() } else { format!("{}/{f}", cwd.to_string_lossy()) };
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => { parts.pop(); }
            s => parts.push(s),
        }
    }
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SHIPPED: &str = include_str!("../tests/fixtures/aggregate-shipped.json");

    fn rust_error(src: &str) -> lwc_shared::CompileError {
        let o = lwc_shared::CompileOptions { name: "foo".into(), namespace: "x".into(), experimental_error_recovery_mode: true, ..Default::default() };
        lwc_compiler::dispatch(src, "foo.js", &o).expect_err("fixture must fail")
    }

    fn entry_json(x: &JsAggregateEntry, cwd: &str) -> Value {
        json!({
            "name": x.name, "message": x.message, "code": x.code,
            "filename": x.filename.as_deref().map(|f| f.replacen(&format!("{cwd}/"), "<cwd>/", 1)),
            "location": x.location.as_ref().map(|l| json!({"line": l.line, "column": l.column, "start": l.start, "length": l.length})),
            "level": x.level, "url": x.url,
        })
    }

    /// errors[] (order + every entry field) and name match shipped for every pinned fixture, in Phase A.
    #[test]
    fn aggregate_errors_match_shipped() {
        let cwd = "/some/cwd";
        for f in serde_json::from_str::<Vec<Value>>(SHIPPED).unwrap() {
            let e = rust_error(f["src"].as_str().unwrap());
            assert!(is_compiler_aggregate(&e), "{}: {e:?}", f["fixture"]);
            assert_eq!(e.name.as_deref(), f["name"].as_str());
            let got: Vec<Value> = entries(&e, true, std::path::Path::new(cwd)).iter().map(|x| entry_json(x, cwd)).collect();
            let want: Vec<Value> = f["errors"].as_array().unwrap().iter().map(|x| {
                let mut x = x.clone();
                x.as_object_mut().unwrap().remove("ownKeys");
                x
            }).collect();
            assert_eq!(got, want, "{}", f["fixture"]);
            // Shipped entry own keys are exactly these (name is prototype-inherited 'Error').
            for x in f["errors"].as_array().unwrap() {
                assert_eq!(x["ownKeys"], json!(["message", "code", "filename", "location", "level", "url"]));
            }
        }
    }

    /// Exact state (switch `true`): message/code/filename equal shipped's aggregate.
    #[test]
    fn exact_state_matches_shipped_aggregate() {
        for f in serde_json::from_str::<Vec<Value>>(SHIPPED).unwrap() {
            let e = rust_error(f["src"].as_str().unwrap());
            let (message, code, filename) = surface(&e, true);
            assert_eq!(message, f["message"].as_str().unwrap());
            assert_eq!(f["hasOwnCode"], json!(false));
            assert_eq!(code, None);
            assert_eq!(f["hasOwnFilename"], json!(false));
            assert_eq!(filename, None);
            assert_eq!(f["ownKeys"], json!(["message", "errors"]));
        }
    }

    /// Phase A state (switch `false`): today's message (header + sub-messages) and first sub-code.
    #[test]
    fn phase_a_state_keeps_current_message_and_code() {
        for f in serde_json::from_str::<Vec<Value>>(SHIPPED).unwrap() {
            let e = rust_error(f["src"].as_str().unwrap());
            let (message, code, filename) = surface(&e, false);
            let subs: Vec<&str> = f["errors"].as_array().unwrap().iter().map(|x| x["message"].as_str().unwrap()).collect();
            assert_eq!(message, format!("{AGGREGATE_MESSAGE}\n{}", subs.join("\n")));
            assert_eq!(code, f["errors"][0]["code"].as_u64().map(|c| c as u32));
            assert_eq!(filename.as_deref(), Some("foo.js"));
        }
    }

    /// The live seam (`error_result`) in the current switch state.
    #[test]
    fn live_seam_in_current_state() {
        let f = &serde_json::from_str::<Vec<Value>>(SHIPPED).unwrap()[0];
        let j = crate::error_result(rust_error(f["src"].as_str().unwrap()), crate::single_error::Origin::Js).error.unwrap();
        assert_eq!(j.name.as_deref(), Some("AggregateError"));
        assert_eq!(j.errors.as_ref().map(Vec::len), f["errors"].as_array().map(Vec::len));
        assert!(j.location.is_none() && j.line.is_none() && j.column.is_none());
        assert!(j.level.is_none() && j.url.is_none());
        // Phase A: shipped's message + errors, plus code/filename. Phase B: only `name` beyond shipped.
        let want: &[&str] = if AGGREGATE_EXACT_SHAPE { &["message", "name", "errors"] } else { &["message", "name", "code", "filename", "errors"] };
        assert_eq!(j.own_keys(), want);
        let (message, code, filename) = surface(&rust_error(f["src"].as_str().unwrap()), AGGREGATE_EXACT_SHAPE);
        assert_eq!((j.message, j.code, j.filename), (message, code, filename));
        if !AGGREGATE_EXACT_SHAPE {
            assert_eq!(j.code, f["errors"][0]["code"].as_u64().map(|c| c as u32));
        }
    }

    /// Phase B: the live seam equals shipped's message, with no code/filename.
    #[test]
    fn live_seam_message_and_code_match_shipped() {
        let f = &serde_json::from_str::<Vec<Value>>(SHIPPED).unwrap()[0];
        let j = crate::error_result(rust_error(f["src"].as_str().unwrap()), crate::single_error::Origin::Js).error.unwrap();
        assert_eq!(j.message, f["message"].as_str().unwrap());
        assert_eq!(j.code, None);
        assert_eq!(j.filename, None);
    }

    /// Phase B (Migrate: code and filename ABSENT, not null, as 9.5.3). Own-key semantics: napi
    /// omits `None` fields, so `own_keys` is `Object.keys(err)`. Shipped: no own code/filename.
    /// `name` stays an own key until Harness stops reading it (shipped inherits it from Error.prototype).
    #[test]
    fn live_seam_has_no_own_code_or_filename() {
        for f in serde_json::from_str::<Vec<Value>>(SHIPPED).unwrap() {
            let j = crate::error_result(rust_error(f["src"].as_str().unwrap()), crate::single_error::Origin::Js).error.unwrap();
            let keys = j.own_keys();
            assert_eq!(keys.contains(&"code"), f["hasOwnCode"].as_bool().unwrap(), "{}", f["fixture"]);
            assert_eq!(keys.contains(&"filename"), f["hasOwnFilename"].as_bool().unwrap(), "{}", f["fixture"]);
            let shipped: Vec<&str> = f["ownKeys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
            let ours: Vec<&str> = keys.into_iter().filter(|k| *k != "name").collect();
            assert_eq!(ours, shipped, "{}", f["fixture"]);
        }
    }

    #[test]
    fn resolve_matches_node_path_resolve() {
        let c = std::path::Path::new("/a/b");
        assert_eq!(resolve_like_node(c, "foo.js"), "/a/b/foo.js");
        assert_eq!(resolve_like_node(c, "./x/../foo.js"), "/a/b/foo.js");
        assert_eq!(resolve_like_node(c, "../foo.js"), "/a/foo.js");
        assert_eq!(resolve_like_node(c, "/abs//x/foo.js"), "/abs/x/foo.js");
    }

    /// Non-aggregate errors get no errors[], but carry location {line, column, start, length}.
    #[test]
    fn single_error_carries_location_with_start_length() {
        let o = lwc_shared::CompileOptions { name: "foo".into(), namespace: "x".into(), ..Default::default() };
        let src = "import {api,LightningElement} from \"lwc\";\nexport default class T extends LightningElement{ @api dataA; }\n";
        let j = crate::error_result(lwc_compiler::dispatch(src, "foo.js", &o).unwrap_err(), crate::single_error::Origin::Js).error.unwrap();
        assert!(j.errors.is_none());
        let Some(napi::Either::A(l)) = j.location else { panic!("location") };
        // shipped 9.5.3: code 1107, location {line 2, column 49, start 91, length 11}
        assert_eq!((j.code, l.line, l.column, l.start, l.length), (Some(1107), 2, 49, Some(91), Some(11)));
    }
}
