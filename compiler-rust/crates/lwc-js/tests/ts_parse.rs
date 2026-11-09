//! Shipped `scriptTransform` parses every script (`.ts`, `.tsx`, `.jsx`, `.mjs` included) with Babel and
//! no TypeScript/JSX/flow plugin, so TS or JSX syntax is an LWC1007 SyntaxError. Fixture: live 9.5.3 oracle via
//! notes/lwc-rust-migrate/proofs/js-divergences/dump-ts-fixture.cjs (cwd stripped from shipped messages).
//!
//! Pinned at parity (D-0248): outcome (ok / throws), LWC code, the error class + code + filename prefix
//! (`SyntaxError: LWC1007: foo.ts: `) and `{ line, column, index }`. NOT compared for syntax errors: the message
//! text, which is oxc's ([`OXC_MESSAGE_DIVERGENCE`]). Positions are oxc's first label; where that is not
//! Babel's position the case is listed in [`LOCATION_DIVERGENCES`] with our pinned value. Non-syntax errors
//! (any other code) compare the whole message.
mod support;

use lwc_shared::CompileOptions;
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/ts-parse-shipped.json");

/// Decision that makes oxc's syntax-error message text and exact position an accepted divergence (restates D-0155).
const OXC_MESSAGE_DIVERGENCE: &str = "D-0248";

/// Accepted outcome divergences from shipped 9.5.3, by fixture id and decision. Each must exist in the fixture,
/// carry the same `accepted_divergence` there, and still differ from shipped (a stale entry fails).
const ACCEPTED_DIVERGENCES: &[(&str, &str)] = &[("interface.ts-rec", "D-0245")];

/// D-0248 position divergences: oxc's first label is not where Babel reports. Our exact
/// `(line, column, index)`, pinned; each must exist, throw LWC1007 and still differ from shipped's position.
const LOCATION_DIVERGENCES: &[(&str, (u32, u32, u32))] = &[
    ("ts-type-anno-fn", (1, 6, 6)),
    ("field-opt.ts", (2, 54, 99)),
    ("field-bang.ts", (2, 54, 99)),
    ("private-mod.ts", (2, 52, 97)),
    ("public-mod.ts", (2, 52, 97)),
    ("readonly-mod.ts", (2, 52, 97)),
    ("method-ret.ts", (2, 55, 100)),
    ("fn-ret.ts", (2, 0, 45)),
    ("const-anno.ts", (2, 6, 51)),
    ("as-cast.ts", (2, 11, 56)),
    ("as-cast.tsx", (2, 11, 56)),
    ("as-cast.jsx", (2, 11, 56)),
    ("as-cast2.ts", (2, 10, 55)),
    ("getter-ret.ts", (2, 62, 107)),
    ("satisfies.ts", (2, 10, 55)),
    ("ts-ret-generic", (2, 55, 100)),
    ("ts-as-call", (2, 4, 49)),
    ("ts-as-array", (2, 11, 56)),
    ("ts-as-ret", (2, 22, 67)),
    ("ts-protected", (2, 52, 97)),
    ("ts-static-private", (2, 52, 97)),
    ("ts-arrow-ret", (2, 10, 55)),
    ("empty-paren", (1, 10, 10)),
    ("empty-paren2", (1, 4, 4)),
    ("as-arr2", (1, 14, 14)),
    // A strict reserved word (`interface`, `type`) before oxc's parser error: Babel throws at the word, oxc at
    // its parse error (the semantic check runs only on a clean parse; AB-L0063 retired the re-parse emulation).
    ("ts-interface", (1, 9, 9)),
    ("ts-syntax", (1, 6, 6)),
    ("interface.ts", (2, 9, 54)),
    ("interface.tsx", (2, 9, 54)),
    ("interface.jsx", (2, 9, 54)),
];

fn opts(recover: bool) -> CompileOptions {
    let mut o = CompileOptions::default();
    o.name = "foo".into();
    o.namespace = "x".into();
    o.experimental_error_recovery_mode = recover;
    o
}

fn cases() -> Vec<Value> { serde_json::from_str(FIXTURE).unwrap() }

/// Every listed id names a fixture case (an orphan entry is an error, never a silent pass).
fn orphans<'a>(cases: &[Value], ids: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    ids.into_iter().filter(|id| !cases.iter().any(|c| c["id"] == *id)).collect()
}

/// Through the @lwc/compiler JS facade (javascript.ts normalizeToCompilerError), as the fixture records it.
/// `Ok(None)`: compiled. `Ok(Some((code, message, (line, column, index))))`: threw.
fn observe(c: &Value) -> Option<(Option<u32>, String, Option<(u32, u32, u32)>)> {
    let r = lwc_compiler::dispatch(c["src"].as_str().unwrap(), c["filename"].as_str().unwrap(), &opts(c["recover"].as_bool().unwrap()));
    r.err().map(|e| {
        let t = support::Thrown::from(e);
        let loc = t.errors.first().and_then(|d| d.location).map(|l| (l.line, l.column, l.index.expect("a parse error location is a Babel Position")));
        (t.code(), t.message, loc)
    })
}

