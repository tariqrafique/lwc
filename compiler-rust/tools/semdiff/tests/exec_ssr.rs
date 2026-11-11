//! SD-2b SSR exec leg. CSR tests stay in exec.rs. These cover the r2 plan: P1–P5, N1–N6, and the
//! 8.2 mutation kills. `Class::EquivalentByExecution` is not constructed.
use semdiff::exec::{self, ExecCfg, ExecEngine, SsrMeta};
use semdiff::{Class, Reason};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!(
        "semdiff-ssr-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn meta(mode: &str, props: Value) -> SsrMeta {
    SsrMeta {
        rel: "for-each-block/basic".into(),
        mode: mode.into(),
        props,
        style_dedupe: json!(true),
        runtime_kind: "dist".into(),
    }
}

/// A trace the SSR parser accepts for `input`.
fn trace(input: &Value, html: &str) -> String {
    let sha = input["props_sha256"].as_str().unwrap();
    let mode = input["mode"].as_str().unwrap();
    format!(
        r#"{{"handled":[],"helpers":{{}},"reads":[],"schema":"semdiff-exec-trace/v0","steps":[{{"dom":{{"body":null,"head":[]}},"events":[{{"e":"enter","mode":"{mode}","props_sha256":"{sha}","styleDedupe":true,"tag":"fixture-test"}},{{"e":"html","html":{html_json}}},{{"e":"exit"}}],"kind":"render","label":"serverSideRenderComponent"}}]}}"#,
        html_json = serde_json::to_string(html).unwrap()
    )
}

fn throw_trace(input: &Value) -> String {
    let sha = input["props_sha256"].as_str().unwrap();
    let mode = input["mode"].as_str().unwrap();
    format!(
        r#"{{"handled":[],"helpers":{{}},"reads":[],"schema":"semdiff-exec-trace/v0","steps":[{{"dom":{{"body":null,"head":[]}},"events":[{{"e":"enter","mode":"{mode}","props_sha256":"{sha}","styleDedupe":true,"tag":"fixture-test"}},{{"e":"throw","message":"mutated-throw","name":"Error"}},{{"e":"exit"}}],"kind":"render","label":"serverSideRenderComponent"}}]}}"#
    )
}

fn decide_traces(
    o_src: &str,
    r_src: &str,
    o_html: &str,
    r_html: Option<&str>,
    r_throw: bool,
    r_trap: bool,
) -> exec::ExecDecision {
    let input = exec::generate_ssr(1, "k", o_src, r_src, &meta("sync", json!({}))).unwrap();
    let o_trace = trace(&input.json, o_html);
    let r_trace = if r_throw {
        throw_trace(&input.json)
    } else if r_trap {
        trap_trace(&input.json, o_html)
    } else {
        trace(&input.json, r_html.unwrap_or(o_html))
    };
    let out = exec::collect(&input.json, |side, _| {
        Ok(if side == "oracle" {
            o_trace.clone()
        } else {
            r_trace.clone()
        })
    });
    exec::decide(&input, out)
}

fn trap_trace(input: &Value, html: &str) -> String {
    let sha = input["props_sha256"].as_str().unwrap();
    let html_json = serde_json::to_string(html).unwrap();
    format!(
        r#"{{"handled":[],"helpers":{{}},"reads":[],"schema":"semdiff-exec-trace/v0","steps":[{{"dom":{{"body":null,"head":[]}},"events":[{{"e":"enter","mode":"sync","props_sha256":"{sha}","styleDedupe":true,"tag":"fixture-test"}},{{"e":"html","html":{html_json}}},{{"count":1,"e":"sandbox-trap","fn":"Math.random"}},{{"e":"exit"}}],"kind":"render","label":"serverSideRenderComponent"}}]}}"#
    )
}

#[test]
fn csr_corpus_stays_on_the_csr_runner() {
    assert_eq!(
        exec::engine_for_corpus("template-diag-codegen-only"),
        ExecEngine::Csr
    );
    assert_eq!(exec::engine_for_corpus("test"), ExecEngine::Csr);
    assert_eq!(exec::engine_for_corpus("ssr-compiler"), ExecEngine::Ssr);
}

#[test]
fn canonical_json_matches_the_emitter() {
    let v = json!({"b": 1, "a": [true, "x"]});
    assert_eq!(
        exec::canonical_json(&v).unwrap(),
        r#"{"a":[true,"x"],"b":1}"#
    );
    let q = json!({"b": "a\"b", "a": true});
    assert_eq!(
        exec::canonical_json(&q).unwrap(),
        r#"{"a":true,"b":"a\"b"}"#
    );
}

