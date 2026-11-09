//! AB-L0019: with the process cwd deleted, a relative `filename` throws Node's `process.cwd()`
//! error the way `@lwc/compiler` 9.5.3 does (checked against JS LWC run in a deleted cwd):
//! `Error: LWC1009: ENOENT: no such file or directory, uv_cwd`, no location, not aggregated in
//! recovery mode; empty source and absolute filenames never resolve the cwd.
//! Its own test binary: it changes the process cwd, which no other test may observe.
use lwc_shared::TransformError;
use lwc_style::{transform, CompileOptions};

fn compiler(e: TransformError) -> lwc_shared::CompilerError {
    match e { TransformError::Compiler(e) => e, e => panic!("not a CompilerError: {e:?}") }
}

#[test]
fn deleted_cwd_throws_like_node() {
    let dir = std::env::temp_dir().join(format!("lwc-style-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_current_dir(&dir).unwrap();
    std::fs::remove_dir(&dir).unwrap();

    for recovery in [false, true] {
        let opts = CompileOptions { experimental_error_recovery_mode: recovery, ..Default::default() };
        for src in ["a{}", "a{"] {
            let e = compiler(lwc_compiler::dispatch(src, "x.css", &opts).expect_err(src));
            assert_eq!(e.message, "Error: LWC1009: ENOENT: no such file or directory, uv_cwd", "{src} recovery={recovery}");
            assert_eq!((e.code, e.location), (Some(1009), None));
            assert_eq!(e.filename.as_deref(), Some("x.css"));
        }
    }
    let opts = CompileOptions::default();
    assert_eq!(transform("", "x.css", &opts).unwrap().code, "export default undefined");
    let e = compiler(lwc_compiler::dispatch("a{", "/abs/x.css", &opts).expect_err("unclosed"));
    assert_eq!(e.message, "CssSyntaxError: LWC1009: /abs/x.css:1:1: Unclosed block");
    assert_eq!(e.location.map(|l| (l.line, l.column)), Some((1, 1)));
}
