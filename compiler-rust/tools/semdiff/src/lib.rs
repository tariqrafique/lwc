//! semdiff SD-0: deterministic semantic-equivalence verdicts for (JS-oracle output, Rust output) module pairs.
//!
//! Rule table file `rt-1` (D-0258): two token-spelling rules. A PENDING rule loads but does not apply (AB-D0003);
//! both R-ASI-SEMI-1 and R-TRAILING-COMMA-1 are enabled (`arch_review = "AB-L0064"`). The applied-set identity
//! names the applied rules (not merely "any rule"), so it differs from the ASI-only `rt-1` used on 681aba0. A pair is EQUIVALENT-BY-REWRITE when:
//!   1. both texts parse as ES modules with oxc and no diagnostics;
//!   2. oxc codegen (comments off) of both ASTs is byte-identical (formatting-only differences vanish);
//!   3. the ASTs are `ContentEq` (span-insensitive structural equality) — guards against printer collisions;
//!   4. the token streams match token by token: same kind and same source spelling, except that string-literal tokens
//!      compare by decoded value (WTF-8, lossless). An approved D-0258 rule may skip one trailing comma or an ASI vs
//!      explicit `;` (only after steps 2–3, so an AST difference never reaches them). ContentEq and the printer both
//!      drop more spelling than D-0196 lists as unobservable (numeric-literal spelling, identifier escapes,
//!      member form); the default remains observed;
//!   5. the comment texts, in source order, are identical (comments are observable via Function#toString).
//!
//! Whitespace (never a token) and string quote/escape spelling stay unobservable. Anything else is UNCLASSIFIED with a
//! reason. The rewrite leg never emits DIFFERENT: DIFFERENT needs the execution leg (SD-2, `exec-differs`) to have
//! failed to prove equivalence, so a rewrite-only mismatch stays UNCLASSIFIED and the decisions map (SD-1) applies to none.
//! Design: notes/arch-board/proposals/semdiff-SD-0.md.

pub mod decisions;
pub mod exec;
pub mod ratchet;
pub mod rules;
mod sha256;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use oxc::allocator::Allocator;
use oxc::ast::ast::{Program, StringLiteral};
use oxc::ast_visit::Visit;
use oxc::codegen::{Codegen, CodegenOptions, CommentOptions};
use oxc::parser::config::TokensParserConfig;
use oxc::parser::{Kind, Parser};
use oxc::span::{ContentEq, SourceType};
use serde_json::{json, Map, Value};

use decisions::{check_case_key, check_corpus_name, DecisionMap};

pub use sha256::sha256_hex;

/// Identity of the *applied* rule set (AB-D0003). PENDING rules do not count; see `rules::applied_table_version`.
pub fn rule_table_version() -> &'static str {
    crate::rules::applied_table_version()
}

