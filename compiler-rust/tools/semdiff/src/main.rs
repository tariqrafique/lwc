//! semdiff CLI. Contract: notes/arch-board/proposals/semdiff-SD-0.md §"Harness CLI contract".
//!   semdiff pair <oracle.js> <rust.js>                 one verdict JSON on stdout
//!   semdiff run --manifest M --out S --registry R [--gate none|ratchet|strict] [--baseline B] [--reset-baseline]
//!               [--exec [--exec-seed N] [--exec-jobs J] [--exec-keep DIR]]
//!     --exec runs the SD-2a execution leg (CSR, oracle engine-dom) on every UNCLASSIFIED non-parse-error case.
//!   semdiff check-registry --registry R [--require <D-id>]...   registry findings JSON on stdout (AB-L0030)
//! R is lwc-rust-migrate/decisions.tsv (the board passes it as of the notes commit it records). The case_key → D-id
//! map is compiled in (tools/semdiff/decisions.tsv), so it has exactly one source.
//! run exit: 0 = ran (and gate passed / none); 1 = ran, summary written, gate failed; 2 = internal/usage error, NO summary.
//! check-registry exit: 0 = valid and every --require is locked; 1 = DEC-UNRESOLVED / DEC-NOT-LOCKED; 2 = DEC-ID-INVALID
//! or MALFORMED (JSON still printed), or a usage error (stderr only, no JSON).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use semdiff::decisions::{
    is_decision_id, DecisionMap, Finding, FindingCode, Registry, DECISIONS_MAP_TSV,
};
use semdiff::{
    compare, exec, load_manifest, ratchet, read_totals, strict_failures, summarize_with,
    to_stable_json, write_atomic, Gate, InternalError, OBSERVATION_MODEL,
};

const USAGE: &str = "usage: semdiff pair <oracle.js> <rust.js>\n       semdiff run --manifest <m.json> --out <summary.json> --registry <decisions.tsv> [--gate none|ratchet|strict] [--baseline <summary.json>] [--reset-baseline] [--exec [--exec-seed <u64>] [--exec-jobs <n>] [--exec-keep <dir>]]\n       semdiff check-registry --registry <decisions.tsv> [--require <D-id>]...";

/// `semdiff check-registry` output schema (design §7).
const REGISTRY_CHECK_SCHEMA: &str = "semdiff-registry-check/v1";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match real_main(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("semdiff: internal error: {e}");
            ExitCode::from(2)
        }
    }
}

fn real_main(args: &[String]) -> Result<ExitCode, InternalError> {
    match args.first().map(String::as_str) {
        Some("pair") if args.len() == 3 => {
            let read = |p: &str| {
                std::fs::read_to_string(p).map_err(|e| InternalError(format!("read {p}: {e}")))
            };
            let v = compare(&read(&args[1])?, &read(&args[2])?);
            print!("{}", to_stable_json(&v.json));
            Ok(ExitCode::SUCCESS)
        }
        Some("run") => run(&args[1..]),
        Some("check-registry") => check_registry(&args[1..]),
        _ => Err(InternalError(USAGE.into())),
    }
}

