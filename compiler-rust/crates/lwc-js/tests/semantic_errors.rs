//! AB-L0063 / D-0259: oxc's semantic checker (`SemanticBuilder::with_check_syntax_error`) runs in
//! [`lwc_js::parse::parse_source`] (user source only, both modes). Intermediate [`parse`] of span-edited
//! source does not run it and is never aimed at codegen output (D-0262). Rust never compiles user source with an ECMAScript early error. Fixture: live 9.5.3 via
//! notes/lwc-rust-migrate/proofs/js-divergences/dump-semantic-fixture.cjs (inputs: semantic-cases.json).
//!
//! D-0259: the 14 inputs both sides reject in recovery (duplicate declarations, dup `#x`, `delete this.#x`).
//! Outcome matches (both throw LWC1007); 9.5.3's TypeError/Error (or SyntaxError) and null location vs our
//! SyntaxError + Position are pinned in [`SHAPE_DIVERGENCES`], not [`ACCEPTED_DIVERGENCES`].
//! D-0245: 9.5.3 recovery compiles an early-error input (e.g. `with`, `00`, `delete x`); we still reject.
//!
//! Pinned at parity: outcome, LWC code, class + code + filename prefix (`SyntaxError: LWC1007: foo.js: `), the
//! CompilerError filename and `{ line, column, index }`. The text is oxc's (D-0248), never compared. Where oxc's
//! first label is not Babel's position the case is in [`LOCATION_DIVERGENCES`] with our pinned value.
mod support;

use lwc_shared::CompileOptions;
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/semantic-errors-shipped.json");

/// D-0259: both sides throw LWC1007; 9.5.3's class (TypeError/Error, or SyntaxError with a null location)
/// and null `location` vs our SyntaxError + Position (D-0248) are the logged difference.
const D0259: &str = "D-0259";

/// Class/location shape divergences (not outcome). Both throw LWC1007; our pinned Position; 9.5.3 location is null.
const SHAPE_DIVERGENCES: &[(&str, &str, (u32, u32, u32))] = &[
    ("dup-let-rec", D0259, (1, 4, 4)),
    ("dup-let-var-rec", D0259, (1, 4, 4)),
    ("dup-var-let-rec", D0259, (1, 4, 4)),
    ("dup-const-let-rec", D0259, (1, 6, 6)),
    ("dup-class-let-rec", D0259, (1, 6, 6)),
    ("dup-fn-let-rec", D0259, (1, 9, 9)),
    ("dup-let-fn-rec", D0259, (1, 4, 4)),
    ("dup-import-let-rec", D0259, (1, 7, 7)),
    ("dup-block-let-rec", D0259, (1, 6, 6)),
    ("dup-param-let-rec", D0259, (1, 11, 11)),
    ("dup-catch-let-rec", D0259, (1, 14, 14)),
    ("lwc-comp-dup-rec", D0259, (2, 4, 44)),
    ("priv-dup-rec", D0259, (1, 10, 10)),
    ("delete-private-rec", D0259, (1, 27, 27)),
];

