//! LWC component JS compiler (Rust port of `@lwc/compiler` javascript.ts +
//! `@lwc/babel-plugin-component`), built on oxc.
//!
//! Pipeline: parse (oxc) -> collect span edits from LWC transforms -> apply ->
//! re-parse + oxc codegen (normalized output) -> version stamp post-pass. See `transforms/mod.rs` for order.

pub mod codegen;
pub mod constants;
pub mod edits;
pub mod helpers;
pub mod parse;
pub mod srcmap;
pub mod tag_name;
pub mod transforms;

use lwc_shared::api_version::{self, ApiFeature};
use lwc_shared::{CompileError, CompileOptions, CompileResult};

pub fn transform(src: &str, filename: &str, options: &CompileOptions) -> Result<CompileResult, CompileError> {
    // LWC-plugin errors (`CompileError::lwc`) are framed here, once: recovery mode collects them into a
    // CompilerAggregateError (sub-messages stay `LWC####: msg`, even for a single one); otherwise the first
    // line is Babel's `SyntaxError: {filename}: LWC####: msg`. Babel-internal errors (`CompileError::babel`)
    // are already final and are never aggregated, as in shipped.
    match transform_inner(src, filename, options) {
        Err(e) if e.is_unframed_lwc() && options.experimental_error_recovery_mode => Err(CompileError::aggregate(vec![e], filename)),
        Err(e) => Err(e.framed(filename, src)),
        r => r,
    }
}

fn transform_inner(src: &str, filename: &str, options: &CompileOptions) -> Result<CompileResult, CompileError> {
    // outputConfig.sourcemap: every text stage below records its offset map in `chain` (no-op when off).
    let mut chain = srcmap::Chain::new(options.sourcemap.is_enabled());
    // 1. private-method forward rename (validate-only when the flag is off)
    let (src1, renamed) = {
        let alloc = oxc::allocator::Allocator::default();
        let program = parse::parse(&alloc, src, filename)?;
        let (edits, names) = transforms::private_methods::forward(&program, filename, options)?;
        let (out, pieces) = edits::apply_mapped(src, edits);
        chain.push_edit(pieces);
        (out, names)
    };
    // 2. LWC class transform
    let mut edited = pass(&src1, filename, &mut chain, |p, s| transforms::run(p, s, filename, options))?;
    // 3a. loose private fields (always on in shipped): `#x` -> classPrivateFieldLoose{Key,Base} + markers
    edited = pass(&edited, filename, &mut chain, |p, s| Ok(transforms::private_fields::run(p, s)))?;
    // 3. class-properties (loose), to a fixpoint so nested classes in moved initializers are lowered
    for _ in 0..32 {
        let mut changed = false;
        edited = pass(&edited, filename, &mut chain, |p, s| {
            let e = transforms::class_properties::run(p, s);
            changed = !e.is_empty();
            Ok(e)
        })?;
        if !changed { break; }
    }
    // 4. private-method reverse
    if options.enable_private_methods {
        edited = pass(&edited, filename, &mut chain, |p, _| transforms::private_methods::reverse(p, filename, &renamed))?;
    }
    let mut printed = {
        let alloc = oxc::allocator::Allocator::default();
        let program = parse::parse(&alloc, &edited, filename)
            .map_err(|e| CompileError::new(format!("internal: transformed output failed to re-parse: {}", e.message), filename))?;
        let (out, map) = codegen::print_mapped(&program, &edited, chain.enabled());
        if let Some(m) = map { chain.push_codegen(&m, &edited, &out); }
        out
    };
    // 5. object-rest-spread (babel runs it unless DISABLE_OBJECT_REST_SPREAD_TRANSFORMATION)
    if !api_version::is_api_feature_enabled(ApiFeature::DisableObjectRestSpreadTransformation, api_version::get_api_version_from_number(options.api_version)) {
        if let Some((out, map)) = transforms::lower::object_rest_spread(&printed, filename, chain.enabled())? {
            if let Some(m) = map { chain.push_codegen(&m, &printed, &out); }
            printed = out;
        }
    }
    // 6. LWS (javascript.ts appends these after rest/spread): unforgeables, async-to-generator,
    //    async-generator-functions.
    if options.enable_lightning_web_security_transforms {
        let e = transforms::unforgeables::run(&printed, filename)?;
        if !e.is_empty() {
            let (out, pieces) = edits::apply_mapped(&printed, e);
            chain.push_edit(pieces);
            printed = out;
        }
        if let Some((out, map)) = transforms::lower::async_functions(&printed, filename, chain.enabled())? {
            if let Some(m) = map { chain.push_codegen(&m, &printed, &out); }
            printed = out;
        }
    }
    // oxc codegen drops comments inside class bodies, so the version stamp is a post-codegen
    // span pass over the printed (normalized) code; output is not re-printed afterwards.
    let mut stamps = Vec::new();
    {
        let alloc = oxc::allocator::Allocator::default();
        let program = parse::parse(&alloc, &printed, filename)
            .map_err(|e| CompileError::new(format!("internal: codegen output failed to re-parse: {}", e.message), filename))?;
        transforms::version_stamp::run(&program, &mut stamps);
        // babel runtime helpers used by the lowering passes: module-local, defined once at the top
        if let Some((refs, pre)) = helpers::localize(&program, &printed) {
            stamps.extend(refs);
            stamps.push(edits::Edit::insert(0, pre));
        }
    }
    let (code, pieces) = edits::apply_mapped(&printed, stamps);
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

fn pass(
    src: &str,
    filename: &str,
    chain: &mut srcmap::Chain,
    f: impl FnOnce(&oxc::ast::ast::Program<'_>, &str) -> Result<Vec<edits::Edit>, CompileError>,
) -> Result<String, CompileError> {
    let alloc = oxc::allocator::Allocator::default();
    let program = parse::parse(&alloc, src, filename)
        .map_err(|e| CompileError::new(format!("internal: intermediate output failed to re-parse: {}", e.message), filename))?;
    let edits = f(&program, src)?;
    let (out, pieces) = edits::apply_mapped(src, edits);
    chain.push_edit(pieces);
    Ok(out)
}
