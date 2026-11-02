# Error call-site form (AB-R003 / AB-0001, slice-1 API fixed at bot/js 1230144)

One table: `lwc_shared::errors::error_info` (generated from oracle 9.5.3 `@lwc/errors`; upstream group and
constant names). Import groups by name:

```rust
use lwc_shared::errors::error_info::{lwc_class::DecoratorErrors, lwc_class::LWCClassErrors, ssr::SsrCompilerErrors,
    template_transform::ParserDiagnostics, compiler::TransformerErrors};
// every group is also glob re-exported: lwc_shared::errors::DecoratorErrors::ONE_WIRE_DECORATOR_ALLOWED
```

## Signatures

```rust
// generateCompilerDiagnostic(info, { messageArgs, origin: { location } }) -- template diagnostics (level from table, no strict)
impl LwcErrorInfo { pub fn generate(&self, args: &[&str], location: SourceLocation) -> CompilerDiagnostic; }
impl LwcErrorInfo { pub fn format(&self, args: &[&str]) -> String; }          // templateString, no "LWCnnnn: " prefix

// generateCompilerError(info, { messageArgs, origin: { filename, location } }) -- unframed thrown error
impl CompileError {
    pub fn generate(info: &LwcErrorInfo, args: &[&str], filename: &str, location: Option<DiagnosticLocation>) -> Self;
    // same, at an oxc node span (byte offsets into src): location = Babel {line, column, start, length} (UTF-16),
    // and the diagnostic keeps the span so lwc-js can frame it (CompileError::framed) and sort.
    pub fn generate_at(info: &LwcErrorInfo, args: &[&str], filename: &str, src: &str, start: u32, end: u32) -> Self;
}

// typed throws (built by lwc-compiler facades; napi only serializes)
pub struct CompilerError { code: Option<u32>, message: String, filename: Option<String>,
    location: Option<DiagnosticLocation>, level: DiagnosticLevel, url: String }
CompilerError::from_diagnostic(&d, filename)            // CompilerError.from
CompilerError::generate(&info, &args, filename)          // validateOptions-style
normalize_to_compiler_error(&FALLBACK_INFO, e, filename) // normalizeToCompilerError
pub struct CompilerAggregateError { errors: Vec<CompilerError>, message: String }
pub enum TransformError { Compiler(CompilerError), Aggregate(CompilerAggregateError), Unnormalized(UnnormalizedError) }
```

## Location type

`lwc_shared::DiagnosticLocation { line: u32 /*1-based*/, column: u32 /*0-based UTF-16*/, start: Option<u32>,
length: Option<u32> /*UTF-16*/, index: Option<u32> /*Babel Position only*/ }` = `@lwc/errors` `Location`.
Diagnostics carry it in `CompilerDiagnostic.loc` (producer-reported) or derive it from the span
`CompilerDiagnostic.location: Option<SourceLocation>`; read it with `d.js_location()`.
The ONE byte-offset conversion: `lwc_shared::LineIndex::new(src)` -> `.location(start, end)` /
`.babel_line_col(off)` / `.utf16_offset(off)` / `.source_location(start, end)` / `.byte_offset(u16)`.

## Before / after

Template (`lwc-template`, slice 3; message keeps `LWCnnnn: `, line/column/level identical):
```rust
// before
messages::lwc(1058, &[name], loc)
// after
ParserDiagnostics::INVALID_HTML_SYNTAX.generate(&[name], loc)   // 1058; constant named after the code's upstream key
```

JS (`lwc-js`, slice 2):
```rust
// before
derr(1107, "{0} ...", &[name], span)  /  CompileError::lwc_at(1199, "Cannot ...", filename, src, s, e)
// after
CompileError::generate_at(&DecoratorErrors::PROPERTY_NAME_CANNOT_START_WITH_ON, &[name], filename, src, s, e)
```

SSR (`lwc-ssr`, slice 2; no location where there was none):
```rust
// before
CompileError::lwc(1105, "Method or property can only have 1 @wire decorator", filename, None, None)
// after
CompileError::generate(&DecoratorErrors::ONE_WIRE_DECORATOR_ALLOWED, &[], filename, None)
```

## Notes (verified against the generated table at 1230144)
- Names in the examples exist: ParserDiagnostics::INVALID_HTML_SYNTAX = 1058, DecoratorErrors::PROPERTY_NAME_CANNOT_START_WITH_ON = 1108.
- `lwc_shared::babel_line_col` remains as a delegate to `LineIndex::babel_line_col`; it now treats lone `\r`, U+2028 and U+2029 as line breaks (Babel-faithful; old impl split on `\n` only).
- Slice 1 migrated no crate; remaining copies are pinned by shrink-only allowlists in crates/lwc-shared/tests/error_literal_gate.rs.
