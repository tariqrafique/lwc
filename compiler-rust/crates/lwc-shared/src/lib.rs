//! Shared types for the LWC Rust compiler workspace.

pub mod api_version;
pub mod template_ir;
pub use template_ir::*;

/// Diagnostic severity matching `@lwc/errors` DiagnosticLevel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Log,
    Informative,
    Warning,
    Error,
    Fatal,
}

impl DiagnosticLevel {
    /// Numeric value of `@lwc/errors` DiagnosticLevel (Fatal=0, Error=1, Warning=2, Log=3).
    pub fn js_value(self) -> u32 {
        match self {
            DiagnosticLevel::Fatal => 0,
            DiagnosticLevel::Error => 1,
            DiagnosticLevel::Warning => 2,
            DiagnosticLevel::Log | DiagnosticLevel::Informative => 3,
        }
    }
}

/// Compiler diagnostic surfaced to harness / callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerDiagnostic {
    /// `@lwc/errors` code (`LWC{code}`); `None` for an uncoded diagnostic (TS plain `Error`, e.g. the
    /// SSR inner/outer-h-t-m-l reject). Never `Some(0)`.
    pub code: Option<u32>,
    pub message: String,
    pub level: DiagnosticLevel,
    pub filename: Option<String>,
    pub location: Option<SourceLocation>,
}

impl CompilerDiagnostic {
    /// Checked constructor: `code` must be `None` (uncoded) or a real `LWC####` code, never `Some(0)`.
    pub fn new(code: Option<u32>, message: impl Into<String>, level: DiagnosticLevel, filename: Option<String>, location: Option<SourceLocation>) -> Self {
        debug_assert!(code != Some(0), "diagnostic code 0 is not a code; use None for uncoded diagnostics");
        Self { code, message: message.into(), level, filename, location }
    }
}

/// 1-based line/column plus absolute offsets (parse5 / LWC convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub start: u32,
    pub end: u32,
}

/// Minimal compile options needed to dispatch (mirrors `@lwc/compiler` TransformOptions subset).
/// `@lwc/shared` `CompilationMode` (`ssrMode`); JS `DEFAULT_SSR_MODE` = sync.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SsrMode {
    #[default]
    Sync,
    Async,
    AsyncYield,
}

impl SsrMode {
    pub fn from_js(s: &str) -> Option<Self> {
        match s { "sync" => Some(Self::Sync), "async" => Some(Self::Async), "asyncYield" => Some(Self::AsyncYield), _ => None }
    }
    pub fn js_value(self) -> &'static str {
        match self { Self::Sync => "sync", Self::Async => "async", Self::AsyncYield => "asyncYield" }
    }
}

/// `@lwc/compiler` `outputConfig.sourcemap` (`false | true | 'inline'`, default false).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sourcemap {
    #[default]
    Off,
    /// `true`: map returned on the result.
    On,
    /// `'inline'`: map appended to `code` as a data-URL comment; result `map` is null.
    Inline,
}

impl Sourcemap {
    pub fn from_bool(b: bool) -> Self { if b { Self::On } else { Self::Off } }
    /// String form of the option. Only `'inline'` is valid (JS `validateOutputConfig`); anything else
    /// is `Err` with LWC1021 `INVALID_SOURCEMAP_PROPERTY`.
    pub fn from_js_str(s: &str, filename: &str) -> Result<Self, CompileError> {
        if s == "inline" {
            Ok(Self::Inline)
        } else {
            Err(CompileError::lwc(
                1021,
                format_message("Expected a boolean value or 'inline' for outputConfig.sourcemap, received \"{0}\".", &[s]),
                filename,
                None,
                None,
            ))
        }
    }
    pub fn is_enabled(self) -> bool { self != Self::Off }
}

/// `@lwc/template-compiler` `CustomRendererElementConfig` (`shared/renderer-hooks.ts`).
/// Lives here (not in lwc-template) so `CompileOptions` can carry it without lwc-shared depending on
/// lwc-template; lwc-template re-exports these types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRendererElementConfig {
    pub tag_name: String,
    pub namespace: Option<String>,
    pub attributes: Option<Vec<String>>,
}

