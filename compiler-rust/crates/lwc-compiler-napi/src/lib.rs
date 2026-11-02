//! napi-rs binding: facade over `lwc_compiler::dispatch` only (no napi in lwc-style).
//! Compile errors are returned as `{ error: {...} }` so the harness can throw a JS Error
//! carrying `name` / `code` / `line` / `column` (napi v2 `Error` cannot carry extra props).
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Deserialize;

/// Options accepted over the seam. Taken as JSON (not a `#[napi(object)]`, which silently ignores extra
/// keys) and deserialized with `deny_unknown_fields`, so any unknown key, top-level or nested, is an
/// error naming the key. Keep in sync with `RUST_OPTION_KEYS` in harness/src/backend.ts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JsCompileOptions {
    pub name: String,
    pub namespace: String,
    pub scoped_styles: Option<bool>,
    pub disable_synthetic_shadow_support: Option<bool>,
    pub api_version: Option<u32>,
    #[serde(rename = "targetSSR")]
    pub target_ssr: Option<bool>,
    pub ssr_mode: Option<String>,
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
    pub is_explicit_import: Option<bool>,
    pub output_config: Option<JsOutputConfig>,
    pub custom_renderer_config: Option<JsCustomRendererConfig>,
    pub stylesheet_config: Option<JsStylesheetConfig>,
}

