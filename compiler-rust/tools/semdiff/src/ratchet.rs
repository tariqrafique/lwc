//! Per-case_key ratchet [AB-D0003 (4)], design §7 "Gate semantics". A totals-only ratchet hides a swap (one key fixed,
//! another regressed), so every key present in both runs is compared on its own.
//!
//! Order (index = rank): equivalent < different_decided < different_undecided < unclassified < crash. A key present
//! in both runs fails if its rank rises; that covers "equivalent → anything worse" and moving up the bad order.
//! Keys new in this run are reported and judged by strict rules for themselves (a new key in a STRICT_BAD bucket
//! fails), so corpus growth needs no reset but cannot add debt. Keys only in the baseline are reported, not failed.
//! A baseline with a different `rule_table_version` is exit 2 (`baseline-rule-table-mismatch`) unless the run is
//! `--reset-baseline`, which is recorded in the summary and is NOT a free pass (AB-L0030):
//!   - a key fails iff it is now in a `STRICT_BAD` bucket that differs from its baseline bucket (AB-L0030 ruling,
//!     notes 3bc1f50): `case_key K is now B, was A`. So equivalent -> different_undecided fails, and so does
//!     crash -> unclassified (fail-closed, even though it is an improvement), the same as a new key under strict;
//!   - the only upward move left, equivalent -> different_decided, is waived (reported in `waived`, never failed);
//!   - new keys are judged by strict exactly as without a reset.
//!
//! Reset and strict share ONE bad-bucket list, `STRICT_BAD`.
//!
//! Improvements (rank falls) are reported as `improved` in both modes.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::{read_totals, sha256_hex, InternalError, STRICT_BAD, TOTALS_KEYS};

fn rank(bucket: &str) -> usize {
    TOTALS_KEYS.iter().position(|k| *k == bucket).expect("bucket validated against TOTALS_KEYS")
}

/// case_key → bucket, from a summary's `cases` (validated: unique ids, known buckets, counts equal `totals`).
fn buckets_of(summary: &Value, what: &str) -> Result<BTreeMap<String, String>, InternalError> {
    let err = |m: String| Err(InternalError(format!("{what}: {m}")));
    let totals = read_totals(summary).map_err(|e| InternalError(format!("{what}: {e}")))?;
    let Some(cases) = summary.get("cases").and_then(Value::as_array) else { return err("no `cases` array".into()) };
    let mut out = BTreeMap::new();
    let mut counts: BTreeMap<&str, u64> = TOTALS_KEYS.iter().map(|k| (*k, 0)).collect();
    for (i, c) in cases.iter().enumerate() {
        let (Some(id), Some(b)) = (c.get("id").and_then(Value::as_str), c.get("bucket").and_then(Value::as_str)) else {
            return err(format!("cases[{i}] needs string `id` and `bucket`"));
        };
        let Some(n) = counts.get_mut(b) else { return err(format!("cases[{i}]: unknown bucket {b:?}")) };
        *n += 1;
        if out.insert(id.to_string(), b.to_string()).is_some() {
            return err(format!("duplicate case_key {id:?}"));
        }
    }
    for k in TOTALS_KEYS {
        if counts[k] != totals[k] {
            return err(format!("totals.{k}={} but {} cases are in that bucket", totals[k], counts[k]));
        }
    }
    Ok(out)
}

pub struct Baseline {
    pub sha256: String,
    pub rule_table_version: String,
    pub corpus: String,
    buckets: BTreeMap<String, String>,
}

/// Parse and validate a baseline summary (any defect: exit 2), then apply the rule-table rule.
pub fn load_baseline(bytes: &[u8], reset: bool) -> Result<Baseline, InternalError> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| InternalError(format!("baseline JSON: {e}")))?;
    let buckets = buckets_of(&v, "baseline")?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let (Some(rule_table_version), Some(corpus)) = (s("rule_table_version"), s("corpus")) else {
        return Err(InternalError("baseline: needs string `rule_table_version` and `corpus`".into()));
    };
    match (rule_table_version == crate::rule_table_version(), reset) {
        (false, false) => {
            return Err(InternalError(format!(
                "baseline-rule-table-mismatch: baseline {rule_table_version}, this build {}; a rule-table bump \
                 resets the baseline only explicitly: rerun with --reset-baseline on a board that records the reset",
                crate::rule_table_version()
            )))
        }
        (true, true) => {
            return Err(InternalError(format!(
                "--reset-baseline: baseline and build are both {}; only a rule-table bump may reset the baseline",
                crate::rule_table_version()
            )))
        }
        _ => {}
    }
    Ok(Baseline { sha256: sha256_hex(bytes), rule_table_version, corpus, buckets })
}

pub struct Outcome {
    /// Goes into the summary as `ratchet`.
    pub json: Value,
    /// Gate failures (empty = pass), one line each.
    pub failures: Vec<String>,
}

/// Compare this run's summary against a validated baseline. `reset` (rule-table bump) waives only movement among
/// equivalent and different_decided (module doc).
pub fn compare(cur: &Value, base: &Baseline, reset: bool) -> Result<Outcome, InternalError> {
    let cur = buckets_of(cur, "summary")?;
    let mut failures = Vec::new();
    let (mut regressed, mut improved, mut waived, mut new_failing) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let new_keys: Vec<&String> = cur.keys().filter(|k| !base.buckets.contains_key(*k)).collect();
    let dropped: BTreeSet<&String> = base.buckets.keys().filter(|k| !cur.contains_key(*k)).collect();
    for (k, now) in &cur {
        let Some(was) = base.buckets.get(k) else {
            if STRICT_BAD.contains(&now.as_str()) {
                failures.push(format!("new case_key {k} is {now} (new keys are judged by strict)"));
                new_failing.push(k);
            }
            continue;
        };
        let mv = json!({"id": k, "from": was, "to": now});
        if reset && STRICT_BAD.contains(&now.as_str()) && now != was {
            failures.push(format!("case_key {k} is now {now}, was {was} (a rule-table reset cannot create {now})"));
            regressed.push(mv);
        } else if rank(now) > rank(was) && reset {
            waived.push(mv);
        } else if rank(now) > rank(was) {
            failures.push(format!("case_key {k} regressed {was} -> {now}"));
            regressed.push(mv);
        } else if rank(now) < rank(was) {
            improved.push(mv);
        }
    }
    let json = json!({
        "baseline_sha256": base.sha256,
        "baseline_rule_table_version": base.rule_table_version,
        "reset": reset,
        "regressed": regressed,
        "improved": improved,
        "waived": waived,
        "new_keys": new_keys,
        "new_failing": new_failing,
        "dropped_keys": dropped,
    });
    Ok(Outcome { json, failures })
}
