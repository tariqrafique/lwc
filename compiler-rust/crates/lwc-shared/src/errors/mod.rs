//! `@lwc/errors` for the Rust compiler (AB-R003 / AB-0001):
//! - [`error_info`]: the ONE port of the error-info tables (generated; every code, message, level,
//!   strictLevel and url). Every crate names its errors from here.
//! - [`CompilerError`] / [`CompilerAggregateError`] / [`TransformError`]: what `@lwc/compiler`
//!   `transformSync` throws, typed. The lwc-compiler facades build them; the napi binding only serializes.
//! - [`normalize_to_compiler_error`]: `normalizeToCompilerError`, for the facades' `catch (e)`.

pub mod error_info;
pub use error_info::*;

use crate::{generate_error_message, template_string, CompileError, CompilerDiagnostic, DiagnosticLevel, DiagnosticLocation, DiagnosticOrigin, LwcErrorInfo, SourceLocation};

/// Where a [`CompileError`]'s position comes from.
///
/// Every Rust producer that has a span puts it on `diagnostics[].location`. The raw
/// PostCSS `CssSyntaxError` (`lwc-style`'s `StyleError::thrown`) carries no diagnostic:
/// its line/column are the producer's own (`None` is JS `undefined`). That position is
/// [`ProducerPosition::Raw`], not loose fields beside `diagnostics` (AB-L0080 follow-up).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProducerPosition {
    /// Read position from `diagnostics[].location` only.
    Diagnostics,
    /// Raw producer (`CssSyntaxError`). Not copied onto a diagnostic.
    Raw { line: Option<u32>, column: Option<u32> },
}

/// The info for `code`, if `code` is a shipped `@lwc/errors` code.
pub fn lookup(code: u32) -> Option<&'static LwcErrorInfo> {
    ALL.binary_search_by_key(&code, |i| i.code).ok().map(|i| &ALL[i])
}

/// `errorInfo.url` for `code` (`""` when the code is unknown or its info has `url: ''`).
pub fn url_of(code: Option<u32>) -> &'static str {
    code.and_then(lookup).and_then(|i| i.url).unwrap_or("")
}

impl LwcErrorInfo {
    /// `templateString(message, args)`: the message body, without the `LWC{code}: ` prefix.
    pub fn format(&self, args: &[&str]) -> String {
        template_string(self.message, args)
    }

    /// `generateCompilerDiagnostic(this, { messageArgs: args, origin: { location } })` (no strict override):
    /// message `LWC{code}: ...`, level from the table, no filename. The form template diagnostics use
    /// (`ParserCtx.throwAtLocation` / `warnAtLocation`).
    pub fn generate(&self, args: &[&str], location: SourceLocation) -> CompilerDiagnostic {
        self.generate_normalized(args, Some(location))
    }

    /// [`LwcErrorInfo::generate`] at an optional node location, through TS `normalizeLocation`: a node
    /// without a parse5 location (or a `ctx.throw` with none) reports `{ line: 0, column: 0, start: 0, length: 0 }`.
    pub fn generate_normalized(&self, args: &[&str], location: Option<SourceLocation>) -> CompilerDiagnostic {
        let origin = DiagnosticOrigin { filename: None, location: Some(crate::location::normalize_location(location)) };
        CompilerDiagnostic::generate(self, Some(args), Some(origin), false)
    }
}

impl CompileError {
    /// `generateCompilerError(info, { messageArgs: args, origin: { filename, location } })`, unframed: message
    /// `LWC{code}: ...`, level from the table (no strict override), `location` as given (`{line, column,
    /// start?, length?}`), no Babel header or code frame. `line` / `column` mirror `location`.
    pub fn generate(info: &LwcErrorInfo, args: &[&str], filename: &str, location: Option<DiagnosticLocation>) -> Self {
        let message = generate_error_message(info, Some(args));
        let d = CompilerDiagnostic::new(Some(info.code), message.clone(), info.level, Some(filename.into()), location);
        Self { message, filename: Some(filename.into()), diagnostics: vec![d], name: None, position: ProducerPosition::Diagnostics }
    }

    /// [`CompileError::generate`] at the source range `start..end` (byte offsets into `src`, e.g. an oxc
    /// node span): location = the node's Babel `{line, column, start, length}` (UTF-16), and the diagnostic
    /// keeps the span for framing ([`CompileError::framed`]) and ordering.
    pub fn generate_at(info: &LwcErrorInfo, args: &[&str], filename: &str, src: &str, start: u32, end: u32) -> Self {
        let ix = crate::LineIndex::new(src);
        let mut e = Self::generate(info, args, filename, Some(ix.location(start, end)));
        e.diagnostics[0].node_location = Some(ix.source_location(start, end));
        e
    }
}

