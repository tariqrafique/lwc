//! JS string semantics shared by every crate. The single definition of JS whitespace and of the
//! JS string-literal quoter: `tests/single_definition.rs` fails if another crate defines its own
//! `js_ws`/`js_trim`/`json_string`/`js_str`.

use std::fmt::Write;

/// JS `WhiteSpace` + `LineTerminator` (ECMA-262 §12.2/§12.3): exactly the set matched by `\s` and
/// stripped by `String.prototype.trim`. TAB VT FF SP NBSP ZWNBSP, the `Zs` category (U+1680,
/// U+2000-U+200A, U+202F, U+205F, U+3000), LF CR LS PS. Not U+0085 (NEL), not U+180E
/// (`char::is_whitespace` accepts U+0085, which JS does not).
pub fn js_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\u{b}' | '\u{c}' | ' ' | '\u{a0}' | '\u{feff}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\n'
                | '\r'
                | '\u{2028}'
                | '\u{2029}'
    )
}

/// JS `String.prototype.trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(js_ws)
}

/// `Array.prototype.sort()` with no comparator on strings: compares UTF-16 code units, so an
/// astral char (surrogate 0xD800-0xDBFF) sorts before U+E000-U+FFFF, unlike `str::cmp`.
pub fn js_default_sort_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// One JavaScript string literal, the way astring 1.9.0 prints an ESTree `Literal` that has no
/// `raw` (`astring@1.9.0` `dist/astring.js` `Literal`: `state.write(JSON.stringify(node.value))`).
///
/// That is the oracle's only generated-literal path at b9e8007:
/// - `@lwc/template-compiler` `src/shared/estree.ts` `literal()` sets `value` and not `raw`, then
///   `astring.generate` prints it.
/// - `@lwc/ssr-compiler` builds holes with estree-toolkit `b.literal` (`Literal.raw` is not a
///   builder argument) and prints with the same astring `Literal`.
/// - `@lwc/style-compiler` `src/serialize.ts` `generateExpressionFromTokens` returns
///   `JSON.stringify(value)` for text tokens.
///
/// `JSON.stringify` on a string: `"` and `\` escaped, `\b \f \n \r \t`, `\u00xx` for the other
/// units below 0x20, a lone surrogate as lowercase `\udxxx`, and everything else — U+2028 and
/// U+2029 included — as itself. Pass `str::encode_utf16()`, or oxc `JSStr::encode_utf16()`, so a
/// string literal's lone surrogates survive. A source literal that already has `raw` is not this
/// function: astring writes that `raw` unchanged (the esTemplate snippets keep their own quotes).
pub fn string_literal(units: impl IntoIterator<Item = u16>) -> String {
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
    use super::*;

    /// Each case checked in node v24.22.1: `("x" + c).trim() === "x"` and `/\s/.test(c)`:
    ///   U+0085 trim: false \s: false
    ///   U+FEFF trim: true  \s: true
    ///   U+1680 trim: true  \s: true
    ///   U+180E trim: false \s: false
    ///   U+00A0, U+2028 trim: true; U+200B trim: false
    #[test]
    fn whitespace_matches_node() {
        for (c, ws) in [
            ('\u{85}', false),
            ('\u{feff}', true),
            ('\u{1680}', true),
            ('\u{180e}', false),
            ('\u{a0}', true),
            ('\u{2028}', true),
            ('\u{200b}', false),
        ] {
            assert_eq!(js_ws(c), ws, "U+{:04X}", c as u32);
            let s = format!("{c}x{c}");
            assert_eq!(js_trim(&s) == "x", ws, "trim U+{:04X}", c as u32);
        }
    }

    /// `JSON.stringify` short escapes, quotes, backslash, and a lone surrogate. The node corpus
    /// in `tests/js_literal_node.rs` covers the same classes against a live `JSON.stringify`.
    #[test]
    fn string_literal_json_stringify_classes() {
        assert_eq!(string_literal("a\"b\\c".encode_utf16()), "\"a\\\"b\\\\c\"");
        assert_eq!(string_literal("\n\r\t".encode_utf16()), "\"\\n\\r\\t\"");
        assert_eq!(string_literal("\u{8}\u{c}".encode_utf16()), "\"\\b\\f\"");
        assert_eq!(
            string_literal("\u{1}\u{7f}".encode_utf16()),
            "\"\\u0001\u{7f}\""
        );
        assert_eq!(
            string_literal("\u{2028}\u{2029}".encode_utf16()),
            "\"\u{2028}\u{2029}\""
        );
        assert_eq!(
            string_literal([0xd800, 0x61, 0xdfff]),
            "\"\\ud800a\\udfff\""
        );
        assert_eq!(string_literal([0xd83d, 0xde00]), "\"😀\"");
    }

    /// node: `["\ue000", "\u{1F600}", "b", "a", "", "B"].sort()` -> `["", "B", "a", "b", "😀", "\ue000"]`.
    #[test]
    fn default_sort_matches_node() {
        let mut v = vec!["\u{e000}", "\u{1F600}", "b", "a", "", "B"];
        v.sort_by(|a, b| js_default_sort_cmp(a, b));
        assert_eq!(v, ["", "B", "a", "b", "\u{1F600}", "\u{e000}"]);
        // Every char `char::is_whitespace` accepts, except NEL, is JS whitespace too.
        for c in (0..=0xffffu32)
            .filter_map(char::from_u32)
            .filter(|c| c.is_whitespace())
        {
            assert_eq!(js_ws(c), c != '\u{85}', "U+{:04X}", c as u32);
        }
    }
}