/// v1 (SD-2a): verdicts gain `exec` (null unless the execution leg ran; then also `rewrite`, the rewrite leg's reason
/// and first_diff), and `input_set_sha256` / `trace_sha256` are filled by that leg.
pub const VERDICT_SCHEMA: &str = "semdiff-verdict/v1";
/// v1 (SD-1): records gain `bucket` and `decision_id`; the summary gains `decisions` and, under `--gate ratchet`, `ratchet`.
/// v2 (SD-2a): the summary gains `exec` (null, or the execution leg's environment: runner hash, oracle commit, engine,
/// seed, cases run). Totals are unchanged, so a v1 summary is still a valid ratchet baseline (`SUMMARY_SCHEMAS_READ`).
pub const SUMMARY_SCHEMA: &str = "semdiff-summary/v2";
/// Summary schemas `read_totals` (and so the ratchet baseline loader) accepts.
pub const SUMMARY_SCHEMAS_READ: [&str; 2] = ["semdiff-summary/v1", SUMMARY_SCHEMA];
pub const MANIFEST_SCHEMA: &str = "semdiff-manifest/v0";
/// The observation model every verdict is judged under (decisions.tsv D-0196, locked): whitespace and string
/// quote/escape spelling are unobservable even through Function#toString; comments and template raw text are observed;
/// anything not listed is observed.
pub const OBSERVATION_MODEL: &str = "D-0196";
/// The five required totals buckets. A summary missing any of them is invalid (Harness contract).
pub const TOTALS_KEYS: [&str; 5] = [
    "equivalent",
    "different_decided",
    "different_undecided",
    "unclassified",
    "crash",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    EquivalentByRewrite,
    /// Produced only by the execution leg (SD-2a: both sides ran, the traces differ). A rewrite-leg mismatch alone is
    /// UNCLASSIFIED, never DIFFERENT, because only execution can rule out equivalence.
    Different,
    Unclassified,
    Crash,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::EquivalentByRewrite => "EQUIVALENT-BY-REWRITE",
            Class::Different => "DIFFERENT",
            Class::Unclassified => "UNCLASSIFIED",
            Class::Crash => "CRASH",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    PrinterEqual,
    ParseErrorOracle,
    ParseErrorRust,
    PrintedDiffers,
    AstDiffers,
    TokensDiffer,
    CommentsDiffer,
    Panic,
    ExecDiffers,
    ExecThrowOneSide,
    ExecUncovered,
    ExecHarnessFailure,
    ExecNondeterministic,
    ExecStaticCommentsDiffer,
    /// Equal traces, but the rewrite leg proved a textual difference Function#toString observes (D-0196).
    StaticObservedDiffers,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::PrinterEqual => "printer-equal",
            Reason::ParseErrorOracle => "parse-error-oracle",
            Reason::ParseErrorRust => "parse-error-rust",
            Reason::PrintedDiffers => "printed-differs",
            Reason::AstDiffers => "printer-equal-ast-differs",
            Reason::TokensDiffer => "token-spelling-differs",
            Reason::CommentsDiffer => "comments-differ",
            Reason::Panic => "panic",
            Reason::ExecDiffers => "exec-differs",
            Reason::ExecThrowOneSide => "exec-throw-one-side",
            Reason::ExecUncovered => "exec-uncovered",
            Reason::ExecHarnessFailure => "exec-harness-failure",
            Reason::ExecNondeterministic => "exec-nondeterministic",
            Reason::ExecStaticCommentsDiffer => "exec-static-comments-differ",
            Reason::StaticObservedDiffers => "static-observed-differs",
        }
    }
}

#[derive(Debug)]
pub struct Verdict {
    pub class: Class,
    pub reason: Reason,
    pub json: Value,
}

struct Side {
    sha256: String,
    parse_errors: usize,
    printed: Option<String>,
    comments: Vec<String>,
    tokens: Vec<Tok>,
}

/// One lexed token: kind, exact source spelling, and (string literals only) the decoded value.
pub(crate) struct Tok {
    pub(crate) kind: Kind,
    pub(crate) text: String,
    pub(crate) start: u32,
    str_value: Option<Vec<u8>>,
}

impl Tok {
    /// D-0196: only string quote/escape spelling is unobservable, so only `Str` tokens compare by value. A `Str` token
    /// without a matching StringLiteral node (should not happen) falls back to exact spelling: fail closed.
    pub(crate) fn same(&self, other: &Tok) -> bool {
        self.kind == other.kind
            && match (&self.str_value, &other.str_value) {
                (Some(a), Some(b)) if self.kind == Kind::Str => a == b,
                _ => self.text == other.text,
            }
    }
}

/// Decoded value (canonical WTF-8, so lone surrogates are kept) of every StringLiteral, by span.
#[derive(Default)]
struct StrValues(BTreeMap<(u32, u32), Vec<u8>>);

impl<'a> Visit<'a> for StrValues {
    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        self.0
            .insert((it.span.start, it.span.end), it.value.as_bytes().to_vec());
    }
}

fn side_json(s: &Side) -> Value {
    json!({
        "sha256": s.sha256,
        "parse_errors": s.parse_errors,
        "printed_sha256": s.printed.as_deref().map(|p| sha256_hex(p.as_bytes())),
    })
}

