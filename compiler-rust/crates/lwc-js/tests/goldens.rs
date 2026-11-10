//! Normalized goldens: compare oxc-printed output with whitespace collapsed (NOT Babel bytes).
//! Regenerate: `UPDATE_GOLDENS=1 cargo test -p lwc-js --test goldens`.
use lwc_shared::CompileOptions;

fn normalize(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ") }

fn golden(name: &str) { golden_with(name, |_| {}) }

fn golden_with(name: &str, tweak: impl FnOnce(&mut CompileOptions)) {
    let dir = lwc_shared::cargo_manifest_dir!().join("tests/goldens").join(name);
    let src = std::fs::read_to_string(dir.join("input.js")).unwrap();
    let mut o = CompileOptions { name: "test".into(), namespace: "lwc".into(), ..Default::default() };
    tweak(&mut o);
    let out = lwc_js::transform(&src, "test.js", &o).unwrap().code;
    let exp_path = dir.join("expected.js");
    if std::env::var_os("UPDATE_GOLDENS").is_some() { std::fs::write(&exp_path, &out).unwrap(); }
    let exp = std::fs::read_to_string(exp_path).unwrap();
    assert_eq!(normalize(&out), normalize(&exp), "golden {name} mismatch\n--- actual ---\n{out}");
}

#[test] fn component_register_basic() { golden("component_register_basic"); }
#[test] fn round_trip_plain() { golden("round_trip_plain"); }
#[test] fn decorators_api() { golden("decorators_api"); }
#[test] fn decorators_track() { golden("decorators_track"); }
#[test] fn decorators_wire() { golden("decorators_wire"); }
#[test] fn decorators_anonymous() { golden("decorators_anonymous"); }
#[test] fn class_properties_lower() { golden("class_properties_lower"); }
#[test] fn private_methods_sandwich() { golden_with("private_methods_sandwich", |o| o.enable_private_methods = true); }
