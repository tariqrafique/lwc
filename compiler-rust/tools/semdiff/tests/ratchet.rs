//! Per-case_key ratchet transitions over synthetic summaries (every bucket, including ones SD-1A cannot produce
//! end-to-end yet: different_decided/undecided need the exec leg).
use semdiff::ratchet::{compare, load_baseline};
use semdiff::rule_table_version;
use serde_json::{json, Value};

const B: [&str; 5] = ["equivalent", "different_decided", "different_undecided", "unclassified", "crash"];

fn summary(rt: &str, cases: &[(&str, &str)]) -> Value {
    let mut totals = serde_json::Map::new();
    totals.insert("cases".into(), json!(cases.len()));
    for b in B {
        totals.insert(b.into(), json!(cases.iter().filter(|c| c.1 == b).count()));
    }
    json!({"schema": "semdiff-summary/v1", "rule_table_version": rt, "corpus": "c", "totals": totals,
           "cases": cases.iter().map(|(id, b)| json!({"id": id, "bucket": b})).collect::<Vec<_>>()})
}

fn base(cases: &[(&str, &str)]) -> semdiff::ratchet::Baseline {
    load_baseline(serde_json::to_string(&summary(rule_table_version(), cases)).unwrap().as_bytes(), false).unwrap()
}

fn fails(was: &[(&str, &str)], now: &[(&str, &str)]) -> Vec<String> {
    compare(&summary(rule_table_version(), now), &base(was), false).unwrap().failures
}

#[test]
fn every_upward_move_fails_every_downward_or_flat_move_passes() {
    for (i, from) in B.iter().enumerate() {
        for (j, to) in B.iter().enumerate() {
            let f = fails(&[("k", from)], &[("k", to)]);
            if j > i {
                assert_eq!(f, vec![format!("case_key k regressed {from} -> {to}")]);
            } else {
                assert!(f.is_empty(), "{from} -> {to} must pass: {f:?}");
            }
        }
    }
}

#[test]
fn swap_fails_even_though_totals_are_unchanged() {
    let was = [("a", "equivalent"), ("b", "unclassified")];
    let now = [("a", "unclassified"), ("b", "equivalent")];
    let o = compare(&summary(rule_table_version(), &now), &base(&was), false).unwrap();
    assert_eq!(o.failures, vec!["case_key a regressed equivalent -> unclassified".to_string()]);
    assert_eq!(o.json["regressed"], json!([{"id": "a", "from": "equivalent", "to": "unclassified"}]));
    assert_eq!(o.json["improved"], json!([{"id": "b", "from": "unclassified", "to": "equivalent"}]));
}

#[test]
fn new_keys_are_reported_and_judged_strictly_dropped_keys_are_reported() {
    let was = [("a", "equivalent"), ("gone", "crash")];
    let now = [("a", "equivalent"), ("n1", "equivalent"), ("n2", "different_decided"), ("n3", "unclassified")];
    let o = compare(&summary(rule_table_version(), &now), &base(&was), false).unwrap();
    assert_eq!(o.json["new_keys"], json!(["n1", "n2", "n3"]));
    assert_eq!(o.json["new_failing"], json!(["n3"]));
    assert_eq!(o.json["dropped_keys"], json!(["gone"]));
    assert_eq!(o.failures, vec!["new case_key n3 is unclassified (new keys are judged by strict)".to_string()]);
    // Growth with only good new keys (and a dropped key) passes: no reset needed.
    assert!(fails(&was, &[("a", "equivalent"), ("n1", "equivalent"), ("n2", "different_decided")]).is_empty());
    for b in ["different_undecided", "crash"] {
        assert_eq!(fails(&was, &[("a", "equivalent"), ("n", b)]).len(), 1, "new key in {b}");
    }
}