fn parse<'a>(alloc: &'a Allocator, src: &'a str) -> (Program<'a>, Side) {
    let ret = Parser::new(alloc, src, SourceType::mjs())
        .with_config(TokensParserConfig)
        .parse();
    let parse_errors = ret.diagnostics.errors().count();
    let program = ret.program;
    let mut values = StrValues::default();
    values.visit_program(&program);
    let tokens = ret
        .tokens
        .iter()
        .filter(|t| t.kind() != Kind::Eof)
        .map(|t| Tok {
            kind: t.kind(),
            text: t.span().source_text(src).to_string(),
            start: t.start(),
            str_value: (t.kind() == Kind::Str)
                .then(|| values.0.get(&(t.start(), t.end())).cloned())
                .flatten(),
        })
        .collect();
    let comments = program
        .comments
        .iter()
        .map(|c| c.span.source_text(src).to_string())
        .collect();
    let printed = (parse_errors == 0).then(|| {
        let opts = CodegenOptions {
            comments: CommentOptions::disabled(),
            ..CodegenOptions::default()
        };
        Codegen::new().with_options(opts).build(&program).code
    });
    (
        program,
        Side {
            sha256: sha256_hex(src.as_bytes()),
            parse_errors,
            printed,
            comments,
            tokens,
        },
    )
}

/// First differing line of the two oxc-printed texts (1-based line in PRINTED coordinates; ≤200 chars per side).
fn first_line_diff(a: &str, b: &str) -> Value {
    let clip = |s: &str| s.chars().take(200).collect::<String>();
    let (mut la, mut lb) = (a.lines(), b.lines());
    let mut n = 0usize;
    loop {
        n += 1;
        match (la.next(), lb.next()) {
            (Some(x), Some(y)) if x == y => continue,
            (None, None) => return Value::Null,
            (x, y) => {
                return json!({"coords": "printed", "line": n, "oracle": x.map(clip), "rust": y.map(clip)});
            }
        }
    }
}

/// Classify one pair. Pure and deterministic: same inputs → byte-identical `json`.
pub fn compare(oracle: &str, rust: &str) -> Verdict {
    compare_with(oracle, rust, None)
}

/// Token-spelling check with an explicit enabled-rule set, ignoring PENDING/APPROVE. For a rule's own pos/neg tests.
pub fn compare_forcing_rules(oracle: &str, rust: &str, force: &[&str]) -> Verdict {
    compare_with(oracle, rust, Some(force))
}

fn compare_with(oracle: &str, rust: &str, force: Option<&[&str]>) -> Verdict {
    let (alloc_o, alloc_r) = (Allocator::default(), Allocator::default());
    let (prog_o, o) = parse(&alloc_o, oracle);
    let (prog_r, r) = parse(&alloc_r, rust);
    let mut first_diff = Value::Null;
    let mut rules_applied: Vec<String> = Vec::new();
    let (class, reason) = if o.parse_errors > 0 {
        (Class::Unclassified, Reason::ParseErrorOracle)
    } else if r.parse_errors > 0 {
        (Class::Unclassified, Reason::ParseErrorRust)
    } else if o.printed != r.printed {
        first_diff = first_line_diff(
            o.printed.as_deref().unwrap_or(""),
            r.printed.as_deref().unwrap_or(""),
        );
        (Class::Unclassified, Reason::PrintedDiffers)
    } else if !prog_o.content_eq(&prog_r) {
        (Class::Unclassified, Reason::AstDiffers)
    } else if let Some(d) = match force {
        Some(f) => crate::rules::first_token_diff_forcing(
            &o.tokens,
            &r.tokens,
            &mut rules_applied,
            f,
            &prog_o,
            &prog_r,
        ),
        None => crate::rules::first_token_diff(
            &o.tokens,
            &r.tokens,
            &mut rules_applied,
            &prog_o,
            &prog_r,
        ),
    } {
        first_diff = d;
        (Class::Unclassified, Reason::TokensDiffer)
    } else if o.comments != r.comments {
        (Class::Unclassified, Reason::CommentsDiffer)
    } else {
        (Class::EquivalentByRewrite, Reason::PrinterEqual)
    };
    verdict(class, reason, Some((&o, &r)), first_diff, rules_applied)
}

