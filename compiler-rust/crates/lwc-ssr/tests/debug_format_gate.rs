//! No `Debug` formatting (`{x:?}`, `{:#?}`, …) in code that prints JavaScript: a Rust `Debug` string is not a
//! JS literal (`"\01"`, `"\u{2028}"`, lone surrogates dropped). Print literals with
//! `lwc_shared::js_literal::json_string`. Scans lwc-ssr `src/` and lwc-template `src/codegen/`, skipping
//! `#[cfg(test)]` items and diagnostics that never reach the output: a placeholder on a line holding
//! `panic!(`, `unreachable!(`, `assert!(`, `assert_eq!(`, `assert_ne!(`, `debug_assert` or `.expect(`.

use repo_tracked::{tracked_files_filtered, workspace_root};
use std::path::PathBuf;

/// lwc-template codegen's existing hits. It only shrinks; lower it when hits are removed.
const LWC_TEMPLATE_CODEGEN_ALLOWANCE: usize = 0;

const DIAGNOSTIC: &[&str] = &[
    "panic!(",
    "unreachable!(",
    "assert!(",
    "assert_eq!(",
    "assert_ne!(",
    "debug_assert",
    ".expect(",
];

/// Byte ranges of string literal contents outside `#[cfg(test)]` items, with their lines.
fn emitted_strings(src: &str) -> Vec<(usize, &str)> {
    let b = src.as_bytes();
    let line_of = |i: usize| src[..i].matches('\n').count() + 1;
    let mut out = Vec::new();
    let (mut i, mut depth, mut skip_until): (usize, i32, Option<i32>) = (0, 0, None);
    let mut pending_test_item = false;
    while i < b.len() {
        let rest = &src[i..];
        if rest.starts_with("//") {
            i += rest.find('\n').unwrap_or(rest.len());
        } else if rest.starts_with("/*") {
            i += rest.find("*/").expect("unterminated block comment") + 2;
        } else if rest.starts_with("#[cfg(test)]") {
            pending_test_item = skip_until.is_none();
            i += "#[cfg(test)]".len();
        } else if let Some((prefix, hashes)) = raw_string_prefix(rest) {
            let open = prefix + hashes + 1;
            let close = format!("\"{}", "#".repeat(hashes));
            let end = rest[open..].find(&close).expect("unterminated raw string") + open;
            if skip_until.is_none() {
                out.push((line_of(i), &rest[open..end]));
            }
            i += end + close.len();
        } else if b[i] == b'"' || rest.starts_with("b\"") {
            let open = if b[i] == b'"' { 1 } else { 2 };
            let mut j = open;
            while rest.as_bytes()[j] != b'"' {
                j += if rest.as_bytes()[j] == b'\\' { 2 } else { 1 };
            }
            if skip_until.is_none() {
                out.push((line_of(i), &rest[open..j]));
            }
            i += j + 1;
        } else if b[i] == b'\'' {
            // A char literal (`'x'`, `'\''`, `'\u{1f}'`) or a lifetime / label (`'a`).
            let body = &rest[1..];
            if let Some(esc) = body.strip_prefix('\\') {
                i += 3 + esc[1..].find('\'').expect("unterminated char literal") + 1;
            } else if let Some(c) = body
                .chars()
                .next()
                .filter(|c| body[c.len_utf8()..].starts_with('\''))
            {
                i += 2 + c.len_utf8();
            } else {
                i += 1;
            }
        } else {
            match b[i] {
                b'{' => {
                    depth += 1;
                    if pending_test_item {
                        skip_until = Some(depth - 1);
                        pending_test_item = false;
                    }
                }
                b'}' => {
                    depth -= 1;
                    if skip_until == Some(depth) {
                        skip_until = None;
                    }
                }
                b';' if pending_test_item => pending_test_item = false,
                _ => {}
            }
            i += 1;
        }
    }
    assert_eq!(depth, 0, "unbalanced braces: the lexer lost track");
    out
}

/// `(prefix length, hash count)` of a raw string literal starting here (`r"`, `r#"`, `br##"`, …).
fn raw_string_prefix(rest: &str) -> Option<(usize, usize)> {
    let r = rest.strip_prefix("br").or_else(|| rest.strip_prefix('r'))?;
    let hashes = r.len() - r.trim_start_matches('#').len();
    r[hashes..]
        .starts_with('"')
        .then_some((rest.len() - r.len(), hashes))
}

fn debug_placeholders(src: &str) -> Vec<String> {
    let lines: Vec<&str> = src.lines().collect();
    let mut hits = Vec::new();
    for (line, text) in emitted_strings(src) {
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            if rest[open + 1..].starts_with('{') {
                rest = &rest[open + 2..];
                continue;
            }
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            let spec = &rest[open + 1..open + close];
            if spec.contains(':') && spec.ends_with('?') && !spec.contains(char::is_whitespace) {
                let at = line + text[..text.len() - rest.len() + open].matches('\n').count();
                let code = lines[at - 1];
                if !DIAGNOSTIC.iter().any(|d| code.contains(d)) {
                    hits.push(format!("{at}: {}", code.trim()));
                }
            }
            rest = &rest[open + close + 1..];
        }
    }
    hits
}

fn hits_under_files(files: &[PathBuf]) -> (usize, Vec<String>) {
    let hits = files.iter().flat_map(|f| {
        let src = std::fs::read_to_string(f).unwrap();
        debug_placeholders(&src)
            .into_iter()
            .map(move |h| format!("{}:{h}", f.display()))
    });
    (files.len(), hits.collect())
}

#[test]
fn no_debug_formatting_in_emit_code() {
    let repo = workspace_root(&lwc_shared::cargo_manifest_dir!());
    let ssr_list = tracked_files_filtered(&repo, &["crates/lwc-ssr/src"], &["rs"]);
    let tpl_list = tracked_files_filtered(&repo, &["crates/lwc-template/src/codegen"], &["rs"]);
    let (ssr_files, ssr) = hits_under_files(&ssr_list);
    let (tpl_files, tpl) = hits_under_files(&tpl_list);
    assert!(
        ssr_files > 10 && tpl_files > 0,
        "scanned {ssr_files} lwc-ssr and {tpl_files} lwc-template files"
    );
    assert!(
        ssr.is_empty(),
        "Debug formatting in lwc-ssr emit code; use lwc_shared::js_literal::json_string:\n{}",
        ssr.join("\n")
    );
    assert_eq!(
        tpl.len(),
        LWC_TEMPLATE_CODEGEN_ALLOWANCE,
        "lwc-template codegen Debug formatting (shrink-only allowance):\n{}",
        tpl.join("\n")
    );
}

#[test]
fn scanner_finds_hits_outside_test_items_only() {
    let src = r##"
fn emit(p: &str) -> String { let c = '{'; let l: &'static str = "}"; format!("[{p:?}] {{x:?}} {:#?}", 1) }
fn ok(p: &str) { panic!("{p:?}"); }
fn raw() -> &'static str { br#"x"#; r#"{a:?}"# }
#[cfg(test)]
mod tests { fn t() { let s = format!("{x:?}"); if true { } } }
#[cfg(test)]
fn helper() -> String { format!("{y:?}") }
fn after() -> String { format!("{z:?}") }
"##;
    let hits = debug_placeholders(src);
    assert_eq!(hits.len(), 4, "{hits:#?}");
    assert!(
        hits[0].starts_with("2:")
            && hits[1].starts_with("2:")
            && hits[2].starts_with("4:")
            && hits[3].starts_with("9:"),
        "{hits:#?}"
    );
}
