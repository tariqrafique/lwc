//! LWC component JS compiler (Rust port of `@lwc/compiler` javascript.ts +
//! `@lwc/babel-plugin-component`), built on oxc.
//!
//! Pipeline: parse_source once (oxc) -> span-edit LWC/class-features passes -> parse the
//! edited source, run rest/spread on that AST, print once -> version stamp and helper
//! localize via the codegen map (no re-parse of printed JS; D-0262 / AB-L0037).
//! See `transforms/mod.rs` for order.

pub mod codegen;
pub mod constants;
pub mod helpers;
pub mod origin;
pub mod parse;
pub mod srcmap;
pub mod tag_name;
pub mod transforms;

pub use origin::Origin;

use lwc_shared::api_version::{self, ApiFeature};
use lwc_shared::edits;
use lwc_shared::{CompileError, CompileOptions, CompileResult, CompilerDiagnostic};

/// `@babel/core`'s `err.code` on an error thrown out of `transformSync`: a parser throw is wrapped as
/// `BABEL_PARSE_ERROR`, any plugin/visitor throw as `BABEL_TRANSFORM_ERROR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BabelCode {
    Parse,
    Transform,
}

/// How shipped `babel.transformSync` ends without output, before javascript.ts surfaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsError {
    /// Thrown (javascript.ts `catch (e)`), shaped as Babel throws it: a raw Babel error
    /// ([`CompileError::babel`]) or an LWC plugin error framed by `buildCodeFrameError`
    /// ([`CompileError::framed`], `lwcCode` on its diagnostic).
    Thrown { error: CompileError, code: BabelCode },
    /// Recovery mode: `result.metadata.lwcErrors`, unframed `LWC####: msg` diagnostics whose filename
    /// is Babel's resolved `file.opts.filename`.
    LwcErrors(Vec<CompilerDiagnostic>),
}

impl From<CompileError> for JsError {
    fn from(error: CompileError) -> Self { JsError::Thrown { error, code: BabelCode::Transform } }
}

impl std::fmt::Display for JsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsError::Thrown { error, .. } => write!(f, "{}: {}", error.name.as_deref().unwrap_or("Error"), error.message),
            JsError::LwcErrors(ds) => ds.iter().map(|d| d.message.as_str()).collect::<Vec<_>>().join("\n").fmt(f),
        }
    }
}

pub fn transform(src: &str, filename: &str, options: &CompileOptions) -> Result<CompileResult, JsError> {
    // LWC-plugin errors (`CompileError::lwc`) are framed here, once: recovery mode collects them into
    // `lwcErrors` (messages stay `LWC####: msg`, even for a single one); otherwise Babel throws them as
    // `buildCodeFrameError`. Babel-internal errors (`CompileError::babel`) are never collected, as in shipped.
    match transform_inner(src, filename, options) {
        Err(JsError::Thrown { error, .. }) if error.is_unframed_lwc() && options.experimental_error_recovery_mode => {
            Err(lwc_errors(error.diagnostics))
        }
        Err(JsError::Thrown { error, code }) => Err(JsError::Thrown { error: error.framed(filename, src), code }),
        Err(JsError::LwcErrors(ds)) => Err(lwc_errors(ds)),
        r => r,
    }
}

/// `lwcErrors` diagnostics carry Babel's `file.opts.filename`, which `@babel/core` resolves against the cwd.
fn lwc_errors(mut ds: Vec<CompilerDiagnostic>) -> JsError {
    let cwd = std::env::current_dir().unwrap_or_default();
    for d in &mut ds {
        d.filename = d.filename.as_deref().map(|f| resolve_like_node(&cwd, f));
    }
    JsError::LwcErrors(ds)
}

/// Node `path.resolve(cwd, f)` for a relative or absolute POSIX path: `.` / `..` segments and repeated
/// slashes collapse, a trailing slash drops.
pub fn resolve_like_node(cwd: &std::path::Path, f: &str) -> String {
    let joined = if f.starts_with('/') { f.to_string() } else { format!("{}/{f}", cwd.to_string_lossy()) };
    let mut out: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => { out.pop(); }
            s => out.push(s),
        }
    }
    format!("/{}", out.join("/"))
}