/// `@lwc/template-compiler` `CustomRendererConfig`: which elements/directives require a customizable renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRendererConfig {
    pub elements: Vec<CustomRendererElementConfig>,
    pub directives: Vec<String>,
}

/// `@lwc/compiler` `CustomPropertiesResolution`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CustomPropertiesResolution {
    #[default]
    Native,
    Module { name: String },
}

/// `@lwc/compiler` `StylesheetConfig` (deprecated upstream: accepted, no output effect;
/// decisions.tsv `optinv:compiler.stylesheetConfig*`). Default = `DEFAULT_STYLESHEET_CONFIG`
/// (`{ customProperties: { resolution: { type: 'native' } } }`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StylesheetConfig {
    pub custom_properties_resolution: CustomPropertiesResolution,
}

/// `@lwc/errors` `CompilerMetrics`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilerMetric {
    LwcDynamicDirective,
    LwcRenderModeDirective,
    LwcSpreadDirective,
    DynamicImportTransform,
}

impl CompilerMetric {
    pub fn js_value(self) -> &'static str {
        match self {
            Self::LwcDynamicDirective => "lwc-dynamic-directive",
            Self::LwcRenderModeDirective => "lwc-render-mode-directive",
            Self::LwcSpreadDirective => "lwc-spread-directive",
            Self::DynamicImportTransform => "dynamic-import-transform",
        }
    }
}

/// `@lwc/errors` `InstrumentationObject` for Rust consumers (not exposed over napi: JS callbacks
/// can't cross the JSON options object).
pub trait Instrumentation: Send + Sync {
    /// JS `log(errorInfo, config)`. Upstream compilers never call it today; kept for API parity.
    fn log(&self, _diagnostic: &CompilerDiagnostic) {}
    /// JS `incrementCounter(metric)`.
    fn increment_counter(&self, metric: CompilerMetric);
}

/// Shared handle so options stay `Clone + Debug + Eq` (equality is pointer identity).
#[derive(Clone)]
pub struct InstrumentationHandle(pub std::sync::Arc<dyn Instrumentation>);

impl InstrumentationHandle {
    pub fn new(i: impl Instrumentation + 'static) -> Self { Self(std::sync::Arc::new(i)) }
    pub fn increment_counter(&self, metric: CompilerMetric) { self.0.increment_counter(metric) }
    pub fn log(&self, d: &CompilerDiagnostic) { self.0.log(d) }
}

impl std::fmt::Debug for InstrumentationHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("InstrumentationHandle(..)") }
}
impl PartialEq for InstrumentationHandle {
    fn eq(&self, o: &Self) -> bool { std::sync::Arc::ptr_eq(&self.0, &o.0) }
}
impl Eq for InstrumentationHandle {}

/// Single source of truth for option defaults: `Default` mirrors shipped `@lwc/compiler`
/// `DEFAULT_OPTIONS` / `normalizeOptions`. Callers (napi) start from `default()` and override only
/// the fields the caller set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOptions {
    pub name: String,
    pub namespace: String,
    pub scoped_styles: Option<bool>,
    pub disable_synthetic_shadow_support: bool,
    pub api_version: Option<u32>,
    pub target_ssr: bool,
    /// SSR compilation mode (only read when `target_ssr`).
    pub ssr_mode: SsrMode,
    // --- JS path (`@lwc/compiler` DEFAULT_OPTIONS) ---
    pub enable_private_methods: bool,
    pub enable_lightning_web_security_transforms: bool,
    pub experimental_error_recovery_mode: bool,
    pub is_explicit_import: bool,
    pub enable_synthetic_element_internals: bool,
    pub component_feature_flag_module_path: String,
    /// `outputConfig.sourcemap` (JS path only; template/style maps are always empty upstream).
    pub sourcemap: Sourcemap,
    pub dynamic_import_loader: String,
    /// `dynamicImports.strictSpecifier`; shipped `DEFAULT_DYNAMIC_IMPORT_CONFIG` = true.
    pub dynamic_import_strict_specifier: bool,
    /// `stylesheetConfig` (style path; no output effect upstream).
    pub stylesheet_config: StylesheetConfig,
    /// `@lwc/errors` `InstrumentationObject` (Rust API only).
    pub instrumentation: Option<InstrumentationHandle>,
    // --- template path (forwarded to lwc_template::Config; None = template default) ---
    pub experimental_complex_expressions: Option<bool>,
    pub preserve_html_comments: Option<bool>,
    pub enable_static_content_optimization: Option<bool>,
    pub enable_dynamic_components: Option<bool>,
    pub enable_lwc_on: Option<bool>,
    /// Raw value as the caller passed it (`None` = omitted). Do not read this directly on the CSR
    /// template path; use [`CompileOptions::resolved_experimental_dynamic_directive`].
    pub experimental_dynamic_directive: Option<bool>,
    pub custom_renderer_config: Option<CustomRendererConfig>,
}

