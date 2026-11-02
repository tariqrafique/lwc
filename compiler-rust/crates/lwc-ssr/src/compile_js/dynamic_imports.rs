//! Port of ssr-compiler compile-js/index.ts `ImportExpression` visitor.
//!
//! `transformSync` always normalizes `dynamicImports` (strictSpecifier defaults to true), so the
//! visitor's `!dynamicImports` early return is unreachable from the napi seam and not modeled.
use lwc_shared::{CompileError, CompilerDiagnostic, DiagnosticLevel, SourceLocation};
use oxc::allocator::Allocator;
use oxc::ast::ast::{Expression, ImportExpression};
use oxc::ast_visit::{walk, Visit};
use oxc::span::SourceType;

/// The `source` of one `import(source)`, as the visitor's checks see it.
enum Specifier {
    /// `is.literal(source) && typeof value === 'string'`.
    String,
    /// Any other literal: `String(source.value)`.
    OtherLiteral(String),
    /// Not a literal (the message arg is `'undefined'`).
    NonLiteral,
}

/// `import()` sites in source order: (span start = the `import` keyword, specifier kind).
fn import_expressions(src: &str, source_type: SourceType) -> Option<Vec<(u32, Specifier)>> {
    struct V(Vec<(u32, Specifier)>);
    impl<'a> Visit<'a> for V {
        fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
            let spec = match &it.source {
                Expression::StringLiteral(_) => Specifier::String,
                Expression::NumericLiteral(n) => Specifier::OtherLiteral(js_number(n.value)),
                Expression::BooleanLiteral(b) => Specifier::OtherLiteral(b.value.to_string()),
                Expression::NullLiteral(_) => Specifier::OtherLiteral("null".to_string()),
                Expression::BigIntLiteral(b) => Specifier::OtherLiteral(b.value.to_string()),
                Expression::RegExpLiteral(r) => Specifier::OtherLiteral(format!("/{}/{}", r.regex.pattern.text, r.regex.flags)),
                _ => Specifier::NonLiteral,
            };
            self.0.push((it.span.start, spec));
            walk::walk_import_expression(self, it);
        }
    }
    let alloc = Allocator::default();
    let ret = oxc::parser::Parser::new(&alloc, src, source_type).parse();
    // Recoverable diagnostics (e.g. duplicate `export default`, accepted by the JS parser) still yield an AST.
    if ret.fatal_error {
        return None;
    }
    let mut v = V(Vec::new());
    v.visit_program(&ret.program);
    Some(v.0)
}

fn js_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e21 { format!("{}", n as i64) } else { format!("{n}") }
}

fn line_col(src: &str, offset: u32) -> (u32, u32) {
    let before = &src[..offset as usize];
    let line = before.matches('\n').count() as u32 + 1;
    let col = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) as u32;
    (line, col)
}

/// `dynamicImports.strictSpecifier`: every `import(source)` must have a string-literal source,
/// else LWC1121 INVALID_DYNAMIC_IMPORT_SOURCE_STRICT at the ImportExpression's location.
pub(super) fn check_strict_specifiers(src: &str, filename: &str, strict: bool) -> Option<CompileError> {
    if !strict {
        return None;
    }
    let source_type = SourceType::from_path(filename).unwrap_or_else(|_| SourceType::mjs());
    let sites = import_expressions(src, source_type)?;
    let (start, arg) = sites.into_iter().find_map(|(start, s)| match s {
        Specifier::String => None,
        Specifier::OtherLiteral(v) => Some((start, v)),
        Specifier::NonLiteral => Some((start, "undefined".to_string())),
    })?;
    let (line, column) = line_col(src, start);
    let message = format!(
        "LWC1121: Invalid import. The argument \"{arg}\" must be a stringLiteral for dynamic imports when strict mode is enabled."
    );
    let location = SourceLocation { start_line: line, start_column: column, ..Default::default() };
    Some(CompileError {
        message: message.clone(),
        filename: Some(filename.into()),
        diagnostics: vec![CompilerDiagnostic {
            code: Some(1121),
            message,
            level: DiagnosticLevel::Error,
            filename: Some(filename.into()),
            location: Some(location),
        }],
        name: None,
        line: Some(line),
        column: Some(column),
    })
}

/// `dynamicImports.loader`: `import(source)` → `__lwcLoad(source)` plus one
/// `import { load as __lwcLoad } from '<loader>'` (importManager.add dedupes).
pub(super) fn rewrite_with_loader(src: &str, loader: &str) -> String {
    if loader.is_empty() {
        return src.to_string();
    }
    let Some(sites) = import_expressions(src, SourceType::mjs()) else { return src.to_string() };
    if sites.is_empty() {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len() + 64);
    let mut last = 0usize;
    for (start, _) in &sites {
        let s = *start as usize;
        out.push_str(&src[last..s]);
        out.push_str("__lwcLoad");
        last = s + "import".len();
    }
    out.push_str(&src[last..]);
    format!("import {{ load as __lwcLoad }} from '{loader}';\n{out}")
}

#[cfg(test)]
mod tests {
    use super::*;
    const SRC: &str = "\nimport { LightningElement } from 'lwc';\nexport default class extends LightningElement {}\nexport default async function rando () {\n  await import(X);\n}\n";
    fn with(x: &str) -> String { SRC.replace('X', x) }

    #[test]
    fn strict_rejects_non_string_specifiers_at_the_import_line() {
        let e = check_strict_specifiers(&with("woohoo"), "c.js", true).unwrap();
        assert_eq!((e.diagnostics[0].code, e.line), (Some(1121), Some(5)));
        assert!(e.message.contains("\"undefined\""), "{}", e.message);
        let e = check_strict_specifiers(&with("123"), "c.js", true).unwrap();
        assert!(e.message.contains("\"123\""), "{}", e.message);
        assert!(check_strict_specifiers(&with("'x/foo'"), "c.js", true).is_none());
        assert!(check_strict_specifiers(&with("woohoo"), "c.js", false).is_none());
    }

    #[test]
    fn loader_rewrites_every_import_call_and_imports_load_once() {
        let src = "async function f() { await import('x/foo'); await import('x/bar'); const s = 'import(x)'; }";
        let out = rewrite_with_loader(src, "myLoader");
        assert_eq!(out.matches("from 'myLoader'").count(), 1);
        assert_eq!(out.matches("__lwcLoad('x/").count(), 2);
        assert!(out.contains("'import(x)'"), "string contents must not be rewritten: {out}");
        assert_eq!(rewrite_with_loader(src, ""), src);
    }
}
