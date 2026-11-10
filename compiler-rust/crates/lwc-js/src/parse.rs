//! The one place lwc-js runs oxc's diagnostic producers (parser, semantic checker, transformer) and the one
//! boundary where an oxc diagnostic becomes a [`CompileError`]: [`first_error`]. Nothing else in lwc-js sees
//! an oxc diagnostic value (gate: tests/oxc_diagnostic_boundary.rs), so no code can key behavior on oxc's
//! message text (D-0248: the text passes through, never compared).
use lwc_shared::location::LineIndex;
use lwc_shared::{CompileError, CompilerDiagnostic, DiagnosticLevel, DiagnosticLocation};
use oxc::allocator::Allocator;
use oxc::ast::ast::{ExportDefaultDeclarationKind, Program, Statement};
use oxc::parser::{ParseOptions, Parser};
use oxc::semantic::{Scoping, Semantic, SemanticBuilder};
use oxc::span::SourceType;
use oxc::transformer::{TransformOptions, Transformer};
use std::cell::Cell;
use std::path::Path;

thread_local! {
    static PARSE_COUNT: Cell<u32> = const { Cell::new(0) };
}

/// Test hook: oxc `Parser::new` invocations since the last [`reset_parse_count`].
pub fn reset_parse_count() { PARSE_COUNT.set(0); }
/// See [`reset_parse_count`].
pub fn parse_count() -> u32 { PARSE_COUNT.get() }

/// Shipped `scriptTransform` runs `babel.transformSync` with no TypeScript/JSX plugin (the LWC plugin adds
/// only `classProperties` + `decorators`), and Babel never infers syntax from the filename. So every script
/// (`.js`, `.ts`, `.jsx`, `.tsx`, `.mjs`) parses as plain ES module JS: TS/JSX syntax is a parse error.
pub fn source_type() -> SourceType {
    SourceType::mjs()
}

/// The user-source parse (pipeline step 1). Always: (1) parse as plain JS, (2) oxc semantic early-error
/// check (`SemanticBuilder::with_check_syntax_error(true)`), (3) the `export default abstract class` AST
/// guard. There is no recover branch for either check (D-0259: never compile an ECMAScript early error;
/// the abstract guard also runs so a named `export default abstract class A {}` is a syntax error here
/// rather than an internal re-parse failure). D-0262 dropped the unused recover parameter.
///
/// Intermediate re-parses of span-edited source use [`parse`], which does not run the semantic check
/// and is never pointed at codegen output (D-0262 / AB-L0037).
///
/// A syntax error is shipped's Babel parser `SyntaxError`, wrapped as LWC1007 by @lwc/compiler. D-0248: the class
/// (`SyntaxError`), the code, the filename and a 1-based line / 0-based column at oxc's FIRST label are at parity;
/// the message text and the exact position are oxc's (an accepted divergence; Babel's prose is not emulated).
pub fn parse_source<'a>(alloc: &'a Allocator, src: &'a str, filename: &str) -> Result<Program<'a>, CompileError> {
    let program = parse(alloc, src, filename)?;
    let sem = SemanticBuilder::new().with_check_syntax_error(true).build(&program);
    if let Some(e) = first_error(src, filename, &sem.diagnostics) { return Err(e); }
    if let Some(at) = default_export_abstract_class(&program) {
        // oxc's parser accepts `export default abstract class` in JS (it rejects every other `abstract class`
        // form); Babel reads `abstract` as an expression and throws at its end. Runs in both modes so recovery
        // cannot print `abstract` and then die on an intermediate re-parse (D-0245 for the named form 9.5.3 recovers).
        return Err(syntax_error(src, filename, at + "abstract".len(), ABSTRACT_CLASS));
    }
    Ok(program)
}

/// LWC-owned text for the one TypeScript-only form oxc's JS parser accepts (not Babel's "Missing semicolon.").
const ABSTRACT_CLASS: &str = "'abstract' classes are TypeScript syntax; LWC compiles JavaScript only.";

