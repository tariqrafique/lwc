//! @lwc/shared `src/style.ts`: `parseStyleText`, `normalizeStyleAttributeValue`, `IMPORTANT_FLAG`.
//! Single definition for lwc-template codegen and lwc-ssr (AB-L0022).

use crate::js::{js_trim, js_ws};

/// style.ts `parseStyleText`, as `Object.entries(styleMap)`: split on `DECLARATION_DELIMITER`
/// `/;(?![^(]*\))/g`, each non-empty declaration on `PROPERTY_DELIMITER` `/:(.+)/s` (first `:` with at
/// least one char after it), both sides trimmed. A repeated property keeps its first position and
/// last value; canonical array-index keys come first, ascending; `__proto__` is not an own key.
pub fn parse_style_text(css: &str) -> Vec<(String, String)> {
    let b = css.as_bytes();
    let mut decls = Vec::new();
    let mut start = 0;
    for i in 0..b.len() {
        if b[i] != b';' { continue; }
        // Negative lookahead: a `)` reachable without crossing a `(` blocks the split.
        let rest = &css[i + 1..];
        let in_parens = rest.find(['(', ')']).is_some_and(|j| rest.as_bytes()[j] == b')');
        if !in_parens { decls.push(&css[start..i]); start = i + 1; }
    }
    decls.push(&css[start..]);
    let mut map: Vec<(String, String)> = Vec::new();
    for d in decls {
        let Some(j) = d.find(':') else { continue };
        if d[j + 1..].is_empty() { continue; }
        let (k, v) = (js_trim(&d[..j]).to_string(), js_trim(&d[j + 1..]).to_string());
        if k == "__proto__" { continue; }
        match map.iter_mut().find(|(mk, _)| *mk == k) { Some(e) => e.1 = v, None => map.push((k, v)) }
    }
    let is_index = |k: &str| !k.is_empty() && k.bytes().all(|c| c.is_ascii_digit()) && (k == "0" || !k.starts_with('0')) && k.parse::<u32>().is_ok_and(|n| n < u32::MAX);
    let (mut idx, rest): (Vec<_>, Vec<_>) = map.into_iter().partition(|(k, _)| is_index(k));
    idx.sort_by_key(|(k, _)| k.parse::<u32>().unwrap());
    idx.extend(rest);
    idx
}

/// `IMPORTANT_FLAG` `/\s*!\s*important\s*$/i`: the value before the match, and whether it matched.
/// Non-unicode `/i` folds only ASCII onto `important`, so an ASCII case-insensitive compare is exact.
pub fn split_important(v: &str) -> (&str, bool) {
    let t = v.trim_end_matches(js_ws);
    let n = t.len().wrapping_sub("important".len());
    if t.len() >= 9 && t.is_char_boundary(n) && t[n..].eq_ignore_ascii_case("important") {
        if let Some(h) = t[..n].trim_end_matches(js_ws).strip_suffix('!') {
            return (h.trim_end_matches(js_ws), true);
        }
    }
    (v, false)
}

/// style.ts `normalizeStyleAttributeValue`: `key: value;` joined by a space, with
/// `value.replace(IMPORTANT_FLAG, ' !important').trim()`.
pub fn normalize_style_attribute_value(style: &str) -> String {
    parse_style_text(style).into_iter().map(|(k, v)| {
        let v = match split_important(&v) { (h, true) => js_trim(&format!("{h} !important")).to_string(), _ => v };
        format!("{k}: {v};")
    }).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values from node v24.22.1 running style.ts's parseStyleText / normalizeStyleAttributeValue.
    #[test]
    fn matches_style_ts() {
        let e = |v: Vec<(String, String)>| v.into_iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("|");
        assert_eq!(e(parse_style_text("__proto__: x; color: red")), "color=red");
        assert_eq!(e(parse_style_text("b: url(a;b); 1: x; a:; c::d; b: 2")), "1=x|b=2|c=:d");
        assert_eq!(e(parse_style_text("a:\u{85}b\u{85};\u{feff}c:d")), "a=\u{85}b\u{85}|c=d");
        assert_eq!(split_important("red ! IMPORTANT  "), ("red", true));
        assert_eq!(split_important("red important"), ("red important", false));
        assert_eq!(normalize_style_attribute_value("color: red !important; w: !important; x: y"), "color: red !important; w: !important; x: y;");
        assert_eq!(normalize_style_attribute_value("a: \u{85}x\u{85} !IMPORTANT"), "a: \u{85}x\u{85} !important;");
    }
}