fn run(args: &[String]) -> Result<ExitCode, InternalError> {
    let (mut manifest, mut out, mut baseline, mut registry, mut gate, mut reset) =
        (None, None, None, None, Gate::None, false);
    let (mut exec_on, mut exec_seed, mut exec_jobs, mut exec_keep) = (false, 1u64, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || {
            it.next()
                .cloned()
                .ok_or_else(|| InternalError(format!("{a} needs a value\n{USAGE}")))
        };
        match a.as_str() {
            "--manifest" => manifest = Some(PathBuf::from(val()?)),
            "--out" => out = Some(PathBuf::from(val()?)),
            "--baseline" => baseline = Some(PathBuf::from(val()?)),
            "--registry" => registry = Some(PathBuf::from(val()?)),
            "--reset-baseline" => reset = true,
            "--exec" => exec_on = true,
            "--exec-seed" => {
                exec_seed = val()?
                    .parse()
                    .map_err(|e| InternalError(format!("--exec-seed: {e}")))?;
            }
            "--exec-jobs" => {
                exec_jobs = Some(
                    val()?
                        .parse::<usize>()
                        .map_err(|e| InternalError(format!("--exec-jobs: {e}")))?
                        .max(1),
                );
            }
            "--exec-keep" => exec_keep = Some(PathBuf::from(val()?)),
            "--gate" => {
                gate = match val()?.as_str() {
                    "none" => Gate::None,
                    "ratchet" => Gate::Ratchet,
                    "strict" => Gate::Strict,
                    g => return Err(InternalError(format!("unknown --gate {g:?}"))),
                }
            }
            _ => return Err(InternalError(format!("unknown argument {a:?}\n{USAGE}"))),
        }
    }
    let (Some(manifest), Some(out)) = (manifest, out) else {
        return Err(InternalError(USAGE.into()));
    };
    // Never leave a stale summary for the board to read: the old file goes before any work starts.
    remove_stale(&out)?;
    let Some(registry) = registry else {
        return Err(InternalError(format!("--registry is required\n{USAGE}")));
    };
    let reg_text = std::fs::read_to_string(&registry)
        .map_err(|e| InternalError(format!("read registry {}: {e}", registry.display())))?;
    let reg = Registry::parse(&reg_text).map_err(InternalError)?;
    // The observation model every verdict cites must itself be a locked decision.
    reg.require_locked(OBSERVATION_MODEL)
        .map_err(|e| InternalError(format!("observation model: {e}")))?;
    let map = DecisionMap::parse(DECISIONS_MAP_TSV, &reg).map_err(InternalError)?;
    let base = match (gate, &baseline) {
        (Gate::Ratchet, None) => {
            return Err(InternalError("--gate ratchet needs --baseline".into()))
        }
        (Gate::Ratchet, Some(b)) => {
            let bytes = std::fs::read(b)
                .map_err(|e| InternalError(format!("read baseline {}: {e}", b.display())))?;
            Some(ratchet::load_baseline(&bytes, reset)?)
        }
        (_, Some(_)) => {
            return Err(InternalError(
                "--baseline is only meaningful with --gate ratchet".into(),
            ))
        }
        (_, None) if reset => {
            return Err(InternalError(
                "--reset-baseline is only meaningful with --gate ratchet".into(),
            ))
        }
        _ => None,
    };
    let m = load_manifest(&manifest)?;
    if let Some(b) = &base {
        if b.corpus != m.corpus {
            return Err(InternalError(format!(
                "baseline-corpus-mismatch: baseline {:?}, manifest {:?}",
                b.corpus, m.corpus
            )));
        }
    }
    let mut summary = if exec_on {
        let (work, keep) = match exec_keep {
            Some(d) => (d, true),
            None => (
                std::env::temp_dir().join(format!("semdiff-exec-{}", std::process::id())),
                false,
            ),
        };
        let jobs = exec_jobs
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()));
        // Corpus `ssr-compiler` selects the SSR observer. Every other corpus, including
        // `template-diag-codegen-only`, stays on the CSR runner.
        let oracle_dir = PathBuf::from(exec::ORACLE_DIR);
        let engine = exec::engine_for_corpus(&m.corpus);
        let ssr = if engine == exec::ExecEngine::Ssr {
            exec::load_ssr_meta(&manifest)?
        } else {
            std::collections::BTreeMap::new()
        };
        let cfg = exec::ExecCfg {
            seed: exec_seed,
            jobs,
            oracle_dir,
            work: work.clone(),
            engine,
            ssr,
        };
        let env = exec::preflight(&cfg)?;
        let s = summarize_with(&m, &map, &reg.sha256, Some((&cfg, env)));
        if !keep {
            let _ = std::fs::remove_dir_all(&work);
        }
        s?
    } else {
        if exec_seed != 1 || exec_jobs.is_some() || exec_keep.is_some() {
            return Err(InternalError(
                "--exec-seed/--exec-jobs/--exec-keep need --exec".into(),
            ));
        }
        summarize_with(&m, &map, &reg.sha256, None)?
    };
    let totals = read_totals(&summary)?; // self-check: the summary we are about to publish satisfies the contract
    let fails = match &base {
        Some(b) => {
            let o = ratchet::compare(&summary, b, reset)?;
            summary["ratchet"] = o.json;
            o.failures
        }
        None if gate == Gate::Strict => strict_failures(&totals),
        None => Vec::new(),
    };
    write_atomic(&out, &to_stable_json(&summary))?;
    let t: Vec<String> = totals.iter().map(|(k, v)| format!("{k}={v}")).collect();
    println!(
        "semdiff {} cases={} {}",
        m.corpus,
        m.cases.len(),
        t.join(" ")
    );
    if let Some(r) = summary.get("ratchet") {
        let n = |k: &str| r[k].as_array().map_or(0, Vec::len);
        println!(
            "semdiff ratchet: baseline {} reset={} regressed={} improved={} waived={} new_keys={} new_failing={} dropped_keys={}",
            r["baseline_rule_table_version"].as_str().unwrap_or("?"),
            r["reset"],
            n("regressed"),
            n("improved"),
            n("waived"),
            n("new_keys"),
            n("new_failing"),
            n("dropped_keys")
        );
    }
    for f in &fails {
        println!("semdiff gate FAIL: {f}");
    }
    // Malformed SSR traces and runner failures are protocol errors: exit 2 after the summary is written.
    let protocol = exec_on && m.corpus == exec::SSR_CORPUS && exec::ssr_protocol_failure(&summary);
    Ok(if protocol {
        ExitCode::from(2)
    } else if fails.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// The registry validator as a command, for coverage-gate.mjs (AB-L0030). Same code as `run`: [`Registry::check`]
/// then [`Registry::resolve`]. Resolution is evaluated only when the registry has no defects (a duplicate id has no
/// single answer).
fn check_registry(args: &[String]) -> Result<ExitCode, InternalError> {
    let (mut registry, mut require) = (None, Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || {
            it.next()
                .cloned()
                .ok_or_else(|| InternalError(format!("{a} needs a value\n{USAGE}")))
        };
        match a.as_str() {
            "--registry" if registry.is_none() => registry = Some(PathBuf::from(val()?)),
            "--require" => {
                let id = val()?;
                if !is_decision_id(&id) {
                    return Err(InternalError(format!(
                        "--require {id:?} does not match ^D-[0-9]{{4}}$"
                    )));
                }
                require.push(id);
            }
            _ => return Err(InternalError(format!("unexpected argument {a:?}\n{USAGE}"))),
        }
    }
    let Some(registry) = registry else {
        return Err(InternalError(format!("--registry is required\n{USAGE}")));
    };
    let malformed = |detail: String| Finding {
        code: FindingCode::Malformed,
        line: None,
        id: None,
        detail,
    };
    let (reg, mut findings) = match std::fs::read(&registry).map(String::from_utf8) {
        Err(e) => (
            None,
            vec![malformed(format!("read {}: {e}", registry.display()))],
        ),
        Ok(Err(e)) => (
            None,
            vec![malformed(format!(
                "{} is not UTF-8: {e}",
                registry.display()
            ))],
        ),
        Ok(Ok(text)) => {
            let (r, f) = Registry::check(&text);
            (Some(r), f)
        }
    };
    let defective = !findings.is_empty();
    if let (Some(r), false) = (&reg, defective) {
        findings.extend(require.iter().filter_map(|id| r.resolve(id)));
    }
    let code = if defective {
        2
    } else if findings.is_empty() {
        0
    } else {
        1
    };
    let out = serde_json::json!({
        "schema": REGISTRY_CHECK_SCHEMA,
        "registry_sha256": reg.as_ref().map(|r| r.sha256.clone()),
        "ids": reg.as_ref().map(Registry::len),
        "locked": reg.as_ref().map(Registry::locked),
        "required": require,
        "required_evaluated": !defective,
        "exit": code,
        "findings": findings.iter().map(|f| serde_json::json!({
            "code": f.code.as_str(), "line": f.line, "id": f.id, "detail": f.detail,
        })).collect::<Vec<_>>(),
    });
    print!("{}", to_stable_json(&out));
    Ok(ExitCode::from(code))
}

fn remove_stale(out: &Path) -> Result<(), InternalError> {
    match std::fs::remove_file(out) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(InternalError(format!(
            "remove stale {}: {e}",
            out.display()
        ))),
    }
}
