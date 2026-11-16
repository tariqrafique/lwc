//! Input `sourceMappingURL` comments are dropped the way `@babel/core` normalize-file drops them
//! (INLINE_SOURCEMAP_REGEX first; EXTERNAL_SOURCEMAP_REGEX only when no inline map decoded), and
//! every other comment, including the eslint/istanbul directives, survives as in the oracle.
//! Each fixture's output comment list is compared with the oracle `@lwc/compiler` output.
use lwc_shared::CompileOptions;
use std::process::Command;

fn opts() -> CompileOptions {
    CompileOptions {
        name: "foo".into(),
        namespace: "x".into(),
        ..Default::default()
    }
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}.js",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Comment texts in source order, delimiters included.
fn comments(code: &str) -> Vec<String> {
    let alloc = oxc::allocator::Allocator::default();
    let ret = oxc::parser::Parser::new(&alloc, code, oxc::span::SourceType::mjs()).parse();
    assert_eq!(
        ret.diagnostics.errors().count(),
        0,
        "output does not parse:\n{code}"
    );
    ret.program
        .comments
        .iter()
        .map(|c| code[c.span.start as usize..c.span.end as usize].to_string())
        .collect()
}

fn oracle_code(src: &str) -> String {
    let root = std::env::var("LWC_ORACLE").expect("LWC_ORACLE");
    let script = format!(
        r#"
import {{ transformSync }} from '{root}/packages/@lwc/compiler/dist/index.js';
const r = transformSync({src:?}, 'foo.js', {{ name: 'foo', namespace: 'x' }});
process.stdout.write(r.code);
"#
    );
    let out = Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .output()
        .expect("node");
    assert!(
        out.status.success(),
        "oracle failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn check(name: &str, expected: &[&str]) {
    let src = fixture(name);
    let rust = lwc_js::transform(&src, "foo.js", &opts()).unwrap().code;
    let oracle = oracle_code(&src);
    assert_eq!(
        comments(&rust),
        comments(&oracle),
        "{name}: comment lists differ\n--- rust ---\n{rust}\n--- oracle ---\n{oracle}"
    );
    // Pin the expectation too, so a changed oracle shows up here rather than silently.
    assert_eq!(comments(&rust), expected, "{name}:\n{rust}");
}

#[test]
fn trailing_mid_file_and_end_sourcemap_comments_are_dropped() {
    check(
        "sourcemap-comments",
        &[
            "// eslint-disable-line no-console",
            "// # sourceMappingURL=not-a-directive.js.map",
            "//# sourceMappingURL=two words",
            "// istanbul ignore next",
            "/*LWC compiler v9.5.3*/",
        ],
    );
}

#[test]
fn a_decoded_inline_map_keeps_the_external_comments() {
    check(
        "sourcemap-comments-inline",
        &["//# sourceMappingURL=kept-because-the-inline-map-decoded.js.map"],
    );
}

#[test]
fn a_failed_inline_map_still_drops_the_external_comments() {
    check("sourcemap-comments-bad-inline", &[]);
}