/// Accepted outcome divergences, by fixture id. Each must exist in the fixture, carry the same
/// `accepted_divergence` there, and still differ from shipped (a stale entry fails).
const ACCEPTED_DIVERGENCES: &[(&str, &str)] = &[
    // 9.5.3 recovery compiles an ECMAScript early error; we reject (D-0245).
    ("with-rec", "D-0245"),
    ("delete-ident-rec", "D-0245"),
    ("legacy-octal-rec", "D-0245"),
    ("var-eval-rec", "D-0245"),
    ("dup-label-rec", "D-0245"),
    ("dup-proto-rec", "D-0245"),
    ("break-undef-label-rec", "D-0245"),
    ("rw-let-1-rec", "D-0245"),
    ("rw-static-rec", "D-0245"),
    ("rw-yield-param-rec", "D-0245"),
    ("rw-let-shorthand-rec", "D-0245"),
    ("rw-let-line2-rec", "D-0245"),
    ("rw-let-template-rec", "D-0245"),
    ("dup-params-rec", "D-0245"),
    ("dup-export-rec", "D-0245"),
    ("dup-default-export-rec", "D-0245"),
    ("export-undeclared-rec", "D-0245"),
    ("priv-undeclared-rec", "D-0245"),
    ("new-target-top-rec", "D-0245"),
    ("super-top-rec", "D-0245"),
    ("return-top-rec", "D-0245"),
    ("continue-undef-rec", "D-0245"),
    ("break-outside-rec", "D-0245"),
    ("assign-eval-rec", "D-0245"),
    ("assign-arguments-rec", "D-0245"),
    ("octal-escape-rec", "D-0245"),
    ("fn-in-if-rec", "D-0245"),
    ("labeled-fn-rec", "D-0245"),
    ("await-top-id-rec", "D-0245"),
    ("yield-id-rec", "D-0245"),
    ("strict-impl-rec", "D-0245"),
    ("getter-param-rec", "D-0245"),
    ("setter-noparam-rec", "D-0245"),
    ("class-ctor-dup-rec", "D-0245"),
    ("super-call-no-extends-rec", "D-0245"),
    ("use-strict-nonsimple-rec", "D-0245"),
    ("abstract-decl-rec", "D-0245"),
    ("abstract-default-named-rec", "D-0245"),
];

/// D-0248 position divergences: oxc's first label is not where Babel reports. Our `(line, column, index)`.
const LOCATION_DIVERGENCES: &[(&str, (u32, u32, u32))] = &[
    ("dup-let", (1, 4, 4)), // 9.5.3 (1, 11, 11)
    ("delete-ident", (1, 7, 7)), // 9.5.3 (1, 0, 0)
    ("dup-label", (1, 0, 0)), // 9.5.3 (1, 3, 3)
    ("dup-proto", (1, 2, 2)), // 9.5.3 (1, 16, 16)
    ("break-undef-label", (1, 6, 6)), // 9.5.3 (1, 0, 0)
    ("rw-let-eq", (1, 6, 6)), // 9.5.3 (1, 0, 0)
    ("rw-let-comment", (1, 12, 12)), // 9.5.3 (1, 0, 0)
    ("rw-let-after", (1, 9, 9)), // 9.5.3 (1, 3, 3)
    ("rw-let-utf", (1, 21, 21)), // 9.5.3 (1, 15, 15)
    ("dup-let-var", (1, 4, 4)), // 9.5.3 (1, 11, 11)
    ("dup-var-let", (1, 4, 4)), // 9.5.3 (1, 11, 11)
    ("dup-const-let", (1, 6, 6)), // 9.5.3 (1, 17, 17)
    ("dup-class-let", (1, 6, 6)), // 9.5.3 (1, 15, 15)
    ("dup-fn-let", (1, 9, 9)), // 9.5.3 (1, 20, 20)
    ("dup-let-fn", (1, 4, 4)), // 9.5.3 (1, 16, 16)
    ("dup-import-let", (1, 7, 7)), // 9.5.3 (1, 23, 23)
    ("dup-block-let", (1, 6, 6)), // 9.5.3 (1, 13, 13)
    ("dup-catch-let", (1, 14, 14)), // 9.5.3 (1, 23, 23)
    ("dup-params", (1, 11, 11)), // 9.5.3 (1, 14, 14)
    ("dup-export", (1, 13, 13)), // 9.5.3 (1, 29, 29)
    ("dup-default-export", (1, 7, 7)), // 9.5.3 (1, 18, 18)
    ("priv-dup", (1, 10, 10)), // 9.5.3 (1, 14, 14)
    ("continue-undef", (1, 19, 19)), // 9.5.3 (1, 10, 10)
    ("octal-escape", (1, 0, 0)), // 9.5.3 (1, 2, 2)
    ("getter-param", (1, 8, 8)), // 9.5.3 (1, 3, 3)
    ("setter-noparam", (1, 8, 8)), // 9.5.3 (1, 3, 3)
    ("class-ctor-dup", (1, 10, 10)), // 9.5.3 (1, 27, 27)
    ("use-strict-nonsimple", (1, 20, 20)), // 9.5.3 (1, 0, 0)
    ("delete-private", (1, 27, 27)), // 9.5.3 (1, 20, 20)
    ("lwc-comp-dup", (2, 4, 44)), // 9.5.3 (2, 11, 51)
    ("abstract-default-rec", (1, 23, 23)), // 9.5.3 (1, 30, 30) "A class name is required."
];

