//! JavaScript literal printing shared by the code generators.

use std::fmt::Write;

/// `JSON.stringify(string)` for a JavaScript string given as UTF-16 code units, byte for byte:
/// `\"` and `\\`, `\b \f \n \r \t`, `\u00xx` for the other units below 0x20, a lone surrogate as
/// lowercase `\udxxx` (well-formed JSON.stringify), and everything else, U+2028 and U+2029
/// included, as itself. Pass `str::encode_utf16()`, or oxc `JSStr::encode_utf16()` to keep a
/// string literal's lone surrogates.
pub fn json_string(units: impl IntoIterator<Item = u16>) -> String {
    let mut out = String::from("\"");
    for unit in char::decode_utf16(units) {
        match unit {
            Ok('"') => out.push_str("\\\""),
            Ok('\\') => out.push_str("\\\\"),
            Ok('\u{8}') => out.push_str("\\b"),
            Ok('\u{c}') => out.push_str("\\f"),
            Ok('\n') => out.push_str("\\n"),
            Ok('\r') => out.push_str("\\r"),
            Ok('\t') => out.push_str("\\t"),
            Ok(c) if c < ' ' => write!(out, "\\u{:04x}", c as u32).unwrap(),
            Ok(c) => out.push(c),
            Err(lone) => write!(out, "\\u{:04x}", lone.unpaired_surrogate()).unwrap(),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::json_string;

    #[test]
    fn escapes() {
        assert_eq!(json_string("a\"b\\c\n\u{1}\u{7f}\u{2028}😀".encode_utf16()), "\"a\\\"b\\\\c\\n\\u0001\u{7f}\u{2028}😀\"");
        assert_eq!(json_string([0xd800, 0x61, 0xdfff]), "\"\\ud800a\\udfff\"");
        assert_eq!(json_string([0xd83d, 0xde00]), "\"😀\"");
    }
}