impl CompileOptions {
    /// `@lwc/compiler` `transformers/template.ts`: `experimentalDynamicDirective ?? Boolean(dynamicImports)`.
    /// `normalizeOptions` always materializes `dynamicImports`, so an omitted value resolves to **true**.
    /// Only the CSR template transformer applies this; the targetSSR path (`compileTemplateForSSR`)
    /// passes the raw option to template-compiler, where omitted means false.
    pub fn resolved_experimental_dynamic_directive(&self) -> bool {
        self.experimental_dynamic_directive.unwrap_or(true)
    }
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            name: String::new(),
            namespace: String::new(),
            scoped_styles: None,
            disable_synthetic_shadow_support: false,
            api_version: None,
            target_ssr: false,
            ssr_mode: SsrMode::Sync,
            enable_private_methods: false,
            enable_lightning_web_security_transforms: false,
            experimental_error_recovery_mode: false,
            is_explicit_import: false,
            enable_synthetic_element_internals: false,
            component_feature_flag_module_path: String::new(),
            sourcemap: Sourcemap::Off,
            dynamic_import_loader: String::new(),
            dynamic_import_strict_specifier: true,
            stylesheet_config: StylesheetConfig::default(),
            instrumentation: None,
            experimental_complex_expressions: None,
            preserve_html_comments: None,
            enable_static_content_optimization: None,
            enable_dynamic_components: None,
            enable_lwc_on: None,
            experimental_dynamic_directive: None,
            custom_renderer_config: None,
        }
    }
}

/// Mirrors `transformSync` result. Style keeps `map == Some("")` during the port.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompileResult {
    pub code: String,
    pub map: Option<String>,
    /// Warning-level diagnostics (JS `transformSync().warnings`).
    pub warnings: Vec<CompilerDiagnostic>,
    /// Template only (JS `transformSync().cssScopeTokens`).
    pub css_scope_tokens: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    pub filename: Option<String>,
    pub diagnostics: Vec<CompilerDiagnostic>,
    /// JS Error.name when thrown across the napi seam (e.g. AggregateError).
    pub name: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl CompileError {
    pub fn not_implemented(what: &str, filename: &str) -> Self {
        Self {
            message: format!("{what}: not implemented"),
            filename: Some(filename.into()),
            diagnostics: vec![],
            name: None,
            line: None,
            column: None,
        }
    }
}

impl CompileError {
    pub fn new(message: impl Into<String>, filename: &str) -> Self {
        Self {
            message: message.into(),
            filename: Some(filename.into()),
            diagnostics: vec![],
            name: None,
            line: None,
            column: None,
        }
    }
}

impl CompileError {
    /// `@lwc/errors`-style error: message `LWC{code}: {message}` plus one Error diagnostic carrying `code`.
    pub fn lwc(code: u32, message: impl Into<String>, filename: &str, line: Option<u32>, column: Option<u32>) -> Self {
        let message = format!("LWC{code}: {}", message.into());
        Self {
            diagnostics: vec![CompilerDiagnostic::new(Some(code), message.clone(), DiagnosticLevel::Error, Some(filename.into()), None)],
            message,
            filename: Some(filename.into()),
            name: None,
            line,
            column,
        }
    }
}

impl CompileError {
    /// A non-LWC error thrown inside Babel (parser, class-features plugins, node validators), as
    /// shipped `normalizeToCompilerError` wraps it: code 1007, message `{kind}: LWC1007: {filename}: {text}`
    /// (`kind` = the original JS error name, e.g. `SyntaxError`/`TypeError`). Never aggregated in recovery mode.
    pub fn babel(kind: &str, text: impl Into<String>, filename: &str, line: Option<u32>, column: Option<u32>) -> Self {
        let message = format!("{kind}: LWC1007: {filename}: {}", text.into());
        Self {
            diagnostics: vec![CompilerDiagnostic::new(Some(1007), message.clone(), DiagnosticLevel::Error, Some(filename.into()), None)],
            message,
            filename: Some(filename.into()),
            name: None,
            line,
            column,
        }
    }

