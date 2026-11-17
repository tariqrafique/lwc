//! G1b / G1b-r behavior: `dispatch` takes only a validated [`NormalizedTransformOptions`] and does not
//! validate again. The by-construction half (private fields: no outside construction, no mutation after
//! validation, no `dispatch(&CompileOptions)`) is the `compile_fail` doctests on the type in options.rs.
use lwc_compiler::{OutputConfig, TransformOptions, dispatch, validate_transform_options};
use lwc_shared::{CompileOptions, DeprecationId};

fn minify_options() -> TransformOptions {
    let mut o = TransformOptions::from_compile(&CompileOptions {
        name: "a".into(),
        namespace: "x".into(),
        ..CompileOptions::default()
    });
    // The minify deprecation warns on every validate (not latched), so a second validate inside
    // dispatch would warn again.
    o.output_config = Some(
        OutputConfig {
            sourcemap: None,
            minify_present: true,
        }
        .into(),
    );
    o
}

/// Run by [`dispatch_does_not_validate_a_second_time`] in a child process (stderr is the observation).
#[test]
#[ignore = "child of dispatch_does_not_validate_a_second_time"]
fn child_dispatch_validated_minify_options() {
    let mut warned = Vec::new();
    let n = validate_transform_options(&minify_options(), &mut |d| {
        warned.push(d);
        Ok(())
    })
    .unwrap();
    assert_eq!(warned, [DeprecationId::OutputConfigMinify]);
    dispatch("export const a = 1;\n", "a.js", &n).unwrap();
}

#[test]
fn dispatch_does_not_validate_a_second_time() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "child_dispatch_validated_minify_options",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("1 passed"), "{stdout}");
    // Any validate without the caller's sink warns on stderr (lwc_compiler::warn_to_stderr).
    assert!(
        !stderr.contains(DeprecationId::OutputConfigMinify.message()),
        "dispatch validated the options a second time: {stderr}"
    );
}
