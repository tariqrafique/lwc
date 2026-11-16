//! G1b: `dispatch` accepts only a validated [`NormalizedTransformOptions`].
//! That type carries a private token, so an unvalidated [`CompileOptions`] cannot be
//! constructed as one and cannot be passed. This test locks the signature and the
//! absence of a second validate inside `dispatch`.
use std::fs;

#[test]
fn dispatch_takes_normalized_transform_options_by_construction() {
    let root = lwc_shared::cargo_manifest_dir!().join("src");
    let lib = fs::read_to_string(root.join("lib.rs")).unwrap();
    let start = lib.find("pub fn dispatch(").expect("public dispatch");
    let rest = &lib[start..];
    // The function is the signature plus its body; stop at the next item.
    let end = rest
        .find("\nfn ")
        .or_else(|| rest.find("\npub fn "))
        .unwrap_or(rest.len());
    let dispatch = &rest[..end];
    assert!(
        dispatch.contains("options: &NormalizedTransformOptions"),
        "dispatch must take the validated type, not CompileOptions:\n{dispatch}"
    );
    assert!(
        !dispatch.contains("prepare_compile_options")
            && !dispatch.contains("validate_transform_options")
            && !dispatch.contains("validate_incoming")
            && !dispatch.contains("normalize_compile_options"),
        "dispatch must not validate a second time:\n{dispatch}"
    );
    assert!(
        dispatch.contains("transformers::transform_file"),
        "dispatch should forward to transform_file:\n{dispatch}"
    );

    let options = fs::read_to_string(root.join("options.rs")).unwrap();
    assert!(
        options.contains("_validated: ()"),
        "NormalizedTransformOptions must carry a private token so only validate_* can construct it"
    );
    // napi validates once and passes the normalized type through (no into_compile_options before dispatch).
    let napi = fs::read_to_string(
        lwc_shared::cargo_manifest_dir!().join("../lwc-compiler-napi/src/errors.rs"),
    )
    .unwrap();
    assert!(
        napi.contains("o: &NormalizedTransformOptions"),
        "napi compile() must pass the validated type to dispatch"
    );
}
