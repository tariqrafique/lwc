//! AB-L0028: `data-*` classification is TS `isDataAttribute` (DATA_RE), ported once in parser/attribute.rs.
//! A raw `data-` prefix test anywhere else in this crate re-forks it (`<x-foo data-a!b>` must be a property).
use std::path::{Path, PathBuf};

const NEEDLE: &str = concat!("starts_with(\"", "data-\")");

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let p = e.unwrap().path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n != "target") { rust_files(&p, out); }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn violations(root: &Path) -> Vec<String> {
    let allowed = root.join("src/parser/attribute.rs");
    let mut files = Vec::new();
    rust_files(root, &mut files);
    files.sort();
    let mut bad = Vec::new();
    for f in files.iter().filter(|f| **f != allowed) {
        let text = std::fs::read_to_string(f).unwrap();
        for (i, line) in text.lines().enumerate() {
            if line.contains(NEEDLE) { bad.push(format!("{}:{}: {}", f.display(), i + 1, line.trim())); }
        }
    }
    bad
}

#[test]
fn no_data_prefix_test_outside_attribute_rs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bad = violations(root);
    assert!(bad.is_empty(), "use parser::attribute::is_data_attribute instead of {NEEDLE}:\n{}", bad.join("\n"));
}

/// The scan sees a planted violation (and ignores the same text in parser/attribute.rs).
#[test]
fn guard_catches_planted_violation() {
    let root = std::env::temp_dir().join(format!("lwc-data-guard-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src/parser")).unwrap();
    std::fs::write(root.join("src/parser/attribute.rs"), format!("fn ok(a: &str) -> bool {{ a.{NEEDLE} }}\n")).unwrap();
    assert!(violations(&root).is_empty());
    std::fs::write(root.join("src/codegen.rs"), format!("fn bad(a: &str) -> bool {{ a.{NEEDLE} }}\n")).unwrap();
    let bad = violations(&root);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(bad[0].contains("codegen.rs:1:"), "{bad:?}");
}
