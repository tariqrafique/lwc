//! SD-2a execution leg (design: notes/arch-board/proposals/semdiff-SD-2a.md). Needs node and the clean oracle at
//! `exec::ORACLE_DIR` @ `exec::ORACLE_COMMIT` (preflight asserts both; these tests fail loudly rather than skip).
//!
//! Fixtures: one base pair where the oracle hoists a static data object (`stc0`) and the Rust side inlines it, both
//! carrying the same version banner. Each negative mutates one token of the Rust side; the expected first divergent
//! step follows from the scripted sequence (load, mount(A), event:*, set:<root>=B in root order, len:*, reset, unmount).
use semdiff::exec::{self, ExecCfg};
use semdiff::{Class, Reason};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

const HEAD: &str = r#"import {freezeTemplate, registerTemplate} from "lwc";
"#;
const HOISTED: &str = r#"const stc0 = {
  classMap: {
    "box": true
  },
  key: 0
};
"#;
const BODY: &str = r#"function tmpl($api, $cmp, $slotset, $ctx) {
  const {t: api_text, d: api_dynamic_text, h: api_element, b: api_bind, k: api_key, i: api_iterator, f: api_flatten} = $api;
  const {_m0} = $ctx;
  return api_flatten([api_element("div", STC0, [api_text("hi")]), api_element("p", {
    attrs: {
      "title": $cmp.title,
      "lang": "en"
    },
    key: 1,
    on: _m0 || ($ctx._m0 = {
      "click": api_bind($cmp.handleClick)
    })
  }, [api_text(api_dynamic_text($cmp.label)), $cmp.flag ? api_text("y") : api_text("n")]), api_iterator($cmp.items, function (item) {
    return api_element("li", {
      key: api_key(2, item.id)
    }, [api_text(api_dynamic_text(item.name))]);
  })]);
  /*LWC compiler v9.5.3*/
}
export default registerTemplate(tmpl);
tmpl.stylesheets = [];
freezeTemplate(tmpl);
"#;
const INLINE_STC0: &str = r#"{
    classMap: {
      "box": true
    },
    key: 0
  }"#;

fn oracle() -> String {
    format!("{HEAD}{HOISTED}{}", BODY.replace("STC0", "stc0"))
}
fn rust() -> String {
    format!("{HEAD}{}", BODY.replace("STC0", INLINE_STC0))
}
/// The Rust side with exactly one substring replaced (the replacement must apply).
fn rust_with(from: &str, to: &str) -> String {
    let r = rust();
    assert!(r.contains(from), "fixture: {from:?} not in rust side");
    r.replacen(from, to, 1)
}

