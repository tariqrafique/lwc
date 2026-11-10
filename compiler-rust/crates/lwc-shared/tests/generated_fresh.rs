//! `generated.rs` and the lwc-template parser tables must equal what
//! `tools/gen-shared-tables/gen.mjs` produces from the pinned @lwc checkout (`LWC_ORACLE`). Fails closed: a missing node or checkout is a failure, not a skip.
use std::path::Path;
use std::process::Command;

#[test]
fn generated_fresh() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let out = Command::new("node")
        .arg(repo.join("tools/gen-shared-tables/gen.mjs"))
        .args(["--check", "--root"])
        .arg(&repo)
        .current_dir(std::env::temp_dir())
        .output()
        .expect("node must be on PATH for generated_fresh");
    assert!(
        out.status.success(),
        "gen.mjs --check exited {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
