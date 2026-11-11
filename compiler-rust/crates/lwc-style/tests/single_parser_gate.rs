//! AB-R002 gate: lwc-style has ONE CSS parser (`parser::parse`, the PostCSS 8.5 parser.js port) and
//! no fallback error paths. Fails if a second parse entry point, a pre-pass checker, cssparser, or
//! the old catch-all reasons come back. Runs under `cargo test -p lwc-style`, which land.sh invokes
//! for any change under crates/lwc-style.
use repo_tracked::{rel_slash, tracked_files_filtered, workspace_root};
use std::fs;

fn src_lines() -> Vec<(String, usize, String)> {
    let repo = workspace_root(&lwc_shared::cargo_manifest_dir!());
    let root = lwc_shared::cargo_manifest_dir!();
    let mut files = tracked_files_filtered(&repo, &["crates/lwc-style/src"], &["rs"]);
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let rel = rel_slash(&root, &f);
        for (i, l) in fs::read_to_string(&f).unwrap().lines().enumerate() {
            if l.trim() == "#[cfg(test)]" {
                break;
            }
            out.push((rel.clone(), i + 1, l.to_string()));
        }
    }
    out
}

fn hits(pred: impl Fn(&str, &str) -> bool) -> Vec<String> {
    src_lines()
        .into_iter()
        .filter(|(f, _, l)| pred(f, l))
        .map(|(f, n, l)| format!("{f}:{n}: {}", l.trim()))
        .collect()
}

fn assert_none(what: &str, found: Vec<String>) {
    assert!(
        found.is_empty(),
        "AB-R002 single-parser gate: {what}\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn no_fallback_reasons() {
    assert_none(
        "catch-all 'Unknown parse error' reason is back",
        hits(|_, l| l.contains("Unknown parse error")),
    );
    // `Unknown word` is raised only by parser.js `unknownWord`, i.e. `parser::unknown_word`.
    // The raise may sit on the `fn unknown_word` line or (rustfmt) on the line after it.
    let lines = src_lines();
    let uw: Vec<_> = lines
        .iter()
        .filter(|(_, _, l)| l.contains("\"Unknown word"))
        .collect();
    let in_unknown_word = |(f, n, _): &&(String, usize, String)| {
        f == "src/parser.rs"
            && lines
                .iter()
                .any(|(g, m, l)| g == f && (m == n || m + 1 == *n) && l.contains("fn unknown_word"))
    };
    let uw_ok = uw.len() == 1 && in_unknown_word(&uw[0]);
    let uw: Vec<String> = uw
        .iter()
        .map(|(f, n, l)| format!("{f}:{n}: {}", l.trim()))
        .collect();
    assert!(
        uw_ok,
        "AB-R002 single-parser gate: 'Unknown word' must come only from parser.rs `unknown_word`, found:\n  {}",
        uw.join("\n  ")
    );
}

#[test]
fn no_second_parser() {
    assert_none(
        "cssparser is referenced again",
        hits(|_, l| l.contains("cssparser")),
    );
    let manifest =
        fs::read_to_string(lwc_shared::cargo_manifest_dir!().join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("cssparser"),
        "AB-R002 single-parser gate: cssparser is a dependency again"
    );
    assert_none(
        "a syntax_check pre-pass is back",
        hits(|_, l| l.contains("syntax_check")),
    );
    // The tree type is produced in exactly one place: `pub fn parse(src: &str) -> Result<Root, StyleError>`.
    let producers =
        hits(|_, l| l.contains("fn ") && (l.contains("-> Result<Root") || l.contains("-> Root")));
    assert!(
        producers.len() == 1
            && producers[0].starts_with("src/parser.rs:")
            && producers[0].contains("pub fn parse(src: &str)"),
        "AB-R002 single-parser gate: exactly one CSS tree entry point (parser.rs `pub fn parse`), found:\n  {}",
        producers.join("\n  ")
    );
    // ...and it is called from exactly one place, the transform.
    let calls = hits(|_, l| l.contains("parser::parse("));
    assert!(
        calls.len() == 1 && calls[0].starts_with("src/lib.rs:"),
        "AB-R002 single-parser gate: `parser::parse(` must be called once, from lib.rs, found:\n  {}",
        calls.join("\n  ")
    );
    // Syntax errors with PostCSS offsets are built only by the parser.
    assert_none(
        "StyleError::syntax_at used outside the parser",
        hits(|f, l| {
            l.contains("syntax_at(") && !f.starts_with("src/parser") && f != "src/error.rs"
        }),
    );
    // The tokenizer is private to the parser.
    assert_none(
        "tokenizer declared outside the parser",
        hits(|f, l| l.contains("mod tokenize") && f != "src/parser.rs"),
    );
}

/// AB-L0019: the parser and tokenizer get positions and texts from the token type, never from a
/// default standing in for an invariant (`start.unwrap_or_default()` was offset 0 for a space).
#[test]
fn no_defaulted_invariants_in_parser() {
    let fake = ["unwrap_or_default", "unwrap_or(\"\")", "unwrap_or(0)"];
    assert_none(
        "a default fakes an invariant under src/parser* (get it from the type, or return a Fail)",
        hits(|f, l| f.starts_with("src/parser") && fake.iter().any(|p| l.contains(p))),
    );
}