    /// True for an `@lwc/errors` error raised by an LWC plugin (`CompileError::lwc`, message `LWC####: ...`)
    /// that has not yet been framed as Babel's `buildCodeFrameError` would.
    pub fn is_unframed_lwc(&self) -> bool {
        self.name.is_none() && !self.diagnostics.is_empty() && self.message.starts_with("LWC")
    }

    /// `CompileError::lwc` at a Babel node: line (1-based) / column (0-based, Babel) from `start`, and the
    /// diagnostic carries the node's `{start, end}` (shipped `location` {line, column, start, length}).
    pub fn lwc_at(code: u32, message: impl Into<String>, filename: &str, src: &str, start: u32, end: u32) -> Self {
        let (line, column) = babel_line_col(src, start);
        let (end_line, end_column) = babel_line_col(src, end);
        let mut e = Self::lwc(code, message, filename, Some(line), Some(column));
        e.diagnostics[0].location = Some(SourceLocation { start_line: line, start_column: column, end_line, end_column, start, end });
        e
    }

    /// Babel `path.buildCodeFrameError`, as shipped surfaces it: `SyntaxError: {filename}: LWC####: msg` (no
    /// `(line:col)` suffix), then `\n` + the `@babel/code-frame` frame when the error has a node location.
    pub fn framed(mut self, filename: &str, src: &str) -> Self {
        if self.is_unframed_lwc() {
            self.message = format!("SyntaxError: {filename}: {}", self.message);
            if let Some(l) = self.diagnostics.first().and_then(|d| d.location) {
                // @babel/core File#buildCodeFrameError: `end` only when the node starts and ends on one line.
                let end = (l.start_line == l.end_line).then_some((l.end_line, l.end_column));
                self.message.push('\n');
                self.message.push_str(&code_frame(src, (l.start_line, l.start_column), end));
            }
        }
        self
    }
}

/// Babel location of byte offset `off`: 1-based line, 0-based column in UTF-16 units (JS string index).
pub fn babel_line_col(src: &str, off: u32) -> (u32, u32) {
    let before = &src[..(off as usize).min(src.len())];
    let line = before.matches('\n').count() as u32 + 1;
    let col = before.rsplit('\n').next().map(|l| l.encode_utf16().count()).unwrap_or(0) as u32;
    (line, col)
}

/// `@babel/code-frame` `codeFrameColumns` without highlighting (linesAbove 2, linesBelow 3).
/// `start`/`end` are (1-based line, 0-based column), as on a Babel node `loc`.
pub fn code_frame(src: &str, start: (u32, u32), end: Option<(u32, u32)>) -> String {
    let normalized = src.replace("\r\n", "\n"); // NEWLINE = /\r\n|[\n\r\u2028\u2029]/
    let lines: Vec<&str> = normalized.split(['\n', '\r', '\u{2028}', '\u{2029}']).collect();
    let (start_line, start_col) = (start.0 as usize, start.1 as usize + 1);
    let (end_line, end_col) = end.map(|(l, c)| (l as usize, c as usize + 1)).unwrap_or((start_line, start_col));
    let first = start_line.saturating_sub(3);
    let last = (end_line + 3).min(lines.len());
    // markerLines: line -> (column, count); count 0 means a single caret.
    let mut markers: std::collections::BTreeMap<usize, (usize, usize)> = Default::default();
    if end_line > start_line {
        for i in 0..=(end_line - start_line) {
            let n = start_line + i;
            // shipped reads `source[lineNumber - i]` (the start line) for middle lines; kept as is.
            let len = lines.get(start_line - 1).map_or(0, |l| l.encode_utf16().count());
            let m = if i == 0 { (start_col, (len + 1).saturating_sub(start_col)) } else if n == end_line { (0, end_col) } else { (0, len) };
            markers.insert(n, m);
        }
    } else if start_col == end_col {
        markers.insert(start_line, (start_col, 0));
    } else {
        markers.insert(start_line, (start_col, end_col - start_col));
    }
    let width = last.to_string().len();
    let mut out = Vec::new();
    for (idx, line) in lines[first.min(last)..last].iter().enumerate() {
        let number = first + 1 + idx;
        let gutter = format!(" {:>width$} |", number);
        let text = if line.is_empty() { String::new() } else { format!(" {line}") };
        match markers.get(&number) {
            Some(&(col, count)) => {
                let spacing: String = line.encode_utf16().take(col.saturating_sub(1)).map(|u| if u == b'\t' as u16 { '\t' } else { ' ' }).collect();
                let gutter_blank: String = gutter.chars().map(|c| if c.is_ascii_digit() { ' ' } else { c }).collect();
                out.push(format!(">{gutter}{text}\n {gutter_blank} {spacing}{}", "^".repeat(count.max(1))));
            }
            None => out.push(format!(" {gutter}{text}")),
        }
    }
    out.join("\n")
}

