//! AB-0015 gate: the postcss-lwc-plugin port stays split per upstream `@lwc/style-compiler/src/`
//! module. Fails if `scoping.rs` (the old monolith) returns, if a module loses its upstream header,
//! if a plugin's diagnostics or transforms move into another module, or if a module outgrows its
//! upstream file by a wide margin. Runs under `cargo test -p lwc-style` (land.sh, changed crates).
use std::fs;
use std::path::{Path, PathBuf};

/// (module file under src/, upstream file or dir its `//!` header must name)
const MODULES: &[(&str, &str)] = &[
    ("postcss_lwc_plugin.rs", "@lwc/style-compiler/src/postcss-lwc-plugin.ts"),
    ("selector_scoping.rs", "@lwc/style-compiler/src/selector-scoping/"),
    ("selector_scoping/transform.rs", "@lwc/style-compiler/src/selector-scoping/transform.ts"),
    ("selector_scoping/validate.rs", "@lwc/style-compiler/src/selector-scoping/validate.ts"),
    ("no_id_selectors.rs", "@lwc/style-compiler/src/no-id-selectors/validate.ts"),
    ("dir_pseudo_class.rs", "@lwc/style-compiler/src/dir-pseudo-class/transform.ts"),
    ("scope_at_rules.rs", "@lwc/style-compiler/src/scope-at-rules/transform.ts"),
    ("css_import.rs", "@lwc/style-compiler/src/css-import/transform.ts"),
    ("utils.rs", "@lwc/style-compiler/src/utils/"),
];

/// Upstream's largest plugin file is 159 lines; a module past this is a monolith growing back.
const MAX_MODULE_LINES: usize = 200;

/// (source marker, the only module allowed to contain it)
const OWNERS: &[(&str, &str)] = &[
    ("Invalid usage of id selector", "no_id_selectors.rs"),
    ("Invalid usage of deprecated selector", "selector_scoping/validate.rs"),
    ("Invalid usage of unsupported selector", "selector_scoping/validate.rs"),
    ("is a template directive", "selector_scoping/validate.rs"),
    ("fn scope_selector", "selector_scoping/transform.rs"),
    ("fn transform_host", "selector_scoping/transform.rs"),
    (":dir() pseudo class expects", "dir_pseudo_class.rs"),
    ("DIR_ATTRIBUTE_", "dir_pseudo_class.rs|serialize.rs"),
    ("\"keyframes\"", "scope_at_rules.rs|postcss_lwc_plugin.rs"),
    ("\"animation", "scope_at_rules.rs"),
    ("@import must precede", "css_import.rs"),
    ("Invalid import statement", "css_import.rs"),
    ("struct Ctx", "postcss_lwc_plugin.rs"),
    ("fn rule_error", "utils.rs"),
];

fn src() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("src") }

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() { rs_files(&p, out) } else if p.extension().is_some_and(|x| x == "rs") { out.push(p) }
    }
}

#[test]
fn no_scoping_monolith() {
    let s = src();
    assert!(!s.join("scoping.rs").exists() && !s.join("scoping").exists(),
        "AB-0015 layout gate: src/scoping.rs is back; keep the plugin split per upstream module");
    let lib = fs::read_to_string(s.join("lib.rs")).unwrap();
    assert!(!lib.contains("mod scoping"), "AB-0015 layout gate: lib.rs declares `mod scoping` again");
    for (m, _) in MODULES.iter().filter(|(m, _)| !m.contains('/')) {
        let name = m.trim_end_matches(".rs");
        assert!(lib.lines().any(|l| l.trim().trim_start_matches("pub ").trim_start_matches("pub(crate) ") == format!("mod {name};")),
            "AB-0015 layout gate: lib.rs must declare `mod {name};`");
    }
}

#[test]
fn modules_name_their_upstream_file_and_stay_small() {
    for (m, upstream) in MODULES {
        let p = src().join(m);
        let text = fs::read_to_string(&p).unwrap_or_else(|_| panic!("AB-0015 layout gate: missing src/{m}"));
        let header: String = text.lines().take_while(|l| l.starts_with("//!")).collect::<Vec<_>>().join(" ");
        assert!(header.contains(&format!("`{upstream}")),
            "AB-0015 layout gate: src/{m} header must name its upstream file `{upstream}`, got: {header}");
        let n = text.lines().count();
        assert!(n <= MAX_MODULE_LINES, "AB-0015 layout gate: src/{m} has {n} lines (> {MAX_MODULE_LINES}); split it per upstream");
    }
}

#[test]
fn plugin_code_lives_in_its_module() {
    let root = src();
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    let mut bad = Vec::new();
    for f in files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        for (i, l) in fs::read_to_string(&f).unwrap().lines().enumerate() {
            if l.trim() == "#[cfg(test)]" { break }
            if l.trim_start().starts_with("//") { continue }
            for (marker, owners) in OWNERS {
                if l.contains(marker) && !owners.split('|').any(|o| o == rel) && rel != "consts.rs" {
                    bad.push(format!("src/{rel}:{}: `{marker}` belongs in {owners}: {}", i + 1, l.trim()));
                }
            }
        }
    }
    assert!(bad.is_empty(), "AB-0015 layout gate: plugin code outside its upstream module\n  {}", bad.join("\n  "));
}
