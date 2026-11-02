//! LWC style compiler (Rust port of `@lwc/style-compiler`).
//! Pipeline: parse (lossless tree) -> transforms -> serialize (JS module).
//!
//! Facade (PROGRAM.md): [`transform`] takes [`CompileOptions`] / returns [`CompileResult`].

pub mod ast;
pub mod consts;
pub mod error;
pub mod parser;
pub mod scoping;
pub mod selector;
pub mod serialize;
pub mod syntax_check;

pub use error::StyleError;
pub use lwc_shared::{CompileError, CompileOptions, CompileResult};

/// Internal options for transforms / fixture harness.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub scoped: bool,
    pub disable_synthetic_shadow_support: bool,
    pub experimental_error_recovery_mode: bool,
    pub api_version: Option<u32>,
}

impl From<&CompileOptions> for Config {
    fn from(o: &CompileOptions) -> Self {
        Self {
            scoped: o.scoped_styles.unwrap_or(false),
            disable_synthetic_shadow_support: o.disable_synthetic_shadow_support,
            experimental_error_recovery_mode: o.experimental_error_recovery_mode,
            api_version: o.api_version,
        }
    }
}

/// `@lwc/compiler` `transformers/style.ts` + `@lwc/errors` `normalizeToCompilerError`, as shipped 9.5.3 throws:
/// - single: `CompilerError` named `Error`, code 1009, message `{e.name}: LWC1009: {postcss message}`, `location`
///   = the CssSyntaxError's `{line, column}` (no start/length), `filename` = the filename passed in.
/// - recovery (`AggregateError`): `CompilerAggregateError('Multiple CSS errors occurred during compilation.')`
///   whose `errors[]` are the single shape above, one per CssSyntaxError.
fn style_err_to_compile(err: StyleError, filename: &str) -> CompileError {
    match err {
        StyleError::Aggregate(errs) => {
            let diagnostics = errs.iter().map(|e| style_diagnostic(e, filename)).collect();
            CompileError {
                message: CSS_AGGREGATE_MESSAGE.into(),
                filename: Some(filename.into()),
                diagnostics,
                name: Some("AggregateError".into()),
                // JS CompilerAggregateError has no location on the outer error.
                line: None,
                column: None,
            }
        }
        e @ StyleError::Syntax { .. } => {
            let d = style_diagnostic(&e, filename);
            let (line, column) = match e { StyleError::Syntax { line, column, .. } => (line, column), _ => (None, None) };
            CompileError { message: d.message.clone(), filename: Some(filename.into()), diagnostics: vec![d], name: Some("Error".into()), line, column }
        }
    }
}

/// `CompilerAggregateError` message thrown by `@lwc/compiler` `transformers/style.ts` in recovery mode.
pub const CSS_AGGREGATE_MESSAGE: &str = "Multiple CSS errors occurred during compilation.";

/// One CssSyntaxError as `normalizeToCompilerError(CSS_TRANSFORMER_ERROR, e, { filename })` builds it.
fn style_diagnostic(e: &StyleError, filename: &str) -> lwc_shared::CompilerDiagnostic {
    let (name, line, column) = match e {
        StyleError::Syntax { name, line, column, .. } => (name.as_str(), *line, *column),
        StyleError::Aggregate(_) => ("AggregateError", None, None),
    };
    let css_message = e.postcss_message(&postcss_file(filename));
    // getLocationFromObject: `{line, column}` only when both are integers. JS CSS errors carry no
    // offsets, so start/end stay absent rather than faked; the napi seam emits line/column only.
    let location = match (line, column) {
        (Some(l), Some(c)) => Some(lwc_shared::SourceLocation { start_line: l, start_column: c, end_line: l, end_column: c, start: None, end: None }),
        _ => None,
    };
    let origin = lwc_shared::DiagnosticOrigin { filename: Some(filename.into()), location };
    let mut d = lwc_shared::CompilerDiagnostic::generate(&lwc_shared::CSS_TRANSFORMER_ERROR, Some(&[css_message.as_str()]), Some(origin), false);
    // normalizeToCompilerError: `${error.name}: ${message}`.
    d.message = format!("{name}: {}", d.message);
    d
}

/// PostCSS `Input#file` for `from: filename`: kept as-is when absolute or URL-like (`/^\w+:\/\//`),
/// else Node `path.resolve(filename)` against the process cwd.
fn postcss_file(filename: &str) -> String {
    let url_like = filename.find("://").is_some_and(|i| i > 0 && filename[..i].chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    if url_like || filename.starts_with('/') {
        return filename.to_string();
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let joined = format!("{}/{filename}", cwd.to_string_lossy());
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => { parts.pop(); }
            s => parts.push(s),
        }
    }
    format!("/{}", parts.join("/"))
}

/// Fixture / internal entry (StyleError). Prefer [`transform`] from the facade.
pub fn transform_with_config(
    src: &str,
    _id: &str,
    config: Config,
) -> Result<String, StyleError> {
    if src.is_empty() {
        return Ok("export default undefined".into());
    }
    // PostCSS throws on input cssparser would auto-close (unclosed block/string/comment/bracket, ...).
    let mut root = match syntax_check::check(src).and_then(|()| parser::parse(src)) {
        Ok(r) => r,
        Err(e) if config.experimental_error_recovery_mode => return Err(StyleError::Aggregate(vec![e])),
        Err(e) => return Err(e),
    };
    scoping::scope_root(&mut root, &config, src)?;
    Ok(serialize::serialize(&root, &config))
}

/// CompilerBackend seam — `lwc-compiler` calls this for `.css`.
/// Phase 0: `map == Some("")`.
pub fn transform(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, CompileError> {
    match transform_with_config(src, filename, Config::from(options)) {
        Ok(code) => Ok(CompileResult {
            code,
            map: Some(String::new()),
            ..Default::default()
        }),
        Err(e) => Err(style_err_to_compile(e, filename)),
    }
}
