//! D-0292: comment attachment and blank lines match `@lwc/compiler` (Babel generator).
use lwc_shared::CompileOptions;
use std::process::Command;

fn opts() -> CompileOptions {
    CompileOptions {
        name: "c".into(),
        namespace: "x".into(),
        ..Default::default()
    }
}

fn rust_code(src: &str) -> String {
    lwc_js::transform(src, "c.js", &opts()).expect("rust").code
}

/// Babel indents with two spaces. The LWC printer indents with a tab. Comment
/// text, including a tab inside a block comment, stays as written.
fn canon(s: &str) -> String {
    let s = if s.ends_with('\n') {
        s.to_string()
    } else {
        format!("{s}\n")
    };
    let mut out = String::new();
    let mut in_block = false;
    for line in s.split_inclusive('\n') {
        if !in_block {
            let rest = line.trim_start_matches([' ', '\t']);
            let indent = &line[..line.len() - rest.len()];
            // A tab is one indent level (two spaces). Babel indents with spaces.
            let width = indent
                .chars()
                .fold(0usize, |n, c| n + if c == '\t' { 2 } else { 1 });
            for _ in 0..width {
                out.push(' ');
            }
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
        if line.contains("/*") && !line.contains("*/") {
            in_block = true;
        }
        if in_block && line.contains("*/") {
            in_block = false;
        }
    }
    out
}

fn oracle_code(src: &str) -> String {
    let root = std::env::var("LWC_ORACLE").expect("LWC_ORACLE");
    let script = format!(
        r#"
import {{ transformSync }} from '{root}/packages/@lwc/compiler/dist/index.js';
const r = transformSync({src:?}, 'c.js', {{ name: 'c', namespace: 'x' }});
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

fn check(src: &str) {
    let rust = canon(&rust_code(src));
    let oracle = canon(&oracle_code(src));
    assert_eq!(rust, oracle, "src:\n{src}");
}

#[test]
fn blank_line_before_a_leading_comment() {
    check("const a = 1;\n\n// lead\nconst b = 2;\n");
}

#[test]
fn no_blank_line_without_one_in_the_source() {
    check("const a = 1;\n// same\nconst b = 2;\n");
}

#[test]
fn trailing_line_comment_stays_on_its_statement() {
    check("const a = 1; // trail\nconst b = 2;\n");
}

#[test]
fn comment_in_an_empty_function_and_after_the_last_statement() {
    check("function f() {\n  // only\n}\n");
    check("function f() { /* inner */ }\n");
    check("function f() {\n  stmt;\n  // after\n}\n");
    check("function g() { /* a */ /* b */ }\n");
}

#[test]
fn object_and_array_inner_comments() {
    check("const o = { a: 1, /* t */ };\n");
    check("const o = {\n  a: 1,\n  // tail\n};\n");
    check("const o = { /* empty */ };\n");
    check("const a = [\n  1, // t\n];\n");
    check("const a = [/* e */];\n");
    check("const o = { a: 1 /* mid */ , b: 2 };\n");
    check("foo(\n  1, // c\n);\n");
}

#[test]
fn block_comment_text_is_verbatim() {
    check("/*\n\tlayout\n*/\nconst a = 1;\n");
    check("const a = 1;\n\n/* block */\n\nconst b = 2;\n");
}

#[test]
fn class_field_comments_travel_with_the_init() {
    check("class C {\n  // field\n  x = 1;\n}\n");
    check("class C {\n  x = 1; // trail\n}\n");
    check("class C {\n  x = 1; // a\n  y = 2; // b\n}\n");
    check("class C {\n  x /* i */ = 1;\n}\n");
    check("class C {\n  x = /* i */ 1;\n}\n");
    check("class C {\n  // s\n  static x = 1;\n}\n");
    check("class C {\n  static x = 1; // a\n  static y = 2; // b\n}\n");
    check("class C {\n  x = 1; // a\n  m() {}\n}\n");
    check("class C {\n  m() {}\n  x = 1; // a\n}\n");
    check("class C {\n  x = 1; // a\n  static y = 2;\n}\n");
    check("class C {\n  static y = 2; // a\n  x = 1;\n}\n");
}