fn transform_inner(src: &str, filename: &str, options: &CompileOptions) -> Result<CompileResult, JsError> {
    // outputConfig.sourcemap: every text stage below records its offset map in `chain` (no-op when off).
    let mut chain = srcmap::Chain::new(options.sourcemap.is_enabled());
    // 1. private-method forward rename (validate-only when the flag is off), then the LWC class
    // transform on the same Program when the rename is identity (D-0262: no extra parse). The
    // deferred "private methods not enabled" check also uses that Program, not a re-parse of the
    // post-LWC text (it only looks for remaining `#m` methods; LWC does not delete them).
    let alloc0 = oxc::allocator::Allocator::default();
    let program0 = parse::parse_source(&alloc0, src, filename)
        .map_err(|error| JsError::Thrown { error, code: BabelCode::Parse })?;
    let (fwd_edits, renamed, deferred_pm_check) = match transforms::private_methods::forward(&program0, filename, options)? {
        transforms::private_methods::Forward::Done(e, n) => (e, n, false),
        transforms::private_methods::Forward::Deferred => (Vec::new(), Default::default(), true),
    };
    let (src1, rename_pieces) = edits::apply_mapped(src, fwd_edits).expect("lwc-js edits never partially overlap");
    chain.push_edit(rename_pieces.clone());
    // 2. LWC class transform. Its error spans are offsets into the renamed `src1`; shipped Babel reports the
    // original `#m` source positions, so map them back through the rename edits before framing.
    let origin = Origin { src, pieces: &rename_pieces };
    let mut edited = if origin.is_identity() {
        let edits = transforms::run(&program0, src, filename, options, origin)
            .map_err(|e| unrename_locations(e, origin))?;
        if deferred_pm_check { transforms::private_methods::not_enabled(&program0, filename)?; }
        let (out, pieces) = edits::apply_mapped(src, edits).expect("lwc-js edits never partially overlap");
        chain.push_edit(pieces);
        out
    } else {
        let (out, check) = pass_with(&src1, filename, &mut chain, |p, s| {
            let edits = transforms::run(p, s, filename, options, origin).map_err(|e| unrename_locations(e, origin))?;
            let check = if deferred_pm_check { transforms::private_methods::not_enabled(p, filename) } else { Ok(()) };
            Ok::<_, JsError>((edits, check))
        })?;
        check?;
        out
    };
    // 3a. loose private fields (always on in shipped): `#x` -> classPrivateFieldLoose{Key,Base} + markers
    edited = pass(&edited, filename, &mut chain, |p, s| Ok::<_, JsError>(transforms::private_fields::run(p, s)))?;
    // 3. class-properties (loose), to a fixpoint so nested classes in moved initializers are lowered
    for _ in 0..32 {
        let mut changed = false;
        edited = pass(&edited, filename, &mut chain, |p, s| {
            let e = transforms::class_properties::run(p, s);
            changed = !e.is_empty();
            Ok::<_, JsError>(e)
        })?;
        if !changed { break; }
    }
    // 4. private-method reverse
    if options.enable_private_methods {
        edited = pass(&edited, filename, &mut chain, |p, _| transforms::private_methods::reverse(p, filename, &renamed))?;
    }
    // Parse the span-edited source once, lower rest/spread on that AST, print once. Version stamp
    // and helper localize walk this Program and remap through the codegen map (D-0262: never parse
    // the printed string).
    let do_rest = !api_version::is_api_feature_enabled(
        ApiFeature::DisableObjectRestSpreadTransformation,
        api_version::get_api_version_from_number(options.api_version),
    );
    let mut printed;
    let mut stamps;
    {
        let alloc = oxc::allocator::Allocator::default();
        let mut program = parse::parse(&alloc, &edited, filename)
            .map_err(|e| CompileError::new(format!("internal: transformed output failed to re-parse: {}", e.message), filename))?;
        if do_rest { transforms::lower::apply_object_rest_spread(&alloc, &edited, filename, &mut program)?; }
        let (out, map) = codegen::print_mapped(&program, &edited, true);
        let map = map.expect("codegen with_map always produces a map");
        if chain.enabled() { chain.push_codegen(&map, &edited, &out); }
        stamps = helpers::post_codegen_edits(&program, &edited, &out, &map, filename)?;
        printed = out;
    }
    // LWS (javascript.ts appends these after rest/spread): unforgeables, then async-to-generator.
    // Unforgeables still parse their input themselves (preserve_parens: false); that input is printed
    // JS, so their edits are remapped onto the pending stamp list. If async reprints, it returns a
    // fresh stamp list computed from its own pre-print Program (no parse of the reprint).
    if options.enable_lightning_web_security_transforms {
        let e = transforms::unforgeables::run(&printed, filename)?;
        if !e.is_empty() {
            let (out, pieces) = edits::apply_mapped(&printed, e).expect("lwc-js edits never partially overlap");
            chain.push_edit(pieces.clone());
            stamps = helpers::remap_edits(stamps, &pieces);
            printed = out;
        }
        if let Some((out, map, async_stamps)) = transforms::lower::async_functions(&printed, filename, chain.enabled())? {
            if let Some(m) = map { chain.push_codegen(&m, &printed, &out); }
            printed = out;
            stamps = async_stamps;
        }
    }
    let (code, pieces) = edits::apply_mapped(&printed, stamps).expect("lwc-js edits never partially overlap");
    chain.push_edit(pieces);
    let map = chain.finish(src, filename, &code);
    Ok(match (options.sourcemap, map) {
        (lwc_shared::Sourcemap::Inline, Some(m)) => {
            // babel: `code + "\n//# sourceMappingURL=data:application/json;charset=utf-8;base64,..."`
            let sep = if code.ends_with('\n') { "" } else { "\n" };
            let code = format!("{code}{sep}//# sourceMappingURL={}", m.to_data_url());
            CompileResult { code, map: None, ..Default::default() }
        }
        (_, map) => CompileResult { code, map: map.map(|m| m.to_json_string()), ..Default::default() },
    })
}

