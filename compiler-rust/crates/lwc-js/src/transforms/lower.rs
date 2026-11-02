//! Babel preset-style syntax lowering passes implemented with oxc_transformer:
//! object rest/spread (`@babel/plugin-transform-object-rest-spread`, apiVersion < 60) and, for
//! `enableLightningWebSecurityTransforms`, `@babel/plugin-transform-async-to-generator` +
//! `@babel/plugin-transform-async-generator-functions`. Helpers load in External mode
//! (`babelHelpers.x`); `crate::helpers::prelude` supplies their bodies once at the end.
use lwc_shared::CompileError;
use oxc::allocator::Allocator;
use oxc::semantic::SemanticBuilder;
use oxc::transformer::{EnvOptions, ES2017Options, ES2018Options, HelperLoaderMode, TransformOptions, Transformer};
use oxc_sourcemap::SourceMap;
use std::path::Path;

pub type Lowered = (String, Option<SourceMap<'static>>);

/// Object rest/spread only (non-loose, babel defaults). Identity (no map) when there is no `...`.
pub fn object_rest_spread(code: &str, filename: &str, with_map: bool) -> Result<Option<Lowered>, CompileError> {
    if !code.contains("...") { return Ok(None); }
    // ObjectRestSpreadOptions is not re-exported; take es2018 from an es2017 target and keep only rest/spread.
    let es2017 = EnvOptions::from_target("es2017").map_err(|e| CompileError::new(format!("internal: {e}"), filename))?;
    let mut env = EnvOptions::default();
    env.es2018 = ES2018Options { async_generator_functions: false, ..es2017.es2018 };
    run(code, filename, env, with_map).map(Some)
}

/// async functions -> generators (asyncToGenerator), async generators -> wrapAsyncGenerator & co.
pub fn async_functions(code: &str, filename: &str, with_map: bool) -> Result<Option<Lowered>, CompileError> {
    if !code.contains("async") { return Ok(None); }
    let mut env = EnvOptions::default();
    env.es2017 = ES2017Options { async_to_generator: true };
    env.es2018 = ES2018Options { object_rest_spread: None, async_generator_functions: true };
    run(code, filename, env, with_map).map(Some)
}

fn run(code: &str, filename: &str, env: EnvOptions, with_map: bool) -> Result<Lowered, CompileError> {
    let alloc = Allocator::default();
    let mut program = crate::parse::parse(&alloc, code, filename)
        .map_err(|e| CompileError::new(format!("internal: lowering input failed to parse: {}", e.message), filename))?;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let mut opts = TransformOptions::default();
    opts.env = env;
    opts.helper_loader.mode = HelperLoaderMode::External;
    let ret = Transformer::new(&alloc, Path::new(filename), &opts).build_with_scoping(scoping, &mut program);
    if ret.diagnostics.has_errors() {
        let msg = ret.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>().join("; ");
        return Err(CompileError::babel("SyntaxError", msg, filename, None, None));
    }
    Ok(crate::codegen::print_mapped(&program, code, with_map))
}