fn opts(recover: bool) -> CompileOptions {
    CompileOptions { name: "foo".into(), namespace: "x".into(), experimental_error_recovery_mode: recover, ..Default::default() }
}

fn cases() -> Vec<Value> { serde_json::from_str(FIXTURE).unwrap() }

type Observed = Option<(Option<u32>, String, Option<String>, Option<(u32, u32, u32)>)>;

/// Through the @lwc/compiler JS facade. `None`: compiled; else (code, message, filename, Position).
fn observe(src: &str, recover: bool) -> Observed {
    lwc_compiler::dispatch(src, "foo.js", &opts(recover)).err().map(|e| {
        let t = support::Thrown::from(e);
        let first = t.errors.first();
        let loc = first.and_then(|d| d.location).map(|l| (l.line, l.column, l.index.expect("a parse error location is a Babel Position")));
        (t.code(), t.message, first.and_then(|d| d.filename.clone()), loc)
    })
}

fn want_loc(w: &Value) -> Option<(u32, u32, u32)> {
    let l = &w["location"];
    (!l.is_null()).then(|| (l["line"].as_u64().unwrap() as u32, l["column"].as_u64().unwrap() as u32, l["index"].as_u64().unwrap() as u32))
}

const PREFIX: &str = "SyntaxError: LWC1007: foo.js: ";

