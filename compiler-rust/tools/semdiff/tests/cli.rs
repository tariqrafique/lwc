//! Harness CLI contract: exit codes, required totals, no summary on internal error, byte-reproducible summaries.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_semdiff"))
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("semdiff-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A case_key in corpus `test` (design §2: corpus/source/sha256(template)/sha256(opts)).
fn key(name: &str) -> String {
    format!("test/{name}/{}/{}", semdiff::sha256_hex(name.as_bytes()), semdiff::sha256_hex(b"{}"))
}

/// A minimal registry: D-0196 (the observation model) and D-0001 locked, D-0002 open.
fn registry(dir: &Path, body: &str) -> PathBuf {
    let p = dir.join("decisions.tsv");
    fs::write(&p, body).unwrap();
    p
}

const REG: &str = "id\tts\tphase\tdecision\twhy\tevidence\tresult\nD-0001\tt\tp\td\tw\te\tlocked\nD-0002\tt\tp\td\tw\te\topen\nD-0196\tt\tp\td\tw\te\tlocked\n";

fn write_manifest(dir: &Path, cases: serde_json::Value) -> PathBuf {
    let m = serde_json::json!({"schema": "semdiff-manifest/v0", "corpus": "test", "cases": cases});
    let p = dir.join("manifest.json");
    fs::write(&p, serde_json::to_string(&m).unwrap()).unwrap();
    p
}

/// `run` with a valid registry next to the manifest.
fn run(manifest: &Path, out: &Path, extra: &[&str]) -> std::process::Output {
    let reg = registry(manifest.parent().unwrap(), REG);
    run_raw(manifest, out, &[&["--registry", reg.to_str().unwrap()], extra].concat())
}

fn run_raw(manifest: &Path, out: &Path, extra: &[&str]) -> std::process::Output {
    bin().arg("run").arg("--manifest").arg(manifest).arg("--out").arg(out).args(extra).output().unwrap()
}

fn read_json(p: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

fn two_cases() -> serde_json::Value {
    serde_json::json!([
        {"id": key("eq"), "oracle_code": "const a = {x: 1};", "rust_code": "const a={x:1};"},
        {"id": key("ne"), "oracle_code": "const a = {x: 1, y: 2};", "rust_code": "const a = {y: 2, x: 1};"},
    ])
}

#[test]
fn run_writes_summary_with_all_required_totals() {
    let d = scratch("ok");
    let m = write_manifest(&d, two_cases());
    let out = d.join("summary.json");
    let o = run(&m, &out, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let s: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    let t = &s["totals"];
    for k in ["equivalent", "different_decided", "different_undecided", "unclassified", "crash"] {
        assert!(t[k].is_u64(), "totals.{k} missing");
    }
    assert_eq!((t["cases"].as_u64(), t["equivalent"].as_u64(), t["unclassified"].as_u64()), (Some(2), Some(1), Some(1)));
    assert_eq!(s["cases"][1]["verdict"]["reason"], "printed-differs");
    let tot = semdiff::read_totals(&s).unwrap();
    assert_eq!(tot.len(), 5);
}

#[test]
fn summary_is_byte_reproducible() {
    let d = scratch("repro");
    let m = write_manifest(&d, two_cases());
    let (a, b) = (d.join("a.json"), d.join("b.json"));
    assert_eq!(run(&m, &a, &[]).status.code(), Some(0));
    assert_eq!(run(&m, &b, &[]).status.code(), Some(0));
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
}

#[test]
fn forced_internal_error_exits_nonzero_and_writes_no_summary() {
    // Case 1 classifies fine; case 2 names a file that does not exist -> internal error mid-run.
    let d = scratch("ierr");
    let m = write_manifest(&d, serde_json::json!([
        {"id": key("eq"), "oracle_code": "1;", "rust_code": "1 ;"},
        {"id": key("missing"), "oracle": "does-not-exist.js", "rust_code": "1;"},
    ]));
    let out = d.join("summary.json");
    fs::write(&out, "{\"stale\": true}").unwrap(); // a stale summary from an earlier run must not survive
    let o = run(&m, &out, &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(!out.exists(), "internal error must leave no summary");
    let leftovers: Vec<_> = fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers.len(), 2, "only manifest.json and the registry may remain: {leftovers:?}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("internal error"));
}

#[test]
fn malformed_manifest_and_usage_errors_exit_2_without_summary() {
    let d = scratch("bad");
    let out = d.join("summary.json");
    let bad = d.join("bad.json");
    fs::write(&bad, "{\"schema\": \"semdiff-manifest/v0\", \"corpus\": \"\", \"cases\": []}").unwrap();
    // (an empty `corpus` is malformed)
    assert_eq!(run(&bad, &out, &[]).status.code(), Some(2));
    assert!(!out.exists());
    let dup = write_manifest(&d, serde_json::json!([
        {"id": key("x"), "oracle_code": "1", "rust_code": "1"}, {"id": key("x"), "oracle_code": "1", "rust_code": "1"}]));
    let o = run(&dup, &out, &[]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("duplicate case_key"), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(run(&dup, &out, &[]).status.code(), Some(2));
    assert!(!out.exists());
    assert_eq!(run(&dup, &out, &["--gate", "ratchet"]).status.code(), Some(2), "ratchet without baseline");
    assert_eq!(bin().arg("frobnicate").output().unwrap().status.code(), Some(2));
}

#[test]
fn manifest_carrying_decision_id_exits_2_without_summary() {
    // AB-L0023: adapters must not author decisions; decision ids come only from the semdiff case_key map (SD-1).
    for dec in [serde_json::json!(null), serde_json::json!("D-0001")] {
        let d = scratch("decid");
        let m = write_manifest(&d, serde_json::json!([{"id": key("a"), "oracle_code": "f();", "rust_code": "f();", "decision_id": dec}]));
        let out = d.join("summary.json");
        let o = run(&m, &out, &[]);
        assert_eq!(o.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&o.stderr).contains("unknown field \"decision_id\""), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(!out.exists());
    }
    // Same for any unknown top-level key (e.g. a manifest-wide decisions table).
    let d = scratch("decid-top");
    let m = d.join("manifest.json");
    fs::write(&m, r#"{"schema":"semdiff-manifest/v0","corpus":"t","decisions":{},"cases":[]}"#).unwrap();
    let out = d.join("summary.json");
    assert_eq!(run(&m, &out, &[]).status.code(), Some(2));
    assert!(!out.exists());
}

#[test]
fn summary_v1_records_carry_bucket_and_map_only_decision_id() {
    let d = scratch("rec");
    let m = write_manifest(&d, two_cases());
    let out = d.join("summary.json");
    assert_eq!(run(&m, &out, &[]).status.code(), Some(0));
    let s: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(s["schema"], "semdiff-summary/v2");
    assert_eq!(s["observation_model"], "D-0196");
    assert_eq!(s["decisions"]["map"], "tools/semdiff/decisions.tsv");
    assert_eq!(s["decisions"]["entries"], 0);
    assert_eq!(s["decisions"]["registry_sha256"], semdiff::sha256_hex(REG.as_bytes()));
    let buckets: Vec<_> = s["cases"].as_array().unwrap().iter().map(|c| c["bucket"].as_str().unwrap()).collect();
    assert_eq!(buckets, ["equivalent", "unclassified"]);
    for c in s["cases"].as_array().unwrap() {
        // Present (schema v1) and null: the shipped map is empty and nothing is DIFFERENT before the exec leg.
        assert!(c["decision_id"].is_null() && c.as_object().unwrap().contains_key("decision_id"));
        assert_eq!(c["verdict"]["observation_model"], "D-0196");
    }
    assert!(s.get("ratchet").is_none(), "no ratchet block without --gate ratchet");
}

#[test]
fn strict_gate() {
    let d = scratch("strict");
    let m = write_manifest(&d, two_cases());
    let out = d.join("summary.json");
    let o = run(&m, &out, &["--gate", "strict"]);
    assert_eq!(o.status.code(), Some(1), "unclassified=1 fails strict");
    assert!(String::from_utf8_lossy(&o.stdout).contains("unclassified=1 (strict requires 0)"));
    assert!(out.exists(), "a gate failure is a valid run: summary is written");
    let ok = write_manifest(&d, serde_json::json!([{"id": key("eq"), "oracle_code": "f(1);", "rust_code": "f( 1 );"}]));
    assert_eq!(run(&ok, &out, &["--gate", "strict"]).status.code(), Some(0));
}

const EQ: (&str, &str) = ("f(1);", "f( 1 );");
const NE: (&str, &str) = ("f(1);", "f(2);");

fn cases(spec: &[(&str, (&str, &str))]) -> serde_json::Value {
    spec.iter().map(|(n, (o, r))| serde_json::json!({"id": key(n), "oracle_code": o, "rust_code": r})).collect()
}

/// Run `was` as the baseline (gate none), then `now` under the ratchet. Returns (exit, stdout, summary if any).
fn ratchet_run(name: &str, was: &[(&str, (&str, &str))], now: &[(&str, (&str, &str))], extra: &[&str]) -> (Option<i32>, String, Option<serde_json::Value>) {
    let d = scratch(name);
    let base = d.join("base.json");
    assert_eq!(run(&write_manifest(&d, cases(was)), &base, &[]).status.code(), Some(0));
    let out = d.join("summary.json");
    let o = run(&write_manifest(&d, cases(now)), &out, &[&["--gate", "ratchet", "--baseline", base.to_str().unwrap()], extra].concat());
    (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned(), out.exists().then(|| read_json(&out)))
}

#[test]
fn ratchet_unchanged_and_improved_pass() {
    let (c, _, s) = ratchet_run("r-same", &[("a", EQ), ("b", NE)], &[("a", EQ), ("b", NE)], &[]);
    assert_eq!(c, Some(0));
    let r = &s.unwrap()["ratchet"];
    assert_eq!((r["reset"].clone(), r["regressed"].clone(), r["new_keys"].clone()), (serde_json::json!(false), serde_json::json!([]), serde_json::json!([])));
    let (c, out, s) = ratchet_run("r-up", &[("a", NE)], &[("a", EQ)], &[]);
    assert_eq!(c, Some(0), "{out}");
    assert_eq!(s.unwrap()["ratchet"]["improved"][0]["to"], "equivalent");
}

#[test]
fn ratchet_per_key_regression_fails() {
    let (c, out, s) = ratchet_run("r-down", &[("a", EQ), ("b", NE)], &[("a", NE), ("b", NE)], &[]);
    assert_eq!(c, Some(1));
    assert!(out.contains(&format!("case_key {} regressed equivalent -> unclassified", key("a"))), "{out}");
    assert_eq!(s.unwrap()["ratchet"]["regressed"][0]["id"], key("a"), "a failing ratchet still writes the summary");
}

#[test]
fn ratchet_swap_fails_although_totals_are_equal() {
    let (c, out, s) = ratchet_run("r-swap", &[("a", EQ), ("b", NE)], &[("a", NE), ("b", EQ)], &[]);
    assert_eq!(c, Some(1), "{out}");
    let s = s.unwrap();
    assert_eq!((s["totals"]["equivalent"].clone(), s["totals"]["unclassified"].clone()), (serde_json::json!(1), serde_json::json!(1)));
    assert_eq!(s["ratchet"]["regressed"].as_array().unwrap().len(), 1);
    assert_eq!(s["ratchet"]["improved"].as_array().unwrap().len(), 1);
}

#[test]
fn ratchet_new_and_dropped_keys() {
    // Growth by an equivalent key, and a dropped key: reported, pass, no reset needed.
    let (c, out, s) = ratchet_run("r-grow", &[("a", EQ), ("gone", NE)], &[("a", EQ), ("new", EQ)], &[]);
    assert_eq!(c, Some(0), "{out}");
    let r = &s.unwrap()["ratchet"];
    assert_eq!((r["new_keys"].clone(), r["dropped_keys"].clone()), (serde_json::json!([key("new")]), serde_json::json!([key("gone")])));
    assert!(out.contains("new_keys=1") && out.contains("dropped_keys=1"), "{out}");
    // A new key in a strict-bad bucket fails (design §7: new keys are judged by strict rules for themselves).
    let (c, out, s) = ratchet_run("r-newbad", &[("a", EQ)], &[("a", EQ), ("new", NE)], &[]);
    assert_eq!(c, Some(1), "{out}");
    assert_eq!(s.unwrap()["ratchet"]["new_failing"], serde_json::json!([key("new")]));
}

#[test]
fn ratchet_rule_table_mismatch_is_an_explicit_reset() {
    let d = scratch("r-rt");
    let m = write_manifest(&d, cases(&[("a", EQ)]));
    let base = d.join("base.json");
    assert_eq!(run(&m, &base, &[]).status.code(), Some(0));
    let mut v = read_json(&base);
    v["rule_table_version"] = serde_json::json!("rt-9");
    fs::write(&base, serde_json::to_string(&v).unwrap()).unwrap();
    let b = base.to_str().unwrap();
    let out = d.join("summary.json");
    let o = run(&m, &out, &["--gate", "ratchet", "--baseline", b]);
    assert_eq!(o.status.code(), Some(2), "never silently passed");
    assert!(String::from_utf8_lossy(&o.stderr).contains("baseline-rule-table-mismatch"));
    assert!(!out.exists());
    let o = run(&m, &out, &["--gate", "ratchet", "--baseline", b, "--reset-baseline"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&o.stdout).contains("semdiff ratchet: baseline rt-9 reset=true"));
    let r = &read_json(&out)["ratchet"];
    assert_eq!((r["reset"].clone(), r["baseline_rule_table_version"].clone()), (serde_json::json!(true), serde_json::json!("rt-9")));
    // A reset with matching rule tables, or outside the ratchet, is a usage error.
    let same = d.join("same-rt.json");
    fs::copy(&out, &same).unwrap();
    let o = run(&m, &out, &["--gate", "ratchet", "--baseline", same.to_str().unwrap(), "--reset-baseline"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("only a rule-table bump"), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(run(&m, &out, &["--reset-baseline"]).status.code(), Some(2));
    assert!(!out.exists());
}

/// AB-L0030 end-to-end: a reset is not a free pass. (crash and different_* have no CLI producer yet; the crash, different_undecided and
/// waiver cases are in tests/ratchet.rs.)
#[test]
fn ratchet_reset_still_fails_new_unclassified_and_regressed_unclassified() {
    let d = scratch("r-reset-bad");
    let base = d.join("base.json");
    assert_eq!(run(&write_manifest(&d, cases(&[("a", EQ), ("b", NE)])), &base, &[]).status.code(), Some(0));
    let mut v = read_json(&base);
    v["rule_table_version"] = serde_json::json!("rt-9");
    fs::write(&base, serde_json::to_string(&v).unwrap()).unwrap();
    let out = d.join("summary.json");
    let reset = ["--gate", "ratchet", "--baseline", base.to_str().unwrap(), "--reset-baseline"];
    // a: equivalent -> unclassified; n: new and unclassified; b: unchanged unclassified (same bucket: not a failure).
    let o = run(&write_manifest(&d, cases(&[("a", NE), ("b", NE), ("n", NE)])), &out, &reset);
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert_eq!(o.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains(&format!("case_key {} is now unclassified, was equivalent (a rule-table reset cannot create unclassified)", key("a"))), "{stdout}");
    assert!(stdout.contains(&format!("new case_key {} is unclassified (new keys are judged by strict)", key("n"))), "{stdout}");
    assert_eq!(stdout.matches("semdiff gate FAIL").count(), 2, "{stdout}");
    let r = &read_json(&out)["ratchet"];
    assert_eq!(r["reset"], true);
    assert_eq!(r["regressed"][0]["id"], key("a"));
    assert_eq!(r["waived"], serde_json::json!([]));
    // The same reset with only an improvement passes.
    let o = run(&write_manifest(&d, cases(&[("a", EQ), ("b", EQ)])), &out, &reset);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stdout));
    assert!(String::from_utf8_lossy(&o.stdout).contains("improved=1 waived=0"));
}

fn check_registry(dir: &Path, body: Option<&str>, require: &[&str]) -> (Option<i32>, serde_json::Value, String) {
    let p = dir.join("reg.tsv");
    if let Some(b) = body {
        fs::write(&p, b).unwrap();
    }
    let mut c = bin();
    c.arg("check-registry").arg("--registry").arg(&p);
    for r in require {
        c.args(["--require", r]);
    }
    let o = c.output().unwrap();
    let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
    let v = serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null);
    (o.status.code(), v, String::from_utf8_lossy(&o.stderr).into_owned())
}

fn codes(v: &serde_json::Value) -> Vec<String> {
    v["findings"].as_array().unwrap().iter().map(|f| f["code"].as_str().unwrap().to_string()).collect()
}

#[test]
fn check_registry_exit_0_ok() {
    let d = scratch("cr-ok");
    let (c, v, _) = check_registry(&d, Some(REG), &["D-0001", "D-0196"]);
    assert_eq!(c, Some(0));
    assert_eq!(v["schema"], "semdiff-registry-check/v1");
    assert_eq!((v["ids"].clone(), v["locked"].clone(), v["exit"].clone()), (serde_json::json!(3), serde_json::json!(2), serde_json::json!(0)));
    assert_eq!(v["registry_sha256"], semdiff::sha256_hex(REG.as_bytes()));
    assert_eq!(v["findings"], serde_json::json!([]));
    assert_eq!(v["required"], serde_json::json!(["D-0001", "D-0196"]));
    // No --require: whole-registry validation only.
    assert_eq!(check_registry(&d, Some(REG), &[]).0, Some(0));
}

#[test]
fn check_registry_exit_1_unresolved_and_not_locked() {
    let d = scratch("cr-1");
    let (c, v, _) = check_registry(&d, Some(REG), &["D-0002", "D-0009", "D-0001"]);
    assert_eq!(c, Some(1));
    assert_eq!(codes(&v), ["DEC-NOT-LOCKED", "DEC-UNRESOLVED"]);
    assert_eq!(v["findings"][0]["id"], "D-0002");
    assert_eq!(v["findings"][1]["id"], "D-0009");
    assert_eq!(v["exit"], 1);
    // No id column: every require is DEC-UNRESOLVED (coverage-gate-spec), and the file itself is not malformed.
    let (c, v, _) = check_registry(&d, Some("ts\tresult\nx\tlocked\n"), &["D-0001"]);
    assert_eq!((c, codes(&v)), (Some(1), vec!["DEC-UNRESOLVED".to_string()]));
    assert!(v["findings"][0]["detail"].as_str().unwrap().contains("no `id` column"));
}

#[test]
fn check_registry_exit_2_invalid_ids_and_malformed_files() {
    let d = scratch("cr-2");
    let h = "id\tts\tphase\tdecision\twhy\tevidence\tresult\n";
    // Every DEC-ID-INVALID is reported (bad shape and union-merge duplicate), with line numbers; requires are not evaluated.
    let (c, v, _) = check_registry(&d, Some(&format!("{h}D-001\tt\nD-0001\tt\tp\td\tw\te\tlocked\nD-0001\tt\n")), &["D-0009"]);
    assert_eq!(c, Some(2));
    assert_eq!(codes(&v), ["DEC-ID-INVALID", "DEC-ID-INVALID"]);
    assert_eq!((v["findings"][0]["line"].clone(), v["findings"][1]["line"].clone()), (serde_json::json!(2), serde_json::json!(4)));
    assert_eq!(v["required_evaluated"], false);
    for (body, what) in [(Some(""), "empty"), (Some("id\tid\nD-0001\tD-0001\n"), "duplicate header"), (None, "missing file")] {
        let _ = fs::remove_file(d.join("reg.tsv"));
        let (c, v, _) = check_registry(&d, body, &[]);
        assert_eq!((c, codes(&v)), (Some(2), vec!["MALFORMED".to_string()]), "{what}");
    }
    fs::write(d.join("reg.tsv"), b"id\tresult\n\xff\tlocked\n").unwrap();
    let o = bin().args(["check-registry", "--registry"]).arg(d.join("reg.tsv")).output().unwrap();
    assert_eq!(o.status.code(), Some(2), "not UTF-8");
    assert!(String::from_utf8_lossy(&o.stdout).contains("MALFORMED"));
    // Usage errors: exit 2, stderr only, no JSON.
    for args in [&["check-registry"][..], &["check-registry", "--registry", "x", "--require", "D-19"], &["check-registry", "--registry", "x", "--bogus"]] {
        let o = bin().args(args).output().unwrap();
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(o.stdout.is_empty(), "{args:?}");
    }
}

/// `run` and `check-registry` agree, because they share Registry::check (AB-L0030: one validator).
#[test]
fn check_registry_and_run_agree() {
    let d = scratch("cr-agree");
    let m = write_manifest(&d, two_cases());
    let out = d.join("summary.json");
    let h = "id\tts\tphase\tdecision\twhy\tevidence\tresult\n";
    for body in [REG.to_string(), format!("{h}D-0196\tt\tp\td\tw\te\topen\n"), format!("{h}D-0196\tt\tp\td\tw\te\tlocked\nD-01\tx\n"), String::new()] {
        let (c, _, _) = check_registry(&d, Some(&body), &["D-0196"]);
        let reg = d.join("reg.tsv");
        let r = run_raw(&m, &out, &["--registry", reg.to_str().unwrap()]).status.code();
        assert_eq!(c == Some(0), r == Some(0), "{body:?}: check-registry {c:?} vs run {r:?}");
    }
}

#[test]
fn ratchet_invalid_baselines_exit_2() {
    let d = scratch("r-bad");
    let m = write_manifest(&d, cases(&[("a", EQ)]));
    let out = d.join("summary.json");
    let base = d.join("base.json");
    let b = base.to_str().unwrap();
    assert_eq!(run(&m, &out, &["--gate", "ratchet"]).status.code(), Some(2), "ratchet without baseline");
    assert_eq!(run(&m, &out, &["--baseline", b]).status.code(), Some(2), "baseline without ratchet");
    // Missing required total; v0 schema (no per-case buckets); a different corpus.
    fs::write(&base, "{\"schema\": \"semdiff-summary/v1\", \"totals\": {\"equivalent\": 1}}").unwrap();
    assert_eq!(run(&m, &out, &["--gate", "ratchet", "--baseline", b]).status.code(), Some(2));
    assert_eq!(run(&m, &base, &[]).status.code(), Some(0));
    let good = read_json(&base);
    let mut v0 = good.clone();
    v0["schema"] = serde_json::json!("semdiff-summary/v0");
    fs::write(&base, serde_json::to_string(&v0).unwrap()).unwrap();
    assert_eq!(run(&m, &out, &["--gate", "ratchet", "--baseline", b]).status.code(), Some(2));
    let mut other = good.clone();
    other["corpus"] = serde_json::json!("other");
    fs::write(&base, serde_json::to_string(&other).unwrap()).unwrap();
    let o = run(&m, &out, &["--gate", "ratchet", "--baseline", b]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("baseline-corpus-mismatch"));
    assert!(!out.exists());
}

#[test]
fn case_key_is_validated_and_bound_to_the_manifest_corpus() {
    let d = scratch("ck");
    let out = d.join("summary.json");
    let h = semdiff::sha256_hex(b"x");
    for (id, why) in [
        ("eq".to_string(), "case_key"),
        (format!("other/a/{h}/{h}"), "manifest corpus is \"test\""),
        (format!("test/a b/{h}/{h}"), "whitespace"),
        (format!("test/a/{}/{h}", h.to_uppercase()), "64-hex"),
    ] {
        let m = write_manifest(&d, serde_json::json!([{"id": id, "oracle_code": "1;", "rust_code": "1;"}]));
        let o = run(&m, &out, &[]);
        assert_eq!(o.status.code(), Some(2), "{id}");
        assert!(String::from_utf8_lossy(&o.stderr).contains(why), "{id}: {}", String::from_utf8_lossy(&o.stderr));
        assert!(!out.exists());
    }
    let m = write_manifest(&d, serde_json::json!([{"id": format!("test/fixtures/a/b.html/{h}/{h}"), "oracle_code": "1;", "rust_code": "1;"}]));
    assert_eq!(run(&m, &out, &[]).status.code(), Some(0), "multi-segment source is fine");
}

#[test]
fn registry_is_required_and_validated() {
    let d = scratch("reg");
    let m = write_manifest(&d, two_cases());
    let out = d.join("summary.json");
    fs::write(&out, "stale").unwrap();
    let o = run_raw(&m, &out, &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("--registry is required"));
    assert!(!out.exists(), "stale summary removed even on a usage error");
    let h = "id\tts\tphase\tdecision\twhy\tevidence\tresult\n";
    for (body, why) in [
        ("ts\tresult\nx\tlocked\n".to_string(), "DEC-UNRESOLVED"),
        (format!("{h}D-0196\tt\tp\td\tw\te\tlocked\nD-19\tt\n"), "DEC-ID-INVALID"),
        (format!("{h}D-0196\tt\tp\td\tw\te\tlocked\nD-0196\tt\n"), "duplicate id D-0196"),
        (format!("{h}D-0196\tt\tp\td\tw\te\topen\n"), "observation model: DEC-NOT-LOCKED"),
        (format!("{h}D-0001\tt\tp\td\tw\te\tlocked\n"), "observation model: DEC-UNRESOLVED"),
    ] {
        let reg = registry(&d, &body);
        let o = run_raw(&m, &out, &["--registry", reg.to_str().unwrap()]);
        assert_eq!(o.status.code(), Some(2), "{why}");
        assert!(String::from_utf8_lossy(&o.stderr).contains(why), "{why}: {}", String::from_utf8_lossy(&o.stderr));
        assert!(!out.exists());
    }
    let reg = registry(&d, REG);
    assert_eq!(run_raw(&m, &out, &["--registry", reg.to_str().unwrap()]).status.code(), Some(0));
    assert_eq!(run_raw(&m, &out, &["--registry", d.join("nope.tsv").to_str().unwrap()]).status.code(), Some(2));
}

#[test]
fn pair_subcommand_prints_one_verdict() {
    let d = scratch("pair");
    let (a, b) = (d.join("a.js"), d.join("b.js"));
    fs::write(&a, "f( 1 ) ;").unwrap();
    fs::write(&b, "f(1);").unwrap();
    let o = bin().arg("pair").arg(&a).arg(&b).output().unwrap();
    assert_eq!(o.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["class"], "EQUIVALENT-BY-REWRITE");
}
