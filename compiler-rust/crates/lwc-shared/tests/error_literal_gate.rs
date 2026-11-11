//! AB-0001 gate: `@lwc/errors` error infos live only in `lwc_shared::errors::error_info`. Fails on an
//! error-info literal (a code next to its message or level, or a hardcoded `"LWC####` message) in any
//! crate's non-test source outside `crates/lwc-shared/src/errors/`. No exemptions: AB-R003 deleted every
//! pending copy, so any new literal, version const, or version comment outside lwc-shared fails here.
use regex::Regex;
use repo_tracked::{rel_slash, tracked_files_filtered, workspace_root};
use std::path::PathBuf;

fn crate_src_rs() -> (PathBuf, Vec<PathBuf>) {
    let repo = workspace_root(&lwc_shared::cargo_manifest_dir!());
    let crates = repo.join("crates");
    let files = tracked_files_filtered(&repo, &["crates"], &["rs"])
        .into_iter()
        .filter(|p| {
            let rel = rel_slash(&crates, p);
            rel.split('/').nth(1) == Some("src")
        })
        .collect();
    (crates, files)
}

#[test]
fn no_error_info_literals_outside_lwc_shared_errors() {
    let (crates, files) = crate_src_rs();
    let pats = [
        Regex::new(r#"\b1[0-2]\d\d\b\s*,\s*""#).unwrap(),
        Regex::new(r#""LWC1[0-2]\d\d\b"#).unwrap(),
        Regex::new(r"\(\s*1[0-2]\d\d\s*,\s*(Error|Warning|Log|Fatal|DiagnosticLevel::)").unwrap(),
    ];
    let test_mod = Regex::new(r"(?m)^#\[cfg\(test\)\]").unwrap();
    let mut hits = vec![];
    for f in files {
        let rel = rel_slash(&crates, &f);
        if rel.starts_with("lwc-shared/src/errors/") {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap();
        let body = &text[..test_mod.find(&text).map_or(text.len(), |m| m.start())];
        for p in &pats {
            for m in p.find_iter(body) {
                let line = body[..m.start()].lines().count().max(1);
                hits.push(format!("{rel}:{line}: {}", m.as_str()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "error-info literals outside lwc-shared/src/errors (use lwc_shared::errors::error_info):\n{}",
        hits.join("\n")
    );
}

#[test]
fn exactly_one_lwc_version_const() {
    let (crates, files) = crate_src_rs();
    let defs: Vec<_> = files
        .iter()
        .filter(|f| {
            std::fs::read_to_string(f)
                .unwrap_or_default()
                .contains(concat!("const ", "LWC_VERSION"))
        })
        .collect();
    let rel: Vec<String> = defs.iter().map(|f| rel_slash(&crates, f)).collect();
    let expected = vec!["lwc-shared/src/meta.rs".to_string()];
    let mut got = rel.clone();
    got.sort();
    assert_eq!(
        got, expected,
        "only lwc-shared/src/meta.rs may define the LWC version"
    );
}

/// `@lwc/shared` owns `LWC_VERSION_COMMENT`: no crate's source outside lwc-shared/src/meta.rs spells it.
#[test]
fn exactly_one_lwc_version_comment() {
    let (crates, files) = crate_src_rs();
    let needle = concat!("LWC compiler", " v");
    let mut got: Vec<String> = files
        .iter()
        .filter(|f| {
            std::fs::read_to_string(f)
                .unwrap_or_default()
                .contains(needle)
        })
        .map(|f| rel_slash(&crates, f))
        .collect();
    got.sort();
    let expected = vec!["lwc-shared/src/meta.rs".to_string()];
    assert_eq!(
        got, expected,
        "only lwc-shared/src/meta.rs may spell the LWC version comment; use lwc_shared::meta::version_comment"
    );
}
