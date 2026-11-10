//! AB-D0004 boundary lint: lwc-shared is the shared vocabulary, so its non-dev dependencies are limited to
//! the oxc *type* crates (no parser, codegen, semantic, or the `oxc` umbrella that re-exports them).
//! Cargo is the only model of its manifest: the dependency list comes from `cargo metadata`, whose
//! `name` is the real package (renames are in `rename`) and whose `kind` is null (normal), "build" or "dev".

use std::path::Path;

const ALLOWED: &[&str] = &["oxc_allocator", "oxc_ast", "oxc_span"];

#[test]
fn lwc_shared_dependencies_are_allowlisted() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest = lwc_shared::cargo_manifest_dir!().join("Cargo.toml");
    let out = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps", "--offline", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("run cargo metadata");
    assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("cargo metadata JSON");
    let pkg = meta["packages"].as_array().expect("packages").iter()
        .find(|p| p["manifest_path"].as_str().map(Path::new) == Some(manifest.as_path()))
        .expect("lwc-shared in cargo metadata");
    let deps = pkg["dependencies"].as_array().expect("dependencies");
    let runtime: Vec<&str> = deps.iter()
        .filter(|d| matches!(d["kind"].as_str(), None | Some("build")))
        .map(|d| d["name"].as_str().expect("dependency name"))
        .collect();
    let bad: Vec<&str> = runtime.iter().copied().filter(|d| !ALLOWED.contains(d)).collect();
    assert!(bad.is_empty(), "lwc-shared may depend only on {ALLOWED:?} (AB-D0004); found {bad:?}");
    // The gate reads real data: the allowlisted crates are present as normal dependencies.
    assert!(ALLOWED.iter().all(|a| runtime.contains(a)), "expected {ALLOWED:?} among {runtime:?}");
}
