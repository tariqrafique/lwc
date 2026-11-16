//! `@lwc/compiler` `src/options.ts` (9.5.3 @ b9e8007). One schema: [`TransformOptions`] is
//! serde-free and nested as upstream. [`validate_transform_options`] is `validateTransformOptions`
//! (`validateOptions` :185, `normalizeOptions` :221-249, `getAPIVersionFromNumber`).
//! napi only deserializes JSON into [`TransformOptions`]. Unknown keys stay rejected at that
//! deserialize (`deny_unknown_fields`, D-0203); the accepted names are [`OPTION_KEYS`], and
//! `harness/src/rust-option-keys.json` is that list (test-checked, not a second hand list).
use std::sync::atomic::{AtomicBool, Ordering};

use lwc_shared::errors::CompilerValidationErrors;
use lwc_shared::{
    CompileError, CompileOptions, CompilerDiagnostic, CustomPropertiesResolution,
    CustomRendererConfig, InstrumentationHandle, ProducerPosition, Sourcemap, SsrMode,
};

/// JSON names of [`TransformOptions`] (camelCase, `targetSSR` kept as upstream spells it).
/// The harness allowlist is generated from this via [`option_schema_document`].
pub const OPTION_KEYS: &[&str] = &[
    "name",
    "namespace",
    "scopedStyles",
    "disableSyntheticShadowSupport",
    "apiVersion",
    "targetSSR",
    "ssrMode",
    "experimentalErrorRecoveryMode",
    "enablePrivateMethods",
    "experimentalComplexExpressions",
    "preserveHtmlComments",
    "enableStaticContentOptimization",
    "enableDynamicComponents",
    "enableLwcOn",
    "experimentalDynamicDirective",
    "enableSyntheticElementInternals",
    "componentFeatureFlagModulePath",
    "dynamicImports",
    "enableLightningWebSecurityTransforms",
    "instrumentation",
    "isExplicitImport",
    "outputConfig",
    "customRendererConfig",
    "stylesheetConfig",
];

/// Nested object keys, same shape the harness walks (`true` = leaf, `"[]"` = array of objects).
const NESTED_JSON: &str = r#"{
    "dynamicImports": {
      "loader": true,
      "strictSpecifier": true
    },
    "outputConfig": {
      "minify": true,
      "sourcemap": true
    },
    "customRendererConfig": {
      "directives": true,
      "elements": {
        "[]": {
          "tagName": true,
          "namespace": true,
          "attributes": true
        }
      }
    },
    "stylesheetConfig": {
      "customProperties": {
        "resolution": {
          "type": true,
          "name": true
        }
      }
    }
  }"#;

/// Checked-in `harness/src/rust-option-keys.json` must equal this. The harness reads the file;
/// this const list is the schema.
pub fn option_schema_document() -> String {
    let mut keys = String::new();
    for (i, k) in OPTION_KEYS.iter().enumerate() {
        if i > 0 {
            keys.push_str(",\n");
        }
        keys.push_str("    \"");
        keys.push_str(k);
        keys.push('"');
    }
    format!("{{\n  \"keys\": [\n{keys}\n  ],\n  \"nested\": {NESTED_JSON}\n}}\n")
}

const DEFAULT_IS_EXPLICIT_IMPORT: bool = false;
const DEFAULT_PRESERVE_HTML_COMMENTS: bool = false;
const DEFAULT_ENABLE_STATIC_CONTENT_OPTIMIZATION: bool = true;
const DEFAULT_EXPERIMENTAL_COMPLEX_EXPRESSIONS: bool = false;
const DEFAULT_DISABLE_SYNTHETIC_SHADOW_SUPPORT: bool = false;
const DEFAULT_ENABLE_LIGHTNING_WEB_SECURITY_TRANSFORMS: bool = false;
const DEFAULT_TARGET_SSR: bool = false;
const DEFAULT_SSR_MODE: SsrMode = SsrMode::Sync;
const DEFAULT_EXPERIMENTAL_ERROR_RECOVERY_MODE: bool = false;
const DEFAULT_COMPONENT_FEATURE_FLAG_MODULE_PATH: &str = "";
const DEFAULT_DYNAMIC_IMPORT_LOADER: &str = "";
const DEFAULT_DYNAMIC_IMPORT_STRICT_SPECIFIER: bool = true;
const DEFAULT_OUTPUT_SOURCEMAP: Sourcemap = Sourcemap::Off;

