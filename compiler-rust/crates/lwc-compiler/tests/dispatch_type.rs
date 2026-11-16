//! G1b / G1b-r behavior: `dispatch` takes only a validated [`NormalizedTransformOptions`] and does not
//! validate again. The by-construction half (private fields: no outside construction, no mutation after
//! validation, no `dispatch(&CompileOptions)`) is the `compile_fail` doctests on the type in options.rs.
use lwc_compiler::{
    OutputConfig, TransformOptions, dispatch, take_validation_deprecations,
    validate_transform_options,
};
use lwc_shared::{CompileOptions, DeprecationId};

#[test]
fn dispatch_does_not_validate_a_second_time() {
    let mut o = TransformOptions::from_compile(&CompileOptions {
        name: "a".into(),
        namespace: "x".into(),
        ..CompileOptions::default()
    });
    // The minify deprecation is recorded on every validate (not latched), so a second validate
    // inside dispatch would record it again.
    o.output_config = Some(OutputConfig {
        sourcemap: None,
        minify_present: true,
    });
    let n = validate_transform_options(&o).unwrap();
    assert_eq!(n.deprecations(), [DeprecationId::OutputConfigMinify]);
    assert_eq!(
        take_validation_deprecations(),
        vec![DeprecationId::OutputConfigMinify]
    );
    dispatch("export const a = 1;\n", "a.js", &n).unwrap();
    assert!(
        take_validation_deprecations().is_empty(),
        "dispatch validated the options a second time"
    );
}