#[test]
fn semantic_early_errors_match_shipped() {
    let cases = cases();
    assert!(cases.len() >= 100);
    for id in ACCEPTED_DIVERGENCES.iter().map(|(id, _)| *id)
        .chain(LOCATION_DIVERGENCES.iter().map(|(id, _)| *id))
        .chain(SHAPE_DIVERGENCES.iter().map(|(id, _, _)| *id))
    {
        assert!(cases.iter().any(|c| c["id"] == *id), "orphan divergence entry {id}");
    }
    let (mut failures, mut loc_parity, mut n1007) = (vec![], 0, 0);
    for c in &cases {
        let id = c["id"].as_str().unwrap();
        let accepted = ACCEPTED_DIVERGENCES.iter().find(|(a, _)| *a == id).map(|(_, d)| *d);
        assert_eq!(c["accepted_divergence"].as_str(), accepted, "{id}: fixture accepted_divergence vs ACCEPTED_DIVERGENCES");
        let got = observe(c["src"].as_str().unwrap(), c["recover"].as_bool().unwrap());
        let w = &c["shipped"];
        if let Some(d) = accepted {
            // Still differs: in outcome, or (both throw) in class prefix or position.
            let same = match &got {
                None => w == "ok",
                Some((code, msg, _, loc)) => w != "ok" && *code == w["code"].as_u64().map(|c| c as u32)
                    && msg.split(": ").take(2).eq(w["message"].as_str().unwrap().split(": ").take(2)) && *loc == want_loc(w),
            };
            if same { failures.push(format!("{id}: listed as accepted divergence {d} but now matches shipped; drop the entry")); }
            continue;
        }
        if let Some((_, decision, pinned)) = SHAPE_DIVERGENCES.iter().find(|(a, _, _)| *a == id) {
            let Some((code, message, filename, loc)) = got else {
                failures.push(format!("{id}: compiled, shipped threw (shape {decision})")); continue;
            };
            if w == "ok" { failures.push(format!("{id}: listed as shape {decision} but shipped compiled")); continue; }
            if code != Some(1007) || w["code"] != 1007 { failures.push(format!("{id}: code {code:?}, shipped {}", w["code"])); continue; }
            if !message.starts_with(PREFIX) { failures.push(format!("{id}: class/code/filename prefix: {message:?}")); }
            if filename.as_deref() != Some("foo.js") { failures.push(format!("{id}: filename {filename:?}")); }
            if loc != Some(*pinned) { failures.push(format!("{id}: {decision} pinned location {pinned:?}, got {loc:?}")); }
            if !w["location"].is_null() && loc == want_loc(w) {
                failures.push(format!("{id}: matches shipped location now; drop it from SHAPE_DIVERGENCES"));
            }
            continue;
        }
        let Some((code, message, filename, loc)) = got else {
            if w != "ok" { failures.push(format!("{id}: compiled, shipped threw {}", w["message"])); }
            continue;
        };
        if w == "ok" { failures.push(format!("{id}: threw {code:?} {message}, shipped compiled")); continue; }
        if code != Some(1007) || w["code"] != 1007 { failures.push(format!("{id}: code {code:?}, shipped {}", w["code"])); continue; }
        n1007 += 1;
        assert!(w["message"].as_str().unwrap().starts_with(PREFIX), "{id}: fixture prefix");
        if !message.starts_with(PREFIX) { failures.push(format!("{id}: class/code/filename prefix: {message:?}")); }
        if filename.as_deref() != w["filename"].as_str() { failures.push(format!("{id}: filename {filename:?}, shipped {}", w["filename"])); }
        match LOCATION_DIVERGENCES.iter().find(|(a, _)| *a == id) {
            Some((_, pinned)) => {
                if loc != Some(*pinned) { failures.push(format!("{id}: D-0248 pinned location {pinned:?}, got {loc:?}")); }
                if loc == want_loc(w) { failures.push(format!("{id}: matches shipped location now; drop it from LOCATION_DIVERGENCES")); }
            }
            None if loc == want_loc(w) => loc_parity += 1,
            None => failures.push(format!("(\"{id}\", {:?}), // 9.5.3 {:?}", loc.map(|l| (l.0, l.1, l.2)), want_loc(w))),
        }
    }
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
    assert_eq!(n1007 - loc_parity, LOCATION_DIVERGENCES.len());
    eprintln!("LWC1007 location parity {loc_parity}/{n1007} (LOCATION_DIVERGENCES {}); SHAPE_DIVERGENCES {}", LOCATION_DIVERGENCES.len(), SHAPE_DIVERGENCES.len());
}

/// The 8 inputs AB-L0063 found compiling on main that 9.5.3 rejects with LWC1007 (normal mode).
/// Each: LWC1007, `SyntaxError` class, filename foo.js, and the position (9.5.3's, or D-0248-pinned ours).
fn closed(src: &str, shipped: (u32, u32, u32), ours: (u32, u32, u32)) {
    let (code, message, filename, loc) = observe(src, false).unwrap_or_else(|| panic!("{src:?} must not compile (9.5.3: LWC1007)"));
    assert_eq!(code, Some(1007), "{src:?}");
    assert!(message.starts_with(PREFIX), "{src:?}: {message}");
    assert_eq!(filename.as_deref(), Some("foo.js"));
    assert_eq!(loc, Some(ours), "{src:?}");
    // the shipped side, from the recorded oracle output
    let cases = cases();
    let c = cases.iter().find(|c| c["src"] == src && c["recover"] == false).expect("recorded in the fixture");
    assert_eq!(want_loc(&c["shipped"]), Some(shipped), "{src:?}: fixture");
    assert_eq!(c["shipped"]["code"], 1007);
    assert_eq!(LOCATION_DIVERGENCES.iter().any(|(id, _)| c["id"] == *id), ours != shipped, "{src:?}: LOCATION_DIVERGENCES entry iff positions differ");
}

