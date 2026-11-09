//! LWC style compiler (Rust port of `@lwc/style-compiler`).
//! Pipeline: parse (lossless tree) -> transforms -> serialize (JS module).
//!
//! Facade (PROGRAM.md): [`transform`] takes [`CompileOptions`] / returns [`CompileResult`].

pub mod ast;
pub mod consts;
pub mod error;
pub mod parser;
pub mod selector;
pub mod serialize;

mod css_import;
mod dir_pseudo_class;
mod no_id_selectors;
mod postcss_lwc_plugin;
mod scope_at_rules;
mod selector_scoping;
mod utils;

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

impl StyleError {
    /// The error object `@lwc/style-compiler` throws, as `transformers/style.ts` catches it: PostCSS's
    /// `CssSyntaxError` (or a plain `Error`), `name` / `message` / `line` / `column`; an `AggregateError`
    /// in recovery mode. The lwc-compiler style facade normalizes it against CSS_TRANSFORMER_ERROR.
    pub fn thrown(&self, filename: &str) -> CompileError {
        let (name, line, column) = match self {
            StyleError::Syntax { name, line, column, .. } => (name.as_str(), *line, *column),
            StyleError::Aggregate(_) => ("AggregateError", None, None),
        };
        // `transform_with_config` already threw any cwd failure for this filename (as JS `new Input`
        // does); hitting it here means the cwd vanished mid-call, so report that throw's text.
        let message = match postcss_file(filename) {
            Ok(file) => self.postcss_message(&file),
            Err(cwd) => cwd.postcss_message(filename),
        };
        CompileError { message, filename: None, diagnostics: vec![], name: Some(name.into()), line, column }
    }
}

/// PostCSS `Input#file` for `from: filename`: kept as-is when absolute or URL-like (`/^\w+:\/\//`),
/// else Node `path.resolve(filename)` against the process cwd, which throws when `process.cwd()`
/// does (e.g. `ENOENT: no such file or directory, uv_cwd` once the cwd is deleted).
fn postcss_file(filename: &str) -> Result<String, StyleError> {
    let url_like = filename.find("://").is_some_and(|i| i > 0 && filename[..i].chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    if url_like || filename.starts_with('/') {
        return Ok(filename.to_string());
    }
    let cwd = std::env::current_dir().map_err(uv_cwd_error)?;
    let joined = format!("{}/{filename}", cwd.to_string_lossy());
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => { parts.pop(); }
            s => parts.push(s),
        }
    }
    Ok(format!("/{}", parts.join("/")))
}

/// Node `process.cwd()`'s libuv error (`{code}: {uv_strerror}, uv_cwd`), a plain `Error`.
fn uv_cwd_error(e: std::io::Error) -> StyleError {
    let message = match e.kind() {
        std::io::ErrorKind::NotFound => "ENOENT: no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "EACCES: permission denied".to_string(),
        _ => e.to_string(),
    };
    StyleError::error(format!("{message}, uv_cwd"))
}

/// Fixture / internal entry (StyleError). Prefer [`transform`] from the facade.
pub fn transform_with_config(
    src: &str,
    id: &str,
    config: Config,
) -> Result<String, StyleError> {
    if src.is_empty() {
        return Ok("export default undefined".into());
    }
    // `postcss().process(src, { from: id })`: `new Input` resolves `from` before parsing, and that
    // throw is a plain Error, so recovery mode does not wrap it.
    postcss_file(id)?;
    let mut root = match parser::parse(src) {
        Ok(r) => r,
        Err(e) if config.experimental_error_recovery_mode => return Err(StyleError::Aggregate(vec![e])),
        Err(e) => return Err(e),
    };
    postcss_lwc_plugin::scope_root(&mut root, &config, src)?;
    Ok(serialize::serialize(&root, &config)?)
}

/// CompilerBackend seam — `lwc-compiler` calls this for `.css` and surfaces the error
/// (`transformers/style.ts`). Phase 0: `map == Some("")`.
pub fn transform(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, StyleError> {
    transform_with_config(src, filename, Config::from(options)).map(|code| CompileResult {
        code,
        map: Some(String::new()),
        ..Default::default()
    })
}
