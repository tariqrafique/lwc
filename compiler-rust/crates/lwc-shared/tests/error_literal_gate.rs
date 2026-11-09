//! AB-0001 gate: `@lwc/errors` error infos live only in `lwc_shared::errors::error_info`. Fails on an
//! error-info literal (a code next to its message or level, or a hardcoded `"LWC####` message) in any
//! crate's non-test source outside `crates/lwc-shared/src/errors/`. No exemptions: AB-R003 deleted every
//! pending copy, so any new literal, version const, or version comment outside lwc-shared fails here.
use regex::Regex;
use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() { rs_files(&p, out) } else if p.extension().is_some_and(|x| x == "rs") { out.push(p) }
    }
}

#[test]
fn no_error_info_literals_outside_lwc_shared_errors() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let pats = [
        Regex::new(r#"\b1[0-2]\d\d\b\s*,\s*""#).unwrap(),
        Regex::new(r#""LWC1[0-2]\d\d\b"#).unwrap(),
        Regex::new(r"\(\s*1[0-2]\d\d\s*,\s*(Error|Warning|Log|Fatal|DiagnosticLevel::)").unwrap(),
    ];
    let test_mod = Regex::new(r"(?m)^#\[cfg\(test\)\]").unwrap();
    let mut hits = vec![];
    let mut files = vec![];
    for c in std::fs::read_dir(crates).unwrap().flatten() {
        let src = c.path().join("src");
        if src.is_dir() { rs_files(&src, &mut files) }
    }
    for f in files {
        let rel = f.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/");
        if rel.starts_with("lwc-shared/src/errors/") { continue }
        let text = std::fs::read_to_string(&f).unwrap();
        let body = &text[..test_mod.find(&text).map_or(text.len(), |m| m.start())];
        for p in &pats {
            for m in p.find_iter(body) {
                let line = body[..m.start()].lines().count().max(1);
                hits.push(format!("{rel}:{line}: {}", m.as_str()));
            }
        }
    }
    assert!(hits.is_empty(), "error-info literals outside lwc-shared/src/errors (use lwc_shared::errors::error_info):\n{}", hits.join("\n"));
}

#[test]
fn exactly_one_lwc_version_const() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = vec![];
    for c in std::fs::read_dir(crates).unwrap().flatten() {
        let src = c.path().join("src");
        if src.is_dir() { rs_files(&src, &mut files) }
    }
    let defs: Vec<_> = files.iter().filter(|f| std::fs::read_to_string(f).unwrap_or_default().contains(concat!("const ", "LWC_VERSION"))).collect();
    let rel: Vec<String> = defs.iter().map(|f| f.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/")).collect();
    let expected = vec!["lwc-shared/src/meta.rs".to_string()];
    let mut got = rel.clone();
    got.sort();
    assert_eq!(got, expected, "only lwc-shared/src/meta.rs may define the LWC version");
}

/// `@lwc/shared` owns `LWC_VERSION_COMMENT`: no crate's source outside lwc-shared/src/meta.rs spells it.
#[test]
fn exactly_one_lwc_version_comment() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = vec![];
    for c in std::fs::read_dir(crates).unwrap().flatten() {
        let src = c.path().join("src");
        if src.is_dir() { rs_files(&src, &mut files) }
    }
    let needle = concat!("LWC compiler", " v");
    let mut got: Vec<String> = files
        .iter()
        .filter(|f| std::fs::read_to_string(f).unwrap_or_default().contains(needle))
        .map(|f| f.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    got.sort();
    let expected = vec!["lwc-shared/src/meta.rs".to_string()];
    assert_eq!(got, expected, "only lwc-shared/src/meta.rs may spell the LWC version comment; use lwc_shared::meta::version_comment");
}
