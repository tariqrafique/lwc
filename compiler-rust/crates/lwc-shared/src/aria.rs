//! @lwc/shared `src/aria.ts`. Data: `generated.rs`. Upstream builds both maps with
//! `Object.create(null)`, so prototype keys such as `constructor` are never members here either.
use crate::generated::{ARIA_ATTR_TO_PROP, ARIA_PROP_TO_ATTR};
pub use crate::generated::{ARIA_PROPERTY_NAMES, ID_REFERENCING_ATTRIBUTES_SET};
use crate::lookup;

/// `AriaAttrNameToPropNameMap[attrName]` (aria.ts:93), e.g. `aria-label` -> `ariaLabel`.
pub fn aria_attr_name_to_prop_name(attr_name: &str) -> Option<&'static str> {
    lookup(ARIA_ATTR_TO_PROP, attr_name)
}

/// `AriaPropNameToAttrNameMap[propName]` (aria.ts:93,135), e.g. `ariaLabel` -> `aria-label`.
pub fn aria_prop_name_to_attr_name(prop_name: &str) -> Option<&'static str> {
    lookup(ARIA_PROP_TO_ATTR, prop_name)
}

/// `isAriaAttribute(attrName)` (aria.ts:116): `attrName in AriaAttrNameToPropNameMap`.
pub fn is_aria_attribute(attr_name: &str) -> bool {
    aria_attr_name_to_prop_name(attr_name).is_some()
}

/// `ID_REFERENCING_ATTRIBUTES_SET.has(attrName)` (aria.ts:122).
pub fn is_id_referencing_attribute(attr_name: &str) -> bool {
    ID_REFERENCING_ATTRIBUTES_SET
        .binary_search(&attr_name)
        .is_ok()
}
