//! AB-R001 gate: a template `SourceLocation` / `ElementSourceLocation` is always a real source range.
//! Fails on a defaulted or zero-filled one in any non-test source of the workspace crates
//! (`crates/*/src`) or the standalone tools (`tools/*/**/src`): `SourceLocation::default()`, a
//! `SourceLocation { .. }` literal completed by `..Default::default()`, `impl Default for SourceLocation`,
//! or a `Default` derive on either type. Where upstream has no location the diagnostic says so
//! (`Option<DiagnosticLocation>` = None, or `location::normalize_location(None)` where upstream calls
//! `normalizeLocation(undefined)`); IR nodes carry the location upstream gives them.
use regex::Regex;
use repo_tracked::{rel_slash, tracked_files_filtered, workspace_root};
use std::path::{Path, PathBuf};

fn production_src_rs(repo: &Path) -> Vec<PathBuf> {
    tracked_files_filtered(repo, &["crates", "tools"], &["rs"])
        .into_iter()
        .filter(|p| {
            let rel = rel_slash(repo, p);
            let parts: Vec<&str> = rel.split('/').collect();
            match parts.iter().position(|c| *c == "src") {
                Some(i) => {
                    !parts[..i].iter().any(|c| {
                        matches!(
                            *c,
                            "target" | "tests" | "examples" | "benches" | "node_modules"
                        )
                    }) && !parts.contains(&"target")
                }
                None => false,
            }
        })
        .collect()
}

/// `text` with each top-level `#[cfg(test)]` item blanked out (from the attribute to the item's closing
/// `}` in column 0, or its `;` for a one-line item); newlines are kept so reported lines stay true.
fn non_test(text: &str) -> String {
    let attr = Regex::new(r"(?m)^#\[cfg\(test\)\]").unwrap();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(m) = attr.find(rest) {
        out.push_str(&rest[..m.start()]);
        let item = &rest[m.start()..];
        let head_end = item.find('\n').map_or(item.len(), |i| i + 1);
        let next = &item[head_end..];
        let first_line = next.lines().next().unwrap_or("");
        let len = if first_line.trim_end().ends_with(';') || !first_line.trim_end().ends_with('{') {
            head_end + next.find('\n').map_or(next.len(), |i| i + 1)
        } else {
            head_end
                + next
                    .find("\n}")
                    .map_or(next.len(), |i| (i + 2).min(next.len()))
        };
        out.extend(
            item[..len]
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' }),
        );
        rest = &item[len..];
    }
    out.push_str(rest);
    out
}

/// `file:line: match` for each banned form in `text`'s non-test code.
fn violations(rel: &str, text: &str) -> Vec<String> {
    let ty = r"\b(?:Element)?SourceLocation";
    let pats = [
        Regex::new(&format!(r"{ty}::default\s*\(")).unwrap(),
        Regex::new(&format!(r"(?s){ty}\s*\{{[^{{}}]*\.\.\s*(?:Default::default|<[^>]*>::default|(?:Element)?SourceLocation::default)\s*\(")).unwrap(),
        Regex::new(&format!(r"impl\s+(?:std::default::|core::default::)?Default\s+for\s+(?:[\w:]*::)?(?:Element)?SourceLocation\b")).unwrap(),
        Regex::new(r"(?s)#\[derive\([^)]*\bDefault\b[^)]*\)\]\s*(?:#\[[^\]]*\]\s*)*pub(?:\([^)]*\))?\s+struct\s+(?:Element)?SourceLocation\b").unwrap(),
    ];
    let body = &non_test(text);
    let mut hits = vec![];
    for p in &pats {
        for m in p.find_iter(body) {
            let line = body[..m.start()].lines().count().max(1);
            hits.push(format!(
                "{rel}:{line}: {}",
                m.as_str().split_whitespace().collect::<Vec<_>>().join(" ")
            ));
        }
    }
    hits
}

#[test]
fn no_defaulted_source_location_outside_tests() {
    let root = workspace_root(&lwc_shared::cargo_manifest_dir!());
    let files = production_src_rs(&root);
    assert!(
        files
            .iter()
            .any(|f| f.ends_with("lwc-shared/src/template_ir.rs")),
        "gate scans nothing: {} files",
        files.len()
    );
    let mut hits = vec![];
    for f in files {
        let rel = rel_slash(&root, &f);
        hits.extend(violations(&rel, &std::fs::read_to_string(&f).unwrap()));
    }
    assert!(
        hits.is_empty(),
        "defaulted SourceLocation outside tests (carry the real location, or None / normalize_location(None) on the diagnostic):\n{}",
        hits.join("\n")
    );
}

/// The patterns catch each banned form (multi-line literals included) and spare test modules and
/// real constructions.
#[test]
fn gate_patterns_catch_each_form() {
    for bad in [
        "let l = SourceLocation::default();",
        "let l = ElementSourceLocation::default();",
        "let l = SourceLocation { start: 1, end: 2, ..Default::default() };",
        "let l = SourceLocation {\n    start_line: 1,\n    ..Default::default()\n};",
        "let l = lwc_shared::template_ir::SourceLocation { start: 0, ..SourceLocation::default() };",
        "impl Default for SourceLocation { fn default() -> Self { todo!() } }",
        "#[derive(Debug, Clone, Default)]\npub struct SourceLocation { pub start: u32 }",
        "#[derive(Default)]\n#[serde(rename_all = \"camelCase\")]\npub struct ElementSourceLocation {}",
        // Non-test code after a test module is still scanned.
        "#[cfg(test)]\nmod tests {\n    fn t() {}\n}\nimpl Default for SourceLocation { fn default() -> Self { todo!() } }",
    ] {
        assert!(!violations("x.rs", bad).is_empty(), "missed: {bad}");
    }
    for ok in [
        "let l = SourceLocation { start_line: 1, start_column: 0, end_line: 1, end_column: 3, start: 0, end: 3 };",
        "let o = ParserOptions { source_code_location_info: true, ..Default::default() };",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct SourceLocation { pub start: u32 }",
        "fn f() {}\n#[cfg(test)]\nmod tests {\n    const L: SourceLocation = SourceLocation::default();\n}\n",
    ] {
        assert!(violations("x.rs", ok).is_empty(), "false positive: {ok}");
    }
}