impl CompileError {
    /// `CompilerAggregateError` (error-recovery mode): name `AggregateError`, diagnostics concatenated,
    /// message lists every sub-error so `LWC####` codes survive the single-message napi seam.
    pub fn aggregate(errors: Vec<CompileError>, filename: &str) -> Self {
        let mut message = String::from("Multiple errors occurred during compilation.");
        for e in &errors { message.push('\n'); message.push_str(&e.message); }
        Self {
            message,
            filename: Some(filename.into()),
            diagnostics: errors.into_iter().flat_map(|e| e.diagnostics).collect(),
            name: Some("AggregateError".into()),
            // JS CompilerAggregateError does not expose location on the aggregate itself.
            line: None,
            column: None,
        }
    }
}

/// `@lwc/errors` `LWCErrorInfo`: one entry of the error-info tables (`compiler/error-info/*.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LwcErrorInfo {
    pub code: u32,
    /// Message template with `{0}`, `{1}`... placeholders.
    pub message: &'static str,
    pub level: DiagnosticLevel,
    /// JS `url` (every current entry is `''`). Not carried on `CompilerDiagnostic`.
    pub url: Option<&'static str>,
    /// JS `strictLevel`, used when `use_strict_error_override` is set.
    pub strict_level: Option<DiagnosticLevel>,
}

/// `@lwc/errors` `Location` shape: `{ line, column, start?, length? }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticLocation {
    pub line: u32,
    pub column: u32,
    pub start: Option<u32>,
    pub length: Option<u32>,
}

impl From<SourceLocation> for DiagnosticLocation {
    /// `@lwc/template-compiler` `normalizeLocation`: line/column = start line/column,
    /// start = start offset, length = end - start.
    fn from(l: SourceLocation) -> Self {
        Self { line: l.start_line, column: l.start_column, start: Some(l.start), length: Some(l.end.saturating_sub(l.start)) }
    }
}

/// `@lwc/errors` `CompilerDiagnosticOrigin`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticOrigin {
    pub filename: Option<String>,
    pub location: Option<SourceLocation>,
}

/// `@lwc/errors` `generateErrorMessage`: `LWC{code}: {message}`, where `{N}` placeholders are filled
/// from `args` only when `args` is `Some` (JS `Array.isArray(args)`). A placeholder with no
/// matching arg becomes `undefined`, as in JS `templateString`. Substitution is single-pass, so an
/// arg containing `{1}` is not re-expanded.
pub fn generate_error_message(info: &LwcErrorInfo, args: Option<&[&str]>) -> String {
    let message = match args {
        Some(args) => template_string(info.message, args),
        None => info.message.to_string(),
    };
    format!("LWC{}: {message}", info.code)
}