#[test] fn closes_duplicate_let() { closed("let a; let a;", (1, 11, 11), (1, 4, 4)); }
#[test] fn closes_with_statement() { closed("with(a){}", (1, 0, 0), (1, 0, 0)); }
#[test] fn closes_delete_identifier() { closed("delete x;", (1, 0, 0), (1, 7, 7)); }
#[test] fn closes_legacy_octal() { closed("00", (1, 0, 0), (1, 0, 0)); }
#[test] fn closes_var_eval() { closed("var eval", (1, 4, 4), (1, 4, 4)); }
#[test] fn closes_duplicate_label() { closed("l: l: ;", (1, 3, 3), (1, 0, 0)); }
#[test] fn closes_duplicate_proto() { closed("({__proto__: 1, __proto__: 2});", (1, 16, 16), (1, 2, 2)); }
#[test] fn closes_break_undefined_label() { closed("break foo;", (1, 0, 0), (1, 6, 6)); }

/// D-0259: both throw; we report SyntaxError / LWC1007 / foo.js / oxc's Position (D-0248).
fn d0259(id: &str, ours: (u32, u32, u32)) {
    let c = cases().into_iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("missing {id}"));
    assert_eq!(c["recover"], true, "{id}");
    assert!(c["accepted_divergence"].is_null(), "{id}: D-0259 is a shape difference, not an outcome ACCEPTED_DIVERGENCES row");
    assert_eq!(SHAPE_DIVERGENCES.iter().find(|(a, _, _)| *a == id).map(|t| t.1), Some(D0259), "{id}");
    assert_ne!(c["shipped"], "ok", "{id}: 9.5.3 throws");
    assert_eq!(c["shipped"]["code"], 1007, "{id}");
    let (code, message, filename, loc) = observe(c["src"].as_str().unwrap(), true).unwrap_or_else(|| panic!("{id} must throw"));
    assert_eq!(code, Some(1007), "{id}: {message}");
    assert!(message.starts_with(PREFIX), "{id}: {message}");
    assert_eq!(filename.as_deref(), Some("foo.js"), "{id}");
    assert_eq!(loc, Some(ours), "{id}");
    // 9.5.3's class (TypeError/Error, or SyntaxError) and null location are the logged difference.
    assert!(c["shipped"]["location"].is_null(), "{id}: 9.5.3 location is null under D-0259");
}

#[test] fn d0259_dup_let() { d0259("dup-let-rec", (1, 4, 4)); }
#[test] fn d0259_dup_let_var() { d0259("dup-let-var-rec", (1, 4, 4)); }
#[test] fn d0259_dup_var_let() { d0259("dup-var-let-rec", (1, 4, 4)); }
#[test] fn d0259_dup_const_let() { d0259("dup-const-let-rec", (1, 6, 6)); }
#[test] fn d0259_dup_class_let() { d0259("dup-class-let-rec", (1, 6, 6)); }
#[test] fn d0259_dup_fn_let() { d0259("dup-fn-let-rec", (1, 9, 9)); }
#[test] fn d0259_dup_let_fn() { d0259("dup-let-fn-rec", (1, 4, 4)); }
#[test] fn d0259_dup_import_let() { d0259("dup-import-let-rec", (1, 7, 7)); }
#[test] fn d0259_dup_block_let() { d0259("dup-block-let-rec", (1, 6, 6)); }
#[test] fn d0259_dup_param_let() { d0259("dup-param-let-rec", (1, 11, 11)); }
#[test] fn d0259_dup_catch_let() { d0259("dup-catch-let-rec", (1, 14, 14)); }
#[test] fn d0259_lwc_comp_dup() { d0259("lwc-comp-dup-rec", (2, 4, 44)); }
#[test] fn d0259_priv_dup() { d0259("priv-dup-rec", (1, 10, 10)); }
#[test] fn d0259_delete_private() { d0259("delete-private-rec", (1, 27, 27)); }

