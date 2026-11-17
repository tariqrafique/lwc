//! Frozen @lwc/ssr-runtime ABI symbol names (from packages/@lwc/ssr-runtime/src/index.ts; excludes `export * from './stubs'`).
pub const RUNTIME_MODULE: &str = "@lwc/ssr-runtime";
pub const HTML_ESCAPE: &str = "htmlEscape";
pub const SET_HOOKS: &str = "setHooks";
pub const SANITIZE_HTML_CONTENT: &str = "sanitizeHtmlContent";
pub const NORMALIZE_CLASS: &str = "normalizeClass";
pub const NORMALIZE_TAB_INDEX: &str = "normalizeTabIndex";
pub const SET_CONTEXT_KEYS: &str = "setContextKeys";
pub const SET_TRUSTED_SIGNAL_SET: &str = "setTrustedSignalSet";
pub const SET_TRUSTED_CONTEXT_SET: &str = "setTrustedContextSet";
pub const DANGEROUS_DO_NOT_USE_ADD_TRUSTED_CONTEXT: &str =
    "__dangerous_do_not_use_addTrustedContext";
pub const IS_TRUSTED_SIGNAL: &str = "isTrustedSignal";
pub const SIGNAL_BASE_CLASS: &str = "SignalBaseClass";
pub const SET_FEATURE_FLAG: &str = "setFeatureFlag";
pub const SET_FEATURE_FLAG_FOR_TEST: &str = "setFeatureFlagForTest";
pub const CLASS_LIST: &str = "ClassList";
pub const LIGHTNING_ELEMENT: &str = "LightningElement";
pub const SYMBOL__DEFAULT_TEMPLATE: &str = "SYMBOL__DEFAULT_TEMPLATE";
pub const SYMBOL__GENERATE_MARKUP: &str = "SYMBOL__GENERATE_MARKUP";
pub const SYMBOL__SET_INTERNALS: &str = "SYMBOL__SET_INTERNALS";
pub const MUTATION_TRACKER: &str = "mutationTracker";
pub const FALLBACK_TMPL: &str = "fallbackTmpl";
pub const FALLBACK_TMPL_NO_YIELD: &str = "fallbackTmplNoYield";
pub const RENDER_ATTRS: &str = "renderAttrs";
pub const RENDER_ATTRS_NO_YIELD: &str = "renderAttrsNoYield";
pub const ADD_SLOTTED_CONTENT: &str = "addSlottedContent";
pub const SERVER_SIDE_RENDER_COMPONENT: &str = "serverSideRenderComponent";
pub const RENDER_COMPONENT: &str = "renderComponent";
pub const NORMALIZE_TEXT_CONTENT: &str = "normalizeTextContent";
pub const RENDER_TEXT_CONTENT: &str = "renderTextContent";
pub const HAS_SCOPED_STATIC_STYLESHEETS: &str = "hasScopedStaticStylesheets";
pub const RENDER_STYLESHEETS: &str = "renderStylesheets";
pub const TO_ITERATOR_DIRECTIVE: &str = "toIteratorDirective";
pub const VALIDATE_STYLE_TEXT_CONTENTS: &str = "validateStyleTextContents";
pub const CREATE_CONTEXT_PROVIDER: &str = "createContextProvider";
pub const ESTABLISH_CONTEXTFUL_RELATIONSHIP: &str = "establishContextfulRelationship";
pub const CONNECT_CONTEXT: &str = "connectContext";
pub const READONLY: &str = "readonly";
pub const SET_STATIC_INTERNALS: &str = "setStaticInternals";
pub const REGISTER_PUBLIC_PROPERTIES: &str = "registerPublicProperties";
pub const REGISTER_TEMPLATE: &str = "registerTemplate";
pub const ALL: &[&str] = &[
    HTML_ESCAPE,
    SET_HOOKS,
    SANITIZE_HTML_CONTENT,
    NORMALIZE_CLASS,
    NORMALIZE_TAB_INDEX,
    SET_CONTEXT_KEYS,
    SET_TRUSTED_SIGNAL_SET,
    SET_TRUSTED_CONTEXT_SET,
    DANGEROUS_DO_NOT_USE_ADD_TRUSTED_CONTEXT,
    IS_TRUSTED_SIGNAL,
    SIGNAL_BASE_CLASS,
    SET_FEATURE_FLAG,
    SET_FEATURE_FLAG_FOR_TEST,
    CLASS_LIST,
    LIGHTNING_ELEMENT,
    SYMBOL__DEFAULT_TEMPLATE,
    SYMBOL__GENERATE_MARKUP,
    SYMBOL__SET_INTERNALS,
    MUTATION_TRACKER,
    FALLBACK_TMPL,
    FALLBACK_TMPL_NO_YIELD,
    RENDER_ATTRS,
    RENDER_ATTRS_NO_YIELD,
    ADD_SLOTTED_CONTENT,
    SERVER_SIDE_RENDER_COMPONENT,
    RENDER_COMPONENT,
    NORMALIZE_TEXT_CONTENT,
    RENDER_TEXT_CONTENT,
    HAS_SCOPED_STATIC_STYLESHEETS,
    RENDER_STYLESHEETS,
    TO_ITERATOR_DIRECTIVE,
    VALIDATE_STYLE_TEXT_CONTENTS,
    CREATE_CONTEXT_PROVIDER,
    ESTABLISH_CONTEXTFUL_RELATIONSHIP,
    CONNECT_CONTEXT,
    READONLY,
    SET_STATIC_INTERNALS,
    REGISTER_PUBLIC_PROPERTIES,
    REGISTER_TEMPLATE,
];
