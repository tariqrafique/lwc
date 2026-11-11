//! `@lwc/compiler` `src/transformers/transformer.ts` `transformFile`.
//! Extension is `Path::extension` (Node `path.extname` on a single path segment): the final
//! `.ext`, not a `rsplit` of the whole string. `.mts` is a script, same as upstream.
//! Anything else is LWC1005 `NO_AVAILABLE_TRANSFORMER` from the generated error info
//! (`strictLevel: Fatal` on the info; `generateCompilerError` throws `level: Error`).
use lwc_shared::errors::TransformerErrors;
use lwc_shared::{CompileOptions, CompileResult, CompilerError, DiagnosticLevel, TransformError};

pub(crate) fn transform_file(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, TransformError> {
    match extension(filename) {
        Some("css") => super::transform_style(src, filename, options),
        // HTML: SSR vs CSR. SSR errors stay unnormalized (transformer.ts:98-99).
        Some("html") if options.target_ssr => {
            lwc_ssr::transform(src, filename, options).map_err(super::unnormalized)
        }
        Some("html") => super::transform_template(src, filename, options),
        Some("tsx" | "jsx" | "ts" | "js" | "mts" | "mjs") if options.target_ssr => {
            lwc_ssr::transform(src, filename, options).map_err(super::ssr_js_error)
        }
        Some("tsx" | "jsx" | "ts" | "js" | "mts" | "mjs") => {
            super::transform_javascript(src, filename, options)
        }
        _ => Err(no_available_transformer(filename)),
    }
}

/// `path.extname` without the dot. `None` when the final segment has no extension
/// (`.gitignore`, `file.`, `dir.js/name`).
fn extension(filename: &str) -> Option<&str> {
    std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
}

/// `generateCompilerError(NO_AVAILABLE_TRANSFORMER, { messageArgs: [filename], origin: { filename } })`.
fn no_available_transformer(filename: &str) -> TransformError {
    let info = &TransformerErrors::NO_AVAILABLE_TRANSFORMER;
    debug_assert_eq!(info.strict_level, Some(DiagnosticLevel::Fatal));
    debug_assert_eq!(info.code, 1005);
    TransformError::Compiler(CompilerError::generate(info, &[filename], Some(filename)))
}