fn verdict(
    class: Class,
    reason: Reason,
    sides: Option<(&Side, &Side)>,
    first_diff: Value,
    rules_applied: Vec<String>,
) -> Verdict {
    let mut m = Map::new();
    m.insert("schema".into(), json!(VERDICT_SCHEMA));
    m.insert("class".into(), json!(class.as_str()));
    m.insert("reason".into(), json!(reason.as_str()));
    m.insert("observation_model".into(), json!(OBSERVATION_MODEL));
    m.insert("rule_table_version".into(), json!(rule_table_version()));
    m.insert("rules_applied".into(), json!(rules_applied));
    // Execution-leg slots (SD-2a): null for rewrite verdicts.
    m.insert("input_set_sha256".into(), Value::Null);
    m.insert("trace_sha256".into(), Value::Null);
    m.insert(
        "oracle".into(),
        sides.map_or(Value::Null, |(o, _)| side_json(o)),
    );
    m.insert(
        "rust".into(),
        sides.map_or(Value::Null, |(_, r)| side_json(r)),
    );
    m.insert("first_diff".into(), first_diff);
    m.insert("exec".into(), Value::Null);
    Verdict {
        class,
        reason,
        json: Value::Object(m),
    }
}

/// `compare` with a panic in oxc (or here) contained as a per-case CRASH verdict instead of aborting the run.
pub fn compare_guarded(oracle: &str, rust: &str) -> Verdict {
    match std::panic::catch_unwind(|| compare(oracle, rust)) {
        Ok(v) => v,
        Err(_) => verdict(Class::Crash, Reason::Panic, None, Value::Null, vec![]),
    }
}

/// Deterministic serialization used for every artifact (verdicts, summaries): pretty JSON + trailing newline.
pub fn to_stable_json(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).expect("serde_json::Value always serializes");
    s.push('\n');
    s
}

// ---------------------------------------------------------------------------------------------------------------------
// Run mode: manifest → one summary JSON (Harness CLI contract, design note §"Harness CLI contract").

/// An internal error: the run is invalid; the CLI exits 2 and writes no summary.
#[derive(Debug)]
pub struct InternalError(pub String);

impl std::fmt::Display for InternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn ie<T>(msg: impl Into<String>) -> Result<T, InternalError> {
    Err(InternalError(msg.into()))
}

pub struct Case {
    pub id: String,
    pub oracle: String,
    pub rust: String,
}

pub struct Manifest {
    pub corpus: String,
    pub sha256: String,
    pub cases: Vec<Case>,
}

fn read_text(p: &Path) -> Result<String, InternalError> {
    fs::read_to_string(p).map_err(|e| InternalError(format!("read {}: {e}", p.display())))
}

fn side_text(
    c: &Map<String, Value>,
    side: &str,
    base: &Path,
    id: &str,
) -> Result<String, InternalError> {
    let code_key = format!("{side}_code");
    match (c.get(side), c.get(&code_key)) {
        (Some(Value::String(p)), None) => read_text(&base.join(p)),
        (None, Some(Value::String(code))) => Ok(code.clone()),
        _ => ie(format!(
            "case {id:?}: need exactly one of string `{side}` (path) or `{code_key}` (inline)"
        )),
    }
}

/// The only keys a manifest case may carry.
pub const CASE_KEYS: [&str; 5] = ["id", "oracle", "oracle_code", "rust", "rust_code"];
/// The only top-level manifest keys.
pub const MANIFEST_KEYS: [&str; 3] = ["schema", "corpus", "cases"];