/// `outputConfig.sourcemap` before validation. Non-bool, non-`'inline'` is [`SourcemapInput::Text`]
/// and becomes LWC1021.
#[derive(Debug, Clone, PartialEq)]
pub enum SourcemapInput {
    Bool(bool),
    Text(String),
}

/// `apiVersion` as received, before `getAPIVersionFromNumber`.
/// A non-number is allowed: upstream `isNumber` is false, so it snaps to latest.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiVersionInput {
    Number(f64),
    /// Any non-number JSON value (string, object, bool, array). Null/absent is `None`.
    NonNumber,
}

/// `ssrMode` as received. Upstream does not validate `CompilationMode` at runtime;
/// unknown values are not a compiler error. [`validate_transform_options`] maps a
/// known name and defaults anything else (including a non-string) to sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsrModeInput {
    Text(String),
    NonString,
}

/// `StylesheetConfig` (options.ts:64).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StylesheetConfig {
    pub custom_properties: Option<CustomProperties>,
}

/// `customProperties`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomProperties {
    pub resolution: Option<CustomPropertiesResolution>,
}

/// `DynamicImportConfig` (options.ts:84).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicImportConfig {
    pub loader: Option<String>,
    pub strict_specifier: Option<bool>,
}

/// `OutputConfig` (options.ts:70). `minify` is deprecated and ignored; presence warns.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputConfig {
    pub sourcemap: Option<SourcemapInput>,
    /// `true` when the key was set (any JSON value). Upstream warns and drops it.
    pub minify_present: bool,
}

/// `TransformOptions` (options.ts:93). Serde-free. Absent fields are `None` (JS omitted).
#[derive(Debug, Clone)]
pub struct TransformOptions {
    pub name: String,
    pub namespace: String,
    pub stylesheet_config: Option<StylesheetConfig>,
    pub dynamic_imports: Option<DynamicImportConfig>,
    pub experimental_dynamic_directive: Option<bool>,
    pub enable_dynamic_components: Option<bool>,
    pub enable_synthetic_element_internals: Option<bool>,
    pub experimental_complex_expressions: Option<bool>,
    pub output_config: Option<OutputConfig>,
    pub is_explicit_import: Option<bool>,
    pub preserve_html_comments: Option<bool>,
    pub scoped_styles: Option<bool>,
    pub enable_static_content_optimization: Option<bool>,
    pub custom_renderer_config: Option<CustomRendererConfig>,
    pub enable_lwc_on: Option<bool>,
    pub disable_synthetic_shadow_support: Option<bool>,
    pub enable_lightning_web_security_transforms: Option<bool>,
    pub instrumentation: Option<InstrumentationHandle>,
    /// JS `number | undefined`. Floats are normalized by `getAPIVersionFromNumber` (D-0206).
    pub api_version: Option<ApiVersionInput>,
    pub target_ssr: Option<bool>,
    pub ssr_mode: Option<SsrModeInput>,
    pub experimental_error_recovery_mode: Option<bool>,
    pub component_feature_flag_module_path: Option<String>,
    pub enable_private_methods: Option<bool>,
}

/// `NormalizedTransformOptions` (options.ts:167): defaults filled, nested configs always present,
/// `apiVersion` always a known version number.
#[derive(Debug, Clone)]
pub struct NormalizedTransformOptions {
    pub name: String,
    pub namespace: String,
    pub is_explicit_import: bool,
    pub preserve_html_comments: bool,
    pub enable_static_content_optimization: bool,
    pub experimental_complex_expressions: bool,
    pub disable_synthetic_shadow_support: bool,
    pub enable_lightning_web_security_transforms: bool,
    pub target_ssr: bool,
    pub ssr_mode: SsrMode,
    pub experimental_error_recovery_mode: bool,
    pub component_feature_flag_module_path: String,
    pub stylesheet_resolution: CustomPropertiesResolution,
    pub sourcemap: Sourcemap,
    pub dynamic_import_loader: String,
    pub dynamic_import_strict_specifier: bool,
    pub api_version: u32,
    pub scoped_styles: Option<bool>,
    pub custom_renderer_config: Option<CustomRendererConfig>,
    pub enable_lwc_on: Option<bool>,
    pub enable_dynamic_components: Option<bool>,
    pub enable_synthetic_element_internals: bool,
    pub experimental_dynamic_directive: Option<bool>,
    pub instrumentation: Option<InstrumentationHandle>,
    pub enable_private_methods: bool,
}

