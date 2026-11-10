//! AB-L0028: `data-*` classification is TS `isDataAttribute` (DATA_RE), ported once in parser/attribute.rs.
//! A raw `data-` prefix test anywhere else in this crate re-forks it (`<x-foo data-a!b>` must be a property).
use repo_tracked::{tracked_files_filtered, workspace_root};
use std::path::{Path, PathBuf};

const NEEDLE: &str = concat!("starts_with(\"", "data-\")");

fn violations_in(files: &[PathBuf], allowed: &Path) -> Vec<String> {
    let mut bad = Vec::new();
    for f in files.iter().filter(|f| **f != allowed) {
        let text = std::fs::read_to_string(f).unwrap();
        for (i, line) in text.lines().enumerate() {
            if line.contains(NEEDLE) { bad.push(format!("{}:{}: {}", f.display(), i + 1, line.trim())); }
        }
    }
    bad
}

fn crate_rs_files() -> (PathBuf, Vec<PathBuf>) {
    let repo = workspace_root(&lwc_shared::cargo_manifest_dir!());
    let root = lwc_shared::cargo_manifest_dir!();
    let files = tracked_files_filtered(&repo, &["crates/lwc-template"], &["rs"]);
    (root, files)
}

#[test]
fn no_data_prefix_test_outside_attribute_rs() {
    let (root, files) = crate_rs_files();
    let bad = violations_in(&files, &root.join("src/parser/attribute.rs"));
    assert!(bad.is_empty(), "use parser::attribute::is_data_attribute instead of {NEEDLE}:\n{}", bad.join("\n"));
}

/// The scan sees a planted violation (and ignores the same text in parser/attribute.rs).
#[test]
fn guard_catches_planted_violation() {
    let root = std::env::temp_dir().join(format!("lwc-data-guard-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src/parser")).unwrap();
    std::fs::write(root.join("src/parser/attribute.rs"), format!("fn ok(a: &str) -> bool {{ a.{NEEDLE} }}\n")).unwrap();
    let allowed = root.join("src/parser/attribute.rs");
    assert!(violations_in(std::slice::from_ref(&allowed), &allowed).is_empty());
    let planted = root.join("src/codegen.rs");
    std::fs::write(&planted, format!("fn bad(a: &str) -> bool {{ a.{NEEDLE} }}\n")).unwrap();
    let bad = violations_in(&[allowed.clone(), planted], &allowed);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(bad[0].contains("codegen.rs:1:"), "{bad:?}");
}
