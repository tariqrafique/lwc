# lwc_shared diagnostic helper (mirror of @lwc/errors `generateCompilerDiagnostic`)

Crate: `crates/lwc-shared/src/lib.rs`. This adds to the crate without changing it: the `CompilerDiagnostic` struct
is untouched, so existing struct-literal and struct-update call sites still compile.

## API

```rust
pub struct LwcErrorInfo {            // one @lwc/errors LWCErrorInfo entry
    pub code: u32,
    pub message: &'static str,       // template with {0}, {1}, ...
    pub level: DiagnosticLevel,
    pub url: Option<&'static str>,   // not carried on CompilerDiagnostic (JS puts it in `url`, never in the message)
    pub strict_level: Option<DiagnosticLevel>,
}
pub struct DiagnosticOrigin { pub filename: Option<String>, pub location: Option<SourceLocation> }   // Default
pub struct DiagnosticLocation { pub line: u32, pub column: u32, pub start: Option<u32>, pub length: Option<u32> }
impl From<SourceLocation> for DiagnosticLocation   // template-compiler normalizeLocation: line/column = start_*, length = end - start

pub fn generate_error_message(info: &LwcErrorInfo, args: Option<&[&str]>) -> String;
impl CompilerDiagnostic {
    pub fn generate(info: &LwcErrorInfo, args: Option<&[&str]>, origin: Option<DiagnosticOrigin>,
                    use_strict_error_override: bool) -> Self;
    pub fn js_location(&self) -> Option<DiagnosticLocation>;   // the JS {line, column, start, length} shape
}
```

How it matches JS:
- `generate_error_message`: with `args = Some(..)` (JS `Array.isArray(args)`) the result is `LWC{code}: ` followed by
  the template, filled in one pass by `/\{([0-9]+)\}/g`. A missing index becomes `"undefined"`, like JS
  `String(undefined)`. With `None` the message has no prefix and no substitution.
- level: `strict_level` when `use_strict_error_override && strict_level.is_some()`, else `level`.
- `filename` and `location` are set only when `origin` is given (JS `if (config?.origin)`).
- `generateCompilerDiagnostic` itself never adds `filename` to a template error. The @lwc/compiler transform layer
  adds it. JS 1165 output for comparison:
  `{"message":"LWC1165: ...","code":1165,"filename":"x.html","location":{"line":1,"column":11,"start":10,"length":20},"level":1,"url":""}`.
  When a template diagnostic has no location, JS normalizeLocation gives zeros, not absent. Callers that need that
  must pass a zeroed `SourceLocation`.

## Example

```rust
use lwc_shared::{CompilerDiagnostic, DiagnosticLevel, DiagnosticOrigin, LwcErrorInfo};
const LWC_ELSE_NO_IF: LwcErrorInfo = LwcErrorInfo {
    code: 1165,
    message: "'{0}' directive must be used immediately after an element with 'lwc:if' or 'lwc:elseif'. No such element found.",
    level: DiagnosticLevel::Error, url: None, strict_level: None,
};
let d = CompilerDiagnostic::generate(&LWC_ELSE_NO_IF, Some(&["lwc:else"]),
    Some(DiagnosticOrigin { filename: None, location: Some(loc) }), false);
// d.message == "LWC1165: 'lwc:else' directive must be used ..."; d.js_location() == {line, column, start, length}
```

Tests in `diagnostic_code_tests`: 1165 error with location (length 20), 1182 warning with filename (length 36),
no origin and no args, single-pass templating, level mapping and strict override. `cargo test -p lwc-shared`: 11 passed.

## Call sites that build diagnostics without the prefix (not migrated; owned by lwc-template / lwc-ssr teams)

- `crates/lwc-template/src/parser/mod.rs`: helpers `warn` (~l.554, `CompilerDiagnostic { level: Warning, ..diag(..) }`),
  `diag` (~l.591), and `diag_opt` (~l.594, `CompilerDiagnostic::new(code, message, Error, None, Some(l))`).
  About 35 uses in mod.rs (for example 1182 at ~l.280 and 1165 at ~l.501-502).
- `crates/lwc-template/src/parser/validate.rs`: about 30 uses through `diag` / `warn` (1176, 1171, 1182, 1041, 1201, 1149, 1145, ...).
- `crates/lwc-template/src/parser/expr_check.rs`: `ExprError { code, message }` (1083, 1196), passed through `diag` in `check_expr`.
- `crates/lwc-template/src/parser/messages.rs`: the `MESSAGES` table (code, level, template), generated from
  template-transform.ts. It is the natural source of `LwcErrorInfo` constants.
- Where they surface: `crates/lwc-compiler/src/lib.rs` (~l.80-100) builds `CompileError` from the first error's
  `d.message` and `start_line`/`start_column`. `crates/lwc-ssr/src/lib.rs` (~l.98-140) does the same for uncoded messages.
- lwc-ssr `compile_js` uses `CompileError::lwc`, which already adds the prefix, so it has no gap. `CompileError::new` at
  compile_js/mod.rs:252 ("@wire cannot be used on computed properties in SSR context.") is uncoded in JS too, which is
  correct. The class_rewrite `map_err` (~l.1600) is uncoded.
- As instructed, no prefixing was added in lwc-compiler dispatch or napi.

## Known column mismatch

`<template><div lwc:else></div></template>`: JS reports 1165 at line 1, **column 11** (start 10, length 20). Rust
lwc-template reports **column 16**. The Rust parser anchors the location at the attribute, not the element start. This
is a lwc-template location issue. The helper reproduces whatever `SourceLocation` it is given.
