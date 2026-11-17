//! Port of `generateCustomElementTagName` (@lwc/shared/src/custom-element.ts).
pub fn generate_custom_element_tag_name(namespace: &str, name: &str) -> String {
    // name.replace(/([a-z])([A-Z])/g, '$1-$2').toLowerCase()
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        out.push(c);
        if c.is_ascii_lowercase() && i + 1 < chars.len() && chars[i + 1].is_ascii_uppercase() {
            out.push('-');
            out.push(chars[i + 1]);
            i += 2; // regex is non-overlapping: consumed the uppercase char
            continue;
        }
        i += 1;
    }
    format!("{namespace}-{}", out.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::generate_custom_element_tag_name as g;
    #[test]
    fn kebab() {
        assert_eq!(g("lwc", "test"), "lwc-test");
        assert_eq!(g("x", "fooBar"), "x-foo-bar");
        assert_eq!(g("x", "fooBAR"), "x-foo-bar");
        assert_eq!(g("x", "aBcD"), "x-a-bc-d");
    }
}
