//! @lwc/shared `src/html-attributes.ts`. Data: `generated.rs`.
use std::borrow::Cow;

use crate::aria::aria_prop_name_to_attr_name;
pub use crate::generated::{
    AMBIGUOUS_PROP_SET, DISALLOWED_PROP_SET, REFLECTIVE_GLOBAL_PROPERTY_SET,
};
use crate::generated::{
    BOOLEAN_ATTRIBUTES, GLOBAL_ATTRIBUTE, SPECIAL_ATTRIBUTE_TO_PROPERTY,
    SPECIAL_PROPERTY_TO_ATTRIBUTE,
};
use crate::lookup;

/// `isBooleanAttribute(attrName, tagName)` (html-attributes.ts:53): the attribute is in
/// `BOOLEAN_ATTRIBUTES` and either allowed on every tag (empty set) or on `tag_name`.
pub fn is_boolean_attribute(attr_name: &str, tag_name: &str) -> bool {
    lookup(BOOLEAN_ATTRIBUTES, attr_name)
        .is_some_and(|tags| tags.is_empty() || tags.binary_search(&tag_name).is_ok())
}

/// `isGlobalHtmlAttribute(attrName)` (html-attributes.ts:98).
pub fn is_global_html_attribute(attr_name: &str) -> bool {
    GLOBAL_ATTRIBUTE.binary_search(&attr_name).is_ok()
}

/// `SPECIAL_PROPERTY_ATTRIBUTE_MAPPING.get(propName)` (html-attributes.ts:103), e.g. `tabIndex` -> `tabindex`.
pub fn special_property_to_attribute(prop_name: &str) -> Option<&'static str> {
    lookup(SPECIAL_PROPERTY_TO_ATTRIBUTE, prop_name)
}

/// The inverse lookup (attr -> prop) that template-compiler parser/constants.ts:113 builds into
/// `ATTRS_PROPS_TRANFORMS`, e.g. `for` -> `htmlFor`.
pub fn special_attribute_to_property(attr_name: &str) -> Option<&'static str> {
    lookup(SPECIAL_ATTRIBUTE_TO_PROPERTY, attr_name)
}

/// `REFLECTIVE_GLOBAL_PROPERTY_SET.has(propName)` (html-attributes.ts:126).
pub fn is_reflective_global_property(prop_name: &str) -> bool {
    REFLECTIVE_GLOBAL_PROPERTY_SET
        .binary_search(&prop_name)
        .is_ok()
}

/// `htmlPropertyToAttribute(propName)` (html-attributes.ts:147): ARIA map, then the special mapping,
/// then each ASCII `A`-`Z` becomes `-` + lowercase (other UTF-16 units copied as-is). The JS result
/// cache is an optimisation only and is not ported.
pub fn html_property_to_attribute(prop_name: &str) -> Cow<'static, str> {
    if let Some(a) =
        aria_prop_name_to_attr_name(prop_name).or_else(|| special_property_to_attribute(prop_name))
    {
        return Cow::Borrowed(a);
    }
    let mut out = String::with_capacity(prop_name.len() + 4);
    for c in prop_name.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// `kebabCaseToCamelCase(attrName)` (html-attributes.ts:190): `attrName.replace(/-([a-z])/g, g => g[1].toUpperCase())`.
/// The regex is ASCII-only, so this is exact for every input. The JS result cache is not ported.
pub fn kebab_case_to_camel_case(attr_name: &str) -> String {
    let mut out = String::with_capacity(attr_name.len());
    let mut chars = attr_name.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek() {
            Some(&n) if c == '-' && n.is_ascii_lowercase() => {
                out.push(n.to_ascii_uppercase());
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// `AMBIGUOUS_PROP_SET.get(attr)` (html-attributes.ts:208), e.g. `tabindex` -> `tabIndex`.
pub fn ambiguous_prop(attr_name: &str) -> Option<&'static str> {
    lookup(AMBIGUOUS_PROP_SET, attr_name)
}

/// `DISALLOWED_PROP_SET.has(propName)` (html-attributes.ts:222).
pub fn is_disallowed_prop(prop_name: &str) -> bool {
    DISALLOWED_PROP_SET.binary_search(&prop_name).is_ok()
}
