//! @lwc/shared `src/void-elements.ts`. Data: `generated::VOID_ELEMENTS`.
use crate::generated::VOID_ELEMENTS;
use crate::namespaces::HTML_NAMESPACE;

/// `isVoidElement(name, namespace)` (void-elements.ts:42):
/// `namespace === HTML_NAMESPACE && VOID_ELEMENTS_SET.has(name.toLowerCase())`.
/// The lowercasing is full Unicode like JS `toLowerCase`, so `"\u{212A}eygen"` (KELVIN SIGN) is void.
pub fn is_void_element(name: &str, namespace: &str) -> bool {
    namespace == HTML_NAMESPACE
        && VOID_ELEMENTS
            .binary_search(&name.to_lowercase().as_str())
            .is_ok()
}
