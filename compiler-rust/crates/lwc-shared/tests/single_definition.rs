//! AB-L0022 guard: `lwc_shared::style` and `lwc_shared::js` are the only definitions of the
//! style-text parser and JS whitespace. Fails if any other crate under `crates/` defines one, or
//! hand-rolls JS whitespace under another name: a `'\u{feff}'` char literal (ZWNBSP is the char a
//! `char::is_whitespace` copy has to add by hand) or `is_whitespace() ||` (that add-on).

use std::path::Path;

const FORBIDDEN: &[&str] = &["fn parse_style_text", "fn normalize_style", "fn js_trim", "fn js_ws"];

/// A U+FEFF char literal that is not JS whitespace: postcss `Input` drops a leading byte-order mark
/// (U+FEFF, and U+FFFE as decoded by the Rust reader) before tokenizing. `(file under crates/, line text)`.
const BOM_ALLOWED: &[(&str, &str)] = &[
    ("lwc-style/src/parser.rs", "src.strip_prefix(['\\u{FEFF}', '\\u{FFFE}'])"),
    ("lwc-style/src/serialize.rs", "css.trim_start_matches(['\\u{feff}', '\\u{fffe}'])"),
    ("lwc-style/src/serialize.rs", "css.trim_start_matches('\\u{feff}')"),
];

/// Every guard hit on one file's text; `rel` is the path under `crates/`.
fn violations(rel: &str, src: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let squashed: String = line.chars().filter(|c| !c.is_ascii_whitespace()).collect::<String>().to_ascii_lowercase();
        let feff_literal = squashed.contains('\u{feff}') || has_feff_char_literal(&squashed);
        let bom_ok = BOM_ALLOWED.iter().any(|(f, l)| *f == rel && line.contains(l));
        let ws_or = squashed.contains("is_whitespace()||") || squashed.contains("||c.is_whitespace()") || squashed.contains("is_whitespace(c)||");
        if FORBIDDEN.iter().any(|f| line.contains(f)) || (feff_literal && !bom_ok) || ws_or {
            hits.push(format!("{rel}:{}: {}", i + 1, line.trim()));
        }
    }
    hits
}

/// `'\u{feff}'` in any spelling (`'\u{FEFF}'`, `'\u{0feff}'`), on a lowercased, space-free line.
fn has_feff_char_literal(s: &str) -> bool {
    s.match_indices("'\\u{").any(|(i, m)| {
        let rest = &s[i + m.len()..];
        rest.trim_start_matches('0').starts_with("feff}'")
    })
}

fn scan(root: &Path, dir: &Path, hits: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        let name = entry.file_name();
        if p.is_dir() {
            if name != "target" && name != "node_modules" { scan(root, &p, hits); }
        } else if p.extension().is_some_and(|e| e == "rs") {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            hits.extend(violations(&rel, &std::fs::read_to_string(&p).unwrap()));
        }
    }
}

#[test]
fn style_text_and_js_whitespace_defined_only_in_lwc_shared() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(crates).unwrap().flatten() {
        if entry.file_name() != "lwc-shared" && entry.path().is_dir() { scan(crates, &entry.path(), &mut hits); }
    }
    assert!(hits.is_empty(), "use lwc_shared::style / lwc_shared::js instead of a local copy:\n{}", hits.join("\n"));
}

/// Plants renamed copies of JS whitespace in a scratch crate and proves the scan reports each one.
#[test]
fn guard_catches_renamed_copies() {
    let root = std::env::temp_dir().join(format!("single-def-{}", std::process::id()));
    let src = root.join("lwc-planted/src");
    std::fs::create_dir_all(&src).unwrap();
    let planted = [
        "fn blank(s: &str) -> bool { s.chars().all(|c| c.is_whitespace() || c == '\\u{feff}') }",
        "fn space_like_acorn(c: char) -> bool { matches!(c, ' ' | '\\u{FEFF}') }",
        "const ZWNBSP: char = '\\u{0feff}';",
        "let ws = |c: char| c == '\\t' || c.is_whitespace();",
        "fn js_ws(c: char) -> bool { c == ' ' }",
        "let bom = s.strip_prefix('\\u{feff}');",
    ];
    std::fs::write(src.join("lib.rs"), planted.join("\n")).unwrap();
    // Not caught: JS whitespace through the shared helper, and plain `char::is_whitespace`.
    std::fs::write(src.join("ok.rs"), "use lwc_shared::js::js_ws;\nfn f(c: char) -> bool { js_ws(c) && !c.is_whitespace() }\n").unwrap();
    let mut hits = Vec::new();
    scan(&root, &root.join("lwc-planted"), &mut hits);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(hits.len(), planted.len(), "{hits:#?}");
    for (h, p) in hits.iter().zip(planted) { assert!(h.starts_with("lwc-planted/src/lib.rs:") && h.ends_with(p), "{h}"); }
}
