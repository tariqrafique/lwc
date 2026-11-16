//! napi-rs binding: facade over `lwc_compiler::dispatch` only (no napi in lwc-style).
//! Compile errors are returned as `{ error: {...} }` so the harness can throw a JS Error
//! carrying the upstream CompilerError / AggregateError own keys (napi v2 `Error` cannot).
//! Position lives only under `location` (D-0239 / D-0241); no own `line` / `column` / Aggregate `name`.
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::{Deserialize, Serialize};

mod errors;

/// Options accepted over the seam. Taken as JSON (not a `#[napi(object)]`, which silently ignores extra
/// keys) and deserialized with `deny_unknown_fields`, so any unknown key, top-level or nested, is an
/// error naming the key. Field names are [`lwc_compiler::OPTION_KEYS`] (harness reads the generated JSON).
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JsCompileOptions {
    pub name: String,
    pub namespace: String,
    pub scoped_styles: Option<bool>,
    pub disable_synthetic_shadow_support: Option<bool>,
    /// Any JSON value. Non-numbers snap to latest, matching `getAPIVersionFromNumber`.
    #[serde(default)]
    pub api_version: Option<serde_json::Value>,
    #[serde(rename = "targetSSR")]
    pub target_ssr: Option<bool>,
    /// Any JSON value. Upstream does not validate `CompilationMode`; unknown is not an error.
    pub ssr_mode: Option<serde_json::Value>,
    pub experimental_error_recovery_mode: Option<bool>,
    pub enable_private_methods: Option<bool>,
    pub experimental_complex_expressions: Option<bool>,
    pub preserve_html_comments: Option<bool>,
    pub enable_static_content_optimization: Option<bool>,
    pub enable_dynamic_components: Option<bool>,
    pub enable_lwc_on: Option<bool>,
    pub experimental_dynamic_directive: Option<bool>,
    pub enable_synthetic_element_internals: Option<bool>,
    pub component_feature_flag_module_path: Option<String>,
    pub dynamic_imports: Option<JsDynamicImports>,
    pub enable_lightning_web_security_transforms: Option<bool>,
    /// Accepted so it is not an unknown key (D-0203). A JS callback cannot cross JSON; dropped here.
    pub instrumentation: Option<serde_json::Value>,
    pub is_explicit_import: Option<bool>,
    pub output_config: Option<JsOutputConfig>,
    pub custom_renderer_config: Option<JsCustomRendererConfig>,
    pub stylesheet_config: Option<JsStylesheetConfig>,
}

