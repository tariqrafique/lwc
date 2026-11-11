//! @lwc/shared `src/custom-element.ts`.

/// `generateCustomElementTagName(namespace, name)` (custom-element.ts:17):
/// `` `${namespace}-${name.replace(/([a-z])([A-Z])/g, '$1-$2').toLowerCase()}` ``.
/// The regex is ASCII-only; the lowercasing is full Unicode like JS `toLowerCase`.
///
/// Not ported: upstream `console.warn`s when `namespace` or `name` is empty (it only writes to stderr
/// and still returns the same string), so this function never warns.
pub fn generate_custom_element_tag_name(namespace: &str, name: &str) -> String {
    let mut kebab = String::with_capacity(name.len() + 4);
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        kebab.push(c);
        if let Some(n) = chars.next_if(|n| c.is_ascii_lowercase() && n.is_ascii_uppercase()) {
            kebab.push('-');
            kebab.push(n);
        }
    }
    format!("{namespace}-{}", kebab.to_lowercase())
}