#[test]
fn whitespace_only_html_is_exec_differs_not_formatted() {
    // 8.2 #2: formatHTML before compare would hide a trailing space. decide sees the raw event.
    let src = "export default 1;\n";
    let d = decide_traces(
        src,
        src,
        "<div>a</div>",
        Some("<div>a</div> "),
        false,
        false,
    );
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecDiffers),
        "{}",
        d.detail
    );
    assert_ne!(d.class.as_str(), "EQUIVALENT-BY-EXECUTION");
}

#[test]
fn equal_traces_and_printed_differs_are_static_observed() {
    // N4 / 8.2 #1. Equal traces must not become equivalent or unclassified.
    let o = "export const unused = \"oracle\";\nexport default 1;\n";
    let r = "export const unused = \"rust\";\nexport default 1;\n";
    assert_eq!(semdiff::compare(o, r).reason, Reason::PrintedDiffers);
    let d = decide_traces(o, r, "<div>a</div>", None, false, false);
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::StaticObservedDiffers),
        "{}",
        d.detail
    );
    assert_eq!(d.detail["trace"], "equal");
    assert_eq!(d.detail["rewrite_reason"], "printed-differs");
}

#[test]
fn comment_only_drift_uses_the_sd2a_reason() {
    // N5. Same decide path as CSR. No new rule-table entry.
    let o = "export default 1;\n/* banner-o */\n";
    let r = "export default 1;\n/* banner-r */\n";
    assert_eq!(semdiff::compare(o, r).reason, Reason::CommentsDiffer);
    let d = decide_traces(o, r, "<x></x>", None, false, false);
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecStaticCommentsDiffer)
    );
    assert_eq!(d.first_diff["coords"], "static-comments");
}

#[test]
fn throw_on_one_side_is_not_dropped() {
    // N2 / 8.2 #3.
    let src = "export default 1;\n";
    let d = decide_traces(src, src, "<x></x>", None, true, false);
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecThrowOneSide),
        "{}",
        d.first_diff
    );
}

#[test]
fn truncated_trace_exits_as_harness_failure() {
    // N3 / 8.2 #4. Malformed JSON is a crash, never UNCLASSIFIED. The CLI maps that reason to exit 2.
    let src = "export default 1;\n";
    let input = exec::generate_ssr(1, "k", src, src, &meta("sync", json!({}))).unwrap();
    let out = exec::collect(&input.json, |_, _| Ok("{".into()));
    let d = exec::decide(&input, out);
    assert_eq!(
        (d.class, d.reason),
        (Class::Crash, Reason::ExecHarnessFailure),
        "{}",
        d.detail
    );
    let summary = json!({"by_reason": {"exec-harness-failure": 1}});
    assert!(exec::ssr_protocol_failure(&summary));
    assert!(!exec::ssr_protocol_failure(
        &json!({"by_reason": {"exec-static-comments-differ": 1}})
    ));
}

#[test]
fn sandbox_trap_on_one_side_is_exec_differs() {
    // N6 at the decide boundary: a Math.random trap event on rust only.
    let src = "export default 1;\n";
    let d = decide_traces(src, src, "<x></x>", None, false, true);
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecDiffers),
        "{}",
        d.first_diff
    );
}

fn node() -> Command {
    let mut c = Command::new("node");
    c.current_dir(repo())
        .env("LWC_ORACLE", exec::ORACLE_DIR)
        .env("LWC_ROOT", exec::ORACLE_DIR)
        .env("PATH", std::env::var("PATH").unwrap_or_default());
    c
}

fn emit_js(dir: &Path, rel: &str, modes: &[&str]) {
    let mut cmd = node();
    cmd.arg("--experimental-transform-types")
        .arg("--import")
        .arg(repo().join("harness/scripts/ts-register.mjs"))
        .arg(repo().join("harness/scripts/ssr-emit-pairs.ts"))
        .arg("--out")
        .arg(dir)
        .arg("--js-only")
        .arg("--only")
        .arg(rel)
        .arg("--jobs")
        .arg("2");
    for m in modes {
        cmd.arg("--mode").arg(m);
    }
    let o = cmd.output().unwrap();
    assert!(
        o.status.success(),
        "emit {}\n{}",
        rel,
        String::from_utf8_lossy(&o.stderr)
    );
}