/// JS `templateString`: replaces each `/\{([0-9]+)\}/g` match with `args[index]` (`undefined` if absent).
fn template_string(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && after.as_bytes().get(digits) == Some(&b'}') {
            let idx: Option<usize> = after[..digits].parse().ok();
            out.push_str(idx.and_then(|i| args.get(i)).copied().unwrap_or("undefined"));
            rest = &after[digits + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

impl CompilerDiagnostic {
    /// `@lwc/errors` `generateCompilerDiagnostic(errorInfo, { messageArgs, origin }, useStrictErrorOverride)`.
    ///
    /// The message is built by [`generate_error_message`] and already carries the `LWC{code}: `
    /// prefix; callers must not add it again. Level is `strict_level` when `use_strict_error_override`
    /// is set and the info has one, else `level`. Filename and location come only from `origin`
    /// (both `None` without one). Use [`CompilerDiagnostic::js_location`] for the JS `Location` shape.
    pub fn generate(info: &LwcErrorInfo, args: Option<&[&str]>, origin: Option<DiagnosticOrigin>, use_strict_error_override: bool) -> Self {
        let level = match (use_strict_error_override, info.strict_level) {
            (true, Some(strict)) => strict,
            _ => info.level,
        };
        let origin = origin.unwrap_or_default();
        Self::new(Some(info.code), generate_error_message(info, args), level, origin.filename, origin.location)
    }

    /// The diagnostic's location in `@lwc/errors` `Location` shape (`{ line, column, start, length }`).
    pub fn js_location(&self) -> Option<DiagnosticLocation> {
        self.location.map(DiagnosticLocation::from)
    }
}

/// `{0}`/`{1}`... substitution as in `@lwc/errors` templateString.
pub fn format_message(template: &str, args: &[&str]) -> String {
    let mut out = template.to_string();
    for (i, a) in args.iter().enumerate() { out = out.replace(&format!("{{{i}}}"), a); }
    out
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.message) }
}
impl std::error::Error for CompileError {}

#[cfg(test)]
mod diagnostic_code_tests {
    use super::*;

    #[test]
    fn option_defaults_match_shipped_transform_sync() {
        let o = CompileOptions::default();
        assert!(o.dynamic_import_strict_specifier);
        assert_eq!(o.experimental_dynamic_directive, None);
        assert!(o.resolved_experimental_dynamic_directive());
        assert!(!CompileOptions { experimental_dynamic_directive: Some(false), ..o.clone() }.resolved_experimental_dynamic_directive());
        assert_eq!(o.sourcemap, Sourcemap::Off);
        assert_eq!(o.stylesheet_config.custom_properties_resolution, CustomPropertiesResolution::Native);
        assert_eq!(o.custom_renderer_config, None);
        assert!(!o.is_explicit_import && !o.enable_lightning_web_security_transforms);
    }

    #[test]
    fn sourcemap_string_validation() {
        assert_eq!(Sourcemap::from_js_str("inline", "a.js").unwrap(), Sourcemap::Inline);
        let e = Sourcemap::from_js_str("foo", "a.js").unwrap_err();
        assert_eq!(e.diagnostics[0].code, Some(1021));
        assert_eq!(e.message, "LWC1021: Expected a boolean value or 'inline' for outputConfig.sourcemap, received \"foo\".");
    }

    #[test]
    fn coded_and_uncoded() {
        assert_eq!(CompilerDiagnostic::new(Some(1083), "m", DiagnosticLevel::Error, None, None).code, Some(1083));
        assert_eq!(CompilerDiagnostic::new(None, "m", DiagnosticLevel::Error, None, None).code, None);
        assert_eq!(CompileError::lwc(1001, "x", "a.js", None, None).diagnostics[0].code, Some(1001));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "diagnostic code 0")]
    fn code_zero_panics_in_debug() {
        let _ = CompilerDiagnostic::new(Some(0), "m", DiagnosticLevel::Error, None, None);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "diagnostic code 0")]
    fn lwc_code_zero_panics_in_debug() {
        let _ = CompileError::lwc(0, "x", "a.js", None, None);
    }

    const LWC_IF_SCOPE_NOT_FOUND: LwcErrorInfo = LwcErrorInfo {
        code: 1165,
        message: "'{0}' directive must be used immediately after an element with 'lwc:if' or 'lwc:elseif'. No such element found.",
        level: DiagnosticLevel::Error,
        url: Some(""),
        strict_level: None,
    };
    const SINGLE_IF_DIRECTIVE_ALLOWED: LwcErrorInfo = LwcErrorInfo {
        code: 1182,
        message: "Multiple if: directives found on '{0}'. Only one if: directive is allowed; the rest are ignored.",
        level: DiagnosticLevel::Warning,
        url: Some(""),
        strict_level: None,
    };

    #[test]
    fn generate_1165_error_with_location() {
        // `<template><div lwc:else></div></template>`; JS @b9e8007 location {line:1,column:11,start:10,length:20}.
        let loc = SourceLocation { start_line: 1, start_column: 11, end_line: 1, end_column: 31, start: 10, end: 30 };
        let d = CompilerDiagnostic::generate(&LWC_IF_SCOPE_NOT_FOUND, Some(&["lwc:else"]), Some(DiagnosticOrigin { filename: None, location: Some(loc) }), false);
        assert_eq!(d.message, "LWC1165: 'lwc:else' directive must be used immediately after an element with 'lwc:if' or 'lwc:elseif'. No such element found.");
        assert_eq!(d.code, Some(1165));
        assert_eq!((d.level, d.level.js_value()), (DiagnosticLevel::Error, 1));
        assert_eq!(d.filename, None);
        assert_eq!(d.js_location(), Some(DiagnosticLocation { line: 1, column: 11, start: Some(10), length: Some(20) }));
    }

    #[test]
    fn generate_1182_warning_with_filename() {
        // `<template><div if:true={a} if:false={b}></div></template>`; JS location {line:1,column:11,start:10,length:36}.
        let loc = SourceLocation { start_line: 1, start_column: 11, end_line: 1, end_column: 47, start: 10, end: 46 };
        let origin = DiagnosticOrigin { filename: Some("x.html".into()), location: Some(loc) };
        let d = CompilerDiagnostic::generate(&SINGLE_IF_DIRECTIVE_ALLOWED, Some(&["div"]), Some(origin), false);
        assert_eq!(d.message, "LWC1182: Multiple if: directives found on 'div'. Only one if: directive is allowed; the rest are ignored.");
        assert_eq!((d.level, d.level.js_value()), (DiagnosticLevel::Warning, 2));
        assert_eq!(d.filename.as_deref(), Some("x.html"));
        assert_eq!(d.js_location(), Some(DiagnosticLocation { line: 1, column: 11, start: Some(10), length: Some(36) }));
    }

    #[test]
    fn generate_without_origin_or_args() {
        // No args: template left as-is (JS `Array.isArray(undefined)` is false). No origin: no filename/location.
        let d = CompilerDiagnostic::generate(&SINGLE_IF_DIRECTIVE_ALLOWED, None, None, false);
        assert_eq!(d.message, "LWC1182: Multiple if: directives found on '{0}'. Only one if: directive is allowed; the rest are ignored.");
        assert_eq!((d.filename, d.location), (None, None));
        // Empty args: placeholder becomes `undefined` (JS `args[0]`).
        assert_eq!(generate_error_message(&SINGLE_IF_DIRECTIVE_ALLOWED, Some(&[])), "LWC1182: Multiple if: directives found on 'undefined'. Only one if: directive is allowed; the rest are ignored.");
    }

    #[test]
    fn template_string_is_single_pass() {
        let info = LwcErrorInfo { code: 1, message: "{0}-{1}-{x}-{}", level: DiagnosticLevel::Error, url: None, strict_level: None };
        assert_eq!(generate_error_message(&info, Some(&["{1}", "b"])), "LWC1: {1}-b-{x}-{}");
    }

    #[test]
    fn level_mapping_and_strict_override() {
        let info = LwcErrorInfo { code: 1, message: "m", level: DiagnosticLevel::Warning, url: None, strict_level: Some(DiagnosticLevel::Error) };
        assert_eq!(CompilerDiagnostic::generate(&info, None, None, false).level, DiagnosticLevel::Warning);
        assert_eq!(CompilerDiagnostic::generate(&info, None, None, true).level, DiagnosticLevel::Error);
        // Override without a strictLevel keeps `level`.
        assert_eq!(CompilerDiagnostic::generate(&SINGLE_IF_DIRECTIVE_ALLOWED, None, None, true).level, DiagnosticLevel::Warning);
        let js: Vec<u32> = [DiagnosticLevel::Fatal, DiagnosticLevel::Error, DiagnosticLevel::Warning, DiagnosticLevel::Log].iter().map(|l| l.js_value()).collect();
        assert_eq!(js, vec![0, 1, 2, 3]);
    }
}