/// `stylesheetConfig`: `{ customProperties?: { resolution?: {type:'native'} | {type:'module', name} } }`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JsStylesheetConfig {
    pub custom_properties: Option<JsCustomProperties>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JsCustomProperties {
    pub resolution: Option<JsResolution>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum JsResolution {
    Native {},
    Module { name: String },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JsDynamicImports {
    pub loader: Option<String>,
    pub strict_specifier: Option<bool>,
}

/// `outputConfig`: `sourcemap?: boolean | 'inline'`, `minify?` (deprecated, ignored upstream).
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JsOutputConfig {
    pub sourcemap: Option<JsSourcemap>,
    /// Deprecated upstream (warns, no effect); accepted and ignored.
    pub minify: Option<serde_json::Value>,
}

/// JS `String(value)` for a JSON value: objects `[object Object]`, arrays `join(",")` (null items empty).
fn js_string(v: &serde_json::Value) -> String {
    use serde_json::Value as V;
    match v {
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::Number(n) => n.to_string(),
        V::String(s) => s.clone(),
        V::Array(a) => a
            .iter()
            .map(|x| {
                if x.is_null() {
                    String::new()
                } else {
                    js_string(x)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        V::Object(_) => "[object Object]".into(),
    }
}

/// Any JSON value: validated into `lwc_shared::Sourcemap` (non-bool, non-'inline' → LWC1021 like JS).
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum JsSourcemap {
    Bool(bool),
    Str(String),
    Other(serde_json::Value),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JsCustomRendererConfig {
    pub directives: Vec<String>,
    pub elements: Vec<JsCustomRendererElement>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JsCustomRendererElement {
    pub tag_name: String,
    pub namespace: Option<String>,
    pub attributes: Option<Vec<String>>,
}

#[napi(object)]
pub struct JsDiagnostic {
    /// `None` for an uncoded diagnostic → JS `null`.
    pub code: Option<u32>,
    pub level: u32,
    pub message: Option<String>,
    pub filename: Option<String>,
    /// `@lwc/errors` CompilerDiagnostic `location` shape (`normalizeLocation`): `{ line, column, start, length }`.
    pub location: Option<JsDiagnosticLocation>,
}

#[napi(object)]
pub struct JsDiagnosticLocation {
    pub line: u32,
    pub column: u32,
    pub start: Option<u32>,
    pub length: Option<u32>,
    /// Babel parser `Position.index` (UTF-16 offset): set only on a Babel-parse LWC1007 (lwc-js `parse_error`).
    pub index: Option<u32>,
}

fn to_js_diagnostic(d: lwc_shared::CompilerDiagnostic) -> JsDiagnostic {
    JsDiagnostic {
        code: d.code,
        level: d.level.js_value(),
        location: d.location.map(Into::into),
        filename: d.filename,
        message: Some(d.message),
    }
}

impl From<lwc_shared::DiagnosticLocation> for JsDiagnosticLocation {
    fn from(l: lwc_shared::DiagnosticLocation) -> Self {
        Self {
            line: l.line,
            column: l.column,
            start: l.start,
            length: l.length,
            index: l.index,
        }
    }
}

#[napi(object)]
pub struct JsCompileError {
    pub message: String,
    pub code: Option<u32>,
    /// Same own-but-`undefined` shape as `location` (`Either::B`): shipped `generateCompilerError` with no origin.
    pub filename: Option<Either<String, Undefined>>,
    /// `{ line, column, start, length }` of the (first) diagnostic. Absent on an aggregate (shipped);
    /// on a single error shipped always has the own key, `undefined` when there is no location (`Either::B`).
    /// The only source of position (D-0239 / D-0241); no mirrored own `line` / `column`.
    pub location: Option<Either<JsDiagnosticLocation, Undefined>>,
    /// Recovery-mode `CompilerAggregateError.errors`: one entry per sub-error, in shipped order.
    pub errors: Option<Vec<JsAggregateEntry>>,
    /// Single errors only (shipped `CompilerError.level`); absent on an aggregate. See `single_error.rs`.
    pub level: Option<u32>,
    /// Single errors only (shipped `CompilerError.url`); absent on an aggregate. See `single_error.rs`.
    pub url: Option<String>,
}

impl JsCompileError {
    /// The JS object's own keys, in order: napi `#[napi(object)]` sets every `Some` field and omits every
    /// `None` field (`Object.keys` / `hasOwnProperty` semantics). Keep in field order with the struct.
    #[cfg(test)]
    pub(crate) fn own_keys(&self) -> Vec<&'static str> {
        let mut k = vec!["message"];
        for (key, set) in [
            ("code", self.code.is_some()),
            ("filename", self.filename.is_some()),
            ("location", self.location.is_some()),
            ("errors", self.errors.is_some()),
            ("level", self.level.is_some()),
            ("url", self.url.is_some()),
        ] {
            if set {
                k.push(key);
            }
        }
        k
    }
}

/// One `CompilerAggregateError.errors[]` entry (shipped `CompilerError.from(diagnostic)`); see `aggregate.rs`.
#[napi(object)]
pub struct JsAggregateEntry {
    /// Always "Error" (shipped: inherited from `Error.prototype`, not an own property).
    pub name: String,
    pub message: String,
    pub code: Option<u32>,
    pub filename: Option<String>,
    pub location: Option<JsDiagnosticLocation>,
    pub level: u32,
    pub url: String,
}

#[napi(object)]
pub struct JsCompileResult {
    pub code: String,
    /// Shipped 9.5.3 shape: `sourcemap: true` -> a plain object `{ version, names, sources, sourcesContent,
    /// mappings, ignoreList }` (shipped also has `file`/`sourceRoot` set to `undefined`); `'inline'` / off -> `null`.
    pub map: serde_json::Value,
    #[napi(js_name = "cssScopeTokens")]
    pub css_scope_tokens: Option<Vec<String>>,
    pub warnings: Option<Vec<JsDiagnostic>>,
    pub error: Option<JsCompileError>,
    /// D-0205: ordered `log` / `incrementCounter` calls. The loader replays them and strips this field.
    #[napi(js_name = "instrumentationEvents")]
    pub instrumentation_events: Option<Vec<JsInstrumentationEvent>>,
    /// D-0212: `console.warn` texts. The loader prints them and strips this field.
    pub deprecations: Option<Vec<String>>,
}

#[napi(object)]
pub struct JsInstrumentationEvent {
    pub kind: String,
    pub metric: Option<String>,
    pub diagnostic: Option<JsDiagnostic>,
}

/// The JSON map string from the compiler -> a JS object (`null` when absent).
fn map_value(map: Option<&str>) -> serde_json::Value {
    map.and_then(|m| serde_json::from_str(m).ok())
        .unwrap_or(serde_json::Value::Null)
}

thread_local! {
    static NAPI_RECORDER: std::cell::RefCell<Option<std::sync::Arc<lwc_shared::InstrumentationRecorder>>> =
        const { std::cell::RefCell::new(None) };
}

fn take_recorded_events() -> Option<Vec<JsInstrumentationEvent>> {
    let rec = NAPI_RECORDER.with(|c| c.borrow_mut().take())?;
    let events: Vec<JsInstrumentationEvent> = rec
        .take()
        .into_iter()
        .map(|e| match e {
            lwc_shared::InstrumentationEvent::IncrementCounter(m) => JsInstrumentationEvent {
                kind: "incrementCounter".into(),
                metric: Some(m.js_value().to_string()),
                diagnostic: None,
            },
            lwc_shared::InstrumentationEvent::Log(d) => JsInstrumentationEvent {
                kind: "log".into(),
                metric: None,
                diagnostic: Some(to_js_diagnostic(d)),
            },
        })
        .collect();
    if events.is_empty() {
        None
    } else {
        Some(events)
    }
}

fn attach_side_channels(
    mut r: JsCompileResult,
    deprecations: Vec<lwc_shared::DeprecationId>,
) -> JsCompileResult {
    r.instrumentation_events = take_recorded_events();
    r.deprecations = if deprecations.is_empty() {
        None
    } else {
        Some(
            deprecations
                .into_iter()
                .map(|d| d.message().to_string())
                .collect(),
        )
    };
    r
}

/// Deserialize only. All validation lives in `lwc_compiler::validate_incoming`.
/// `Err(napi)` = malformed JSON or an unknown key (D-0203, `deny_unknown_fields`).
/// `Ok(Err(compile))` = LWC validation error (LWC1021 / LWC1023).
fn to_compile_options(
    options: serde_json::Value,
) -> Result<std::result::Result<lwc_compiler::NormalizedTransformOptions, lwc_shared::CompileError>>
{
    if options.is_null() {
        NAPI_RECORDER.with(|c| *c.borrow_mut() = None);
        return Ok(lwc_compiler::validate_incoming(None, &js_string(&options)));
    }
    // `Option<Value>` maps JSON null to None, so read ssrMode off the raw object to
    // distinguish omitted (default sync) from explicit null (asyncYield path, emit null).
    let ssr_mode_raw = options.get("ssrMode").cloned();
    let inst_present = options.get("instrumentation").is_some_and(|v| !v.is_null());
    let wire: JsCompileOptions = serde_json::from_value(options).map_err(|e| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid compile options: {e}"),
        )
    })?;
    let mut raw = wire_to_transform(wire);
    if let Some(v) = ssr_mode_raw {
        raw.ssr_mode = match v {
            serde_json::Value::String(s) => Some(lwc_compiler::SsrModeInput::Text(s)),
            other => Some(lwc_compiler::SsrModeInput::NonString(other.to_string())),
        };
    }
    if inst_present && raw.instrumentation.is_none() {
        // A JS InstrumentationObject cannot cross JSON; attach a recorder so events can
        // be returned for the loader to replay (D-0205 / G2b).
        let rec = std::sync::Arc::new(lwc_shared::InstrumentationRecorder::default());
        raw.instrumentation = Some(lwc_shared::InstrumentationHandle(rec.clone()));
        NAPI_RECORDER.with(|c| *c.borrow_mut() = Some(rec));
    } else {
        NAPI_RECORDER.with(|c| *c.borrow_mut() = None);
    }
    Ok(lwc_compiler::validate_incoming(
        Some(&raw),
        &js_string(&serde_json::Value::Null),
    ))
}

/// Map the wire struct onto [`lwc_compiler::TransformOptions`]. No defaults, no type checks, no LWC codes.
fn wire_to_transform(options: JsCompileOptions) -> lwc_compiler::TransformOptions {
    use serde_json::Value as V;
    let api_version = match options.api_version {
        None | Some(V::Null) => None,
        Some(V::Number(n)) => Some(lwc_compiler::ApiVersionInput::Number(
            n.as_f64().unwrap_or(f64::NAN),
        )),
        Some(_) => Some(lwc_compiler::ApiVersionInput::NonNumber),
    };
    let ssr_mode = match options.ssr_mode {
        None => None,
        Some(V::String(s)) => Some(lwc_compiler::SsrModeInput::Text(s)),
        Some(v) => Some(lwc_compiler::SsrModeInput::NonString(v.to_string())),
    };
    lwc_compiler::TransformOptions {
        name: options.name,
        namespace: options.namespace,
        scoped_styles: options.scoped_styles,
        disable_synthetic_shadow_support: options.disable_synthetic_shadow_support,
        api_version,
        target_ssr: options.target_ssr,
        ssr_mode,
        experimental_error_recovery_mode: options.experimental_error_recovery_mode,
        enable_private_methods: options.enable_private_methods,
        experimental_complex_expressions: options.experimental_complex_expressions,
        preserve_html_comments: options.preserve_html_comments,
        enable_static_content_optimization: options.enable_static_content_optimization,
        enable_dynamic_components: options.enable_dynamic_components,
        enable_lwc_on: options.enable_lwc_on,
        experimental_dynamic_directive: options.experimental_dynamic_directive,
        enable_synthetic_element_internals: options.enable_synthetic_element_internals,
        component_feature_flag_module_path: options.component_feature_flag_module_path,
        enable_lightning_web_security_transforms: options.enable_lightning_web_security_transforms,
        is_explicit_import: options.is_explicit_import,
        // Callbacks cannot cross JSON; `to_compile_options` attaches a recorder when the key is present.
        instrumentation: None,
        dynamic_imports: options
            .dynamic_imports
            .map(|d| lwc_compiler::DynamicImportConfig {
                loader: d.loader,
                strict_specifier: d.strict_specifier,
            }),
        output_config: options.output_config.map(|c| lwc_compiler::OutputConfig {
            sourcemap: c.sourcemap.map(|sm| match sm {
                JsSourcemap::Bool(b) => lwc_compiler::SourcemapInput::Bool(b),
                JsSourcemap::Str(s) => lwc_compiler::SourcemapInput::Text(s),
                JsSourcemap::Other(v) => lwc_compiler::SourcemapInput::Text(js_string(&v)),
            }),
            minify_present: c.minify.is_some(),
        }),
        custom_renderer_config: options.custom_renderer_config.map(|c| {
            lwc_shared::CustomRendererConfig {
                directives: c.directives,
                elements: c
                    .elements
                    .into_iter()
                    .map(|e| lwc_shared::CustomRendererElementConfig {
                        tag_name: e.tag_name,
                        namespace: e.namespace,
                        attributes: e.attributes,
                    })
                    .collect(),
            }
        }),
        stylesheet_config: options
            .stylesheet_config
            .map(|c| lwc_compiler::StylesheetConfig {
                custom_properties: c.custom_properties.map(|p| lwc_compiler::CustomProperties {
                    resolution: p.resolution.map(|r| match r {
                        JsResolution::Native {} => lwc_shared::CustomPropertiesResolution::Native,
                        JsResolution::Module { name } => {
                            lwc_shared::CustomPropertiesResolution::Module { name }
                        }
                    }),
                }),
            }),
    }
}

#[cfg(test)]
mod tests {

    /// Nested key equality (not subset). `Err` names the path when the sets differ.
    fn nested_keys_equal(
        wire: &serde_json::Value,
        schema: &serde_json::Value,
        at: &str,
    ) -> std::result::Result<(), String> {
        use std::collections::BTreeSet;
        let wire_obj = wire
            .as_object()
            .ok_or_else(|| format!("{at}: wire is not an object"))?;
        let schema_obj = schema
            .as_object()
            .ok_or_else(|| format!("{at}: schema is not an object"))?;
        let wk: BTreeSet<_> = wire_obj.keys().cloned().collect();
        let sk: BTreeSet<_> = schema_obj
            .keys()
            .filter(|k| k.as_str() != "[]")
            .cloned()
            .collect();
        if wk != sk {
            return Err(format!(
                "nested keys at {at} must be equal, not a subset: wire={wk:?} schema={sk:?}"
            ));
        }
        for k in &sk {
            match schema_obj.get(k).unwrap() {
                serde_json::Value::Bool(true) => {}
                sub if sub.get("[]").is_some() => {
                    for (i, el) in wire_obj[k].as_array().unwrap().iter().enumerate() {
                        nested_keys_equal(el, &sub["[]"], &format!("{at}.{k}[{i}]"))?;
                    }
                }
                sub if sub.is_object() => {
                    nested_keys_equal(&wire_obj[k], sub, &format!("{at}.{k}"))?;
                }
                _ => return Err(format!("{at}.{k}: bad schema")),
            }
        }
        Ok(())
    }

    #[test]
    fn warning_carries_js_location_shape() {
        // JS: LWC1182 "Multiple if: directives ..." warning with location {line,column,start,length}.
        let o = to_compile_options(json!({"name": "a", "namespace": "x"}))
            .unwrap()
            .unwrap();
        let r = lwc_compiler::dispatch(
            "<template><div if:true={a} if:false={b}></div></template>",
            "a.html",
            &o,
        )
        .unwrap();
        let w = r
            .warnings
            .into_iter()
            .find(|w| w.code == Some(1182))
            .expect("1182 warning");
        let j = to_js_diagnostic(w);
        let l = j.location.expect("location");
        assert_eq!(l.line, 1);
        assert!(l.start.is_some() && l.length.is_some());
    }

    #[test]
    fn sourcemap_result_shape_matches_shipped() {
        let src = "export const a = 1;\n";
        let run = |sm: serde_json::Value| {
            let o = to_compile_options(
                json!({"name": "a", "namespace": "x", "outputConfig": {"sourcemap": sm}}),
            )
            .unwrap()
            .unwrap();
            map_value(
                lwc_compiler::dispatch(src, "a.js", &o)
                    .unwrap()
                    .map
                    .as_deref(),
            )
        };
        let m = run(json!(true));
        let o = m.as_object().expect("map is an object for sourcemap: true");
        assert_eq!(o["version"], json!(3));
        assert_eq!(o["sources"], json!(["a.js"]));
        assert_eq!(o["sourcesContent"], json!([src]));
        assert!(o["names"].is_array() && o["mappings"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(
            o.keys().all(|k| [
                "version",
                "names",
                "sources",
                "sourcesContent",
                "mappings",
                "ignoreList"
            ]
            .contains(&k.as_str())),
            "{o:?}"
        );
        assert_eq!(run(json!("inline")), serde_json::Value::Null);
        assert_eq!(run(json!(false)), serde_json::Value::Null);
    }

    use super::*;
    use serde_json::json;

    fn err(v: serde_json::Value) -> String {
        to_compile_options(v).err().expect("must reject").reason
    }

    #[test]
    fn rejects_unknown_keys_naming_them() {
        assert!(err(json!({"name": "a", "namespace": "x", "bogusTop": 1})).contains("bogusTop"));
        assert!(
            err(json!({"name": "a", "namespace": "x", "dynamicImports": {"bogusNested": true}}))
                .contains("bogusNested")
        );
        assert!(
            err(json!({"name": "a", "namespace": "x", "outputConfig": {"sourceMap": true}}))
                .contains("sourceMap")
        );
        assert!(err(json!({"name": "a", "namespace": "x", "customRendererConfig": {"directives": [], "elements": [{"tagName": "div", "attrs": []}]}})).contains("attrs"));
        assert!(err(json!({"name": "a", "namespace": "x", "stylesheetConfig": {"customProperties": {"resolverModule": "m"}}})).contains("resolverModule"));
        assert!(err(json!({"name": "a", "namespace": "x", "stylesheetConfig": {"customProperties": {"resolution": {"type": "native", "extra": 1}}}})).contains("extra"));
        assert!(
            err(
                json!({"name": "a", "namespace": "x", "experimentalComputedMemberExpression": true})
            )
            .contains("experimentalComputedMemberExpression")
        );
    }

    #[test]
    fn defaults_come_from_lwc_shared() {
        let o = to_compile_options(json!({"name": "a", "namespace": "x"}))
            .unwrap()
            .unwrap()
            .into_compile_options();
        assert_eq!(
            o,
            lwc_shared::CompileOptions {
                name: "a".into(),
                namespace: "x".into(),
                api_version: Some(lwc_shared::api_version::HIGHEST_API_VERSION),
                experimental_complex_expressions: Some(false),
                preserve_html_comments: Some(false),
                enable_static_content_optimization: Some(true),
                ..Default::default()
            }
        );
        assert!(o.dynamic_import_strict_specifier);
        assert!(o.resolved_experimental_dynamic_directive());
        // dynamicImports present without strictSpecifier keeps the default (JS spreads over DEFAULT_DYNAMIC_IMPORT_CONFIG).
        let o = to_compile_options(
            json!({"name": "a", "namespace": "x", "dynamicImports": {"loader": "l"}}),
        )
        .unwrap()
        .unwrap()
        .into_compile_options();
        assert!(o.dynamic_import_strict_specifier);
    }

    #[test]
    fn maps_new_options() {
        let o = to_compile_options(json!({
            "name": "a", "namespace": "x", "isExplicitImport": true, "enableLightningWebSecurityTransforms": true,
            "outputConfig": {"sourcemap": "inline", "minify": false},
            "customRendererConfig": {"directives": ["dom"], "elements": [{"tagName": "svg", "namespace": "ns", "attributes": ["href"]}]},
            "stylesheetConfig": {"customProperties": {"resolution": {"type": "module", "name": "@x/r"}}}
        })).unwrap().unwrap().into_compile_options();
        assert!(o.is_explicit_import && o.enable_lightning_web_security_transforms);
        assert_eq!(o.sourcemap, lwc_shared::Sourcemap::Inline);
        let c = o.custom_renderer_config.unwrap();
        assert_eq!(
            (
                c.directives,
                c.elements[0].tag_name.as_str(),
                c.elements[0].attributes.clone()
            ),
            (
                vec!["dom".to_string()],
                "svg",
                Some(vec!["href".to_string()])
            )
        );
        assert_eq!(
            o.stylesheet_config.custom_properties_resolution,
            lwc_shared::CustomPropertiesResolution::Module {
                name: "@x/r".into()
            }
        );
        let o = to_compile_options(
            json!({"name": "a", "namespace": "x", "outputConfig": {"sourcemap": true}}),
        )
        .unwrap()
        .unwrap()
        .into_compile_options();
        assert_eq!(o.sourcemap, lwc_shared::Sourcemap::On);
    }

    #[test]
    fn js_string_matches_js() {
        assert_eq!(js_string(&serde_json::json!({})), "[object Object]");
        assert_eq!(js_string(&serde_json::json!([1, null, "a"])), "1,,a");
        assert_eq!(js_string(&serde_json::json!(42)), "42");
    }

    #[test]
    fn invalid_sourcemap_is_lwc1021() {
        for bad in [json!("foo"), json!(1)] {
            let e = to_compile_options(
                json!({"name": "a", "namespace": "x", "outputConfig": {"sourcemap": bad}}),
            )
            .unwrap()
            .unwrap_err();
            assert_eq!(e.diagnostics[0].code, Some(1021));
            assert!(
                e.message.starts_with(
                    "LWC1021: Expected a boolean value or 'inline' for outputConfig.sourcemap"
                ),
                "{}",
                e.message
            );
        }
    }

    /// AB-0008: the wire struct's names are the schema, not a second hand list. Unknown keys stay rejected (D-0203).
    #[test]
    fn wire_fields_equal_option_schema_and_unknown_keys_stay_rejected() {
        use std::collections::BTreeSet;
        let wire = JsCompileOptions {
            name: "a".into(),
            namespace: "x".into(),
            scoped_styles: None,
            disable_synthetic_shadow_support: None,
            api_version: Some(json!(60.5)),
            target_ssr: None,
            ssr_mode: None,
            experimental_error_recovery_mode: None,
            enable_private_methods: None,
            experimental_complex_expressions: None,
            preserve_html_comments: None,
            enable_static_content_optimization: None,
            enable_dynamic_components: None,
            enable_lwc_on: None,
            experimental_dynamic_directive: None,
            enable_synthetic_element_internals: None,
            component_feature_flag_module_path: None,
            dynamic_imports: Some(JsDynamicImports {
                loader: Some("l".into()),
                strict_specifier: Some(true),
            }),
            enable_lightning_web_security_transforms: None,
            instrumentation: None,
            is_explicit_import: None,
            output_config: Some(JsOutputConfig {
                sourcemap: Some(JsSourcemap::Str("inline".into())),
                minify: Some(serde_json::json!(false)),
            }),
            custom_renderer_config: Some(JsCustomRendererConfig {
                directives: vec!["dom".into()],
                elements: vec![JsCustomRendererElement {
                    tag_name: "svg".into(),
                    namespace: Some("ns".into()),
                    attributes: Some(vec!["href".into()]),
                }],
            }),
            stylesheet_config: Some(JsStylesheetConfig {
                custom_properties: Some(JsCustomProperties {
                    resolution: Some(JsResolution::Module {
                        name: "@x/r".into(),
                    }),
                }),
            }),
        };
        let got = serde_json::to_value(&wire).unwrap();
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../harness/src/rust-option-keys.json"))
                .unwrap();
        let keys = |v: &serde_json::Value| -> BTreeSet<String> {
            v.as_object().unwrap().keys().cloned().collect()
        };
        let want: BTreeSet<String> = lwc_compiler::OPTION_KEYS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(
            keys(&got),
            want,
            "wire field names drifted from OPTION_KEYS"
        );
        let schema_keys: BTreeSet<String> = schema["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_string())
            .collect();
        assert_eq!(keys(&got), schema_keys);
        for (k, sub) in schema["nested"].as_object().unwrap() {
            nested_keys_equal(&got[k], sub, k).unwrap();
        }
        let snapped =
            to_compile_options(json!({"name": "a", "namespace": "x", "apiVersion": 60.5}))
                .unwrap()
                .unwrap();
        assert_eq!(snapped.api_version, 60);
        let snapped =
            to_compile_options(json!({"name": "a", "namespace": "x", "apiVersion": 63.9}))
                .unwrap()
                .unwrap();
        assert_eq!(snapped.api_version, 63);
        assert!(
            err(json!({"name": "a", "namespace": "x", "notARealOption": true}))
                .contains("notARealOption")
        );
    }

    #[test]
    fn nested_key_equality_catches_a_schema_key_missing_from_the_wire() {
        use std::collections::BTreeSet;
        // Plant: a one-sided extra schema key would pass a subset check and must fail equality.
        let wire = serde_json::json!({"loader": true});
        let schema = serde_json::json!({"loader": true, "strictSpecifier": true});
        let err = nested_keys_equal(&wire, &schema, "dynamicImports").unwrap_err();
        assert!(
            err.contains("must be equal"),
            "equality helper must reject a schema key missing from the wire: {err}"
        );
        let wk: BTreeSet<_> = wire.as_object().unwrap().keys().cloned().collect();
        let sk: BTreeSet<_> = schema.as_object().unwrap().keys().cloned().collect();
        assert!(wk.is_subset(&sk), "fixture: that difference is subset-only");
    }

    #[test]
    fn null_options_are_lwc1023() {
        let e = to_compile_options(serde_json::Value::Null)
            .unwrap()
            .unwrap_err();
        assert_eq!(e.diagnostics[0].code, Some(1023));
        assert_eq!(
            e.message,
            "LWC1023: Expected options object, received \"null\"."
        );
    }

    #[test]
    fn non_number_api_version_is_not_a_seam_error() {
        for v in [
            json!("60"),
            json!({"n": 60}),
            json!(true),
            serde_json::Value::Null,
        ] {
            let o = to_compile_options(json!({"name": "a", "namespace": "x", "apiVersion": v}))
                .unwrap()
                .unwrap();
            assert_eq!(
                o.api_version,
                lwc_shared::api_version::HIGHEST_API_VERSION,
                "{v}"
            );
        }
    }

    #[test]
    fn unknown_ssr_mode_is_not_a_seam_error() {
        let o = to_compile_options(json!({"name": "a", "namespace": "x", "ssrMode": "bogus"}))
            .unwrap()
            .unwrap();
        assert_eq!(o.ssr_mode, lwc_shared::SsrMode::AsyncYield);
        assert_eq!(o.ssr_mode_emit.as_deref(), Some(r#""bogus""#));
        let o = to_compile_options(json!({"name": "a", "namespace": "x", "ssrMode": 1}))
            .unwrap()
            .unwrap();
        assert_eq!(o.ssr_mode, lwc_shared::SsrMode::AsyncYield);
        assert_eq!(o.ssr_mode_emit.as_deref(), Some("1"));
        let o = to_compile_options(
            json!({"name": "a", "namespace": "x", "ssrMode": serde_json::Value::Null}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(o.ssr_mode, lwc_shared::SsrMode::AsyncYield);
        assert_eq!(o.ssr_mode_emit.as_deref(), Some("null"));
        let o = to_compile_options(json!({"name": "a", "namespace": "x", "ssrMode": "asyncYield"}))
            .unwrap()
            .unwrap();
        assert_eq!(o.ssr_mode, lwc_shared::SsrMode::AsyncYield);
        assert_eq!(o.ssr_mode_emit, None);
    }

    #[test]
    fn instrumentation_key_attaches_a_recorder() {
        let o = to_compile_options(json!({"name": "a", "namespace": "x", "instrumentation": {}}))
            .unwrap()
            .unwrap();
        assert!(
            o.instrumentation.is_some(),
            "a present instrumentation key must attach a recorder (D-0205)"
        );
        let o = to_compile_options(json!({"name": "a", "namespace": "x"}))
            .unwrap()
            .unwrap();
        assert!(
            o.instrumentation.is_none(),
            "an omitted key must not invent a recorder"
        );
    }
}

#[napi]
pub fn transform(
    src: String,
    filename: String,
    options: serde_json::Value,
) -> Result<JsCompileResult> {
    let o = match to_compile_options(options)? {
        Ok(o) => o,
        Err(e) => {
            return Ok(attach_side_channels(
                errors::options_error(e),
                lwc_compiler::take_validation_deprecations(),
            ));
        }
    };
    let deprecations = o.deprecations.clone();
    Ok(match errors::compile(&src, &filename, &o) {
        Ok(r) => {
            let warnings = if r.warnings.is_empty() {
                None
            } else {
                Some(r.warnings.into_iter().map(to_js_diagnostic).collect())
            };
            attach_side_channels(
                JsCompileResult {
                    code: r.code,
                    map: map_value(r.map.as_deref()),
                    css_scope_tokens: r.css_scope_tokens,
                    warnings,
                    error: None,
                    instrumentation_events: None,
                    deprecations: None,
                },
                deprecations,
            )
        }
        Err(e) => attach_side_channels(e, deprecations),
    })
}