fn load_case(dir: &Path) -> (String, String, String, Value) {
    let m: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    let c = &m["cases"][0];
    let id = c["id"].as_str().unwrap().to_string();
    let oracle = fs::read_to_string(dir.join(c["oracle"].as_str().unwrap())).unwrap();
    let rust = fs::read_to_string(dir.join(c["rust"].as_str().unwrap())).unwrap();
    let meta_line = fs::read_to_string(dir.join("meta.jsonl")).unwrap();
    let meta: Value = serde_json::from_str(meta_line.lines().next().unwrap()).unwrap();
    (id, oracle, rust, meta)
}

fn ssr_meta_from(v: &Value) -> SsrMeta {
    SsrMeta {
        rel: v["rel"].as_str().unwrap().into(),
        mode: v["mode"].as_str().unwrap().into(),
        props: v["props"].clone(),
        style_dedupe: v["styleDedupe"].clone(),
        runtime_kind: v["runtimeKind"].as_str().unwrap().into(),
    }
}

fn run_real(
    dir: &Path,
    oracle: &str,
    rust: &str,
    meta: &SsrMeta,
    mutate: Option<&str>,
) -> exec::ExecDecision {
    exec::preflight(&ExecCfg::csr(
        1,
        1,
        PathBuf::from(exec::ORACLE_DIR),
        dir.to_path_buf(),
    ))
    .expect("oracle preflight");
    let input = exec::generate_ssr(1, "k", oracle, rust, meta).unwrap();
    let runtime = exec::ssr_runtime_path(Path::new(exec::ORACLE_DIR), &meta.runtime_kind);
    let out = exec::run_case_ssr_mut(
        &dir.join("case"),
        &input.json,
        oracle,
        rust,
        &runtime,
        mutate,
    );
    exec::decide(&input, out)
}

#[test]
fn identical_bundles_short_circuit_and_real_traces_agree() {
    // P1. Printer-equal pairs do not require exec. The runner still produces byte-equal traces.
    let dir = scratch("p1");
    emit_js(&dir, "for-each-block/basic", &["sync"]);
    let (_id, oracle, rust, mv) = load_case(&dir);
    assert_eq!(oracle, rust);
    let v = semdiff::compare(&oracle, &rust);
    assert_eq!(
        (v.class, v.reason),
        (Class::EquivalentByRewrite, Reason::PrinterEqual)
    );
    let meta = ssr_meta_from(&mv);
    let d = run_real(&dir, &oracle, &rust, &meta, None);
    assert_ne!(d.reason, Reason::ExecDiffers, "{}", d.first_diff);
    assert_ne!(d.class.as_str(), "EQUIVALENT-BY-EXECUTION");
    let otrace = fs::read_to_string(dir.join("case/oracle.0.json")).unwrap();
    let rtrace = fs::read_to_string(dir.join("case/rust.0.json")).unwrap();
    assert_eq!(
        otrace, rtrace,
        "identical bundles must produce byte-equal traces"
    );
    // Props hash is in the enter event (P2): the kept trace file.
    let t: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("case/oracle.0.json")).unwrap()).unwrap();
    let enter = &t["steps"][0]["events"][0];
    assert_eq!(enter["e"], "enter");
    assert_eq!(enter["tag"], "fixture-test");
    assert_eq!(enter["mode"], "sync");
    let want = semdiff::sha256_hex(exec::canonical_json(&meta.props).unwrap().as_bytes());
    assert_eq!(enter["props_sha256"], want);
    let html = t["steps"][0]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["e"] == "html")
        .unwrap();
    assert!(
        html["html"].as_str().unwrap().contains("paris"),
        "prop value reached the HTML event"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn slotted_fixture_html_contains_the_slot() {
    // P3.
    let dir = scratch("p3");
    emit_js(&dir, "slots", &["sync"]);
    let (_id, oracle, rust, mv) = load_case(&dir);
    let d = run_real(&dir, &oracle, &rust, &ssr_meta_from(&mv), None);
    assert_ne!(d.reason, Reason::ExecDiffers, "{}", d.first_diff);
    assert_ne!(d.reason, Reason::ExecThrowOneSide, "{}", d.first_diff);
    let t: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("case/oracle.0.json")).unwrap()).unwrap();
    let html = t["steps"][0]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["e"] == "html")
        .unwrap()["html"]
        .as_str()
        .unwrap();
    assert!(
        html.contains("my-slot") || html.contains("Default slot") || html.contains("slot"),
        "{html}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn three_modes_are_independent_case_keys() {
    // P4.
    let dir = scratch("p4");
    emit_js(
        &dir,
        "for-each-block/basic",
        &["sync", "async", "asyncYield"],
    );
    let m: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    let cases = m["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    let mut modes = Vec::new();
    for line in fs::read_to_string(dir.join("meta.jsonl")).unwrap().lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        modes.push(v["mode"].as_str().unwrap().to_string());
    }
    modes.sort();
    assert_eq!(modes, ["async", "asyncYield", "sync"]);
    let ids: Vec<&str> = cases.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        3,
        "{ids:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn observer_html_suffix_is_exec_differs() {
    // N1. Observer post-hook on the rust side only.
    let dir = scratch("n1");
    emit_js(&dir, "for-each-block/basic", &["sync"]);
    let (_id, oracle, rust, mv) = load_case(&dir);
    let d = run_real(
        &dir,
        &oracle,
        &rust,
        &ssr_meta_from(&mv),
        Some("html-suffix"),
    );
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecDiffers),
        "{}",
        d.first_diff
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn observer_throw_rust_is_throw_one_side() {
    let dir = scratch("n2");
    emit_js(&dir, "for-each-block/basic", &["sync"]);
    let (_id, oracle, rust, mv) = load_case(&dir);
    let d = run_real(
        &dir,
        &oracle,
        &rust,
        &ssr_meta_from(&mv),
        Some("throw-rust"),
    );
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecThrowOneSide),
        "{}",
        d.first_diff
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn observer_random_rust_is_exec_differs() {
    // N6 through the runner.
    let dir = scratch("n6");
    emit_js(&dir, "for-each-block/basic", &["sync"]);
    let (_id, oracle, rust, mv) = load_case(&dir);
    let d = run_real(
        &dir,
        &oracle,
        &rust,
        &ssr_meta_from(&mv),
        Some("random-rust"),
    );
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecDiffers),
        "{}",
        d.first_diff
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn completeness_drop_exits_2() {
    let o = node()
        .arg("--experimental-transform-types")
        .arg("--import")
        .arg(repo().join("harness/scripts/ts-register.mjs"))
        .arg(repo().join("harness/scripts/ssr-emit-pairs.ts"))
        .arg("--prove-drop")
        .output()
        .unwrap();
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("dropped fixture"));
}