/// `stylesheetConfig`: `{ customProperties?: { resolution?: {type:'native'} | {type:'module', name} } }`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JsStylesheetConfig {
    pub custom_properties: Option<JsCustomProperties>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsCustomProperties {
    pub resolution: Option<JsResolution>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum JsResolution {
    Native {},
    Module { name: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct JsDynamicImports {
    pub loader: Option<String>,
    pub strict_specifier: Option<bool>,
}

/// `outputConfig`: `sourcemap?: boolean | 'inline'`, `minify?` (deprecated, ignored upstream).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsOutputConfig {
    pub sourcemap: Option<JsSourcemap>,
    /// Deprecated upstream (warns, no effect); accepted and ignored.
    pub minify: Option<serde_json::Value>,
}

/// Any JSON value: validated into `lwc_shared::Sourcemap` (non-bool, non-'inline' → LWC1021 like JS).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum JsSourcemap {
    Bool(bool),
    Str(String),
    Other(serde_json::Value),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsCustomRendererConfig {
    pub directives: Vec<String>,
    pub elements: Vec<JsCustomRendererElement>,
}

#[derive(Debug, Deserialize)]
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
}

fn to_js_diagnostic(d: lwc_shared::CompilerDiagnostic) -> JsDiagnostic {
    JsDiagnostic {
        code: d.code,
        level: d.level.js_value(),
        location: d.js_location().map(Into::into),
        filename: d.filename,
        message: Some(d.message),
    }
}

impl From<lwc_shared::DiagnosticLocation> for JsDiagnosticLocation {
    fn from(l: lwc_shared::DiagnosticLocation) -> Self {
        Self { line: l.line, column: l.column, start: l.start, length: l.length }
    }
}

#[napi(object)]
pub struct JsCompileError {
    pub message: String,
    pub name: Option<String>,
    pub code: Option<u32>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub filename: Option<String>,
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
}


/// The JSON map string from the compiler -> a JS object (`null` when absent).
fn map_value(map: Option<&str>) -> serde_json::Value {
    map.and_then(|m| serde_json::from_str(m).ok()).unwrap_or(serde_json::Value::Null)
}

fn error_result(e: lwc_shared::CompileError) -> JsCompileResult {
    JsCompileResult {
        code: String::new(),
        map: serde_json::Value::Null,
        css_scope_tokens: None,
        warnings: None,
        error: Some(JsCompileError {
            code: e.diagnostics.first().and_then(|d| d.code),
            message: e.message,
            name: e.name,
            line: e.line,
            column: e.column,
            filename: e.filename,
        }),
    }
}

/// Parse + validate the JSON options into `lwc_shared::CompileOptions`.
/// `Err(napi)` = malformed/unknown key (thrown); `Ok(Err(compile))` = LWC validation error (returned).
fn to_compile_options(
    options: serde_json::Value,
    filename: &str,
) -> Result<std::result::Result<lwc_shared::CompileOptions, lwc_shared::CompileError>> {
    let options: JsCompileOptions = serde_json::from_value(options)
        .map_err(|e| napi::Error::new(napi::Status::InvalidArg, format!("invalid compile options: {e}")))?;
    // Start from the single source of defaults; override only fields the caller set.
    let mut o = lwc_shared::CompileOptions { name: options.name, namespace: options.namespace, ..Default::default() };
    if let Some(m) = options.ssr_mode.as_deref() {
        o.ssr_mode = lwc_shared::SsrMode::from_js(m).ok_or_else(|| {
            napi::Error::new(napi::Status::InvalidArg, format!("Invalid ssrMode {m:?}: expected sync | async | asyncYield"))
        })?;
    }
    macro_rules! set { ($($src:ident => $dst:ident),* $(,)?) => { $(if let Some(v) = options.$src { o.$dst = v; })* } }
    macro_rules! set_opt { ($($f:ident),* $(,)?) => { $(if options.$f.is_some() { o.$f = options.$f; })* } }
    set!(
        disable_synthetic_shadow_support => disable_synthetic_shadow_support,
        target_ssr => target_ssr,
        experimental_error_recovery_mode => experimental_error_recovery_mode,
        enable_private_methods => enable_private_methods,
        enable_synthetic_element_internals => enable_synthetic_element_internals,
        component_feature_flag_module_path => component_feature_flag_module_path,
        enable_lightning_web_security_transforms => enable_lightning_web_security_transforms,
        is_explicit_import => is_explicit_import,
    );
    set_opt!(
        scoped_styles, api_version, experimental_complex_expressions, preserve_html_comments,
        enable_static_content_optimization, enable_dynamic_components, enable_lwc_on, experimental_dynamic_directive,
    );
    if let Some(d) = options.dynamic_imports {
        if let Some(l) = d.loader { o.dynamic_import_loader = l; }
        if let Some(s) = d.strict_specifier { o.dynamic_import_strict_specifier = s; }
    }
    if let Some(sm) = options.output_config.and_then(|c| c.sourcemap) {
        o.sourcemap = match sm {
            JsSourcemap::Bool(b) => lwc_shared::Sourcemap::from_bool(b),
            JsSourcemap::Str(s) => match lwc_shared::Sourcemap::from_js_str(&s, filename) {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            },
            // JS templateString renders the value with String(); `null` is treated as unset by serde.
            JsSourcemap::Other(v) => return Ok(Err(lwc_shared::Sourcemap::from_js_str(&v.to_string(), filename).unwrap_err())),
        };
    }
    o.custom_renderer_config = options.custom_renderer_config.map(|c| lwc_shared::CustomRendererConfig {
        directives: c.directives,
        elements: c
            .elements
            .into_iter()
            .map(|e| lwc_shared::CustomRendererElementConfig { tag_name: e.tag_name, namespace: e.namespace, attributes: e.attributes })
            .collect(),
    });
    if let Some(r) = options.stylesheet_config.and_then(|c| c.custom_properties).and_then(|c| c.resolution) {
        o.stylesheet_config.custom_properties_resolution = match r {
            JsResolution::Native {} => lwc_shared::CustomPropertiesResolution::Native,
            JsResolution::Module { name } => lwc_shared::CustomPropertiesResolution::Module { name },
        };
    }
    Ok(Ok(o))
}

#[cfg(test)]
mod tests {
    #[test]
    fn warning_carries_js_location_shape() {
        // JS: LWC1182 "Multiple if: directives ..." warning with location {line,column,start,length}.
        let o = to_compile_options(json!({"name": "a", "namespace": "x"}), "a.html").unwrap().unwrap();
        let r = lwc_compiler::dispatch("<template><div if:true={a} if:false={b}></div></template>", "a.html", &o).unwrap();
        let w = r.warnings.into_iter().find(|w| w.code == Some(1182)).expect("1182 warning");
        let j = to_js_diagnostic(w);
        let l = j.location.expect("location");
        assert_eq!(l.line, 1);
        assert!(l.start.is_some() && l.length.is_some());
    }

    #[test]
    fn sourcemap_result_shape_matches_shipped() {
        let src = "export const a = 1;\n";
        let run = |sm: serde_json::Value| {
            let o = to_compile_options(json!({"name": "a", "namespace": "x", "outputConfig": {"sourcemap": sm}}), "a.js").unwrap().unwrap();
            map_value(lwc_compiler::dispatch(src, "a.js", &o).unwrap().map.as_deref())
        };
        let m = run(json!(true));
        let o = m.as_object().expect("map is an object for sourcemap: true");
        assert_eq!(o["version"], json!(3));
        assert_eq!(o["sources"], json!(["a.js"]));
        assert_eq!(o["sourcesContent"], json!([src]));
        assert!(o["names"].is_array() && o["mappings"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(o.keys().all(|k| ["version", "names", "sources", "sourcesContent", "mappings", "ignoreList"].contains(&k.as_str())), "{o:?}");
        assert_eq!(run(json!("inline")), serde_json::Value::Null);
        assert_eq!(run(json!(false)), serde_json::Value::Null);
    }

    use super::*;
    use serde_json::json;

    fn err(v: serde_json::Value) -> String {
        to_compile_options(v, "a.js").err().expect("must reject").reason
    }

    #[test]
    fn rejects_unknown_keys_naming_them() {
        assert!(err(json!({"name": "a", "namespace": "x", "bogusTop": 1})).contains("bogusTop"));
        assert!(err(json!({"name": "a", "namespace": "x", "dynamicImports": {"bogusNested": true}})).contains("bogusNested"));
        assert!(err(json!({"name": "a", "namespace": "x", "outputConfig": {"sourceMap": true}})).contains("sourceMap"));
        assert!(err(json!({"name": "a", "namespace": "x", "customRendererConfig": {"directives": [], "elements": [{"tagName": "div", "attrs": []}]}})).contains("attrs"));
        assert!(err(json!({"name": "a", "namespace": "x", "stylesheetConfig": {"customProperties": {"resolverModule": "m"}}})).contains("resolverModule"));
        assert!(err(json!({"name": "a", "namespace": "x", "stylesheetConfig": {"customProperties": {"resolution": {"type": "native", "extra": 1}}}})).contains("extra"));
        assert!(err(json!({"name": "a", "namespace": "x", "experimentalComputedMemberExpression": true})).contains("experimentalComputedMemberExpression"));
    }

    #[test]
    fn defaults_come_from_lwc_shared() {
        let o = to_compile_options(json!({"name": "a", "namespace": "x"}), "a.js").unwrap().unwrap();
        assert_eq!(o, lwc_shared::CompileOptions { name: "a".into(), namespace: "x".into(), ..Default::default() });
        assert!(o.dynamic_import_strict_specifier);
        assert!(o.resolved_experimental_dynamic_directive());
        // dynamicImports present without strictSpecifier keeps the default (JS spreads over DEFAULT_DYNAMIC_IMPORT_CONFIG).
        let o = to_compile_options(json!({"name": "a", "namespace": "x", "dynamicImports": {"loader": "l"}}), "a.js").unwrap().unwrap();
        assert!(o.dynamic_import_strict_specifier);
    }

    #[test]
    fn maps_new_options() {
        let o = to_compile_options(json!({
            "name": "a", "namespace": "x", "isExplicitImport": true, "enableLightningWebSecurityTransforms": true,
            "outputConfig": {"sourcemap": "inline", "minify": false},
            "customRendererConfig": {"directives": ["dom"], "elements": [{"tagName": "svg", "namespace": "ns", "attributes": ["href"]}]},
            "stylesheetConfig": {"customProperties": {"resolution": {"type": "module", "name": "@x/r"}}}
        }), "a.js").unwrap().unwrap();
        assert!(o.is_explicit_import && o.enable_lightning_web_security_transforms);
        assert_eq!(o.sourcemap, lwc_shared::Sourcemap::Inline);
        let c = o.custom_renderer_config.unwrap();
        assert_eq!((c.directives, c.elements[0].tag_name.as_str(), c.elements[0].attributes.clone()), (vec!["dom".to_string()], "svg", Some(vec!["href".to_string()])));
        assert_eq!(o.stylesheet_config.custom_properties_resolution, lwc_shared::CustomPropertiesResolution::Module { name: "@x/r".into() });
        let o = to_compile_options(json!({"name": "a", "namespace": "x", "outputConfig": {"sourcemap": true}}), "a.js").unwrap().unwrap();
        assert_eq!(o.sourcemap, lwc_shared::Sourcemap::On);
    }

    #[test]
    fn invalid_sourcemap_is_lwc1021() {
        for bad in [json!("foo"), json!(1)] {
            let e = to_compile_options(json!({"name": "a", "namespace": "x", "outputConfig": {"sourcemap": bad}}), "a.js").unwrap().unwrap_err();
            assert_eq!(e.diagnostics[0].code, Some(1021));
            assert!(e.message.starts_with("LWC1021: Expected a boolean value or 'inline' for outputConfig.sourcemap"), "{}", e.message);
        }
    }
}

#[napi]
pub fn transform(src: String, filename: String, options: serde_json::Value) -> Result<JsCompileResult> {
    let o = match to_compile_options(options, &filename)? {
        Ok(o) => o,
        Err(e) => return Ok(error_result(e)),
    };
    Ok(match lwc_compiler::dispatch(&src, &filename, &o) {
        Ok(r) => {
            let warnings = if r.warnings.is_empty() {
                None
            } else {
                Some(
                    r.warnings
                        .into_iter()
                        .map(to_js_diagnostic)
                        .collect(),
                )
            };
            JsCompileResult {
                code: r.code,
                map: map_value(r.map.as_deref()),
                css_scope_tokens: r.css_scope_tokens,
                warnings,
                error: None,
            }
        }
        Err(e) => error_result(e),
    })
}
