//! G1: validation outcomes match @lwc/compiler 9.5.3 @ b9e8007.
//! Null / non-number apiVersion / unknown ssrMode are compared against live `transformSync`.
use lwc_compiler::{
    ApiVersionInput, SsrModeInput, TransformOptions, validate_incoming, validate_transform_options,
};
use lwc_shared::{CompileOptions, SsrMode};

fn bare() -> TransformOptions {
    TransformOptions::from_compile(&CompileOptions {
        name: "a".into(),
        namespace: "x".into(),
        ..CompileOptions::default()
    })
}

fn oracle(cases_js: &str) -> serde_json::Value {
    let root = lwc_shared::oracle_root();
    let head = std::process::Command::new("git")
        .args(["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .expect("git is required");
    let head = String::from_utf8(head.stdout).unwrap();
    assert!(
        head.starts_with("b9e8007"),
        "oracle {} is at {head:?}, want b9e8007",
        root.display()
    );
    let script = format!(
        r#"
const {{ transformSync }} = require(process.env.LWC_ORACLE + '/packages/@lwc/compiler/dist/index.cjs');
const src = 'export const a = 1;\n';
const file = 'x.js';
const base = {{ name: 'a', namespace: 'x' }};
function cap(fn) {{
  try {{
    const r = fn();
    return {{ ok: true, code: r.code, len: r.code.length }};
  }} catch (e) {{
    return {{
      ok: false,
      name: e && e.name,
      message: String(e && e.message),
      code: e && e.code === undefined ? null : e.code,
      ctor: e && e.constructor && e.constructor.name,
    }};
  }}
}}
const out = {{}};
{cases_js}
console.log(JSON.stringify(out));
"#
    );
    let out = std::process::Command::new("node")
        .args(["-e", &script])
        .env("LWC_ORACLE", &root)
        .output()
        .expect("node is required");
    assert!(
        out.status.success(),
        "oracle failed: {} {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("oracle json")
}

#[test]
fn undefined_options_is_lwc1023_matching_oracle() {
    let want = oracle(
        r#"
out.undefined = cap(() => transformSync(src, file, undefined));
out.missing = cap(() => transformSync(src, file));
"#,
    );
    for key in ["undefined", "missing"] {
        assert_eq!(want[key]["ok"], false, "{key} {want}");
        assert_eq!(want[key]["code"], 1023);
        assert_eq!(
            want[key]["message"],
            "LWC1023: Expected options object, received \"undefined\"."
        );
    }
    let e = validate_incoming(None, "undefined").unwrap_err();
    assert_eq!(e.diagnostics[0].code, Some(1023));
    assert_eq!(e.message, want["undefined"]["message"].as_str().unwrap());
}

#[test]
fn js_null_options_is_typeerror_on_the_oracle_and_lwc1023_on_the_json_seam() {
    let want = oracle(r#"out.null = cap(() => transformSync(src, file, null));"#);
    assert_eq!(want["null"]["ok"], false, "{want}");
    assert_eq!(want["null"]["ctor"], "TypeError");
    assert!(
        want["null"]["message"]
            .as_str()
            .unwrap()
            .contains("Cannot read properties of null (reading 'stylesheetConfig')"),
        "{want}"
    );
    // The JSON napi seam cannot distinguish JS null from undefined (both are JSON null),
    // so we emit LWC1023 with received "null" rather than a typeless TypeError.
    let e = validate_incoming(None, "null").unwrap_err();
    assert_eq!(e.diagnostics[0].code, Some(1023));
    assert_eq!(
        e.message,
        "LWC1023: Expected options object, received \"null\"."
    );
}

#[test]
fn bad_api_version_matches_oracle_success_and_snaps_to_latest() {
    let want = oracle(
        r#"
for (const [k, v] of Object.entries({
  string: '60', object: { n: 60 }, bool: true, nil: null,
})) {
  out[k] = cap(() => transformSync(src, file, { ...base, apiVersion: v }));
}
out.good = cap(() => transformSync(src, file, base));
"#,
    );
    for k in ["string", "object", "bool", "nil", "good"] {
        assert_eq!(want[k]["ok"], true, "{k} {want}");
        assert_eq!(want[k]["code"], want["good"]["code"], "{k}");
    }
    let mut o = bare();
    o.api_version = Some(ApiVersionInput::NonNumber);
    assert_eq!(
        validate_transform_options(&o).unwrap().api_version,
        lwc_shared::api_version::HIGHEST_API_VERSION
    );
}

#[test]
fn bad_ssr_mode_matches_oracle_success_on_csr() {
    let want = oracle(
        r#"
for (const [k, v] of Object.entries({
  bogus: 'bogus', number: 1, nil: null, empty: '', SYNC: 'SYNC',
})) {
  out[k] = cap(() => transformSync(src, file, { ...base, ssrMode: v }));
}
out.sync = cap(() => transformSync(src, file, { ...base, ssrMode: 'sync' }));
out.good = cap(() => transformSync(src, file, base));
"#,
    );
    for k in ["bogus", "number", "nil", "empty", "SYNC", "sync", "good"] {
        assert_eq!(want[k]["ok"], true, "{k} {want}");
        assert_eq!(
            want[k]["code"], want["good"]["code"],
            "CSR output must match (ssrMode unused) {k}"
        );
    }
    let mut o = bare();
    o.ssr_mode = Some(SsrModeInput::Text("bogus".into()));
    assert_eq!(
        validate_transform_options(&o).unwrap().ssr_mode,
        SsrMode::Sync
    );
    o.ssr_mode = Some(SsrModeInput::NonString);
    assert_eq!(
        validate_transform_options(&o).unwrap().ssr_mode,
        SsrMode::Sync
    );
}

#[test]
fn csr_render_derives_keys_from_the_schema_and_includes_instrumentation() {
    let json: serde_json::Value =
        serde_json::from_str(include_str!("../../../harness/src/rust-option-keys.json")).unwrap();
    let keys: Vec<_> = json["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert!(
        keys.contains(&"instrumentation"),
        "instrumentation must be in the schema"
    );
    let src = include_str!("../../../harness/src/csrRender.ts");
    assert!(
        src.contains("rust-option-keys.json"),
        "csrRender.ts must derive TRANSFORM_OPTION_KEYS from rust-option-keys.json"
    );
    assert!(
        !src.contains("const TRANSFORM_OPTION_KEYS = new Set(["),
        "csrRender.ts must not keep a hand-maintained key list"
    );
}

#[test]
fn options_rs_does_not_compute_discarded_deprecation_messages() {
    let src = include_str!("../src/options.rs");
    assert!(
        !src.contains("let _text"),
        "D-0212: do not compute unused DeprecationId::message()"
    );
}

#[test]
fn napi_to_compile_options_has_no_unused_filename_parameter() {
    let src = include_str!("../../lwc-compiler-napi/src/lib.rs");
    assert!(
        !src.contains("_filename:"),
        "dead unused filename parameter must stay gone"
    );
}
