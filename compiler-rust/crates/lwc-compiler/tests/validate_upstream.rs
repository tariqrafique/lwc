//! G1: validation outcomes match @lwc/compiler 9.5.3 @ b9e8007.
//! Null / non-number apiVersion / unknown ssrMode are compared against live `transformSync`.
use lwc_compiler::{
    ApiVersionInput, DynamicImportConfig, SsrModeInput, TransformOptions, dispatch,
    validate_incoming, validate_transform_options,
};
use lwc_shared::{
    CompileOptions, CompilerMetric, InstrumentationEvent, InstrumentationHandle,
    InstrumentationRecorder, SsrMode,
};

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
    let n = validate_transform_options(&o).unwrap();
    assert_eq!(n.ssr_mode, SsrMode::AsyncYield);
    assert_eq!(n.ssr_mode_emit.as_deref(), Some(r#""bogus""#));
    o.ssr_mode = Some(SsrModeInput::NonString("null".into()));
    let n = validate_transform_options(&o).unwrap();
    assert_eq!(n.ssr_mode, SsrMode::AsyncYield);
    assert_eq!(n.ssr_mode_emit.as_deref(), Some("null"));
}

#[test]
fn unknown_ssr_mode_matches_oracle_ssr_output() {
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
    let js = "import { LightningElement } from 'lwc';\nexport default class extends LightningElement {}\n";
    let html = "<template>hi</template>";
    let script = format!(
        r#"
const {{ transformSync }} = require(process.env.LWC_ORACLE + '/packages/@lwc/compiler/dist/index.cjs');
const js = {js};
const html = {html};
const base = {{ name: 'widget', namespace: 'x', targetSSR: true }};
const out = {{}};
for (const [k, v] of Object.entries({{
  bogus: 'bogus', asyncYield: 'asyncYield', sync: 'sync', nil: null,
}})) {{
  const opts = {{ ...base, ssrMode: v }};
  out[k] = {{
    js: transformSync(js, 'x/widget/widget.js', opts).code,
    html: transformSync(html, 'x/widget/widget.html', opts).code,
  }};
}}
console.log(JSON.stringify(out));
"#,
        js = serde_json::to_string(js).unwrap(),
        html = serde_json::to_string(html).unwrap(),
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
    let want: serde_json::Value = serde_json::from_slice(&out.stdout).expect("oracle json");
    let rust = |mode: Option<SsrModeInput>, file: &str, src: &str| {
        let mut o = bare();
        o.name = "widget".into();
        o.namespace = "x".into();
        o.target_ssr = Some(true);
        o.ssr_mode = mode;
        let n = validate_transform_options(&o).unwrap();
        dispatch(src, file, &n).unwrap().code
    };
    // Printer whitespace already differs from astring; compare the mode literal and the
    // transmogrify branch (asyncYield shape vs sync), which is the G1a contract.
    fn mode_arg(code: &str) -> &str {
        let needle = ", __lwcTmpl)";
        let end = code.find(needle).expect(code);
        let head = &code[..end];
        let start = head.rfind(", ").expect(code) + 2;
        &head[start..]
    }
    fn async_yield_shape(html: &str) -> bool {
        html.contains("async function*") && html.contains("yield ") && !html.contains("__lwcYield")
    }
    let mut rust_html = std::collections::BTreeMap::new();
    for (k, mode) in [
        ("bogus", Some(SsrModeInput::Text("bogus".into()))),
        ("asyncYield", Some(SsrModeInput::Text("asyncYield".into()))),
        ("sync", Some(SsrModeInput::Text("sync".into()))),
        ("nil", Some(SsrModeInput::NonString("null".into()))),
    ] {
        let got_js = rust(mode.clone(), "x/widget/widget.js", js);
        let got_html = rust(mode, "x/widget/widget.html", html);
        let want_js = want[k]["js"].as_str().unwrap();
        let want_html = want[k]["html"].as_str().unwrap();
        assert_eq!(
            mode_arg(&got_js),
            mode_arg(want_js),
            "js compilationMode {k}\n{got_js}\n{want_js}"
        );
        assert_eq!(
            async_yield_shape(&got_html),
            async_yield_shape(want_html),
            "html shape {k}\n{got_html}\n{want_html}"
        );
        rust_html.insert(k, got_html);
    }
    assert_eq!(
        mode_arg(want["bogus"]["js"].as_str().unwrap()),
        r#""bogus""#
    );
    assert_eq!(mode_arg(want["nil"]["js"].as_str().unwrap()), "null");
    assert!(async_yield_shape(want["bogus"]["html"].as_str().unwrap()));
    assert!(async_yield_shape(want["nil"]["html"].as_str().unwrap()));
    assert!(!async_yield_shape(want["sync"]["html"].as_str().unwrap()));
    assert_eq!(
        rust_html["bogus"], rust_html["asyncYield"],
        "unknown mode keeps the asyncYield template shape"
    );
    assert_eq!(
        rust_html["bogus"], rust_html["nil"],
        "null ssrMode keeps the asyncYield template shape"
    );
    assert_ne!(
        rust_html["bogus"], rust_html["sync"],
        "unknown mode must not snap to sync"
    );
}

#[test]
fn instrumentation_event_sequence_matches_oracle_for_js_and_template() {
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
    let js_src = r#"
            export async function test() {
                const x = await import("foo");
                const y = await import("bar");
                const z = await import("baz");
                return x + y + z + "yay";
            }
        "#;
    let html_src = r#"
                <template>
                    <x-dynamic lwc:dynamic={ctor}></x-dynamic>
                    <x-dynamic-two lwc:dynamic={ctor2}></x-dynamic-two>
                </template>
            "#;
    let script = format!(
        r#"
const {{ transformSync }} = require(process.env.LWC_ORACLE + '/packages/@lwc/compiler/dist/index.cjs');
function collect(src, file, extra) {{
  const events = [];
  transformSync(src, file, {{
    name: 'a',
    namespace: 'x',
    instrumentation: {{
      log: (d) => events.push({{ kind: 'log', message: d && (d.message || String(d)) }}),
      incrementCounter: (m) => events.push({{ kind: 'incrementCounter', metric: m }}),
    }},
    ...extra,
  }});
  return events;
}}
const out = {{
  js: collect({js}, 'foo.js', {{ dynamicImports: {{ loader: '@custom/loader', strictSpecifier: true }} }}),
  html: collect({html}, 'foo.html', {{ experimentalDynamicDirective: true }}),
}};
console.log(JSON.stringify(out));
"#,
        js = serde_json::to_string(js_src).unwrap(),
        html = serde_json::to_string(html_src).unwrap(),
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
    let want: serde_json::Value = serde_json::from_slice(&out.stdout).expect("oracle json");

    fn rust_events(
        src: &str,
        file: &str,
        tweak: impl FnOnce(&mut TransformOptions),
    ) -> Vec<String> {
        let rec = std::sync::Arc::new(InstrumentationRecorder::default());
        let mut o = bare();
        o.instrumentation = Some(InstrumentationHandle(rec.clone()));
        tweak(&mut o);
        let n = validate_transform_options(&o).unwrap();
        dispatch(src, file, &n).unwrap();
        rec.take()
            .into_iter()
            .map(|e| match e {
                InstrumentationEvent::IncrementCounter(m) => {
                    format!("incrementCounter:{}", m.js_value())
                }
                InstrumentationEvent::Log(d) => format!("log:{}", d.message),
            })
            .collect()
    }
    let rust_js = rust_events(js_src, "foo.js", |o| {
        o.dynamic_imports = Some(DynamicImportConfig {
            loader: Some("@custom/loader".into()),
            strict_specifier: Some(true),
        });
    });
    let rust_html = rust_events(html_src, "foo.html", |o| {
        o.experimental_dynamic_directive = Some(true);
    });
    let fmt = |arr: &serde_json::Value| -> Vec<String> {
        arr.as_array()
            .unwrap()
            .iter()
            .map(|e| {
                format!(
                    "{}:{}",
                    e["kind"].as_str().unwrap(),
                    e["metric"]
                        .as_str()
                        .or_else(|| e["message"].as_str())
                        .unwrap_or("")
                )
            })
            .collect()
    };
    assert_eq!(rust_js, fmt(&want["js"]), "js events vs oracle");
    assert_eq!(rust_html, fmt(&want["html"]), "template events vs oracle");
    assert_eq!(
        rust_js,
        vec![
            format!(
                "incrementCounter:{}",
                CompilerMetric::DynamicImportTransform.js_value()
            );
            3
        ]
    );
    assert_eq!(
        rust_html,
        vec![
            format!(
                "incrementCounter:{}",
                CompilerMetric::LwcDynamicDirective.js_value()
            );
            2
        ]
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
