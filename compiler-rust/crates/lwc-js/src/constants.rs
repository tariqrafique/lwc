//! Mirrors `@lwc/babel-plugin-component/src/constants.ts`. Runtime-visible names must not change.
pub const LWC_PACKAGE_ALIAS: &str = "lwc";
pub const REGISTER_COMPONENT_ID: &str = "registerComponent";
pub const TEMPLATE_KEY: &str = "tmpl";
pub const COMPONENT_NAME_KEY: &str = "sel";
pub const API_VERSION_KEY: &str = "apiVersion";
pub const COMPONENT_CLASS_ID: &str = "__lwc_component_class_internal";
pub const PRIVATE_METHOD_PREFIX: &str = "__lwc_component_class_internal_private_";
pub const ENABLE_PRIVATE_METHODS_KEY: &str = "enablePrivateMethods";
pub const SYNTHETIC_ELEMENT_INTERNALS_KEY: &str = "enableSyntheticElementInternals";
pub const COMPONENT_FEATURE_FLAG_KEY: &str = "componentFeatureFlag";
/// LWC version stamped into `/*LWC compiler v…*/`. Pinned to the reference repo's
/// package.json (9.5.3) until release tooling wires the real value.
pub const LWC_VERSION: &str = "9.5.3";
pub fn version_comment() -> String { format!("/*LWC compiler v{LWC_VERSION}*/") }
pub const REGISTER_DECORATORS_ID: &str = "registerDecorators";