pub fn load_manifest(path: &Path) -> Result<Manifest, InternalError> {
    let bytes = fs::read(path)
        .map_err(|e| InternalError(format!("read manifest {}: {e}", path.display())))?;
    let v: Value =
        serde_json::from_slice(&bytes).map_err(|e| InternalError(format!("manifest JSON: {e}")))?;
    if v.get("schema").and_then(Value::as_str) != Some(MANIFEST_SCHEMA) {
        return ie(format!("manifest schema must be {MANIFEST_SCHEMA:?}"));
    }
    if let Some(k) = v.as_object().and_then(|o| {
        o.keys()
            .find(|k| !MANIFEST_KEYS.contains(&k.as_str()))
            .cloned()
    }) {
        return ie(format!(
            "manifest: unknown top-level field {k:?} (allowed: {MANIFEST_KEYS:?})"
        ));
    }
    let Some(corpus) = v
        .get("corpus")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return ie("manifest needs a non-empty string `corpus`");
    };
    check_corpus_name(corpus).map_err(|e| InternalError(format!("manifest: {e}")))?;
    let Some(raw) = v.get("cases").and_then(Value::as_array) else {
        return ie("manifest needs array `cases`");
    };
    let base = path.parent().unwrap_or(Path::new("."));
    let mut seen = BTreeSet::new();
    let mut cases = Vec::with_capacity(raw.len());
    for (i, c) in raw.iter().enumerate() {
        let Some(c) = c.as_object() else {
            return ie(format!("cases[{i}] is not an object"));
        };
        let Some(id) = c
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            return ie(format!("cases[{i}] needs a non-empty string `id`"));
        };
        // `id` IS the case_key (design §2/§7): the ratchet and the decisions map key on it, so its shape is checked,
        // and its corpus segment must be this manifest's corpus (keys from different corpora never collide).
        match check_case_key(id) {
            Err(e) => return ie(format!("cases[{i}]: {e}")),
            Ok(c) if c != corpus => {
                return ie(format!(
                    "cases[{i}]: case_key {id:?} has corpus {c:?}, manifest corpus is {corpus:?}"
                ))
            }
            Ok(_) => {}
        }
        if !seen.insert(id.to_string()) {
            return ie(format!("duplicate case_key {id:?}"));
        }
        // Closed key set: adapters must not author decisions (AB-L0023), so `decision_id` (or any other unknown key)
        // makes the run invalid instead of being silently ignored. Decisions come only from the case_key map (SD-1).
        if let Some(k) = c.keys().find(|k| !CASE_KEYS.contains(&k.as_str())) {
            return ie(format!(
                "case {id:?}: unknown field {k:?} (allowed: {CASE_KEYS:?})"
            ));
        }
        cases.push(Case {
            id: id.to_string(),
            oracle: side_text(c, "oracle", base, id)?,
            rust: side_text(c, "rust", base, id)?,
        });
    }
    Ok(Manifest {
        corpus: corpus.to_string(),
        sha256: sha256_hex(&bytes),
        cases,
    })
}

/// Totals bucket for a verdict class, and the decision id that bucket carries. The map is consulted ONLY for DIFFERENT
/// (AB-D0003 (3)): a mapped key whose case is equivalent, unclassified or crashed keeps its own bucket and no id.
pub fn bucket(class: Class, key: &str, map: &DecisionMap) -> (&'static str, Option<String>) {
    match class {
        Class::EquivalentByRewrite => ("equivalent", None),
        Class::Different => match map.get(key) {
            Some(d) => ("different_decided", Some(d.to_string())),
            None => ("different_undecided", None),
        },
        Class::Unclassified => ("unclassified", None),
        Class::Crash => ("crash", None),
    }
}

/// Which rewrite verdicts the execution leg runs on: UNCLASSIFIED except parse errors (an unparsable side is not a
/// module the engine could load the same way; it stays the rewrite leg's finding).
pub fn exec_eligible(v: &Verdict) -> bool {
    v.class == Class::Unclassified
        && !matches!(v.reason, Reason::ParseErrorOracle | Reason::ParseErrorRust)
}

