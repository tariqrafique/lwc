use lwc_shared::CompileError;
use oxc::allocator::Allocator;
use oxc::ast::ast::Program;
use oxc::parser::Parser;
use oxc::span::SourceType;

/// Parses as an ES module; TS/JSX inferred from extension (.ts/.tsx/.jsx), decorators accepted.
pub fn parse<'a>(alloc: &'a Allocator, src: &'a str, filename: &str) -> Result<Program<'a>, CompileError> {
    let st = SourceType::from_path(filename).unwrap_or_else(|_| SourceType::mjs()).with_module(true);
    let ret = Parser::new(alloc, src, st).parse();
    if ret.diagnostics.has_errors() {
        // Shipped: Babel's parser SyntaxError (`msg (line:col)`, 1-based line, 0-based column), wrapped as LWC1007.
        let first = ret.diagnostics.errors().next().expect("has_errors");
        let at = first.labels.as_ref().first().map(|l| l.offset() as usize).unwrap_or(0).min(src.len());
        let (line, col) = lwc_shared::babel_line_col(src, at as u32);
        let msg = first.message.to_string();
        let mut e = CompileError::babel("SyntaxError", format!("{msg} ({line}:{col})"), filename, Some(line), Some(col));
        // Babel's `err.loc` is a Position `{ line, column, index }` (UTF-16 index); normalizeToCompilerError
        // copies it as the CompilerError location. Zero-width here; napi shapes a Babel 1007 location as
        // `{ line, column, index }` (index = start).
        let index = src[..at].encode_utf16().count() as u32;
        e.diagnostics[0].location = Some(lwc_shared::SourceLocation { start_line: line, start_column: col, end_line: line, end_column: col, start: index, end: index });
        return Err(e);
    }
    Ok(ret.program)
}
#[cfg(test)]
mod tmp_probe {
    #[test]
    fn tmp_probe_diags() {
        for src in ["let = ;", "let x = ;", "a b", "class {", "function (", "}", "const x = 1;\nconst y = 2 +;\n", "export default class { @api x: number; }\n", "var 😀 = 1;", "const s = \"é\"; let = ;"] {
            let alloc = oxc::allocator::Allocator::default();
            let ret = oxc::parser::Parser::new(&alloc, src, oxc::span::SourceType::mjs()).parse();
            let v: Vec<_> = ret.diagnostics.errors().map(|d| (d.message.to_string(), d.labels.as_ref().iter().map(|l| l.offset()).collect::<Vec<_>>())).collect();
            eprintln!("{src:?} => {v:?}");
        }
    }
}