#[test]
fn ts_and_jsx_parse_failures_match_shipped() {
    let cases = cases();
    assert!(cases.len() >= 70);
    assert_eq!(orphans(&cases, ACCEPTED_DIVERGENCES.iter().map(|(id, _)| *id)), Vec::<&str>::new(), "ACCEPTED_DIVERGENCES orphan");
    assert_eq!(orphans(&cases, LOCATION_DIVERGENCES.iter().map(|(id, _)| *id)), Vec::<&str>::new(), "LOCATION_DIVERGENCES orphan");
    let (mut failures, mut loc_parity) = (vec![], 0);
    for c in &cases {
        let id = c["id"].as_str().unwrap();
        let filename = c["filename"].as_str().unwrap();
        let accepted = ACCEPTED_DIVERGENCES.iter().find(|(a, _)| *a == id).map(|(_, d)| *d);
        assert_eq!(c["accepted_divergence"].as_str(), accepted, "{id}: fixture accepted_divergence vs ACCEPTED_DIVERGENCES");
        let got = observe(c);
        let w = &c["shipped"];
        if let Some(d) = accepted {
            let shipped_ok = w == "ok";
            assert_ne!(got.is_none(), shipped_ok, "{id}: listed as accepted divergence {d} but now matches shipped; drop the entry");
            continue;
        }
        let Some((code, message, loc)) = got else {
            if w != "ok" { failures.push(format!("{id}: compiled, shipped threw {w}")); }
            continue;
        };
        if w == "ok" { failures.push(format!("{id}: threw {code:?} {message}, shipped compiled")); continue; }
        let want_code = w["code"].as_u64().map(|c| c as u32);
        if code != want_code { failures.push(format!("{id}: code {code:?}, shipped {want_code:?}")); continue; }
        let want_msg = w["message"].as_str().unwrap();
        if want_code != Some(1007) {
            if message != want_msg { failures.push(format!("{id}: message\n  got  {message}\n  want {want_msg}")); }
            continue;
        }
        // LWC1007 syntax error: class + code + filename at parity, message text per OXC_MESSAGE_DIVERGENCE.
        let prefix = format!("SyntaxError: LWC1007: {filename}: ");
        assert!(want_msg.starts_with(&prefix), "{id}: fixture message prefix {want_msg:?}");
        if !message.starts_with(&prefix) { failures.push(format!("{id}: class/code/filename prefix: {message:?}")); }
        let l = &w["location"];
        // Babel `err.loc`; null when the throw carries none (a plugin `buildCodeFrameError`, e.g. class-features).
        let want_loc = (!l.is_null()).then(|| (l["line"].as_u64().unwrap() as u32, l["column"].as_u64().unwrap() as u32, l["index"].as_u64().unwrap() as u32));
        match LOCATION_DIVERGENCES.iter().find(|(a, _)| *a == id) {
            Some((_, pinned)) => {
                if loc != Some(*pinned) { failures.push(format!("{id}: {OXC_MESSAGE_DIVERGENCE} pinned location {pinned:?}, got {loc:?}")); }
                if loc == want_loc { failures.push(format!("{id}: matches shipped location {want_loc:?} now; drop it from LOCATION_DIVERGENCES")); }
            }
            None if loc == want_loc => loc_parity += 1,
            None => failures.push(format!("{id}: location {loc:?}, shipped {want_loc:?}")),
        }
    }
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
    let n1007 = cases.iter().filter(|c| c["shipped"]["code"] == 1007 && c["accepted_divergence"].is_null()).count();
    assert_eq!((loc_parity, n1007 - loc_parity), (n1007 - LOCATION_DIVERGENCES.len(), LOCATION_DIVERGENCES.len()));
}

#[test]
fn divergence_lists_reject_orphan_ids() {
    let cases = cases();
    let with_orphan: Vec<(&str, &str)> = ACCEPTED_DIVERGENCES.iter().copied().chain([("no-such-case", "D-9999")]).collect();
    assert_eq!(orphans(&cases, with_orphan.iter().map(|(id, _)| *id)), vec!["no-such-case"]);
    assert_eq!(orphans(&cases, ["interface.ts-rec", "ts-anon-api"]), Vec::<&str>::new());
}

/// D-0245 (option B): lwc-js does not emulate Babel `errorRecovery` syntax recovery. `interface A {..}` in a
/// `.ts` file under experimentalErrorRecoveryMode is invalid as plain JS, so we throw the parse error
/// (LWC1007 with its Position location; text and position oxc's per D-0248). Shipped 9.5.3 recovers and
/// compiles it. Permanent accepted divergence; notes/lwc-rust-migrate/proofs/js-divergences/D0245.md.
#[test]
fn d0245_recovery_mode_ts_interface_is_lwc1007_not_ok() {
    const ID: &str = "interface.ts-rec";
    const SRC: &str = "import { LightningElement, api } from 'lwc';\ninterface A { x: number }\nexport default class Foo extends LightningElement {}";
    let e = support::Thrown::from(lwc_compiler::dispatch(SRC, "foo.ts", &opts(true)).expect_err("D-0245: we throw"));
    assert_eq!(e.name, None, "single CompilerError, not an aggregate");
    assert_eq!(e.code(), Some(1007));
    // Pinned: class + code + filename and the position; the text between is oxc's (D-0248), never compared.
    assert!(e.message.starts_with("SyntaxError: LWC1007: foo.ts: "), "{}", e.message);
    assert_eq!(e.errors[0].filename.as_deref(), Some("foo.ts"));
    let l = e.errors[0].location.expect("Babel Position");
    assert_eq!((l.line, l.column, l.index, l.start, l.length), (2, 9, Some(54), None, None));
    // The recorded 9.5.3 oracle output for the same input (dump-ts-fixture.cjs) compiles: the divergence is explicit.
    let cases = cases();
    let c = cases.iter().find(|c| c["id"] == ID).expect("D-0245 case recorded in the fixture");
    assert_eq!((c["src"].as_str(), c["filename"].as_str(), c["recover"].as_bool()), (Some(SRC), Some("foo.ts"), Some(true)));
    assert_eq!(c["accepted_divergence"], "D-0245");
    assert_eq!(c["shipped"], "ok", "9.5.3 compiles this input under errorRecovery");
}