impl NormalizedTransformOptions {
    /// Flatten into the compile options the style/template/js/ssr crates already take.
    pub fn into_compile_options(self) -> CompileOptions {
        CompileOptions {
            name: self.name,
            namespace: self.namespace,
            scoped_styles: self.scoped_styles,
            disable_synthetic_shadow_support: self.disable_synthetic_shadow_support,
            api_version: Some(self.api_version),
            target_ssr: self.target_ssr,
            ssr_mode: self.ssr_mode,
            enable_private_methods: self.enable_private_methods,
            enable_lightning_web_security_transforms: self.enable_lightning_web_security_transforms,
            experimental_error_recovery_mode: self.experimental_error_recovery_mode,
            is_explicit_import: self.is_explicit_import,
            enable_synthetic_element_internals: self.enable_synthetic_element_internals,
            component_feature_flag_module_path: self.component_feature_flag_module_path,
            sourcemap: self.sourcemap,
            dynamic_import_loader: self.dynamic_import_loader,
            dynamic_import_strict_specifier: self.dynamic_import_strict_specifier,
            stylesheet_config: lwc_shared::StylesheetConfig {
                custom_properties_resolution: self.stylesheet_resolution,
            },
            instrumentation: self.instrumentation,
            experimental_complex_expressions: Some(self.experimental_complex_expressions),
            preserve_html_comments: Some(self.preserve_html_comments),
            enable_static_content_optimization: Some(self.enable_static_content_optimization),
            enable_dynamic_components: self.enable_dynamic_components,
            enable_lwc_on: self.enable_lwc_on,
            experimental_dynamic_directive: self.experimental_dynamic_directive,
            custom_renderer_config: self.custom_renderer_config,
        }
    }
}

/// Process latch for the stylesheetConfig deprecation (`alreadyWarnedOnStylesheetConfig`).
/// D-0212 / D-0250: the JS loader emits [`DeprecationId::message`]; this latch is the
/// once-per-process gate upstream keeps in the options module. Minify is not latched.
static STYLESHEET_WARNED: AtomicBool = AtomicBool::new(false);

fn take_stylesheet_warn(present: bool, latch: &AtomicBool) -> bool {
    present && !latch.swap(true, Ordering::Relaxed)
}

