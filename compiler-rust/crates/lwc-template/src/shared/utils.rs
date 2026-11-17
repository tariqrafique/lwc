//! @lwc/template-compiler `src/shared/utils.ts`.

/// `toPropertyName(attr)` (shared/utils.ts:10): drop each `-` and uppercase the next UTF-16 unit.
/// Upper-casing follows JS exactly: full Unicode for BMP chars (`x-ß` -> `xSS`), astral chars unchanged.
pub fn to_property_name(attr: &str) -> String {
    let mut prop = String::with_capacity(attr.len());
    let mut upper_next = false;
    for c in attr.chars() {
        if c == '-' {
            upper_next = true;
        } else {
            if upper_next {
                super::push_upper_utf16_unit(&mut prop, c);
            } else {
                prop.push(c);
            }
            upper_next = false;
        }
    }
    prop
}