#[test]
fn rule_table_mismatch_needs_an_explicit_reset() {
    // Applied identity now names both approved rules and is not the ASI-only string "rt-1". An rt-0 baseline needs --reset-baseline.
    let other = serde_json::to_string(&summary("rt-0", &[("a", "equivalent")])).unwrap();
    let e = load_baseline(other.as_bytes(), false).err().unwrap().0;
    assert!(e.starts_with("baseline-rule-table-mismatch"), "{e}");
    let b = load_baseline(other.as_bytes(), true).unwrap();
    let o = compare(&summary(rule_table_version(), &[("a", "different_decided")]), &b, true).unwrap();
    assert!(o.failures.is_empty());
    assert_eq!((o.json["reset"].clone(), o.json["baseline_rule_table_version"].clone()), (json!(true), json!("rt-0")));
    // A reset without a rule-table bump is refused.
    let same = serde_json::to_string(&summary(rule_table_version(), &[("a", "equivalent")])).unwrap();
    assert!(load_baseline(same.as_bytes(), true).err().unwrap().0.contains("only a rule-table bump"));
}

fn reset_base(cases: &[(&str, &str)]) -> semdiff::ratchet::Baseline {
    load_baseline(serde_json::to_string(&summary("rt-0", cases)).unwrap().as_bytes(), true).unwrap()
}

/// AB-L0030 ruling (notes 3bc1f50): under a reset a key fails iff it is now in a STRICT_BAD bucket that differs
/// from its baseline bucket; only equivalent <-> different_decided is waived.
#[test]
fn reset_fails_iff_now_strict_bad_and_bucket_changed() {
    let strict_bad = ["different_undecided", "unclassified", "crash"];
    let mut waived_moves = Vec::new();
    for from in B {
        for to in B {
            let o = compare(&summary(rule_table_version(), &[("k", to)]), &reset_base(&[("k", from)]), true).unwrap();
            let mv = json!([{"id": "k", "from": from, "to": to}]);
            let (fi, ti) = (B.iter().position(|b| *b == from).unwrap(), B.iter().position(|b| *b == to).unwrap());
            if strict_bad.contains(&to) && to != from {
                let want = format!("case_key k is now {to}, was {from} (a rule-table reset cannot create {to})");
                assert_eq!(o.failures, vec![want], "{from} -> {to}");
                assert_eq!(o.json["regressed"], mv);
            } else {
                assert!(o.failures.is_empty(), "{from} -> {to} must pass under reset: {:?}", o.failures);
                assert_eq!(o.json["regressed"], json!([]));
                assert_eq!(o.json["waived"], if ti > fi { mv.clone() } else { json!([]) }, "{from} -> {to}");
                if ti > fi {
                    waived_moves.push((from, to));
                }
                assert_eq!(o.json["improved"], if ti < fi { mv } else { json!([]) }, "{from} -> {to}");
            }
        }
    }
    assert_eq!(waived_moves, [("equivalent", "different_decided")], "the only waivable upward move");
}

#[test]
fn reset_equivalent_to_different_undecided_fails() {
    let o = compare(&summary(rule_table_version(), &[("a", "different_undecided")]), &reset_base(&[("a", "equivalent")]), true).unwrap();
    assert_eq!(
        o.failures,
        vec!["case_key a is now different_undecided, was equivalent (a rule-table reset cannot create different_undecided)".to_string()]
    );
    assert_eq!(o.json["waived"], json!([]));
    assert_eq!(o.json["regressed"], json!([{"id": "a", "from": "equivalent", "to": "different_undecided"}]));
    // crash -> unclassified also fails under a reset (fail-closed), though it is an improvement without one.
    let o = compare(&summary(rule_table_version(), &[("a", "unclassified")]), &reset_base(&[("a", "crash")]), true).unwrap();
    assert_eq!(o.failures.len(), 1);
    assert!(fails(&[("a", "crash")], &[("a", "unclassified")]).is_empty());
}