/// `@lwc/errors` `CompilerError`: a single error `transformSync` throws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerError {
    /// Always `Some` from [`normalize_to_compiler_error`] / [`CompilerError::generate`]; `None` only when a
    /// diagnostic without a code is carried over by [`CompilerError::from_diagnostic`].
    pub code: Option<u32>,
    pub message: String,
    pub filename: Option<String>,
    pub location: Option<DiagnosticLocation>,
    pub level: DiagnosticLevel,
    pub url: String,
}

impl CompilerError {
    /// `CompilerError.from(diagnostic, origin)`: code / message / location from the diagnostic, filename
    /// `origin.filename` else the diagnostic's, level `Error` (the constructor default). A Rust
    /// diagnostic carries no url; `generateCompilerDiagnostic` copies `errorInfo.url`, so it is the info's.
    pub fn from_diagnostic(d: &CompilerDiagnostic, filename: Option<&str>) -> Self {
        Self {
            code: d.code,
            message: d.message.clone(),
            filename: filename.map(str::to_string).or_else(|| d.filename.clone()),
            location: d.location,
            level: DiagnosticLevel::Error,
            url: url_of(d.code).to_string(),
        }
    }

    /// `generateCompilerError(info, { messageArgs: args, origin: { filename } })`: a validation error
    /// thrown typed (e.g. `validateOptions`), no strict override.
    pub fn generate(info: &LwcErrorInfo, args: &[&str], filename: Option<&str>) -> Self {
        Self {
            code: Some(info.code),
            message: generate_error_message(info, Some(args)),
            filename: filename.map(str::to_string),
            location: None,
            level: info.level,
            url: info.url.unwrap_or("").to_string(),
        }
    }
}

/// `@lwc/errors` `CompilerAggregateError(errors, message)` (recovery mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerAggregateError {
    pub errors: Vec<CompilerError>,
    pub message: String,
}

/// An error thrown by a compiler step no `@lwc/compiler` facade normalizes (the SSR compiler; an
/// unsupported file extension): the producer's error, carried through unchanged.
/// `location` is only the first diagnostic's, when present (D-0239 / D-0241 / AB-L0080).
/// A [`ProducerPosition::Raw`] error has no diagnostic; this path does not invent one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnnormalizedError {
    pub message: String,
    pub filename: Option<String>,
    /// The producer's first diagnostic code / location, when it has one.
    pub code: Option<u32>,
    pub location: Option<DiagnosticLocation>,
}

impl From<CompileError> for UnnormalizedError {
    fn from(e: CompileError) -> Self {
        let first = e.diagnostics.first();
        Self {
            code: first.and_then(|d| d.code),
            location: first.and_then(|d| d.location),
            message: e.message,
            filename: e.filename,
        }
    }
}

/// What the compiler seam throws (`lwc_compiler::dispatch`'s error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformError {
    Compiler(CompilerError),
    Aggregate(CompilerAggregateError),
    Unnormalized(UnnormalizedError),
}