fn pass<E: From<CompileError>>(
    src: &str,
    filename: &str,
    chain: &mut srcmap::Chain,
    f: impl FnOnce(&oxc::ast::ast::Program<'_>, &str) -> Result<Vec<edits::Edit>, E>,
) -> Result<String, E> {
    Ok(pass_with(src, filename, chain, |p, s| f(p, s).map(|e| (e, ())))?.0)
}

/// SHIM(D-0262 P1 pass_with, expires D-0265): re-parses span-edited source between class-features edit passes (D-0264 residual 1).
fn pass_with<E: From<CompileError>, T>(
    src: &str,
    filename: &str,
    chain: &mut srcmap::Chain,
    f: impl FnOnce(&oxc::ast::ast::Program<'_>, &str) -> Result<(Vec<edits::Edit>, T), E>,
) -> Result<(String, T), E> {
    let alloc = oxc::allocator::Allocator::default();
    let program = parse::parse(&alloc, src, filename)
        .map_err(|e| CompileError::new(format!("internal: intermediate output failed to re-parse: {}", e.message), filename))?;
    let (edits, extra) = f(&program, src)?;
    let (out, pieces) = edits::apply_mapped(src, edits).expect("lwc-js edits never partially overlap");
    chain.push_edit(pieces);
    Ok((out, extra))
}

fn unrename_locations(e: JsError, origin: Origin<'_>) -> JsError {
    if origin.is_identity() { return e; }
    match e {
        JsError::Thrown { mut error, code } => {
            if let Some(first) = unrename_diagnostics(&mut error.diagnostics, origin) {
                if error.line.is_some() { error.line = Some(first.start_line); error.column = Some(first.start_column); }
            }
            JsError::Thrown { error, code }
        }
        JsError::LwcErrors(mut ds) => {
            unrename_diagnostics(&mut ds, origin);
            JsError::LwcErrors(ds)
        }
    }
}

/// Maps every diagnostic's node span back to the original source; returns the first one's new span.
fn unrename_diagnostics(ds: &mut [CompilerDiagnostic], origin: Origin<'_>) -> Option<lwc_shared::SourceLocation> {
    let index = lwc_shared::LineIndex::new(origin.src);
    let mut first = None;
    for (i, d) in ds.iter_mut().enumerate() {
        // Every transform error is `CompileError::generate_at`, whose location is its node span converted once.
        let Some(l) = d.node_location else { continue };
        let (start, end) = (origin.map(l.start, false), origin.map(l.end, true));
        let span = index.source_location(start, end);
        d.node_location = Some(span);
        d.location = Some(span.into());
        if i == 0 { first = Some(span); }
    }
    first
}