#[test]
fn reset_with_new_crash_and_regressed_unclassified_fails() {
    let was = [("a", "equivalent"), ("b", "different_decided")];
    let now = [("a", "unclassified"), ("b", "equivalent"), ("n", "crash")];
    let o = compare(&summary(rule_table_version(), &now), &reset_base(&was), true).unwrap();
    assert_eq!(
        o.failures,
        vec![
            "case_key a is now unclassified, was equivalent (a rule-table reset cannot create unclassified)".to_string(),
            "new case_key n is crash (new keys are judged by strict)".to_string(),
        ]
    );
    assert_eq!(o.json["new_failing"], json!(["n"]));
    assert_eq!(o.json["improved"], json!([{"id": "b", "from": "different_decided", "to": "equivalent"}]));
}

#[test]
fn reset_still_judges_new_keys_by_strict_including_different_undecided() {
    for b in ["different_undecided", "unclassified", "crash"] {
        let o = compare(&summary(rule_table_version(), &[("a", "equivalent"), ("n", b)]), &reset_base(&[("a", "equivalent")]), true).unwrap();
        assert_eq!(o.failures, vec![format!("new case_key n is {b} (new keys are judged by strict)")], "new key in {b}");
    }
    for b in ["equivalent", "different_decided"] {
        let o = compare(&summary(rule_table_version(), &[("a", "equivalent"), ("n", b)]), &reset_base(&[("a", "equivalent")]), true).unwrap();
        assert!(o.failures.is_empty(), "new key in {b}");
    }
    // The one waived move: equivalent -> different_decided passes under reset and is reported; it fails without one.
    let o = compare(&summary(rule_table_version(), &[("a", "different_decided")]), &reset_base(&[("a", "equivalent")]), true).unwrap();
    assert!(o.failures.is_empty());
    assert_eq!(o.json["waived"], json!([{"id": "a", "from": "equivalent", "to": "different_decided"}]));
    assert_eq!(fails(&[("a", "equivalent")], &[("a", "different_decided")]).len(), 1);
}

#[test]
fn no_reset_never_waives() {
    let o = compare(&summary(rule_table_version(), &[("a", "different_decided")]), &base(&[("a", "equivalent")]), false).unwrap();
    assert_eq!(o.json["waived"], json!([]));
    assert_eq!(o.failures.len(), 1);
}

#[test]
fn invalid_baselines_are_rejected() {
    let ok = summary(rule_table_version(), &[("a", "equivalent")]);
    let bad = |f: &dyn Fn(&mut Value)| {
        let mut v = ok.clone();
        f(&mut v);
        load_baseline(serde_json::to_string(&v).unwrap().as_bytes(), false).err().map(|e| e.0)
    };
    assert!(bad(&|_| {}).is_none());
    assert!(bad(&|v| v["schema"] = json!("semdiff-summary/v0")).unwrap().contains("schema"), "v0 has no buckets");
    assert!(bad(&|v| { v["totals"].as_object_mut().unwrap().remove("crash"); }).unwrap().contains("totals.crash missing"));
    assert!(bad(&|v| v["totals"]["cases"] = json!(2)).unwrap().contains("bucket sum"));
    assert!(bad(&|v| v["cases"][0]["bucket"] = json!("crash")).unwrap().contains("totals.equivalent=1 but 0"));
    assert!(bad(&|v| v["cases"][0]["bucket"] = json!("nope")).unwrap().contains("unknown bucket"));
    assert!(bad(&|v| { v["cases"][0].as_object_mut().unwrap().remove("bucket"); }).unwrap().contains("`bucket`"));
    assert!(bad(&|v| { v.as_object_mut().unwrap().remove("cases"); }).unwrap().contains("`cases`"));
    assert!(bad(&|v| { v.as_object_mut().unwrap().remove("rule_table_version"); }).unwrap().contains("rule_table_version"));
    assert!(bad(&|v| {
        v["cases"] = json!([{"id": "a", "bucket": "equivalent"}, {"id": "a", "bucket": "equivalent"}]);
        v["totals"]["cases"] = json!(2);
        v["totals"]["equivalent"] = json!(2);
    }).unwrap().contains("duplicate case_key"));
    assert!(load_baseline(b"not json", false).is_err());
}
