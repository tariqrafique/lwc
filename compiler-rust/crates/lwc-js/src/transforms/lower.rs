//! Babel preset-style syntax lowering passes implemented with oxc_transformer:
//! object rest/spread (`@babel/plugin-transform-object-rest-spread`, apiVersion < 60) and, for
//! `enableLightningWebSecurityTransforms`, `@babel/plugin-transform-async-to-generator` +
//! `@babel/plugin-transform-async-generator-functions`. Helpers load in External mode
//! (`babelHelpers.x`); `crate::helpers::localize` rewrites those members on the Program
//! and builds helper bodies from AST templates before the single print (D-0262 / D-0265).
use lwc_shared::CompileError;
use oxc::allocator::Allocator;
use oxc::ast::ast::Program;
use oxc::transformer::{EnvOptions, ES2017Options, ES2018Options, HelperLoaderMode, TransformOptions};
use oxc_sourcemap::SourceMap;

/// Object rest/spread on an already-parsed program (the pre-print AST). No-op when `src` has no `...`.
pub fn apply_object_rest_spread<'a>(
    alloc: &'a Allocator,
    src: &str,
    filename: &str,
    program: &mut Program<'a>,
) -> Result<(), CompileError> {
    if !src.contains("...") { return Ok(()); }
    let es2017 = EnvOptions::from_target("es2017").map_err(|e| CompileError::new(format!("internal: {e}"), filename))?;
    let mut env = EnvOptions::default();
    env.es2018 = ES2018Options { async_generator_functions: false, ..es2017.es2018 };
    apply_env(alloc, src, filename, env, program)
}

/// The sole copy of async_functions' cheap-exit predicate (AB-L0075). Called only from
/// [`async_functions`]; [`apply_async_functions`] does not repeat it.
fn source_has_async(src: &str) -> bool {
    src.contains("async")
}

/// async functions -> generators on an already-parsed program.
pub fn apply_async_functions<'a>(
    alloc: &'a Allocator,
    src: &str,
    filename: &str,
    program: &mut Program<'a>,
) -> Result<(), CompileError> {
    let mut env = EnvOptions::default();
    env.es2017 = ES2017Options { async_to_generator: true };
    env.es2018 = ES2018Options { object_rest_spread: None, async_generator_functions: true };
    apply_env(alloc, src, filename, env, program)
}

/// Used when async must run on already-printed code (after unforgeables text edits). Returns the
/// reprinted code, its map, and post-codegen stamp/helper edits so the caller never re-parses the
/// reprint (D-0262).
/// SHIM(D-0262 P1 async_functions, expires D-0265): parses printed JS to reprint after unforgeables (D-0264 residual 3).
pub fn async_functions(
    code: &str,
    filename: &str,
    with_map: bool,
) -> Result<Option<(String, Option<SourceMap<'static>>, Vec<lwc_shared::edits::Edit>, crate::helpers::LocalizedHelpers)>, CompileError> {
    if !source_has_async(code) { return Ok(None); }
    let alloc = Allocator::default();
    let mut program = crate::parse::parse(&alloc, code, filename)
        .map_err(|e| CompileError::new(format!("internal: lowering input failed to parse: {}", e.message), filename))?;
    apply_async_functions(&alloc, code, filename, &mut program)?;
    // Only members this pass introduced are still `babelHelpers.*`; earlier localize
    // already rewrote the rest to local idents (AB-L0075).
    let extra = crate::helpers::localize(&alloc, &mut program);
    let (out, map) = crate::codegen::print_mapped(&program, code, true);
    let stamps = match &map {
        Some(m) => crate::helpers::post_codegen_edits(&program, code, &out, m, filename)?,
        None => panic!("codegen with_map always produces a map"),
    };
    let map = if with_map { map } else { None };
    Ok(Some((out, map, stamps, extra)))
}

fn apply_env<'a>(
    alloc: &'a Allocator,
    src: &str,
    filename: &str,
    env: EnvOptions,
    program: &mut Program<'a>,
) -> Result<(), CompileError> {
    let scoping = crate::parse::scoping(program);
    let mut opts = TransformOptions::default();
    opts.env = env;
    opts.helper_loader.mode = HelperLoaderMode::External;
    crate::parse::transform(alloc, src, filename, &opts, scoping, program)
        .map_err(|e| CompileError::new(format!("internal: lowering failed: {}", e.message), filename))
}
