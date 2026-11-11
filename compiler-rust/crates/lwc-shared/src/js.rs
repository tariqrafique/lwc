//! JS string semantics shared by every crate. The single definition of JS whitespace: a guard in
//! `tests/single_definition.rs` fails if another crate defines its own `js_ws`/`js_trim`.

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
