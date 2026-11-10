//! S4 lint: codegen must never emit a placeholder `"undefined"` string literal
//! (AB-D0004 / AB-X0012). Identifiers named undefined are printed from the AST;
//! the binder may compare a name to that identifier (expression.ts:134).
use repo_tracked::{tracked_files_filtered, workspace_root};
use std::path::Path;

fn production_src(src: &str) -> String {
    // Drop `#[cfg(test)]` modules and line comments so test tables can mention the identifier.
    let mut out = String::new();
    let mut i = 0;
    let b = src.as_bytes();
    while i < b.len() {
        if src[i..].starts_with("#[cfg(test)]") {
            if let Some(rel) = src[i..].find("mod ") {
                let rest = &src[i + rel..];
                if let Some(brace) = rest.find('{') {
                    let mut depth = 0i32;
                    let mut j = brace;
                    let rb = rest.as_bytes();
                    while j < rb.len() {
                        match rb[j] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    i += rel + j + 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    continue;
                }
            }
        }
        if src[i..].starts_with("//") {
            i += src[i..].find('\n').unwrap_or(src[i..].len().saturating_sub(1)) + 1;
            continue;
        }
        let ch = src[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn stripped(src: &str) -> String {
    production_src(src).replace("== \"undefined\"", "").replace("\"undefined\" ==", "")
}

#[test]
fn codegen_has_no_undefined_string_literal() {
    let repo = workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")));
    let files = tracked_files_filtered(&repo, &["crates/lwc-template/src/codegen"], &["rs"]);
    let mut hits = Vec::new();
    for p in &files {
        let src = std::fs::read_to_string(p).unwrap();
        if stripped(&src).contains("\"undefined\"") {
            hits.push(p.display().to_string());
        }
    }
    assert!(hits.is_empty(), "forbidden placeholder string in codegen:\n{}", hits.join("\n"));
}

#[test]
fn guard_catches_planted_undefined_literal() {
    assert!(stripped("fn f() { let x = \"undefined\"; }\n").contains("\"undefined\""));
    assert!(!stripped("if name == \"undefined\" { return; }\n").contains("\"undefined\""));
    let skipped = "#[cfg(test)]\nmod tests {\n    fn f() { let x = \"undefined\"; }\n}\nfn g() {}\n";
    assert!(!stripped(skipped).contains("\"undefined\""), "{}", stripped(skipped));
}