/// `normalizeToCompilerError(fallback, e, { filename })` for a non-`CompilerError` `e` (a typed
/// `CompilerError` never reaches a facade's `catch` in Rust). `e` is the thrown JS error as `catch (e)` sees
/// it: `e.name` = `name` (`None` = `"Error"`), `e.message` = `message`, `e.lwcCode` = the first diagnostic's
/// code, `e.location` / `e.loc` = its location (the diagnostic's `location`). A raw
/// `CssSyntaxError` ([`ProducerPosition::Raw`]) has no diagnostic; its line/column are that variant.
/// - code: `e.lwcCode || fallback.code`; message: `${e.name}: ` + (`e.message` when it has an lwcCode, else
///   `generateErrorMessage(fallback, [e.message])`);
/// - level: `fallback.level` (a raw JS error has no `level`); url: `fallback.url`;
/// - filename: `filename || e.filename`; location: as above.
pub fn normalize_to_compiler_error(fallback: &LwcErrorInfo, e: CompileError, filename: Option<&str>) -> CompilerError {
    let first = e.diagnostics.first();
    let lwc_code = first.and_then(|d| d.code);
    let body = match lwc_code {
        Some(_) => e.message.clone(),
        None => generate_error_message(fallback, Some(&[e.message.as_str()])),
    };
    let location = match e.position {
        ProducerPosition::Raw { line, column } => {
            line.zip(column).map(|(line, column)| DiagnosticLocation { line, column, ..Default::default() })
        }
        ProducerPosition::Diagnostics => first.and_then(|d| d.location),
    };
    CompilerError {
        code: Some(lwc_code.unwrap_or(fallback.code)),
        message: format!("{}: {body}", e.name.as_deref().unwrap_or("Error")),
        filename: filename.filter(|f| !f.is_empty()).map(str::to_string).or(e.filename),
        location,
        level: fallback.level,
        url: fallback.url.unwrap_or("").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_normalized_without_location_is_normalize_location_zeros() {
        let d = template_transform::ParserDiagnostics::SINGLE_IF_DIRECTIVE_ALLOWED.generate_normalized(&["div"], None);
        assert_eq!(d.message, "LWC1182: Multiple if: directives found on 'div'. Only one if: directive is allowed; the rest are ignored.");
        assert_eq!(d.level, DiagnosticLevel::Warning);
        assert_eq!(d.location, Some(DiagnosticLocation { line: 0, column: 0, start: Some(0), length: Some(0), index: None }));
    }

    #[test]
    fn table_is_sorted_complete_and_names_resolve() {
        assert!(ALL.windows(2).all(|w| w[0].code < w[1].code));
        assert_eq!(ALL.len(), 190);
        assert_eq!(lookup(1009), Some(&TransformerErrors::CSS_TRANSFORMER_ERROR));
        assert_eq!(url_of(Some(1007)), "https://lwc.dev/guide/error_codes#lwc1007");
        assert_eq!(url_of(Some(1008)), "");
        assert_eq!(SsrCompilerErrors::RESERVED_IDENTIFIER_PREFIX.code, 1202);
        assert_eq!(lwc_class::DecoratorErrors::ONE_WIRE_DECORATOR_ALLOWED.code, 1105);
    }

    /// AB-0001 guard: the committed table equals a fresh extraction from the oracle's @lwc/errors.
    #[test]
    fn table_equals_fresh_extraction_from_oracle() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let script = format!("{root}/tools/gen-error-info/gen.mjs");
        let out = match std::process::Command::new("node").arg(&script).arg("--stdout").output() {
            Ok(o) if o.status.success() => o.stdout,
            Ok(o) => panic!("{script} failed: {}", String::from_utf8_lossy(&o.stderr)),
            Err(e) => panic!("node is required for this gate: {e}"),
        };
        assert!(String::from_utf8(out).unwrap() == include_str!("error_info.rs"), "error_info.rs is stale: run node tools/gen-error-info/gen.mjs");
    }

    /// Independent of the generator: entry count and the code set equal every info the oracle's
    /// `@lwc/errors` exports (walked from its `*Errors` / `*Diagnostics` / GENERIC_COMPILER_ERROR exports).
    #[test]
    fn count_and_codes_match_upstream() {
        let pkg = crate::oracle_root().join("packages/@lwc/errors");
        let js = format!("const m=require({:?});", pkg.to_str().expect("LWC_ORACLE must be UTF-8"))
            + r#"const s=new Set();
const walk=o=>{if(!o||typeof o!=="object")return;if(typeof o.code==="number"&&typeof o.message==="string"){s.add(o.code);return;}for(const v of Object.values(o))walk(v);};
for(const[k,v]of Object.entries(m))if(/Errors$|Diagnostics$|^GENERIC_COMPILER_ERROR$/.test(k))walk(v);
console.log([...s].sort((a,b)=>a-b).join(","));"#;
        let out = match std::process::Command::new("node").arg("-e").arg(js).output() {
            Ok(o) if o.status.success() => String::from_utf8(o.stdout).unwrap(),
            r => panic!("cannot read @lwc/errors from LWC_ORACLE {}: {r:?}", pkg.display()),
        };
        let upstream: Vec<u32> = out.trim().split(',').map(|c| c.parse().unwrap()).collect();
        let ours: Vec<u32> = ALL.iter().map(|i| i.code).collect();
        assert_eq!(ours.len(), upstream.len());
        assert_eq!(ours, upstream);
    }

    #[test]
    fn normalize_mirrors_upstream() {
        let raw = CompileError {
            name: Some("CssSyntaxError".into()),
            position: ProducerPosition::Raw { line: Some(2), column: Some(5) },
            ..CompileError::new("x.css:2:5: Unknown word", "ignored")
        };
        let e = normalize_to_compiler_error(&TransformerErrors::CSS_TRANSFORMER_ERROR, raw, Some("x.css"));
        assert_eq!(e.message, "CssSyntaxError: LWC1009: x.css:2:5: Unknown word");
        assert_eq!((e.code, e.filename.as_deref(), e.level), (Some(1009), Some("x.css"), DiagnosticLevel::Error));
        assert_eq!(e.location, Some(DiagnosticLocation { line: 2, column: 5, ..Default::default() }));
        assert_eq!(e.url, "https://lwc.dev/guide/error_codes#lwc1009");
        // an lwcCode keeps the message and the code
        let lwc = CompileError::generate(&DecoratorErrors::ONE_WIRE_DECORATOR_ALLOWED, &[], "a.js", None);
        let e = normalize_to_compiler_error(&TransformerErrors::JS_TRANSFORMER_ERROR, lwc, Some("a.js"));
        assert_eq!(e.message, "Error: LWC1105: Method or property can only have 1 @wire decorator");
        assert_eq!(e.code, Some(1105));
    }
}
