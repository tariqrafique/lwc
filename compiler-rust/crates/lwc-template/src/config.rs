//! Compile options mirroring `@lwc/template-compiler` `Config` / `normalizeConfig`.

// Defined in lwc-shared so `lwc_shared::CompileOptions` can carry it; re-exported as the template type.
pub use lwc_shared::{CustomRendererConfig, CustomRendererElementConfig};
use lwc_shared::InstrumentationHandle;

/// Public compile/parse options (all optional at the call site; normalized below).
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub custom_renderer_config: Option<CustomRendererConfig>,
    pub experimental_computed_member_expression: Option<bool>,
    pub experimental_complex_expressions: Option<bool>,
    pub experimental_dynamic_directive: Option<bool>,
    pub enable_dynamic_components: Option<bool>,
    pub preserve_html_comments: Option<bool>,
    pub enable_static_content_optimization: Option<bool>,
    pub enable_lwc_on: Option<bool>,
    pub api_version: Option<u32>,
    pub disable_synthetic_shadow_support: Option<bool>,
    /// Compile for `@lwc/ssr-compiler` semantics (enables SSR-only template errors).
    pub ssr: Option<bool>,
    /// TS `instrumentation` (`incrementCounter` for directive metrics).
    pub instrumentation: Option<InstrumentationHandle>,
}

/// Config after `normalize_config` defaults + validation.
#[derive(Debug, Clone)]
pub struct NormalizedConfig {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub custom_renderer_config: Option<CustomRendererConfig>,
    pub experimental_computed_member_expression: bool,
    pub experimental_complex_expressions: bool,
    pub experimental_dynamic_directive: bool,
    pub enable_dynamic_components: bool,
    pub preserve_html_comments: bool,
    pub enable_static_content_optimization: bool,
    pub enable_lwc_on: bool,
    pub api_version: u32,
    pub disable_synthetic_shadow_support: bool,
    pub ssr: bool,
    pub instrumentation: Option<InstrumentationHandle>,
}

/// Error from invalid custom-renderer entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub message: String,
}

/// Version used when `api_version` is omitted (`getAPIVersionFromNumber(undefined)` = HIGHEST_API_VERSION).
pub const DEFAULT_API_VERSION: u32 = lwc_shared::api_version::HIGHEST_API_VERSION;

impl Config {
    pub fn normalize(self) -> Result<NormalizedConfig, ConfigError> {
        let custom_renderer_config = self
            .custom_renderer_config
            .map(normalize_custom_renderer_config)
            .transpose()?;

        Ok(NormalizedConfig {
            name: self.name,
            namespace: self.namespace,
            custom_renderer_config,
            experimental_computed_member_expression: self
                .experimental_computed_member_expression
                .unwrap_or(false),
            experimental_complex_expressions: self
                .experimental_complex_expressions
                .unwrap_or(false),
            experimental_dynamic_directive: self.experimental_dynamic_directive.unwrap_or(false),
            enable_dynamic_components: self.enable_dynamic_components.unwrap_or(false),
            preserve_html_comments: self.preserve_html_comments.unwrap_or(false),
            enable_static_content_optimization: self
                .enable_static_content_optimization
                .unwrap_or(true),
            enable_lwc_on: self.enable_lwc_on.unwrap_or(false),
            // TS normalizeConfig: `getAPIVersionFromNumber(config.apiVersion)` (default + clamp), the one
            // resolved version every parser/codegen gate reads.
            api_version: lwc_shared::api_version::get_api_version_from_number(self.api_version),
            disable_synthetic_shadow_support: self
                .disable_synthetic_shadow_support
                .unwrap_or(false),
            ssr: self.ssr.unwrap_or(false),
            instrumentation: self.instrumentation,
        })
    }
}

fn normalize_custom_renderer_config(
    config: CustomRendererConfig,
) -> Result<CustomRendererConfig, ConfigError> {
    let mut tag_names = Vec::new();
    let elements = config
        .elements
        .into_iter()
        .map(|e| {
            let tag_name = e.tag_name.to_ascii_lowercase();
            // Custom elements (hyphenated) cannot own a custom renderer hook.
            if tag_name.contains('-') {
                return Err(ConfigError {
                    message: format!(
                        "custom renderer config disallows custom element tag: {}",
                        e.tag_name
                    ),
                });
            }
            tag_names.push(tag_name.clone());
            Ok(CustomRendererElementConfig {
                tag_name,
                namespace: e.namespace.map(|n| n.to_ascii_lowercase()),
                attributes: e
                    .attributes
                    .map(|attrs| attrs.into_iter().map(|a| a.to_ascii_lowercase()).collect()),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    for (i, name) in tag_names.iter().enumerate() {
        if tag_names.iter().position(|n| n == name) != Some(i) {
            return Err(ConfigError {
                message: format!("duplicate custom renderer element entry: {name}"),
            });
        }
    }

    Ok(CustomRendererConfig {
        elements,
        directives: config
            .directives
            .into_iter()
            .map(|d| d.to_ascii_lowercase())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_defaults() {
        let n = Config::default().normalize().unwrap();
        assert!(n.enable_static_content_optimization);
        assert!(!n.preserve_html_comments);
        assert_eq!(n.api_version, DEFAULT_API_VERSION);
    }

    #[test]
    fn normalize_api_version_like_get_api_version_from_number() {
        let v = |x: Option<u32>| Config { api_version: x, ..Config::default() }.normalize().unwrap().api_version;
        assert_eq!(v(None), 66);
        assert_eq!(v(Some(57)), 58);
        assert_eq!(v(Some(0)), 58);
        assert_eq!(v(Some(58)), 58);
        assert_eq!(v(Some(63)), 63);
        assert_eq!(v(Some(66)), 66);
        assert_eq!(v(Some(67)), 66);
        assert_eq!(v(Some(1000)), 66);
    }
}
