# lwc-style

Rust port of `@lwc/style-compiler` (cssparser-based lossless tree + PostCSS-compatible stringify).

Modules: `parser` (cssparser -> `ast` lossless tree with raws) · `scoping` (selector transforms) · `serialize` (port of serialize.ts) · `consts` · `error` (CssSyntaxError/AggregateError mirror).

API: `transform(src, filename, &CompileOptions) -> Result<CompileResult, CompileError>` (facade, map=Some("")),
`transform_with_config(src, id, Config) -> Result<String, StyleError>` (fixtures).

## Fixtures
    cargo test -p lwc-style --test fixtures
    LWC_FIXTURE=keyframes LWC_STYLE_DIFF=1 cargo test -p lwc-style --test fixtures   # one fixture, show diff
    LWC_STYLE_STRICT=1 ...   # exit 1 on any failure (CI gate)

Current: **69/69** passing.