/// Rewrite leg for every case, then (with `exec`) the execution leg for the eligible ones, in parallel.
fn classify_all(m: &Manifest, exec: Option<&exec::ExecCfg>) -> (Vec<Verdict>, Value) {
    let mut verdicts: Vec<Verdict> = m
        .cases
        .iter()
        .map(|c| compare_guarded(&c.oracle, &c.rust))
        .collect();
    let Some(cfg) = exec else {
        return (verdicts, Value::Null);
    };
    let todo: Vec<usize> = (0..verdicts.len())
        .filter(|i| exec_eligible(&verdicts[*i]))
        .collect();
    let results = exec::par_map(&todo, cfg.jobs, |_, &i| {
        let c = &m.cases[i];
        if cfg.engine == exec::ExecEngine::Ssr {
            let meta = cfg
                .ssr
                .get(&c.id)
                .expect("ssr meta checked before classify");
            match exec::generate_ssr(cfg.seed, &c.id, &c.oracle, &c.rust, meta) {
                Ok(mut input) => {
                    input.observe_rewrite(verdicts[i].reason, &verdicts[i].json["first_diff"]);
                    let runtime = exec::ssr_runtime_path(&cfg.oracle_dir, &meta.runtime_kind);
                    let out = exec::run_case_ssr(
                        &exec::case_dir(&cfg.work, i),
                        &input.json,
                        &c.oracle,
                        &c.rust,
                        &runtime,
                    );
                    let d = exec::decide(&input, out);
                    (input, d)
                }
                Err(e) => {
                    let input = exec::InputSet {
                        json: json!({"schema": exec::INPUT_SCHEMA, "engine": "ssr-runtime", "seed": cfg.seed, "steps": []}),
                        o: exec::analyze(&c.oracle),
                        r: exec::analyze(&c.rust),
                        slots: BTreeSet::new(),
                        events: BTreeSet::new(),
                        gaps: BTreeSet::new(),
                        rewrite_reason: verdicts[i].reason,
                        rewrite_first_diff: verdicts[i].json["first_diff"].clone(),
                        engine: exec::ExecEngine::Ssr,
                    };
                    let d = exec::decide(
                        &input,
                        exec::Outcome::HarnessFailure(format!("ssr input: {e}")),
                    );
                    (input, d)
                }
            }
        } else {
            let mut input = exec::generate(cfg.seed, &c.id, &c.oracle, &c.rust);
            input.observe_rewrite(verdicts[i].reason, &verdicts[i].json["first_diff"]);
            let out = exec::run_case(
                &exec::case_dir(&cfg.work, i),
                &input.json,
                &c.oracle,
                &c.rust,
            );
            let d = exec::decide(&input, out);
            (input, d)
        }
    });
    for (&i, (input, d)) in todo.iter().zip(&results) {
        let v = &mut verdicts[i];
        let Value::Object(obj) = &mut v.json else {
            unreachable!("verdict json is an object")
        };
        exec::apply(obj, input, d);
        v.class = d.class;
        v.reason = d.reason;
    }
    (verdicts, json!({"cases_run": todo.len()}))
}

/// Classify every case and build the summary. Cases are emitted in manifest order (stable, caller-defined).
/// `exec`: run the execution leg (`--exec`), with its preflight facts (`exec::preflight`) merged into `summary.exec`.
pub fn summarize(
    m: &Manifest,
    map: &DecisionMap,
    registry_sha256: &str,
) -> Result<Value, InternalError> {
    summarize_with(m, map, registry_sha256, None)
}

pub fn summarize_with(
    m: &Manifest,
    map: &DecisionMap,
    registry_sha256: &str,
    exec: Option<(&exec::ExecCfg, Value)>,
) -> Result<Value, InternalError> {
    let mut totals: BTreeMap<&str, u64> = TOTALS_KEYS.iter().map(|k| (*k, 0)).collect();
    let mut by_reason: BTreeMap<&str, u64> = BTreeMap::new();
    let mut records = Vec::with_capacity(m.cases.len());
    let (mut applied, mut mapped_unused) = (0u64, Vec::new());
    let (verdicts, ran) = {
        if let Some(cfg) = exec.as_ref().map(|(c, _)| *c) {
            if cfg.engine == exec::ExecEngine::Ssr {
                if cfg.ssr.len() != m.cases.len() {
                    return ie(format!(
                        "ssr meta has {} cases, manifest has {}",
                        cfg.ssr.len(),
                        m.cases.len()
                    ));
                }
                for c in &m.cases {
                    if !cfg.ssr.contains_key(&c.id) {
                        return ie(format!("ssr meta missing for {}", c.id));
                    }
                }
            } else if !cfg.ssr.is_empty() {
                return ie("ssr meta was supplied for a non-ssr corpus");
            }
        }
        classify_all(m, exec.as_ref().map(|(c, _)| *c))
    };
    let exec_json = match (exec, ran) {
        (Some((_, Value::Object(mut env))), Value::Object(r)) => {
            env.extend(r);
            Value::Object(env)
        }
        _ => Value::Null,
    };
    for (c, v) in m.cases.iter().zip(verdicts) {
        let (b, decision_id) = bucket(v.class, &c.id, map);
        *totals.get_mut(b).expect("bucket is a TOTALS_KEYS member") += 1;
        *by_reason.entry(v.reason.as_str()).or_default() += 1;
        if decision_id.is_some() {
            applied += 1;
        } else if map.get(&c.id).is_some() {
            mapped_unused.push(c.id.clone());
        }
        records
            .push(json!({"id": c.id, "bucket": b, "decision_id": decision_id, "verdict": v.json}));
    }
    let sum: u64 = totals.values().sum();
    if sum != m.cases.len() as u64 {
        return ie(format!(
            "internal: totals sum {sum} != cases {}",
            m.cases.len()
        ));
    }
    let mut t = Map::new();
    t.insert("cases".into(), json!(m.cases.len()));
    for k in TOTALS_KEYS {
        t.insert(k.into(), json!(totals[k]));
    }
    Ok(json!({
        "schema": SUMMARY_SCHEMA,
        "tool": {"name": "semdiff", "version": env!("CARGO_PKG_VERSION")},
        "observation_model": OBSERVATION_MODEL,
        "rule_table_version": rule_table_version(),
        "corpus": m.corpus,
        "manifest_sha256": m.sha256,
        "totals": Value::Object(t),
        "by_reason": by_reason,
        "exec": exec_json,
        // The ONE decisions source [AB-D0003 (3)], pinned by hash with the registry it was validated against.
        // `mapped_not_different`: keys in this manifest that the map names but whose case is not DIFFERENT (a stale
        // or premature decision); reported, not a failure.
        "decisions": {
            "map": decisions::DECISIONS_MAP_PATH,
            "map_sha256": map.sha256,
            "registry_sha256": registry_sha256,
            "entries": map.len(),
            "applied": applied,
            "mapped_not_different": mapped_unused,
        },
        "cases": records,
    }))
}

