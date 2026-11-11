//! Babel class-features (`shouldTransform`) throws: message text, LWC code and the exact code frame,
//! which Babel always builds from the ORIGINAL source (`#m`, never the private-method forward rename).
//! Fixture: 9.5.3 oracle via notes/lwc-rust-migrate/proofs/js-divergences/dump-fixture.cjs.
mod support;

use lwc_shared::CompileOptions;
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("fixtures/class-features-shipped.json");

/// D-0245: 9.5.3 recovery skips the undeclared-private-field early error (`this.#a` refers to
/// another class's `#a`) and later reports LWC1198 (decorator on a non-LightningElement).
/// We reject the early error (LWC1007) and never reach the decorator transform.
const ACCEPTED_DIVERGENCES: &[(&str, &str)] = &[("rename-before-cls-deco-rec", "D-0245")];

fn observe(c: &Value) -> Value {
    let mut o = CompileOptions::default();
    o.name = "foo".into();
    o.namespace = "x".into();
    let opt = |k: &str| c["opts"][k].as_bool().unwrap_or(false);
    o.enable_private_methods = opt("enablePrivateMethods");
    o.experimental_error_recovery_mode = opt("experimentalErrorRecoveryMode");
    // Through the @lwc/compiler JS facade (javascript.ts normalizeToCompilerError), as the fixture records it.
    match lwc_compiler::dispatch(
        c["src"].as_str().unwrap(),
        c["filename"].as_str().unwrap(),
        &o,
    ) {
        Ok(_) => json!("ok"),
        Err(e) => {
            let t = support::Thrown::from(e);
            json!({ "code": t.code(), "message": t.message })
        }
    }
}

#[test]
fn class_features_errors_match_shipped_frames() {
    let cases: Vec<Value> = serde_json::from_str(FIXTURE).unwrap();
    assert!(cases.len() >= 25, "{}", cases.len());
    for (id, _) in ACCEPTED_DIVERGENCES {
        assert!(
            cases.iter().any(|c| c["id"] == *id),
            "orphan ACCEPTED_DIVERGENCES entry {id}"
        );
    }
    let failures: Vec<String> = cases.iter().filter_map(|c| {
        let id = c["id"].as_str().unwrap();
        let got = observe(c);
        let accepted = ACCEPTED_DIVERGENCES.iter().find(|(a, _)| *a == id).map(|(_, d)| *d);
        if let Some(d) = accepted {
            return (got == c["shipped"]).then(|| {
                format!("{id}: listed as accepted divergence {d} but now matches shipped; drop the entry")
            });
        }
        (got != c["shipped"]).then(|| format!("{id}:\n  got  {got}\n  want {}", c["shipped"]))
    }).collect();
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// D-0245 pin: recovery of `class Z { #a() {} }\n@track\nclass X { y = this.#a; }` throws LWC1007
/// at the undeclared private field, not 9.5.3's later LWC1198.
#[test]
fn d0245_rename_before_cls_deco_rec() {
    let cases: Vec<Value> = serde_json::from_str(FIXTURE).unwrap();
    let c = cases
        .iter()
        .find(|c| c["id"] == "rename-before-cls-deco-rec")
        .unwrap();
    assert_eq!(c["shipped"]["code"], 1198);
    let mut o = CompileOptions::default();
    o.name = "foo".into();
    o.namespace = "x".into();
    o.enable_private_methods = true;
    o.experimental_error_recovery_mode = true;
    let t = support::thrown(
        c["src"].as_str().unwrap(),
        c["filename"].as_str().unwrap(),
        &o,
    );
    assert_eq!(t.code(), Some(1007));
    assert!(
        t.message.starts_with("SyntaxError: LWC1007: foo.js: "),
        "{}",
        t.message
    );
    let loc = t.errors[0].location.expect("parse error has a Position");
    assert_eq!(
        (loc.line, loc.column, loc.index.expect("index")),
        (3, 19, 46)
    );
}