fn scratch(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!(
        "semdiff-exec-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A case_key in corpus `test` (corpus/source/sha256(template)/sha256(opts)).
fn key(name: &str) -> String {
    format!(
        "test/{name}/{}/{}",
        semdiff::sha256_hex(name.as_bytes()),
        semdiff::sha256_hex(b"{}")
    )
}

fn cfg(work: &Path) -> ExecCfg {
    ExecCfg::csr(1, 1, PathBuf::from(exec::ORACLE_DIR), work.to_path_buf())
}

/// Generate, run, decide one pair; returns (decision, input set json, oracle trace, rust trace).
fn run_pair(name: &str, o: &str, r: &str) -> (exec::ExecDecision, Value, Option<(Value, Value)>) {
    let work = scratch(name);
    exec::preflight(&cfg(&work))
        .expect("preflight: oracle clean at ORACLE_COMMIT and node present");
    let id = format!("test/{name}");
    let input = exec::generate(1, &id, o, r);
    let out = exec::run_case(&work.join("c"), &input.json, o, r);
    let traces = match &out {
        exec::Outcome::Traces { o, r } => Some((
            serde_json::to_value(o).unwrap(),
            serde_json::to_value(r).unwrap(),
        )),
        _ => None,
    };
    let json = input.json.clone();
    let d = exec::decide(&input, out);
    let _ = fs::remove_dir_all(&work);
    (d, json, traces)
}

fn assert_different_at(name: &str, r: &str, label: &str) -> exec::ExecDecision {
    let (d, input, _) = run_pair(name, &oracle(), r);
    assert_eq!(
        d.class,
        Class::Different,
        "{name}: {:?} {}",
        d.reason,
        d.detail
    );
    assert_eq!(d.reason, Reason::ExecDiffers, "{name}");
    assert_eq!(
        d.first_diff["coords"], "exec-step",
        "{name}: {}",
        d.first_diff
    );
    assert_eq!(d.first_diff["label"], label, "{name}: {}", d.first_diff);
    let idx = d.first_diff["index"].as_u64().unwrap() as usize;
    assert_eq!(
        input["steps"][idx]["label"], label,
        "{name}: index points at the labelled step"
    );
    d
}

#[test]
fn static_hoisting_pair_is_different_static_observed() {
    // A hoist is printed-differs: Function#toString sees it, so equal traces must not become EXEC_EQUAL.
    let (d, input, traces) = run_pair("pos", &oracle(), &rust());
    assert_eq!(
        d.class,
        Class::Different,
        "{:?} {} {}",
        d.reason,
        d.first_diff,
        d.detail
    );
    assert_eq!(d.reason, Reason::StaticObservedDiffers);
    assert_eq!(d.detail["trace"], "equal");
    assert_eq!(d.detail["rewrite_reason"], "printed-differs");
    assert!(d.trace_sha256.is_some());
    // The script exercised what the brief lists: initial props, an event, prop changes, iterator-length changes.
    let labels: Vec<&str> = input["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .collect();
    for l in [
        "load",
        "mount(A)",
        "event:click",
        "set:flag=B",
        "set:items=B",
        "len:items=3",
        "len:items=1",
        "reset(A)",
        "unmount",
    ] {
        assert!(labels.contains(&l), "missing step {l}: {labels:?}");
    }
    let (o, _) = traces.unwrap();
    let reads: Vec<&str> = o["reads"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for p in [
        "flag",
        "items",
        "items[].id",
        "items[].name",
        "label",
        "title",
    ] {
        assert!(reads.contains(&p), "slot {p} not read: {reads:?}");
    }
    assert_eq!(o["handled"], json!(["click"]));
}

#[test]
fn swapped_attr_value_differs_at_mount() {
    let r = rust_with(
        r#""title": $cmp.title,
      "lang": "en""#,
        r#""title": "en",
      "lang": $cmp.title"#,
    );
    assert_different_at("attr", &r, "mount(A)");
}

#[test]
fn dropped_listener_differs_at_mount() {
    // The `on` data is a recorded argument of the element call, so the drop is seen at mount, before the event step.
    let r = rust_with(
        r#",
    on: _m0 || ($ctx._m0 = {
      "click": api_bind($cmp.handleClick)
    })"#,
        "",
    );
    let d = assert_different_at("listener", &r, "mount(A)");
    // First divergent event: the oracle reads $cmp.handleClick (to bind it); the Rust side moves on to `label`.
    assert_eq!(
        d.first_diff["oracle"], r#"{"e":"get","path":"handleClick"}"#,
        "{}",
        d.first_diff
    );
}

#[test]
fn different_key_differs_at_mount() {
    assert_different_at(
        "key",
        &rust_with("api_key(2, item.id)", "api_key(3, item.id)"),
        "mount(A)",
    );
}

#[test]
fn different_class_token_differs_at_mount() {
    assert_different_at(
        "class",
        &rust_with(r#""box": true"#, r#""box2": true"#),
        "mount(A)",
    );
}

#[test]
fn branch_only_reached_by_b_values_differs_at_that_set_step() {
    // A values are truthy, so mount and the event agree; the first set step is set:flag=B (roots in name order).
    assert_different_at(
        "branch",
        &rust_with(r#"api_text("n")"#, r#"api_text("N")"#),
        "set:flag=B",
    );
}

#[test]
fn throw_on_one_side_only_is_different() {
    let r = rust_with("f: api_flatten}", "f: api_flatten, zz: api_zz}").replacen(
        r#"[api_text("hi")]"#,
        r#"[api_zz("hi")]"#,
        1,
    );
    let (d, _, _) = run_pair("throw", &oracle(), &r);
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecThrowOneSide),
        "{}",
        d.first_diff
    );
}

#[test]
fn rust_only_field_is_in_the_input_set_and_driven() {
    // AB-D0003 (i): the generator reads the UNION of both sides' $cmp chains.
    let r = rust_with(
        r#"[api_text("hi")]"#,
        r#"[api_text(api_dynamic_text($cmp.extra))]"#,
    );
    let (d, input, traces) = run_pair("union", &oracle(), &r);
    assert!(
        input["slots"].as_array().unwrap().contains(&json!("extra")),
        "{}",
        input["slots"]
    );
    assert!(input["roots"]
        .as_array()
        .unwrap()
        .contains(&json!({"name": "extra", "kind": "prop"})));
    let labels: Vec<&str> = input["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"set:extra=B"), "{labels:?}");
    let (o, r) = traces.expect("both sides ran");
    let reads = |t: &Value| t["reads"].as_array().unwrap().contains(&json!("extra"));
    assert!(reads(&r) && !reads(&o), "extra is read by rust only");
    assert_eq!(d.class, Class::Different);
    assert_eq!(d.first_diff["label"], "mount(A)");
}

#[test]
fn exec_uncovered_case_is_unclassified() {
    // Both sides alias api_comment behind a branch no generated value reaches: traces agree, but a helper the
    // module can call never ran, so equality is not evidence (AB-D0003 (iii)).
    let body = |s: String| {
        s.replacen("f: api_flatten}", "f: api_flatten, co: api_comment}", 1)
            .replacen(
                r#"[api_text("hi")]"#,
                r#"[$cmp.never === 12345 ? api_comment("x") : api_text("hi")]"#,
                1,
            )
    };
    let (d, _, _) = run_pair("uncovered", &body(rust()), &body(rust()));
    assert_eq!(
        (d.class, d.reason),
        (Class::Unclassified, Reason::ExecUncovered),
        "{}",
        d.detail
    );
    assert!(
        d.detail.to_string().contains("helper-not-run:rust:co"),
        "{}",
        d.detail
    );
}

#[test]
fn comment_difference_is_different_even_when_traces_agree() {
    // D-0196: comments are observed (statically: Function#toString / the engine's banner check).
    let (d, _, _) = run_pair(
        "banner",
        &oracle(),
        &rust_with("  /*LWC compiler v9.5.3*/\n", ""),
    );
    assert_eq!(
        (d.class, d.reason),
        (Class::Different, Reason::ExecStaticCommentsDiffer)
    );
    assert_eq!(d.first_diff["coords"], "static-comments");
    assert!(d.trace_sha256.is_some(), "traces ran and agreed");
}

#[test]
fn generator_is_a_pure_function_of_seed_and_case() {
    let a = exec::generate(7, "k", &oracle(), &rust()).json;
    assert_eq!(a, exec::generate(7, "k", &oracle(), &rust()).json);
    assert_ne!(
        a["rng_seed"],
        exec::generate(8, "k", &oracle(), &rust()).json["rng_seed"]
    );
    assert_eq!(a["seed"], 7);
}

#[test]
fn cli_exec_summaries_are_byte_identical_across_runs() {
    let dir = scratch("cli");
    let m = json!({"schema": "semdiff-manifest/v0", "corpus": "test", "cases": [
        {"id": key("pos"), "oracle_code": oracle(), "rust_code": rust()},
        {"id": key("key"), "oracle_code": oracle(), "rust_code": rust_with("api_key(2, item.id)", "api_key(3, item.id)")},
    ]});
    let mp = dir.join("manifest.json");
    fs::write(&mp, serde_json::to_string(&m).unwrap()).unwrap();
    let reg = dir.join("decisions.tsv");
    fs::write(
        &reg,
        "id\tts\tphase\tdecision\twhy\tevidence\tresult\nD-0196\tt\tp\td\tw\te\tlocked\n",
    )
    .unwrap();
    let run = |out: &str, jobs: &str| {
        let o = Command::new(env!("CARGO_BIN_EXE_semdiff"))
            .args(["run", "--exec", "--exec-jobs", jobs, "--manifest"])
            .arg(&mp)
            .arg("--out")
            .arg(dir.join(out))
            .arg("--registry")
            .arg(&reg)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        fs::read(dir.join(out)).unwrap()
    };
    let (a, b) = (run("a.json", "1"), run("b.json", "2"));
    assert_eq!(a, b, "summaries differ across runs / job counts");
    let s: Value = serde_json::from_slice(&a).unwrap();
    assert_eq!(s["schema"], "semdiff-summary/v2");
    assert_eq!(s["exec"]["oracle_commit"], exec::ORACLE_COMMIT);
    assert_eq!(s["exec"]["seed"], 1);
    assert_eq!(s["exec"]["cases_run"], 2);
    assert_eq!(
        s["by_reason"],
        json!({"exec-differs": 1, "static-observed-differs": 1})
    );
    let v = &s["cases"][0]["verdict"];
    assert_eq!(v["schema"], "semdiff-verdict/v1");
    assert_eq!(
        v["exec"]["input_set"]["seed"], 1,
        "the input set and seed are in the verdict"
    );
    let eq = s["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["verdict"])
        .find(|v| v["reason"] == "static-observed-differs")
        .unwrap();
    assert_eq!(eq["exec"]["detail"]["rewrite_reason"], "printed-differs");
    assert_eq!(eq["exec"]["detail"]["trace"], "equal");
    assert_eq!(eq["class"], "DIFFERENT");
    assert!(v["input_set_sha256"].is_string());
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------------------------------
// AB-L0051: a trace is parsed into exec::Trace against the input script at one boundary; anything else is
// CRASH exec-harness-failure. Each case below feeds the SAME malformed trace as all four runs (both sides, both runs),
// the shape that used to compare equal through the downstream fallbacks.

/// The positive pair's input set and its real oracle trace text (run once, by the real runner).
fn base() -> &'static (Value, String) {
    static B: OnceLock<(Value, String)> = OnceLock::new();
    B.get_or_init(|| {
        let work = scratch("base");
        exec::preflight(&cfg(&work))
            .expect("preflight: oracle clean at ORACLE_COMMIT and node present");
        let input = exec::generate(1, "test/base", &rust(), &rust());
        let dir = work.join("c");
        let out = exec::run_case(&dir, &input.json, &rust(), &rust());
        assert!(
            matches!(out, exec::Outcome::Traces { .. }),
            "base pair must run cleanly"
        );
        let t = fs::read_to_string(dir.join("oracle.0.json")).unwrap();
        let _ = fs::remove_dir_all(&work);
        (input.json, t)
    })
}

/// Decide the base pair with `trace` as every run's stdout.
fn decide_with(trace: &str) -> exec::ExecDecision {
    let input = exec::generate(1, "test/base", &rust(), &rust());
    assert_eq!(input.json, base().0, "generate is pure");
    let out = exec::collect(&input.json, |_, _| Ok(trace.to_string()));
    exec::decide(&input, out)
}

fn base_trace() -> Value {
    serde_json::from_str(&base().1).unwrap()
}

#[track_caller]
fn assert_harness_crash(trace: &str, why: &str) {
    let d = decide_with(trace);
    assert_eq!(
        (d.class, d.reason),
        (Class::Crash, Reason::ExecHarnessFailure),
        "{why}: {}",
        d.detail
    );
}

#[track_caller]
fn assert_mutation_crashes(why: &str, f: impl FnOnce(&mut Value)) {
    let mut t = base_trace();
    f(&mut t);
    assert_harness_crash(&serde_json::to_string(&t).unwrap(), why);
}

#[test]
fn unmodified_trace_is_byte_equal() {
    // AB-L0051 rust-vs-rust control: assert trace equality directly, not an EXEC_EQUAL verdict.
    let input = exec::generate(1, "test/base", &rust(), &rust());
    assert_eq!(input.json, base().0, "generate is pure");
    let out = exec::collect(&input.json, |_, _| Ok(base().1.clone()));
    let exec::Outcome::Traces { o, r } = out else {
        panic!("control must produce traces")
    };
    assert!(
        exec::first_divergence(&o, &r).is_none(),
        "rust-vs-rust traces must agree: {}",
        exec::first_divergence(&o, &r).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&o).unwrap(),
        serde_json::to_string(&r).unwrap()
    );
    // The typed parse is lossless: it re-serializes to the bytes of the sorted JSON (what trace_sha256 hashes).
    let t = exec::parse_trace(&base().1, &base().0).unwrap();
    assert_eq!(
        serde_json::to_string(&t).unwrap(),
        serde_json::to_string(&base_trace()).unwrap()
    );
}

#[test]
fn stepless_trace_is_a_crash_never_equivalent() {
    assert_mutation_crashes("steps: []", |t| t["steps"] = json!([]));
    assert_mutation_crashes("no steps", |t| {
        t.as_object_mut().unwrap().remove("steps");
    });
    assert_harness_crash(
        r#"{"schema":"semdiff-exec-trace/v0","steps":[],"helpers":{},"reads":[],"handled":[]}"#,
        "empty",
    );
    assert_harness_crash("{}", "{}");
    assert_harness_crash("", "no output");
}

#[test]
fn trace_missing_events_is_a_crash() {
    assert_mutation_crashes("mount(A) without events", |t| {
        t["steps"][1].as_object_mut().unwrap().remove("events");
    });
    assert_mutation_crashes("no step has events", |t| {
        for s in t["steps"].as_array_mut().unwrap() {
            s.as_object_mut().unwrap().remove("events");
        }
    });
    assert_mutation_crashes("events not a list", |t| t["steps"][1]["events"] = json!({}));
    assert_mutation_crashes("event without e", |t| {
        t["steps"][1]["events"][0] = json!({"path": "title"})
    });
}

#[test]
fn trace_missing_helpers_reads_or_handled_is_a_crash() {
    for k in ["helpers", "reads", "handled", "schema"] {
        assert_mutation_crashes(k, |t| {
            t.as_object_mut().unwrap().remove(k);
        });
        assert_mutation_crashes(&format!("{k}: null"), |t| t[k] = Value::Null);
    }
}

#[test]
fn trace_not_starting_load_mount_a_is_a_crash() {
    assert_mutation_crashes("step 0 dropped (mount(A) first)", |t| {
        t["steps"].as_array_mut().unwrap().remove(0);
    });
    assert_mutation_crashes("mount(A) dropped", |t| {
        t["steps"].as_array_mut().unwrap().remove(1);
    });
    assert_mutation_crashes("step 1 relabelled", |t| {
        t["steps"][1]["label"] = json!("set:flag=B")
    });
    assert_mutation_crashes("step 1 kind", |t| t["steps"][1]["kind"] = json!("set"));
    assert_mutation_crashes("steps 0 and 1 swapped", |t| {
        t["steps"].as_array_mut().unwrap().swap(0, 1)
    });
}

#[test]
fn trace_off_script_or_with_unknown_fields_is_a_crash() {
    assert_mutation_crashes("last step dropped", |t| {
        t["steps"].as_array_mut().unwrap().pop();
    });
    assert_mutation_crashes("extra step", |t| {
        let last = t["steps"].as_array().unwrap().last().unwrap().clone();
        t["steps"].as_array_mut().unwrap().push(last);
    });
    assert_mutation_crashes("unknown top-level field", |t| t["extra"] = json!(1));
    assert_mutation_crashes("unknown step field", |t| t["steps"][1]["extra"] = json!(1));
    assert_mutation_crashes("ran step without dom", |t| {
        t["steps"][1].as_object_mut().unwrap().remove("dom");
    });
    assert_mutation_crashes("skipped load", |t| {
        t["steps"][0] = json!({"kind": "load", "label": "load", "skipped": "x"})
    });
    assert_mutation_crashes("wrong schema", |t| {
        t["schema"] = json!("semdiff-exec-trace/v1")
    });
}

#[test]
fn oracle_dir_is_the_harness_lwc_root() {
    // AB-L0051 (2): build.rs derives ORACLE_DIR from harness/src/ssrRender.ts `LWC`; this is the end-to-end check
    // that the derived value is what the runner imports through, and that it is the checkout preflight pins.
    let ssr = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join("../../harness/src/ssrRender.ts");
    let text = fs::read_to_string(&ssr).unwrap();
    let literal = format!("export const LWC = '{}/packages/@lwc';", exec::ORACLE_DIR);
    let portable = "export const LWC = `${oracleRoot()}/packages/@lwc`;";
    assert!(
        text.contains(&literal) || text.contains(portable),
        "{}",
        exec::ORACLE_DIR
    );
    let head = Command::new("git")
        .args(["-C", exec::ORACLE_DIR, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&head.stdout).trim(),
        exec::ORACLE_COMMIT
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// AB-L0064 rework 2 / D-0258: Function#toString observes any textual difference the rewrite leg did not already
// treat as unobservable. Equal traces of a printed-differs / token-spelling-differs / printer-equal-ast-differs pair
// are DIFFERENT static-observed-differs, never EXEC_EQUAL and never UNCLASSIFIED. A hoist-only pair is printed-differs,
// so it is DIFFERENT. Quote-style-only stays rewrite-equivalent. Trailing commas and ASI-vs-`;`
// are rewrite-equivalent (both rules arch_review = AB-L0064).

fn cli_exec(name: &str, cases: Value) -> Value {
    let dir = scratch(name);
    let m = json!({"schema": "semdiff-manifest/v0", "corpus": "test", "cases": cases});
    let mp = dir.join("manifest.json");
    fs::write(&mp, serde_json::to_string(&m).unwrap()).unwrap();
    let reg = dir.join("decisions.tsv");
    fs::write(
        &reg,
        "id\tts\tphase\tdecision\twhy\tevidence\tresult\nD-0196\tt\tp\td\tw\te\tlocked\n",
    )
    .unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_semdiff"))
        .args([
            "run",
            "--exec",
            "--exec-jobs",
            "2",
            "--gate",
            "none",
            "--manifest",
        ])
        .arg(&mp)
        .arg("--out")
        .arg(dir.join("s.json"))
        .arg("--registry")
        .arg(&reg)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let s: Value = serde_json::from_slice(&fs::read(dir.join("s.json")).unwrap()).unwrap();
    let _ = fs::remove_dir_all(&dir);
    s
}

/// (name, oracle, rust, expected rewrite reason, expected (class, reason) after exec).
type Probe = (
    &'static str,
    String,
    String,
    &'static str,
    (&'static str, &'static str),
);

fn extra(side: String, decls: &str) -> String {
    let needle = "import {freezeTemplate, registerTemplate} from \"lwc\";\n";
    assert!(side.contains(needle), "HEAD missing");
    side.replacen(needle, &format!("{needle}{decls}\n"), 1)
}

/// Route the iterator through an arrow whose parameter spelling is under test (same semantics both sides).
fn foo(side: String, arrow: &str) -> String {
    let from = "api_iterator($cmp.items,";
    assert!(side.contains(from));
    side.replacen(
        from,
        &format!("api_iterator(($cmp.items || []).map({arrow}),"),
        1,
    )
}

fn arch_probes() -> Vec<Probe> {
    let arrow = |s: String| s.replacen("function (item) {\n    return", "item => {\n    return", 1);
    let cap = ("DIFFERENT", "static-observed-differs");
    let rewrite = ("EQUIVALENT-BY-REWRITE", "printer-equal");
    vec![
        (
            "hoist-paren-oracle",
            oracle().replacen(
                "api_element(\"div\", stc0,",
                "api_element(\"div\", (stc0),",
                1,
            ),
            rust(),
            "printed-differs",
            cap,
        ),
        (
            "hoist-paren-rust",
            oracle(),
            format!(
                "{HEAD}{}",
                BODY.replace("STC0", &format!("({INLINE_STC0})"))
            ),
            "printed-differs",
            cap,
        ),
        (
            "hoist-num-0x0",
            oracle().replacen("  key: 0\n};", "  key: 0x0\n};", 1),
            rust(),
            "printed-differs",
            cap,
        ),
        (
            "key-quoting",
            rust(),
            rust_with(r#""title": $cmp.title"#, "title: $cmp.title"),
            "printed-differs",
            cap,
        ),
        (
            "masked-key-quoting",
            oracle(),
            rust_with(r#""title": $cmp.title"#, "title: $cmp.title"),
            "printed-differs",
            cap,
        ),
        (
            "shorthand",
            rust(),
            rust_with("const {_m0} = $ctx;", "const {_m0: _m0} = $ctx;"),
            "printer-equal-ast-differs",
            cap,
        ),
        (
            "masked-shorthand",
            oracle(),
            rust_with("const {_m0} = $ctx;", "const {_m0: _m0} = $ctx;"),
            "printed-differs",
            cap,
        ),
        (
            "backtick",
            rust(),
            rust_with(r#"api_text("hi")"#, "api_text(`hi`)"),
            "printed-differs",
            cap,
        ),
        (
            "masked-backtick",
            oracle(),
            rust_with(r#"api_text("hi")"#, "api_text(`hi`)"),
            "printed-differs",
            cap,
        ),
        (
            "regex-src",
            extra(rust(), "const re = /a/;"),
            extra(rust(), "const re = /[a]/;"),
            "printed-differs",
            cap,
        ),
        (
            "regex-flags",
            extra(rust(), "const re = /a/g;"),
            extra(rust(), "const re = /a/gi;"),
            "printed-differs",
            cap,
        ),
        (
            "masked-regex-src",
            extra(oracle(), "const re = /a/;"),
            extra(rust(), "const re = /[a]/;"),
            "printed-differs",
            cap,
        ),
        (
            "masked-regex-flags",
            extra(oracle(), "const re = /a/g;"),
            extra(rust(), "const re = /a/gi;"),
            "printed-differs",
            cap,
        ),
        (
            "cross-classmap",
            extra(
                rust(),
                "const a = {\"box\": true}; const b = {box: true}; void a; void b;",
            ),
            extra(rust(), "void {box: true}; void {\"box\": true};"),
            "printed-differs",
            cap,
        ),
        (
            "cross-template",
            extra(rust(), "const a = `x`; const b = \"x\"; void a; void b;"),
            extra(rust(), "void \"x\"; void `x`;"),
            "printed-differs",
            cap,
        ),
        (
            "cross-parens",
            extra(rust(), "void (0); void 1;"),
            extra(rust(), "void 0; void (1);"),
            "printer-equal-ast-differs",
            cap,
        ),
        (
            "member",
            rust(),
            rust_with("$cmp.label", "$cmp[\"label\"]"),
            "printed-differs",
            cap,
        ),
        (
            "masked-member",
            oracle(),
            rust_with("$cmp.label", "$cmp[\"label\"]"),
            "printed-differs",
            cap,
        ),
        (
            "masked-asi",
            extra(oracle(), "void 0\n"),
            extra(rust(), "void 0;"),
            "printed-differs",
            cap,
        ),
        (
            "num",
            rust(),
            rust_with("key: 1,", "key: 0b1,"),
            "token-spelling-differs",
            cap,
        ),
        (
            "num-75",
            rust_with("key: 1,", "key: 75,"),
            rust_with("key: 1,", "key: 0b1001011,"),
            "token-spelling-differs",
            cap,
        ),
        (
            "masked-num-75",
            oracle().replacen("key: 1,", "key: 75,", 1),
            rust_with("key: 1,", "key: 0b1001011,"),
            "printed-differs",
            cap,
        ),
        (
            "paren-arrow",
            arrow(rust()),
            arrow(rust()).replacen("item => {", "(item) => {", 1),
            "token-spelling-differs",
            cap,
        ),
        (
            "paren-foo",
            foo(rust(), "(foo)=>foo"),
            foo(rust(), "foo=>foo"),
            "token-spelling-differs",
            cap,
        ),
        (
            "paren-expr",
            rust(),
            rust_with("$cmp.flag ? api_text", "($cmp.flag) ? api_text"),
            "printer-equal-ast-differs",
            cap,
        ),
        ("hoist-only", oracle(), rust(), "printed-differs", cap),
        (
            "quotes-only",
            rust(),
            rust_with(r#""lang": "en""#, r#"'lang': 'en'"#),
            "printer-equal",
            rewrite,
        ),
        (
            "masked-quotes",
            oracle(),
            rust_with(r#""lang": "en""#, r#"'lang': '\x65n'"#),
            "printed-differs",
            cap,
        ),
        (
            "trailing-comma",
            rust(),
            rust_with(r#"[api_text("hi")]"#, r#"[api_text("hi"),]"#),
            "printer-equal",
            rewrite,
        ),
        (
            "asi-only",
            rust(),
            rust().replacen("freezeTemplate(tmpl);", "freezeTemplate(tmpl)", 1),
            "printer-equal",
            rewrite,
        ),
    ]
}

#[test]
fn arch_probes_are_never_equivalent_by_execution() {
    let cases = arch_probes();
    let manifest: Vec<Value> = cases
        .iter()
        .map(|(n, o, r, ..)| json!({"id": key(n), "oracle_code": o, "rust_code": r}))
        .collect();
    let s = cli_exec("probes", json!(manifest));
    for (c, (n, _, _, rewrite, (class, reason))) in
        s["cases"].as_array().unwrap().iter().zip(&cases)
    {
        let v = &c["verdict"];
        let rw = if v["exec"].is_null() {
            &v["reason"]
        } else {
            &v["rewrite"]["reason"]
        };
        assert_eq!(
            rw, rewrite,
            "{n}: fixture must hit rewrite reason {rewrite}: {v}"
        );
        assert_eq!(
            (v["class"].as_str().unwrap(), v["reason"].as_str().unwrap()),
            (*class, *reason),
            "{n}: {v}"
        );
        if *reason == "static-observed-differs" {
            assert_eq!(v["exec"]["detail"]["rewrite_reason"], *rewrite, "{n}: {v}");
            assert_eq!(v["exec"]["detail"]["trace"], "equal", "{n}: {v}");
        }
    }
}

#[test]
fn approved_trailing_comma_is_rewrite_equivalent() {
    // R-TRAILING-COMMA-1 applies: a trailing-comma-only pair is EQUIVALENT-BY-REWRITE and never reaches exec.
    let (o, r) = (
        rust(),
        rust_with(r#"[api_text("hi")]"#, r#"[api_text("hi"),]"#),
    );
    let v = semdiff::compare(&o, &r);
    assert_eq!(
        (v.class, v.reason.as_str()),
        (Class::EquivalentByRewrite, "printer-equal"),
        "{}",
        v.json
    );
    assert_eq!(
        v.json["rules_applied"],
        json!([semdiff::rules::table()
            .1
            .iter()
            .find(|x| x.id.starts_with("R-TRAILING-COMMA"))
            .unwrap()
            .tagged()])
    );
    assert_eq!(v.json["rule_table_version"], semdiff::rule_table_version());
    let s = cli_exec(
        "m5",
        json!([{"id": key("m5"), "oracle_code": o, "rust_code": r}]),
    );
    let vv = &s["cases"][0]["verdict"];
    assert_eq!(
        (
            vv["class"].as_str().unwrap(),
            vv["reason"].as_str().unwrap()
        ),
        ("EQUIVALENT-BY-REWRITE", "printer-equal"),
        "{vv}"
    );
    assert!(vv["exec"].is_null(), "{vv}");
}