/// Required-totals validation shared by the gate (baseline) and by Harness consumers.
///
/// Historical v1/v2 summaries are keyed by the five totals buckets (`equivalent`, `different_decided`,
/// `different_undecided`, `unclassified`, `crash`) and each case's `bucket` string. Neither `read_totals` nor the
/// ratchet parses a verdict `class` such as `EQUIVALENT-BY-EXECUTION`. Removing that Class variant therefore does not
/// break reading a historical baseline. A baseline whose `bucket` is not in `TOTALS_KEYS` still fails closed
/// (`unknown bucket`).
pub fn read_totals(summary: &Value) -> Result<BTreeMap<String, u64>, InternalError> {
    if !summary
        .get("schema")
        .and_then(Value::as_str)
        .is_some_and(|s| SUMMARY_SCHEMAS_READ.contains(&s))
    {
        return ie(format!(
            "summary schema must be one of {SUMMARY_SCHEMAS_READ:?}"
        ));
    }
    let Some(t) = summary.get("totals").and_then(Value::as_object) else {
        return ie("summary has no `totals`");
    };
    let mut out = BTreeMap::new();
    for k in TOTALS_KEYS {
        let Some(n) = t.get(k).and_then(Value::as_u64) else {
            return ie(format!("summary totals.{k} missing"));
        };
        out.insert(k.to_string(), n);
    }
    let sum: u64 = out.values().sum();
    if t.get("cases").and_then(Value::as_u64) != Some(sum) {
        return ie(format!(
            "summary totals.cases must equal the bucket sum {sum}"
        ));
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    None,
    Ratchet,
    Strict,
}

/// The buckets `strict` requires to be 0 (and that a NEW key may not land in under `ratchet`).
pub const STRICT_BAD: [&str; 3] = ["different_undecided", "unclassified", "crash"];

/// Strict-gate failures over totals (empty = pass): different_undecided = unclassified = crash = 0.
/// The per-case_key ratchet lives in `ratchet`.
pub fn strict_failures(cur: &BTreeMap<String, u64>) -> Vec<String> {
    STRICT_BAD
        .iter()
        .filter(|k| cur[**k] != 0)
        .map(|k| format!("{k}={} (strict requires 0)", cur[*k]))
        .collect()
}

/// Write `text` to `out` atomically (tmp in the same dir, then rename), so a reader never sees a partial summary.
pub fn write_atomic(out: &Path, text: &str) -> Result<(), InternalError> {
    let dir = out
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = out
        .file_name()
        .ok_or_else(|| InternalError(format!("bad --out {}", out.display())))?;
    let mut tmp = PathBuf::from(dir);
    tmp.push(format!(
        ".{}.tmp.{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    fs::write(&tmp, text).map_err(|e| InternalError(format!("write {}: {e}", tmp.display())))?;
    fs::rename(&tmp, out).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        InternalError(format!(
            "rename {} -> {}: {e}",
            tmp.display(),
            out.display()
        ))
    })
}