/// `validateTransformOptions`.
pub fn validate_transform_options(
    options: &TransformOptions,
) -> Result<NormalizedTransformOptions, CompileError> {
    // validateOptions: stylesheet warn (once), then outputConfig (sourcemap throws before minify warn).
    // D-0212: the loader prints DeprecationId::message(); do not compute it here. The latch matches upstream.
    let _ = take_stylesheet_warn(options.stylesheet_config.is_some(), &STYLESHEET_WARNED);
    let sourcemap = match options.output_config.as_ref() {
        None => DEFAULT_OUTPUT_SOURCEMAP,
        Some(config) => match config.sourcemap.clone() {
            None => DEFAULT_OUTPUT_SOURCEMAP,
            Some(SourcemapInput::Bool(b)) => Sourcemap::from_bool(b),
            Some(SourcemapInput::Text(s)) if s == "inline" => Sourcemap::Inline,
            Some(SourcemapInput::Text(s)) => return Err(invalid_sourcemap(&s)),
        },
    };

    let stylesheet_resolution = options
        .stylesheet_config
        .as_ref()
        .and_then(|c| c.custom_properties.as_ref())
        .and_then(|c| c.resolution.clone())
        .unwrap_or(CustomPropertiesResolution::Native);
    let dynamic_import_loader = options
        .dynamic_imports
        .as_ref()
        .and_then(|d| d.loader.clone())
        .unwrap_or_else(|| DEFAULT_DYNAMIC_IMPORT_LOADER.to_string());
    let dynamic_import_strict_specifier = options
        .dynamic_imports
        .as_ref()
        .and_then(|d| d.strict_specifier)
        .unwrap_or(DEFAULT_DYNAMIC_IMPORT_STRICT_SPECIFIER);
    let api_version = match options.api_version {
        None | Some(ApiVersionInput::NonNumber) => {
            lwc_shared::api_version::get_api_version_from_number(None)
        }
        Some(ApiVersionInput::Number(n)) => lwc_shared::api_version::get_api_version_from_f64(n),
    };
    let ssr_mode = match options.ssr_mode.as_ref() {
        None => DEFAULT_SSR_MODE,
        Some(SsrModeInput::Text(s)) => SsrMode::from_js(s).unwrap_or(DEFAULT_SSR_MODE),
        Some(SsrModeInput::NonString) => DEFAULT_SSR_MODE,
    };

    Ok(NormalizedTransformOptions {
        name: options.name.clone(),
        namespace: options.namespace.clone(),
        is_explicit_import: options
            .is_explicit_import
            .unwrap_or(DEFAULT_IS_EXPLICIT_IMPORT),
        preserve_html_comments: options
            .preserve_html_comments
            .unwrap_or(DEFAULT_PRESERVE_HTML_COMMENTS),
        enable_static_content_optimization: options
            .enable_static_content_optimization
            .unwrap_or(DEFAULT_ENABLE_STATIC_CONTENT_OPTIMIZATION),
        experimental_complex_expressions: options
            .experimental_complex_expressions
            .unwrap_or(DEFAULT_EXPERIMENTAL_COMPLEX_EXPRESSIONS),
        disable_synthetic_shadow_support: options
            .disable_synthetic_shadow_support
            .unwrap_or(DEFAULT_DISABLE_SYNTHETIC_SHADOW_SUPPORT),
        enable_lightning_web_security_transforms: options
            .enable_lightning_web_security_transforms
            .unwrap_or(DEFAULT_ENABLE_LIGHTNING_WEB_SECURITY_TRANSFORMS),
        target_ssr: options.target_ssr.unwrap_or(DEFAULT_TARGET_SSR),
        ssr_mode,
        experimental_error_recovery_mode: options
            .experimental_error_recovery_mode
            .unwrap_or(DEFAULT_EXPERIMENTAL_ERROR_RECOVERY_MODE),
        component_feature_flag_module_path: options
            .component_feature_flag_module_path
            .clone()
            .unwrap_or_else(|| DEFAULT_COMPONENT_FEATURE_FLAG_MODULE_PATH.to_string()),
        stylesheet_resolution,
        sourcemap,
        dynamic_import_loader,
        dynamic_import_strict_specifier,
        api_version,
        scoped_styles: options.scoped_styles,
        custom_renderer_config: options.custom_renderer_config.clone(),
        enable_lwc_on: options.enable_lwc_on,
        enable_dynamic_components: options.enable_dynamic_components,
        enable_synthetic_element_internals: options
            .enable_synthetic_element_internals
            .unwrap_or(false),
        experimental_dynamic_directive: options.experimental_dynamic_directive,
        instrumentation: options.instrumentation.clone(),
        enable_private_methods: options.enable_private_methods.unwrap_or(false),
    })
}

/// Validate and flatten. Callers of [`crate::dispatch`] go through this too.
pub fn validated_compile_options(
    options: &TransformOptions,
) -> Result<CompileOptions, CompileError> {
    Ok(validate_transform_options(options)?.into_compile_options())
}

/// One validate entry the napi seam calls after deserializing (or when there is no object).
/// `received` is JS `String(options)` for the LWC1023 message, as upstream `invariant`.
///
/// Upstream only codes LWC1023 when `options` is `undefined` (`isUndefined`). JS `null`
/// throws `TypeError: Cannot read properties of null (reading 'stylesheetConfig')`.
/// The JSON napi seam represents both `null` and `undefined` as JSON null, so we emit
/// LWC1023 with `received` (typically `"null"`).
pub fn validate_incoming(
    options: Option<&TransformOptions>,
    received: &str,
) -> Result<NormalizedTransformOptions, CompileError> {
    match options {
        None => Err(missing_options_error(received)),
        Some(o) => validate_transform_options(o),
    }
}

