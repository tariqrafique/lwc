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

fn style_err_to_compile(err: StyleError, filename: &str) -> CompileError {
    match err {
        StyleError::Syntax { name: _, reason, line, column } => CompileError {
            message: format!("lwc-style: {reason}"),
            filename: Some(filename.into()),
            diagnostics: vec![],
            // JS @lwc/compiler wraps CssSyntaxError as Error; keep expectedName via reason only.
            name: Some("Error".into()),
            line: Some(line),
            column: Some(column),
        },
        StyleError::Aggregate(errs) => {
            let reason = match errs.first() {
                Some(StyleError::Syntax { reason, .. }) => reason.clone(),
                _ => String::new(),
            };
            CompileError {
                message: format!("lwc-style: {reason}"),
                filename: Some(filename.into()),
                diagnostics: vec![],
                name: Some("AggregateError".into()),
                // JS AggregateError from compiler has no location on the outer error.
                line: None,
                column: None,
            }
        }
    }
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
    let mut root = match parser::parse(src) {
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
