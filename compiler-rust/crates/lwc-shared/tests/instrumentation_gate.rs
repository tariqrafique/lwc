//! J3 (D-0205 / D-0212): `lwc_shared::instrumentation` is the one port of `@lwc/errors` `CompilerMetrics` and of
//! `@lwc/compiler` `validateTransformOptions`' deprecation warnings.
//! - `generated_is_fresh`: generated.rs equals `tools/gen-instrumentation/gen.mjs` run on the pinned oracle.
//! - `matches_live_oracle`: the enums equal what the oracle's runtime exports and prints.
//! - `defined_only_in_lwc_shared`: no code file anywhere in the repo spells a metric value or a deprecation text,
//!   except the named, existence-asserted `ALLOWED` entries (AB-L0065).
//!
//! All fail closed: a missing node, tsx or LWC_ORACLE checkout is a failure, not a skip.
use lwc_shared::{CompilerMetric, DeprecationId};
use repo_tracked::{rel_slash, tracked_files_filtered, workspace_root};
use std::path::{Path, PathBuf};
use std::process::Command;

fn run_gen(flag: &str) -> String {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let out = Command::new("node")
        .arg(repo.join("tools/gen-instrumentation/gen.mjs"))
        .args([flag, "--root"])
        .arg(&repo)
        .current_dir(std::env::temp_dir())
        .output()
        .expect("node must be on PATH for the instrumentation gate");
    assert!(
        out.status.success(),
        "gen.mjs {flag} exited {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn generated_is_fresh() { run_gen("--check"); }

#[test]
fn matches_live_oracle() {
    let rows = run_gen("--oracle");
    let (mut metrics, mut deps) = (vec![], vec![]);
    for r in rows.lines() {
        let f: Vec<&str> = r.split('\t').collect();
        match f.as_slice() {
            ["metric", _key, v] => metrics.push(v.to_string()),
            ["deprecation", path, n, text] => deps.push((path.to_string(), n.parse::<u32>().unwrap(), text.to_string())),
            _ => panic!("unexpected oracle row {r:?}"),
        }
    }
    let ours: Vec<String> = CompilerMetric::ALL.iter().map(|m| m.js_value().to_string()).collect();
    assert_eq!(ours, metrics, "CompilerMetric != @lwc/errors CompilerMetrics");
    // Two validateTransformOptions calls: a latched warning prints once, an unlatched one twice.
    let ours: Vec<(String, u32, String)> = DeprecationId::ALL
        .iter()
        .map(|d| (d.option_path().to_string(), if d.once_per_process() { 1 } else { 2 }, d.message().to_string()))
        .collect();
    assert_eq!(ours, deps, "DeprecationId != @lwc/compiler validateTransformOptions warnings");
}

/// Every spelling the gate forbids outside [`ALLOWED`].
fn needles() -> Vec<String> {
    let mut n: Vec<String> = CompilerMetric::ALL.iter().map(|m| m.js_value().to_string()).collect();
    n.extend(DeprecationId::ALL.iter().map(|d| d.message().to_string()));
    n.push(concat!("property is ", "deprecated").to_string()); // a reworded or partial copy of a warning
    n.push(concat!("doesn't impact the ", "compilation").to_string());
    n
}

/// The only files (repo-relative) that may spell a needle; each must exist (`allow_entries_exist`).
/// - generated.rs: the one definition (`CompilerMetric::js_value`, `DeprecationId::message`).
/// - gen.mjs: the generator; its regex over the upstream warning text derives the ids.
///
/// This file needs no entry: it builds its needles from the enums and spells fragments with `concat!`.
const ALLOWED: &[&str] = &["crates/lwc-shared/src/instrumentation/generated.rs", "tools/gen-instrumentation/gen.mjs"];

const SCANNED: &[&str] = &["rs", "js", "mjs", "cjs", "ts", "mts", "cts"];

fn repo_root() -> PathBuf { workspace_root(Path::new(env!("CARGO_MANIFEST_DIR"))) }

fn listed_source(root: &Path) -> Vec<PathBuf> {
    tracked_files_filtered(root, &[], SCANNED)
}

/// Hits in `files` (`rel` paths are relative to `root`). An unreadable scanned file panics with its path.
fn scan_files(root: &Path, files: &[PathBuf], needles: &[String], hits: &mut Vec<String>) {
    for p in files {
        if !p.extension().and_then(|x| x.to_str()).is_some_and(|x| SCANNED.contains(&x)) { continue }
        let rel = rel_slash(root, p);
        if ALLOWED.contains(&rel.as_str()) { continue }
        let text = std::fs::read_to_string(p).unwrap_or_else(|err| panic!("cannot read {}: {err}", p.display()));
        for (i, line) in text.lines().enumerate() {
            if let Some(n) = needles.iter().find(|n| line.contains(n.as_str())) { hits.push(format!("{rel}:{}: {n}", i + 1)) }
        }
    }
}

#[test]
fn allow_entries_exist() {
    let root = repo_root();
    for a in ALLOWED { assert!(root.join(a).is_file(), "allow-entry {a} does not exist under {}", root.display()) }
}

#[test]
fn defined_only_in_lwc_shared() {
    let root = repo_root();
    let mut hits = vec![];
    scan_files(&root, &listed_source(&root), &needles(), &mut hits);
    assert!(hits.is_empty(), "use lwc_shared::{{CompilerMetric, DeprecationId}} instead of a literal:\n{}", hits.join("\n"));
}

/// A scratch tree under the temp dir, removed on drop (also when the test panics).
struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("instr-gate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn put(&self, rel: &str, body: &[u8]) {
        let f = self.0.join(rel);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, body).unwrap();
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

/// Plants copies in a scratch tree, including Arch AB-L0065's two escapes (napi `npm/` and `tools/`), and proves
/// the content scan reports each planted file while skipping named allow-entries and non-code files.
/// Directory skipping (`.oracle/`, `target/`, untracked files) is `repo-tracked`'s job.
#[test]
fn gate_catches_planted_copies() {
    let t = Scratch::new("plant");
    let m = DeprecationId::OutputConfigMinify.message();
    let planted: &[(&str, String)] = &[
        ("crates/lwc-compiler-napi/npm/linux-x64-gnu/plant.js", format!("console.warn({m:?});")),
        ("tools/semdiff/zz/p.mjs", format!("export const M = '{}';", CompilerMetric::LwcDynamicDirective.js_value())),
        ("crates/lwc-planted/src/lib.rs", format!("const M: &str = {:?};", CompilerMetric::LwcSpreadDirective.js_value())),
        ("crates/lwc-planted/src/w.rs", concat!("let s = \"\\\"stylesheetConfig\\\" property is ", "deprecated.\";").to_string()),
        ("harness/src/x.cjs", concat!("console.warn(`The value doesn't impact the ", "compilation and can safely be removed.`);").to_string()),
        ("scripts/y.mts", format!("const a = {:?};", CompilerMetric::DynamicImportTransform.js_value())),
        ("npm/z.ts", format!("const a = {:?};", CompilerMetric::LwcRenderModeDirective.js_value())),
    ];
    for (rel, body) in planted { t.put(rel, body.as_bytes()) }
    // Not caught: the enum, the named allow-entries, skipped dir names, and non-code files.
    t.put("crates/lwc-planted/src/ok.rs", b"fn f() -> &'static str { lwc_shared::CompilerMetric::LwcSpreadDirective.js_value() }\n");
    for a in ALLOWED { t.put(a, m.as_bytes()) }
    t.put("crates/lwc-compiler-napi/node_modules/dep/index.js", m.as_bytes());
    t.put("target/debug/build/out.rs", m.as_bytes());
    t.put("harness/goldens/a.json", m.as_bytes());
    let mut scan_list: Vec<PathBuf> = planted.iter().map(|(rel, _)| t.0.join(rel)).collect();
    scan_list.push(t.0.join("crates/lwc-planted/src/ok.rs"));
    for a in ALLOWED { scan_list.push(t.0.join(a)); }
    scan_list.push(t.0.join("harness/goldens/a.json"));
    let mut hits = vec![];
    scan_files(&t.0, &scan_list, &needles(), &mut hits);
    let mut got: Vec<&str> = hits.iter().map(|h| h.split(':').next().unwrap()).collect();
    got.sort();
    let mut want: Vec<&str> = planted.iter().map(|(r, _)| *r).collect();
    want.sort();
    assert_eq!(got, want, "{hits:#?}");
}

/// A scanned file that can't be read as text fails the gate with its path instead of being skipped.
#[test]
#[should_panic(expected = "bad.js")]
fn unreadable_file_panics_with_its_path() {
    let t = Scratch::new("unreadable");
    t.put("crates/x/bad.js", &[0xff, 0xfe, 0x00, 0xc3]);
    scan_files(&t.0, &[t.0.join("crates/x/bad.js")], &needles(), &mut vec![]);
}
