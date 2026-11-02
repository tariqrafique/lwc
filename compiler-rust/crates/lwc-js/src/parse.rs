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
        let before = &src[..at];
        let line = before.matches('\n').count() as u32 + 1;
        let col = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) as u32;
        let msg = first.message.to_string();
        return Err(CompileError::babel("SyntaxError", format!("{msg} ({line}:{col})"), filename, Some(line), Some(col)));
    }
    Ok(ret.program)
}
