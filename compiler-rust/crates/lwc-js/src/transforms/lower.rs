//! Babel preset-style syntax lowering passes implemented with oxc_transformer:
//! object rest/spread (`@babel/plugin-transform-object-rest-spread`, apiVersion < 60) and, for
//! `enableLightningWebSecurityTransforms`, `@babel/plugin-transform-async-to-generator` +
//! `@babel/plugin-transform-async-generator-functions`. Helpers load in External mode
//! (`babelHelpers.x`); `crate::helpers::localize` rewrites those members on the Program
//! and builds helper bodies from AST templates before the single print (D-0262 / D-0265).
use lwc_shared::CompileError;
use oxc::allocator::Allocator;
use oxc::ast::ast::Program;
use oxc::transformer::{
    ES2017Options, ES2018Options, EnvOptions, HelperLoaderMode, TransformOptions,
};

/// Object rest/spread on an already-parsed program (the pre-print AST).
/// No source-text prefilter: a `...` that lives only in a comment must not force the transform,
/// and a spread that is not spelled with those three characters in a way `contains` sees must not skip it.
pub fn apply_object_rest_spread<'a>(
    alloc: &'a Allocator,
    src: &str,
    filename: &str,
    program: &mut Program<'a>,
) -> Result<(), CompileError> {
    if !program_has_object_rest(program) {
        return Ok(());
    }
    let es2017 = EnvOptions::from_target("es2017")
        .map_err(|e| CompileError::new(format!("internal: {e}"), filename))?;
    let mut env = EnvOptions::default();
    env.es2018 = ES2018Options {
        async_generator_functions: false,
        ..es2017.es2018
    };
    apply_env(alloc, src, filename, env, program)
}

/// `true` when the program has an async function or arrow. Comments are not functions:
/// the word `async` in a comment does not count (AB-L0075).
pub fn program_has_async(program: &Program<'_>) -> bool {
    use oxc::ast_visit::{Visit, walk};
    struct F(bool);
    impl<'a> Visit<'a> for F {
        fn visit_function(
            &mut self,
            it: &oxc::ast::ast::Function<'a>,
            flags: oxc::semantic::ScopeFlags,
        ) {
            if it.r#async {
                self.0 = true;
                return;
            }
            if !self.0 {
                walk::walk_function(self, it, flags);
            }
        }
        fn visit_arrow_function_expression(
            &mut self,
            it: &oxc::ast::ast::ArrowFunctionExpression<'a>,
        ) {
            if it.r#async {
                self.0 = true;
                return;
            }
            if !self.0 {
                walk::walk_arrow_function_expression(self, it);
            }
        }
    }
    let mut f = F(false);
    f.visit_program(program);
    f.0
}

fn program_has_object_rest(program: &Program<'_>) -> bool {
    use oxc::ast_visit::{Visit, walk};
    struct F(bool);
    impl<'a> Visit<'a> for F {
        fn visit_object_expression(&mut self, it: &oxc::ast::ast::ObjectExpression<'a>) {
            if it
                .properties
                .iter()
                .any(|p| matches!(p, oxc::ast::ast::ObjectPropertyKind::SpreadProperty(_)))
            {
                self.0 = true;
                return;
            }
            if !self.0 {
                walk::walk_object_expression(self, it);
            }
        }
        fn visit_object_pattern(&mut self, it: &oxc::ast::ast::ObjectPattern<'a>) {
            if it.rest.is_some() {
                self.0 = true;
                return;
            }
            if !self.0 {
                walk::walk_object_pattern(self, it);
            }
        }
    }
    let mut f = F(false);
    f.visit_program(program);
    f.0
}

/// async functions -> generators on an already-parsed program.
pub fn apply_async_functions<'a>(
    alloc: &'a Allocator,
    src: &str,
    filename: &str,
    program: &mut Program<'a>,
) -> Result<(), CompileError> {
    let mut env = EnvOptions::default();
    env.es2017 = ES2017Options {
        async_to_generator: true,
    };
    env.es2018 = ES2018Options {
        object_rest_spread: None,
        async_generator_functions: true,
    };
    apply_env(alloc, src, filename, env, program)
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
    crate::parse::transform(alloc, src, filename, &opts, scoping, program).map_err(|e| {
        CompileError::new(
            format!("internal: lowering failed: {}", e.message),
            filename,
        )
    })
}
