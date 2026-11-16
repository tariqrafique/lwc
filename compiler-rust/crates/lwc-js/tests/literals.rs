//! D-0292: source number, string, and BigInt literals print as written.
//! Fixture is the census probe `probes/literals.js`. The oracle is
//! `@lwc/compiler` transformSync at the pinned LWC_ORACLE. Literal `raw`
//! sequences must match. Synthesized strings (no source `raw`) stay on the
//! D-0243 quoter and are not required to echo the probe.
use lwc_shared::CompileOptions;
use oxc::allocator::Allocator;
use oxc::ast::ast::{BigIntLiteral, NumericLiteral, RegExpLiteral, StringLiteral, TemplateLiteral};
use oxc::ast_visit::{Visit, walk};
use oxc::parser::Parser;
use oxc::span::SourceType;
use std::process::Command;

fn opts() -> CompileOptions {
    CompileOptions {
        name: "literals".into(),
        namespace: "x".into(),
        ..Default::default()
    }
}

fn oracle_code(src: &str) -> String {
    let root = std::env::var("LWC_ORACLE").expect("LWC_ORACLE");
    let script = format!(
        r#"
import {{ transformSync }} from '{root}/packages/@lwc/compiler/dist/index.js';
const r = transformSync({src:?}, 'literals.js', {{ name: 'literals', namespace: 'x' }});
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

struct Lits {
    out: Vec<String>,
}

impl<'a> Visit<'a> for Lits {
    fn visit_numeric_literal(&mut self, it: &NumericLiteral<'a>) {
        if let Some(raw) = it.raw.as_deref() {
            self.out.push(raw.to_string());
        }
        walk::walk_numeric_literal(self, it);
    }
    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        if let Some(raw) = it.raw.as_deref() {
            self.out.push(raw.to_string());
        }
        walk::walk_string_literal(self, it);
    }
    fn visit_big_int_literal(&mut self, it: &BigIntLiteral<'a>) {
        if let Some(raw) = it.raw.as_deref() {
            self.out.push(raw.to_string());
        }
        walk::walk_big_int_literal(self, it);
    }
    fn visit_reg_exp_literal(&mut self, it: &RegExpLiteral<'a>) {
        if let Some(raw) = it.raw.as_deref() {
            self.out.push(raw.to_string());
        }
        walk::walk_reg_exp_literal(self, it);
    }
    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        for quasi in &it.quasis {
            self.out.push(quasi.value.raw.to_string());
        }
        walk::walk_template_literal(self, it);
    }
}

fn literal_raws(code: &str) -> Vec<String> {
    let alloc = Allocator::default();
    let parsed = Parser::new(&alloc, code, SourceType::mjs()).parse();
    let errors: Vec<_> = parsed.diagnostics.errors().collect();
    assert!(errors.is_empty(), "parse: {errors:?}");
    let mut lits = Lits { out: Vec::new() };
    lits.visit_program(&parsed.program);
    lits.out
}

#[test]
fn source_literals_match_the_oracle() {
    let src = include_str!("fixtures/literals.js");
    let rust = lwc_js::transform(src, "literals.js", &opts())
        .expect("rust transform")
        .code;
    let oracle = oracle_code(src);
    let rust_lits = literal_raws(&rust);
    let oracle_lits = literal_raws(&oracle);
    // Source tokens only. The `lwc` import is rewritten into a synthesized
    // specifier (D-0243 quoter on the Rust side, source quotes on the oracle);
    // that specifier is not a printed source literal.
    let from_source = |lits: &[String]| -> Vec<String> {
        lits.iter()
            .filter(|raw| src.contains(raw.as_str()))
            .filter(|raw| raw.as_str() != "'lwc'" && raw.as_str() != "\"lwc\"")
            .cloned()
            .collect()
    };
    let rust_src = from_source(&rust_lits);
    let oracle_src = from_source(&oracle_lits);
    assert_eq!(
        rust_src, oracle_src,
        "source literal raws differ\n--- rust ---\n{rust}\n--- oracle ---\n{oracle}"
    );
    for spelling in [
        "10000",
        "0xFF",
        "0xff",
        "0o17",
        "0b1010",
        "1_000_000",
        "1E21",
        "1.0",
        "0.50",
        ".5",
        "0.0001",
        "123456789012345680000",
        "0xFFn",
        "1_000n",
        "'single'",
        r"'\u00e9\x41\n\t'",
        r"'\u{1F600}'",
    ] {
        assert!(
            rust_src.iter().any(|s| s == spelling),
            "rust output dropped source spelling {spelling}\n{rust}"
        );
    }
}