/// D-0245 reverse case: 9.5.3 recovery compiles; we reject with LWC1007.
fn d0245_recovery(id: &str, ours: Option<(u32, u32, u32)>) {
    let c = cases().into_iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("missing {id}"));
    assert_eq!(c["recover"], true, "{id}");
    assert_eq!(c["accepted_divergence"], "D-0245", "{id}");
    assert_eq!(c["shipped"], "ok", "{id}: 9.5.3 recovery compiles");
    let (code, message, filename, loc) = observe(c["src"].as_str().unwrap(), true).unwrap_or_else(|| panic!("{id} must throw under D-0245"));
    assert_eq!(code, Some(1007), "{id}: {message}");
    assert_eq!(filename.as_deref(), Some("foo.js"), "{id}");
    if let Some(ours) = ours {
        assert!(message.starts_with(PREFIX), "{id}: {message}");
        assert_eq!(loc, Some(ours), "{id}");
    }
}

#[test] fn d0245_with() { d0245_recovery("with-rec", Some((1, 0, 0))); }
#[test] fn d0245_delete_ident() { d0245_recovery("delete-ident-rec", Some((1, 7, 7))); }
#[test] fn d0245_legacy_octal() { d0245_recovery("legacy-octal-rec", Some((1, 0, 0))); }
#[test] fn d0245_var_eval() { d0245_recovery("var-eval-rec", Some((1, 4, 4))); }
#[test] fn d0245_dup_label() { d0245_recovery("dup-label-rec", Some((1, 0, 0))); }
#[test] fn d0245_dup_proto() { d0245_recovery("dup-proto-rec", Some((1, 2, 2))); }
#[test] fn d0245_break_undef_label() { d0245_recovery("break-undef-label-rec", Some((1, 6, 6))); }
#[test] fn d0245_rw_let_1() { d0245_recovery("rw-let-1-rec", Some((1, 0, 0))); }
#[test] fn d0245_rw_static() { d0245_recovery("rw-static-rec", Some((1, 4, 4))); }
#[test] fn d0245_rw_yield_param() { d0245_recovery("rw-yield-param-rec", Some((1, 11, 11))); }
#[test] fn d0245_rw_let_shorthand() { d0245_recovery("rw-let-shorthand-rec", Some((1, 2, 2))); }
#[test] fn d0245_rw_let_line2() { d0245_recovery("rw-let-line2-rec", Some((2, 4, 15))); }
#[test] fn d0245_rw_let_template() { d0245_recovery("rw-let-template-rec", Some((1, 13, 13))); }
#[test] fn d0245_dup_params() { d0245_recovery("dup-params-rec", Some((1, 11, 11))); }
#[test] fn d0245_dup_export() { d0245_recovery("dup-export-rec", Some((1, 13, 13))); }
#[test] fn d0245_dup_default_export() { d0245_recovery("dup-default-export-rec", Some((1, 7, 7))); }
#[test] fn d0245_export_undeclared() { d0245_recovery("export-undeclared-rec", Some((1, 9, 9))); }
#[test] fn d0245_priv_undeclared() { d0245_recovery("priv-undeclared-rec", Some((1, 21, 21))); }
#[test] fn d0245_new_target_top() { d0245_recovery("new-target-top-rec", Some((1, 0, 0))); }
#[test] fn d0245_super_top() { d0245_recovery("super-top-rec", Some((1, 0, 0))); }
#[test] fn d0245_return_top() { d0245_recovery("return-top-rec", Some((1, 0, 0))); }
#[test] fn d0245_continue_undef() { d0245_recovery("continue-undef-rec", Some((1, 19, 19))); }
#[test] fn d0245_break_outside() { d0245_recovery("break-outside-rec", Some((1, 0, 0))); }
#[test] fn d0245_assign_eval() { d0245_recovery("assign-eval-rec", Some((1, 0, 0))); }
#[test] fn d0245_assign_arguments() { d0245_recovery("assign-arguments-rec", Some((1, 0, 0))); }
#[test] fn d0245_octal_escape() { d0245_recovery("octal-escape-rec", Some((1, 0, 0))); }
#[test] fn d0245_fn_in_if() { d0245_recovery("fn-in-if-rec", Some((1, 7, 7))); }
#[test] fn d0245_labeled_fn() { d0245_recovery("labeled-fn-rec", Some((1, 3, 3))); }
#[test] fn d0245_await_top_id() { d0245_recovery("await-top-id-rec", Some((1, 4, 4))); }
#[test] fn d0245_yield_id() { d0245_recovery("yield-id-rec", Some((1, 4, 4))); }
#[test] fn d0245_strict_impl() { d0245_recovery("strict-impl-rec", Some((1, 4, 4))); }
#[test] fn d0245_getter_param() { d0245_recovery("getter-param-rec", Some((1, 8, 8))); }
#[test] fn d0245_setter_noparam() { d0245_recovery("setter-noparam-rec", Some((1, 8, 8))); }
#[test] fn d0245_class_ctor_dup() { d0245_recovery("class-ctor-dup-rec", Some((1, 10, 10))); }
#[test] fn d0245_super_call_no_extends() { d0245_recovery("super-call-no-extends-rec", Some((1, 26, 26))); }
#[test] fn d0245_use_strict_nonsimple() { d0245_recovery("use-strict-nonsimple-rec", Some((1, 20, 20))); }
#[test] fn d0245_abstract_decl() { d0245_recovery("abstract-decl-rec", Some((1, 8, 8))); }
#[test] fn d0245_abstract_default_named() { d0245_recovery("abstract-default-named-rec", Some((1, 23, 23))); }