/// Re-parse of lwc-js's own intermediate span-edited source (never codegen output; D-0262 / AB-L0037):
/// parser errors only, no semantic early-error check (that check is [`parse_source`] only, D-0259 / Arch).
/// See [`source_type`].
pub fn parse<'a>(alloc: &'a Allocator, src: &'a str, filename: &str) -> Result<Program<'a>, CompileError> {
    parse_with(alloc, src, filename, ParseOptions::default())
}

/// [`parse`] with explicit oxc parser options.
pub fn parse_with<'a>(alloc: &'a Allocator, src: &'a str, filename: &str, options: ParseOptions) -> Result<Program<'a>, CompileError> {
    PARSE_COUNT.with(|n| n.set(n.get() + 1));
    let ret = Parser::new(alloc, src, source_type()).with_options(options).parse();
    match first_error(src, filename, &ret.diagnostics) {
        Some(e) => Err(e),
        None => Ok(ret.program),
    }
}

/// Scope tree of a parsed program (no syntax checks).
pub fn scoping(program: &Program<'_>) -> Scoping {
    SemanticBuilder::new().build(program).semantic.into_scoping()
}

/// Semantic model with the AST node table (no syntax checks).
pub fn semantic_with_nodes<'a>(program: &'a Program<'a>) -> Semantic<'a> {
    SemanticBuilder::new().with_build_nodes(true).build(program).semantic
}

/// Runs oxc's transformer over `program` (source `src`); its first error becomes a [`CompileError`].
pub fn transform<'a>(alloc: &'a Allocator, src: &str, filename: &str, options: &TransformOptions, scoping: Scoping, program: &mut Program<'a>) -> Result<(), CompileError> {
    let ret = Transformer::new(alloc, Path::new(filename), options).build_with_scoping(scoping, program);
    match first_error(src, filename, &ret.diagnostics) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// THE oxc-diagnostic boundary (D-0248). oxc's first error: its message text unchanged, its first label's
/// offset as the position. Every oxc error carries a span; one without a label is an oxc/lwc-js bug, not a
/// position 0. No branching on the diagnostic beyond "is there one" and "has it a label".
fn first_error(src: &str, filename: &str, diagnostics: &oxc::diagnostics::Diagnostics) -> Option<CompileError> {
    let d = diagnostics.errors().next()?;
    let Some(label) = d.labels.as_ref().first() else {
        return Some(CompileError::new(format!("internal: oxc error without a label: {d}"), filename));
    };
    Some(syntax_error(src, filename, label.offset() as usize, &d.message))
}

/// Babel parser `SyntaxError`: `{file}: {text} ({line}:{col})` plus @babel/core's code frame (start only, after
/// a blank line). `err.loc` is a Position `{ line, column, index }` (index in UTF-16 units, no start/length);
/// normalizeToCompilerError copies it as the CompilerError location.
fn syntax_error(src: &str, filename: &str, at: usize, text: &str) -> CompileError {
    assert!(src.is_char_boundary(at), "syntax error offset {at} is not a position in the source");
    let (line, column) = lwc_shared::babel_line_col(src, at as u32);
    let frame = lwc_shared::code_frame(src, (line, column), None);
    let message = format!("{filename}: {text} ({line}:{column})\n\n{frame}");
    let index = LineIndex::new(src).utf16_offset(at as u32);
    let location = DiagnosticLocation { line, column, start: None, length: None, index: Some(index) };
    CompileError::syntax(message.clone(), filename, vec![CompilerDiagnostic::new(None, message, DiagnosticLevel::Error, Some(filename.into()), Some(location))])
}

/// Start of `export default abstract class ..` (the class span starts at `abstract`).
fn default_export_abstract_class(program: &Program<'_>) -> Option<usize> {
    program.body.iter().find_map(|s| match s {
        Statement::ExportDefaultDeclaration(d) => match &d.declaration {
            ExportDefaultDeclarationKind::ClassDeclaration(c) if c.r#abstract => Some(c.span.start as usize),
            _ => None,
        },
        _ => None,
    })
}