/// `CompileOptions` as the options object `dispatch` already holds, then the same validator.
pub fn prepare_compile_options(options: &CompileOptions) -> Result<CompileOptions, CompileError> {
    validated_compile_options(&TransformOptions::from_compile(options))
}

/// LWC1023 `MISSING_OPTIONS_OBJECT` (`received` is JS `String(options)`).
pub fn missing_options_error(received: &str) -> CompileError {
    validation_error(&CompilerValidationErrors::MISSING_OPTIONS_OBJECT, received)
}

fn invalid_sourcemap(received: &str) -> CompileError {
    validation_error(
        &CompilerValidationErrors::INVALID_SOURCEMAP_PROPERTY,
        received,
    )
}

fn validation_error(info: &lwc_shared::LwcErrorInfo, received: &str) -> CompileError {
    let d = CompilerDiagnostic::generate(info, Some(&[received]), None, false);
    CompileError {
        message: d.message.clone(),
        filename: None,
        diagnostics: vec![d],
        name: None,
        position: ProducerPosition::Diagnostics,
    }
}

impl TransformOptions {
    /// View a [`CompileOptions`] as the object the validator accepts.
    /// Default-native stylesheet and an absent minify do not count as "passed"
    /// (those warns are for an explicit key). Values that CompileOptions stores are kept.
    pub fn from_compile(o: &CompileOptions) -> Self {
        let stylesheet_config = match &o.stylesheet_config.custom_properties_resolution {
            CustomPropertiesResolution::Native => None,
            CustomPropertiesResolution::Module { name } => Some(StylesheetConfig {
                custom_properties: Some(CustomProperties {
                    resolution: Some(CustomPropertiesResolution::Module { name: name.clone() }),
                }),
            }),
        };
        Self {
            name: o.name.clone(),
            namespace: o.namespace.clone(),
            stylesheet_config,
            dynamic_imports: Some(DynamicImportConfig {
                loader: Some(o.dynamic_import_loader.clone()),
                strict_specifier: Some(o.dynamic_import_strict_specifier),
            }),
            experimental_dynamic_directive: o.experimental_dynamic_directive,
            enable_dynamic_components: o.enable_dynamic_components,
            enable_synthetic_element_internals: Some(o.enable_synthetic_element_internals),
            experimental_complex_expressions: o.experimental_complex_expressions,
            output_config: Some(OutputConfig {
                sourcemap: Some(match o.sourcemap {
                    Sourcemap::Off => SourcemapInput::Bool(false),
                    Sourcemap::On => SourcemapInput::Bool(true),
                    Sourcemap::Inline => SourcemapInput::Text("inline".into()),
                }),
                minify_present: false,
            }),
            is_explicit_import: Some(o.is_explicit_import),
            preserve_html_comments: o.preserve_html_comments,
            scoped_styles: o.scoped_styles,
            enable_static_content_optimization: o.enable_static_content_optimization,
            custom_renderer_config: o.custom_renderer_config.clone(),
            enable_lwc_on: o.enable_lwc_on,
            disable_synthetic_shadow_support: Some(o.disable_synthetic_shadow_support),
            enable_lightning_web_security_transforms: Some(
                o.enable_lightning_web_security_transforms,
            ),
            instrumentation: o.instrumentation.clone(),
            api_version: o.api_version.map(|v| ApiVersionInput::Number(v as f64)),
            target_ssr: Some(o.target_ssr),
            ssr_mode: Some(SsrModeInput::Text(o.ssr_mode.js_value().to_string())),
            experimental_error_recovery_mode: Some(o.experimental_error_recovery_mode),
            component_feature_flag_module_path: Some(o.component_feature_flag_module_path.clone()),
            enable_private_methods: Some(o.enable_private_methods),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lwc_shared::DeprecationId;

    #[test]
    fn rust_option_keys_json_is_the_schema() {
        let got = include_str!("../../../harness/src/rust-option-keys.json");
        assert_eq!(got, option_schema_document());
    }

    #[test]
    fn stylesheet_warn_is_once_minify_is_not_latched() {
        let latch = AtomicBool::new(false);
        assert!(take_stylesheet_warn(true, &latch));
        assert!(!take_stylesheet_warn(true, &latch));
        assert!(!take_stylesheet_warn(false, &latch));
        assert!(DeprecationId::StylesheetConfig.once_per_process());
        assert!(!DeprecationId::OutputConfigMinify.once_per_process());
    }

    fn bare() -> TransformOptions {
        TransformOptions::from_compile(&CompileOptions {
            name: "a".into(),
            namespace: "x".into(),
            ..CompileOptions::default()
        })
    }

    #[test]
    fn api_version_float_snaps_like_upstream() {
        let mut o = bare();
        o.api_version = Some(ApiVersionInput::Number(60.5));
        assert_eq!(validate_transform_options(&o).unwrap().api_version, 60);
        o.api_version = Some(ApiVersionInput::Number(63.9));
        assert_eq!(validate_transform_options(&o).unwrap().api_version, 63);
        o.api_version = None;
        assert_eq!(validate_transform_options(&o).unwrap().api_version, 66);
        o.api_version = Some(ApiVersionInput::Number(57.0));
        assert_eq!(validate_transform_options(&o).unwrap().api_version, 58);
    }

    #[test]
    fn invalid_sourcemap_is_lwc1021_before_minify_matters() {
        let mut o = bare();
        o.output_config = Some(OutputConfig {
            sourcemap: Some(SourcemapInput::Text("foo".into())),
            minify_present: true,
        });
        let e = validate_transform_options(&o).unwrap_err();
        assert_eq!(e.diagnostics[0].code, Some(1021));
        assert_eq!(
            e.message,
            "LWC1021: Expected a boolean value or 'inline' for outputConfig.sourcemap, received \"foo\"."
        );
    }

    #[test]
    fn missing_options_is_lwc1023() {
        let e = missing_options_error("null");
        assert_eq!(e.diagnostics[0].code, Some(1023));
        assert!(e.message.contains("received \"null\""));
    }

    #[test]
    fn defaults_match_default_options() {
        let n = validate_transform_options(&bare()).unwrap();
        assert!(!n.is_explicit_import);
        assert!(!n.preserve_html_comments);
        assert!(n.enable_static_content_optimization);
        assert!(!n.experimental_complex_expressions);
        assert!(!n.disable_synthetic_shadow_support);
        assert!(!n.enable_lightning_web_security_transforms);
        assert!(!n.target_ssr);
        assert_eq!(n.ssr_mode, SsrMode::Sync);
        assert!(!n.experimental_error_recovery_mode);
        assert_eq!(n.component_feature_flag_module_path, "");
        assert_eq!(n.dynamic_import_loader, "");
        assert!(n.dynamic_import_strict_specifier);
        assert_eq!(n.sourcemap, Sourcemap::Off);
        assert_eq!(n.stylesheet_resolution, CustomPropertiesResolution::Native);
        assert!(n.experimental_dynamic_directive.is_none());
    }

    #[test]
    fn non_number_api_version_snaps_to_latest_like_upstream() {
        let mut o = bare();
        o.api_version = Some(ApiVersionInput::NonNumber);
        assert_eq!(
            validate_transform_options(&o).unwrap().api_version,
            lwc_shared::api_version::HIGHEST_API_VERSION
        );
    }

    #[test]
    fn unknown_ssr_mode_defaults_to_sync_and_is_not_an_error() {
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
        o.ssr_mode = Some(SsrModeInput::Text("asyncYield".into()));
        assert_eq!(
            validate_transform_options(&o).unwrap().ssr_mode,
            SsrMode::AsyncYield
        );
    }

    #[test]
    fn validate_incoming_none_is_lwc1023_with_received() {
        let e = validate_incoming(None, "null").unwrap_err();
        assert_eq!(e.diagnostics[0].code, Some(1023));
        assert_eq!(
            e.message,
            "LWC1023: Expected options object, received \"null\"."
        );
        let e = validate_incoming(None, "undefined").unwrap_err();
        assert_eq!(
            e.message,
            "LWC1023: Expected options object, received \"undefined\"."
        );
    }

    #[test]
    fn option_keys_include_instrumentation() {
        assert!(OPTION_KEYS.contains(&"instrumentation"));
    }
}