#[test]
fn jobs_1_and_4_are_byte_identical() {
    // P5. Printer-equal short-circuit still records the same summary; add a comment-only pair so exec runs.
    let dir = scratch("p5");
    emit_js(&dir, "for-each-block/basic", &["sync"]);
    let (id, oracle, mut rust, _mv) = load_case(&dir);
    rust.push_str("\n/* ssr job invariance */\n");
    let manifest = json!({
        "schema": "semdiff-manifest/v0",
        "corpus": "ssr-compiler",
        "cases": [{"id": id, "oracle_code": oracle, "rust_code": rust}]
    });
    // The case id's corpus segment must match. id already starts with ssr-compiler.
    let mp = dir.join("manifest.json");
    fs::write(&mp, serde_json::to_string(&manifest).unwrap()).unwrap();
    // meta for the mutated pair: reuse the emitted meta line (same props/mode) but the id changed?
    // We kept the same id. meta.jsonl from the emit still matches.
    let reg = dir.join("decisions.tsv");
    fs::write(
        &reg,
        "id\tts\tphase\tdecision\twhy\tevidence\tresult\nD-0196\tt\tp\td\tw\te\tlocked\n",
    )
    .unwrap();
    let run = |jobs: &str, out: &str| {
        let o = Command::new(env!("CARGO_BIN_EXE_semdiff"))
            .args([
                "run",
                "--exec",
                "--exec-jobs",
                jobs,
                "--gate",
                "none",
                "--manifest",
            ])
            .arg(&mp)
            .arg("--out")
            .arg(dir.join(out))
            .arg("--registry")
            .arg(&reg)
            .env("LWC_ORACLE", exec::ORACLE_DIR)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        fs::read(dir.join(out)).unwrap()
    };
    let a = run("1", "a.json");
    let b = run("4", "b.json");
    assert_eq!(a, b, "jobs 1 vs 4 summaries differ");
    let s: Value = serde_json::from_slice(&a).unwrap();
    assert_eq!(s["exec"]["engine"], "ssr-runtime");
    assert_eq!(s["exec"]["ssr_mode"], "sync");
    assert_eq!(s["exec"]["cases_run"], 1);
    assert_eq!(s["cases"][0]["verdict"]["class"], "DIFFERENT");
    let _ = fs::remove_dir_all(&dir);
}
