//! TS optimize.ts `optimizeStaticExpressions`: hoist every static object/array literal in the
//! template function to a module-level `const stcN = ...;`.
//!
//! TS walks the `tmpl` ESTree pre-order (estree-walker `enter`); the outermost node for which
//! `isStaticObjectOrArray` holds is replaced by an identifier and not descended into. Nodes are
//! deduped by their `astring` print, and `stcN` is numbered by first encounter. This crate emits
//! JS text, so the same walk runs over the tokens of the emitted function body.
//!
//! Static (optimize.ts isStaticObjectOrArray): an ObjectExpression whose properties are all plain
//! `key: value` (not computed, method, shorthand or spread) and an ArrayExpression with no holes,
//! where every value is a `Literal` (string, number, boolean, null) or itself static. `{}` and `[]`
//! are static. `-1` is a UnaryExpression and template literals are not Literals, so neither is static.

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok<'a> {
    /// String, numeric, `true`/`false`/`null` literal (ESTree `Literal`).
    Lit(&'a str),
    Ident(&'a str),
    /// Template literal, opaque (never a Literal).
    Tmpl(&'a str),
    Punct(&'a str),
}

impl<'a> Tok<'a> {
    fn text(&self) -> &'a str { match *self { Tok::Lit(s) | Tok::Ident(s) | Tok::Tmpl(s) | Tok::Punct(s) => s } }
}

/// Byte-range tokens of `src`. Regex literals are not produced by the template codegen's
/// expression printer for anything that reaches here except via complex expressions, where a
/// mis-lexed `/` only makes a span non-static or skips it (both leave the text unchanged).
fn tokenize(src: &str) -> Vec<(usize, usize, Tok<'_>)> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() { i += 1; continue; }
        let s = i;
        if c == b'"' || c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != c { if b[i] == b'\\' { i += 1; } i += 1; }
            i = (i + 1).min(b.len());
            out.push((s, i, Tok::Lit(&src[s..i])));
        } else if c == b'`' {
            i = skip_template(b, i);
            out.push((s, i, Tok::Tmpl(&src[s..i])));
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') { i += 1; }
            out.push((s, i, Tok::Lit(&src[s..i])));
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80 {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$' || b[i] >= 0x80) { i += 1; }
            let w = &src[s..i];
            out.push((s, i, if matches!(w, "true" | "false" | "null") { Tok::Lit(w) } else { Tok::Ident(w) }));
        } else {
            let rest = &src[i..];
            let n = ["...", "=>", "?.", "??", "||", "&&"].iter().find(|p| rest.starts_with(**p)).map_or(1, |p| p.len());
            i += n;
            out.push((s, i, Tok::Punct(&src[s..i])));
        }
    }
    out
}

/// End (exclusive) of the template literal starting at `i`, skipping `${ ... }` substitutions.
fn skip_template(b: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'`' => return i + 1,
            b'$' if b.get(i + 1) == Some(&b'{') => {
                i += 2;
                let mut depth = 1;
                while i < b.len() && depth > 0 {
                    match b[i] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        b'`' => { i = skip_template(b, i); continue; }
                        q @ (b'"' | b'\'') => { i += 1; while i < b.len() && b[i] != q { if b[i] == b'\\' { i += 1; } i += 1; } }
                        _ => {}
                    }
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    b.len()
}

/// Index of the bracket closing the one at `open`, if balanced.
fn closing(toks: &[(usize, usize, Tok)], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (j, t) in toks.iter().enumerate().skip(open) {
        match t.2 {
            Tok::Punct("{" | "[" | "(") => depth += 1,
            Tok::Punct("}" | "]" | ")") => { depth -= 1; if depth == 0 { return Some(j); } }
            _ => {}
        }
    }
    None
}

/// `{` here starts an ObjectExpression (not a block / pattern), judged by the previous token.
fn opens_object(prev: Option<&Tok>) -> bool {
    match prev {
        None => true,
        Some(Tok::Punct(p)) => !matches!(*p, ")" | "=>" | ";" | "{" | "}"),
        Some(Tok::Ident(w)) => matches!(*w, "return" | "typeof" | "void" | "in" | "of" | "case"),
        _ => false,
    }
}

/// `[` here starts an ArrayExpression (not a computed member access).
fn opens_array(prev: Option<&Tok>) -> bool {
    match prev {
        None => true,
        Some(Tok::Punct(p)) => !matches!(*p, ")" | "]" | "}"),
        Some(Tok::Ident(w)) => matches!(*w, "return" | "typeof" | "void" | "in" | "of" | "case"),
        _ => false,
    }
}

/// Top-level comma-separated segments of `toks[from..to]`.
fn segments(toks: &[(usize, usize, Tok)], from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let (mut s, mut depth) = (from, 0i32);
    for j in from..to {
        match toks[j].2 {
            Tok::Punct("{" | "[" | "(") => depth += 1,
            Tok::Punct("}" | "]" | ")") => depth -= 1,
            Tok::Punct(",") if depth == 0 => { out.push((s, j)); s = j + 1; }
            _ => {}
        }
    }
    // A trailing comma is not a hole.
    if s < to || out.is_empty() && from < to { out.push((s, to)); }
    out
}

/// optimize.ts isStaticObjectOrArray for the literal spanning `toks[open..=close]`.
fn is_static(toks: &[(usize, usize, Tok)], open: usize, close: usize) -> bool {
    let value = |s: usize, e: usize| -> bool {
        match e - s {
            1 => matches!(toks[s].2, Tok::Lit(_)),
            0 => false,
            _ => matches!(toks[s].2, Tok::Punct("{" | "[")) && closing(toks, s) == Some(e - 1) && is_static(toks, s, e - 1),
        }
    };
    let segs = segments(toks, open + 1, close);
    match toks[open].2 {
        Tok::Punct("{") => segs.iter().all(|&(s, e)| {
            e >= s + 3 && matches!(toks[s].2, Tok::Ident(_) | Tok::Lit(_)) && toks[s + 1].2 == Tok::Punct(":") && value(s + 2, e)
        }),
        Tok::Punct("[") => segs.iter().all(|&(s, e)| value(s, e)),
        _ => false,
    }
}

/// astring print of the static literal `toks[open..=close]` (2-space indent at `depth`).
fn print(toks: &[(usize, usize, Tok)], open: usize, close: usize, depth: usize) -> String {
    let segs = segments(toks, open + 1, close);
    let val = |s: usize, e: usize, d: usize| if e - s == 1 { toks[s].2.text().to_string() } else { print(toks, s, e - 1, d) };
    if segs.is_empty() { return toks[open].2.text().to_string() + toks[close].2.text(); }
    if toks[open].2 == Tok::Punct("[") {
        return format!("[{}]", segs.iter().map(|&(s, e)| val(s, e, depth)).collect::<Vec<_>>().join(", "));
    }
    let pad = "  ".repeat(depth + 1);
    let props: Vec<String> = segs.iter().map(|&(s, e)| format!("{pad}{}: {}", toks[s].2.text(), val(s + 2, e, depth + 1))).collect();
    format!("{{\n{}\n{}}}", props.join(",\n"), "  ".repeat(depth))
}

/// Whether the `(` at `paren` is a call (or similar) rather than grouping.
fn is_call_open(toks: &[(usize, usize, Tok<'_>)], paren: usize) -> bool {
    if paren == 0 { return false; }
    match toks[paren - 1].2 {
        // Unary/binary keyword operators are Ident tokens but not callees (`typeof (`, `in (`).
        Tok::Ident(s) => !matches!(s, "typeof" | "void" | "delete" | "in" | "instanceof" | "new" | "await" | "yield"),
        Tok::Punct(")") | Tok::Punct("]") | Tok::Punct(".") => true,
        _ => false,
    }
}

/// SHIM(AB-D0004 S4 grouping_wrap, expires S6): grouping vs call paren heuristic in the text
/// hoist pass (AB-D0004 S4 cond 3). Consumes nested grouping pairs so `(( {a:1} ))` becomes `stcN`,
/// but leaves call parens (`foo({a:1})` → `foo(stcN)`). S6 moves static hoist onto the AST
/// (isStaticObjectOrArray + pre-order replace) and deletes this tokenizer.
fn grouping_wrap(toks: &[(usize, usize, Tok<'_>)], open: usize, close: usize) -> Option<(usize, usize, usize)> {
    let mut lo = open;
    let mut hi = close;
    let mut found = false;
    while lo > 0 && hi + 1 < toks.len()
        && toks[lo - 1].2 == Tok::Punct("(")
        && toks[hi + 1].2 == Tok::Punct(")")
        && !is_call_open(toks, lo - 1)
    {
        lo -= 1;
        hi += 1;
        found = true;
    }
    if !found { return None; }
    Some((toks[lo].0, toks[hi].1, hi + 1))
}

/// Hoist static literals out of `body` (the tmpl function body). `decls` holds the
/// `(dedupe key, astring print)` of each `stcN` in order; returns the rewritten body.
pub(super) fn hoist_static_expressions(body: &str, decls: &mut Vec<(String, String)>) -> String {
    let toks = tokenize(body);
    let mut out = String::with_capacity(body.len());
    let mut last = 0;
    let mut i = 0;
    while i < toks.len() {
        let prev = i.checked_sub(1).map(|p| &toks[p].2);
        let candidate = match toks[i].2 {
            Tok::Punct("{") => opens_object(prev),
            Tok::Punct("[") => opens_array(prev),
            _ => false,
        };
        if candidate {
            if let Some(close) = closing(&toks, i).filter(|&c| is_static(&toks, i, c)) {
                let key: String = segments(&toks, i + 1, close).iter()
                    .map(|&(s, e)| toks[s..e].iter().map(|t| t.2.text()).collect::<String>())
                    .collect::<Vec<_>>().join(",");
                let key = format!("{}{key}{}", toks[i].2.text(), toks[close].2.text());
                let n = match decls.iter().position(|(k, _)| *k == key) {
                    Some(n) => n,
                    None => { decls.push((key, print(&toks, i, close, 0))); decls.len() - 1 }
                };
                // astring parenthesizes Object/Array (NEEDS_PARENTHESES) as a member object, then
                // TS AST-hoist replaces the node so the identifier prints without those parens
                // (`stcN.toString()`, not `(stcN).toString()`). Consume grouping parens only;
                // never call parens (`foo({a:1})` must become `foo(stcN)`, not `foostcN`).
                let (from, to, next) = grouping_wrap(&toks, i, close)
                    .unwrap_or((toks[i].0, toks[close].1, close + 1));
                out.push_str(&body[last..from]);
                out.push_str(&format!("stc{n}"));
                last = to;
                i = next;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&body[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> (String, Vec<String>) {
        let mut d = Vec::new();
        let out = hoist_static_expressions(src, &mut d);
        (out, d.into_iter().map(|(_, p)| p).collect())
    }

    #[test]
    fn hoists_outermost_static_and_dedupes() {
        let (out, d) = run(r#"return [api_element("div", { classMap: { "a": true }, key: 0 }), api_element("p", { key: 1 }, [api_text("x")]), api_element("i", { key: 0, classMap: { "a": true } })];"#);
        assert_eq!(out, r#"return [api_element("div", stc0), api_element("p", stc1, [api_text("x")]), api_element("i", stc2)];"#);
        assert_eq!(d, vec!["{\n  classMap: {\n    \"a\": true\n  },\n  key: 0\n}", "{\n  key: 1\n}", "{\n  key: 0,\n  classMap: {\n    \"a\": true\n  }\n}"]);
        let (out, d) = run(r#"f({ key: 1 }, { key: 1, }, [], {})"#);
        assert_eq!(out, "f(stc0, stc0, stc1, stc2)");
        assert_eq!(d, vec!["{\n  key: 1\n}", "[]", "{}"]);
    }

    #[test]
    fn non_static_shapes_are_kept_and_descended() {
        for src in [
            "f({ a: $cmp.x })", "f({ a: -1 })", "f({ a })", "f({ [k]: 1 })", "f({ ...x })",
            "f({ __proto__: null, ...x })", "f([1, , 2])", "f({ a: `t` })", "f([g()])", "a[0]", "x.y[\"z\"]",
            "function (a) { const {b: c} = a; }",
        ] {
            assert_eq!(run(src), (src.to_string(), vec![]), "{src}");
        }
        let (out, d) = run("f({ a: $cmp.x, b: { c: 1 }, d: [\"e\"] })");
        assert_eq!(out, "f({ a: $cmp.x, b: stc0, d: stc1 })");
        assert_eq!(d, vec!["{\n  c: 1\n}", "[\"e\"]"]);
    }

    #[test]
    fn grouping_parens_around_hoisted_object_are_dropped() {
        let (out, d) = run(r#"return [api_text(api_dynamic_text(({ obj: 'literal' }).toString()))];"#);
        assert_eq!(out, r#"return [api_text(api_dynamic_text(stc0.toString()))];"#);
        assert_eq!(d, vec!["{\n  obj: 'literal'\n}"]);
        let (out, d) = run(r#"f({ a: 1 })"#);
        assert_eq!(out, "f(stc0)", "{out}");
        assert_eq!(d, vec!["{\n  a: 1\n}"]);
        assert_eq!(run("typeof ({ a: 1 })").0, "typeof stc0");
        assert_eq!(run("!({ a: 1 })").0, "!stc0");
        assert_eq!(run("x in ({ a: 1 })").0, "x in stc0");
        assert_eq!(run("(({ a: 1 }))").0, "stc0");
        assert_eq!(run("foo({ a: 1 })").0, "foo(stc0)");
    }

    #[test]
    fn strings_and_templates_are_opaque() {
        let (out, d) = run(r#"f("{ a: 1 }", `${ {a: 1} }`, { "k}": "v]" })"#);
        assert_eq!(out, r#"f("{ a: 1 }", `${ {a: 1} }`, stc0)"#);
        assert_eq!(d, vec!["{\n  \"k}\": \"v]\"\n}"]);
    }
}