/// Both throw LWC1007 (not an outcome divergence): 9.5.3 "A class name is required." at (1, 30);
/// our abstract guard (D-0248 position) at (1, 23).
#[test]
fn abstract_default_recovery_is_a_location_divergence() {
    let c = cases().into_iter().find(|c| c["id"] == "abstract-default-rec").unwrap();
    assert!(c["accepted_divergence"].is_null());
    assert_eq!(c["shipped"]["code"], 1007);
    assert_eq!(want_loc(&c["shipped"]), Some((1, 30, 30)));
    let (code, message, filename, loc) = observe(c["src"].as_str().unwrap(), true).expect("we throw");
    assert_eq!(code, Some(1007));
    assert!(message.starts_with(PREFIX), "{message}");
    assert_eq!(filename.as_deref(), Some("foo.js"));
    assert_eq!(loc, Some((1, 23, 23)));
}

/// D-0262: recovery of `export default abstract class A {}` is LWC1007 with a position, never the
/// internal "intermediate output failed to re-parse" that the post-codegen re-parse used to raise.
#[test]
fn recovery_export_default_abstract_class_never_hits_internal_reparse() {
    let src = "export default abstract class A {}";
    let Some((code, message, filename, loc)) = observe(src, true) else { panic!("recovery must throw"); };
    assert_eq!(code, Some(1007));
    assert_eq!(filename.as_deref(), Some("foo.js"));
    assert_eq!(loc, Some((1, 23, 23)), "{message}");
    assert!(!message.contains("failed to re-parse"), "{message}");
    assert!(!message.contains("intermediate output"), "{message}");
    assert!(message.contains("LWC1007") || message.contains("abstract"), "{message}");
}

/// No placeholder decision id may remain in this crate (src or tests).
#[test]
fn lwc_js_contains_no_d_pending_token() {
    fn walk(dir: &std::path::Path, bad: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target") { continue; }
                walk(&p, bad);
                continue;
            }
            let Some(ext) = p.extension() else { continue };
            if !matches!(ext.to_str(), Some("rs" | "json" | "md" | "cjs" | "js" | "txt")) { continue; }
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("unreadable {}: {e}", p.display()));
            let needle = concat!("D-", "PENDING");
            for (n, line) in text.lines().enumerate() {
                if line.contains(needle) { bad.push(format!("{}:{}: {line}", p.display(), n + 1)); }
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bad = vec![];
    walk(root, &mut bad);
    assert!(bad.is_empty(), "{} token in lwc-js:\n{}", concat!("D-", "PENDING"), bad.join("\n"));
}
