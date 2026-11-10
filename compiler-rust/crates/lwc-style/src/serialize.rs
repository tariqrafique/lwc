//! Port of serialize.ts: postcss.stringify-style walk -> tokens -> JS module.
use crate::ast::*;
use crate::consts::*;
use crate::Config;
use crate::StyleError;

#[derive(Clone, Copy, PartialEq)]
enum Kind { RuleStart, RuleEnd(HostKind), Decl, AtRule, Comment, Other }

#[derive(Clone, PartialEq, Debug)]
enum Tok { Text(String), Expr(String), Ident(&'static str) }

const BINARY_EXPRESSION_LIMIT: usize = 100;

/// Emits parts exactly like PostCSS's Stringifier builder callback.
fn stringify(root: &Root, out: &mut Vec<(String, Kind)>) {
    body(&root.nodes, root.semicolon, out);
    if !root.after.is_empty() { out.push((root.after.clone(), Kind::Other)); }
}

fn body(nodes: &[Node], semicolon: bool, out: &mut Vec<(String, Kind)>) {
    // `while (last > 0) { if (nodes[last].type !== 'comment') break; last -= 1 }`
    let last = nodes.iter().rposition(|n| !matches!(n, Node::Comment(_))).unwrap_or(0);
    for (i, n) in nodes.iter().enumerate() {
        if !n.before().is_empty() { out.push((n.before().to_string(), Kind::Other)); }
        let semi = last != i || semicolon;
        match n {
            Node::Comment(c) => out.push((format!("/*{}*/", c.text), Kind::Comment)),
            Node::Decl(d) => {
                let mut s = format!("{}{}{}", d.prop, d.between, raw_value(&d.value, &d.raw_value));
                if let Some(imp) = &d.important { s.push_str(imp); }
                if semi { s.push(';'); }
                out.push((s, Kind::Decl));
            }
            Node::Rule(r) => {
                block(&format!("{}{}", raw_value(&r.selector, &r.raw_selector), r.between), &r.block, Kind::RuleStart, Kind::RuleEnd(r.host), out);
                if !r.own_semicolon.is_empty() { out.push((r.own_semicolon.clone(), Kind::RuleEnd(r.host))); }
            }
            Node::AtRule(a) => {
                let name = format!("@{}{}", a.name, a.after_name);
                let params = if a.params.is_empty() { "" } else { raw_value(&a.params, &a.raw_params) };
                match &a.block {
                    Some(b) => block(&format!("{name}{params}{}", a.between), b, Kind::AtRule, Kind::AtRule, out),
                    None => out.push((format!("{name}{params}{}{}", a.between, if semi { ";" } else { "" }), Kind::AtRule)),
                }
            }
        }
    }
}

fn block(start: &str, b: &Block, sk: Kind, ek: Kind, out: &mut Vec<(String, Kind)>) {
    out.push((format!("{start}{{"), sk));
    body(&b.nodes, b.semicolon, out);
    if !b.after.is_empty() { out.push((b.after.clone(), Kind::Other)); }
    out.push(("}".into(), ek));
}

/// JS: str.replace(/(\r\n\t|\n|\r\t)/gm, '').trim()
fn normalize_string(s: &str) -> String {
    s.replace("\r\n\t", "").replace('\n', "").replace("\r\t", "").trim().to_string()
}

/// JS: data.replace(/( {2,})/gm, ' ')
fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev = false;
    for c in s.chars() {
        if c == ' ' { if !prev { out.push(' '); } prev = true; } else { out.push(c); prev = false; }
    }
    out
}

/// JS regex: /[[-](attrs)]?/g
fn tokenize_css(data: &str) -> Vec<Tok> {
    let data = collapse_spaces(data);
    let attrs = [SHADOW_ATTRIBUTE, HOST_ATTRIBUTE, DIR_ATTRIBUTE_NATIVE_LTR, DIR_ATTRIBUTE_NATIVE_RTL, DIR_ATTRIBUTE_SYNTHETIC_LTR, DIR_ATTRIBUTE_SYNTHETIC_RTL];
    let mut toks = Vec::new();
    let (mut last, mut i) = (0, 0);
    let bytes = data.as_bytes();
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'[' || c == b'-' {
            if let Some(a) = attrs.iter().find(|a| data[i + 1..].starts_with(**a)).copied() {
                let mut end = i + 1 + a.len();
                if bytes.get(end) == Some(&b']') { end += 1; }
                if i > last { toks.push(Tok::Text(data[last..i].to_string())); }
                if c == b'[' {
                    if a == SHADOW_ATTRIBUTE { toks.push(Tok::Ident("shadowSelector")); }
                    else if a == HOST_ATTRIBUTE { toks.push(Tok::Ident("hostSelector")); }
                    else {
                        let native = a == DIR_ATTRIBUTE_NATIVE_LTR || a == DIR_ATTRIBUTE_NATIVE_RTL;
                        let dir = if a == DIR_ATTRIBUTE_NATIVE_LTR || a == DIR_ATTRIBUTE_SYNTHETIC_LTR { "ltr" } else { "rtl" };
                        toks.push(Tok::Expr(if native { format!("useNativeDirPseudoclass ? ':dir({dir})' : ''") } else { format!("useNativeDirPseudoclass ? '' : '[dir=\"{dir}\"]'") }));
                    }
                } else {
                    toks.push(Tok::Ident("suffixToken"));
                }
                i = end; last = end; continue;
            }
        }
        i += 1;
    }
    if last < data.len() { toks.push(Tok::Text(data[last..].to_string())); }
    toks
}

fn reduce_tokens(tokens: Vec<Tok>) -> Vec<Tok> {
    let mut acc: Vec<Tok> = Vec::new();
    for t in tokens {
        if let (Tok::Text(v), Some(Tok::Text(p))) = (&t, acc.last_mut()) { p.push_str(v); continue; }
        acc.push(t);
    }
    acc.into_iter().filter(|t| !matches!(t, Tok::Text(s) | Tok::Expr(s) if s.is_empty())).collect()
}

/// JSON.stringify for strings.
fn js_string(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"), '\t' => o.push_str("\\t"), '\u{8}' => o.push_str("\\b"), '\u{c}' => o.push_str("\\f"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn generate_expression(tokens: Vec<Tok>) -> String {
    let parts: Vec<String> = reduce_tokens(tokens).into_iter().map(|t| match t {
        Tok::Text(v) => js_string(&v),
        Tok::Expr(v) => format!("({v})"),
        Tok::Ident(v) => v.to_string(),
    }).collect();
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        n if n < BINARY_EXPRESSION_LIMIT => parts.join(" + "),
        _ => format!("[{}].join('')", parts.join(", ")),
    }
}

/// Port of deduplicateHostTokens: shared token suffix is emitted once after a ternary.
fn dedup_host_tokens(native: &[Tok], synthetic: &[Tok]) -> Vec<Tok> {
    let mut dup = 0;
    while dup < native.len() && dup < synthetic.len() && native[native.len() - 1 - dup] == synthetic[synthetic.len() - 1 - dup] { dup += 1; }
    let ne = generate_expression(native[..native.len() - dup].to_vec());
    let se = generate_expression(synthetic[..synthetic.len() - dup].to_vec());
    let mut out = vec![Tok::Expr(format!("(useActualHostSelector ? {ne} : {se})"))];
    out.extend_from_slice(&synthetic[synthetic.len() - dup..]);
    out
}

fn serialize_css(root: &Root) -> Result<String, StyleError> {
    let mut parts = Vec::new();
    stringify(root, &mut parts);
    let mut tokens = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut native: Option<Vec<Tok>> = None;
    for (part, kind) in parts {
        match kind {
            Kind::RuleStart => cur.extend(tokenize_css(&normalize_string(&part))),
            Kind::RuleEnd(host) => {
                cur.push(Tok::Text(part));
                match host {
                    HostKind::Native => native = Some(std::mem::take(&mut cur)),
                    HostKind::Synthetic => {
                        // serialize.ts: `throw new Error('Unexpected host rules ordering')` (reachable, e.g. `:host{};`).
                        let n = native.take().ok_or_else(|| StyleError::error("Unexpected host rules ordering"))?;
                        tokens.extend(dedup_host_tokens(&n, &cur));
                        cur.clear();
                    }
                    HostKind::None => tokens.append(&mut cur),
                }
            }
            Kind::Decl => cur.extend(tokenize_css(&part)),
            Kind::AtRule => {
                tokens.append(&mut cur);
                tokens.extend(tokenize_css(&normalize_string(&part)));
            }
            Kind::Comment => {}
            Kind::Other => cur.push(Tok::Text(normalize_string(&part))),
        }
    }
    Ok(generate_expression(tokens))
}

pub fn serialize(root: &Root, config: &Config) -> Result<String, StyleError> {
    let imported = &root.imports;
    let mut buf = String::new();
    for (i, id) in imported.iter().enumerate() { buf += &format!("import stylesheet{i} from \"{id}\";\n"); }
    if !imported.is_empty() { buf.push('\n'); }
    let mut list: Vec<String> = (0..imported.len()).map(|i| format!("stylesheet{i}")).collect();
    let style = serialize_css(root)?;
    let style = style.trim();
    let (dss, scoped) = (config.disable_synthetic_shadow_support, config.scoped);
    if !style.is_empty() {
        if dss && !scoped {
            buf += "function stylesheet() {\n  var token;\n  var useActualHostSelector = true;\n  var useNativeDirPseudoclass = true;\n";
        } else {
            buf += "function stylesheet(token, useActualHostSelector, useNativeDirPseudoclass) {\n";
        }
        if scoped {
            buf += "  var shadowSelector = token ? (\".\" + token) : \"\";\n  var hostSelector = token ? (\".\" + token + \"-host\") : \"\";\n";
        } else {
            buf += "  var shadowSelector = token ? (\"[\" + token + \"]\") : \"\";\n  var hostSelector = token ? (\"[\" + token + \"-host]\") : \"\";\n";
        }
        buf += "  var suffixToken = token ? (\"-\" + token) : \"\";\n";
        buf += &format!("  return {style};\n  {}\n}}\n", lwc_shared::meta::version_comment());
        if scoped { buf += &format!("stylesheet.{KEY_SCOPED_CSS} = true;\n"); }
        if dss { buf += &format!("stylesheet.{KEY_NATIVE_ONLY_CSS} = true;\n"); }
        list.push("stylesheet".into());
    }
    if list.is_empty() { buf += "export default undefined;"; } else { buf += &format!("export default [{}];", list.join(", ")); }
    Ok(buf)
}

#[cfg(test)]
mod lossless {
    //! AB-R002: every corpus input that parses goes through `parser::parse` and stringifies back
    //! byte-for-byte (minus a leading BOM, which PostCSS Input strips), like `postcss.parse(css).toString()`.
    use super::*;
    use std::{fs, path::{Path, PathBuf}};

    fn roundtrip(css: &str) -> Option<String> {
        let root = crate::parser::parse(css).ok()?;
        let mut parts = Vec::new();
        stringify(&root, &mut parts);
        Some(parts.into_iter().map(|(s, _)| s).collect())
    }

    fn inputs(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() { inputs(&p, out) } else if p.file_name().is_some_and(|n| n == "actual.css") { out.push(p) }
        }
    }

    #[test]
    fn corpus_stringifies_losslessly() {
        let upstream = std::env::var("LWC_FIXTURES").unwrap_or_else(|_| lwc_shared::oracle_root().join("packages/@lwc/style-compiler/src/__tests__/fixtures").to_string_lossy().into_owned());
        let mut files = Vec::new();
        inputs(Path::new(&upstream), &mut files);
        inputs(&lwc_shared::cargo_manifest_dir!().join("tests/parse-errors"), &mut files);
        let (mut parsed, mut bad) = (0, Vec::new());
        for f in &files {
            let css = fs::read_to_string(f).unwrap();
            if let Some(out) = roundtrip(&css) {
                parsed += 1;
                if out != css.trim_start_matches(['\u{feff}', '\u{fffe}']) { bad.push(f.display().to_string()); }
            }
        }
        assert!(bad.is_empty(), "not lossless:\n  {}", bad.join("\n  "));
        assert!(parsed > 0 || files.is_empty());
    }

    #[test]
    fn edge_cases_stringify_losslessly() {
        for css in ["a{b:c}", "a { b : c ; }\r\n", "\u{feff}a{}", "a{b:c!important}", "a{b:c ! important ;}", "a{*b:c;_d:e}",
            "@media  screen /* x */ {a{}}", "@import 'x' ;", "a /* s */ , b {}", "a{}; b{};", ";;a{;;b:c;;}", "/* only */", "a{/*c*/}",
            "a{--x: { y } ;--z:}", "a{b:url( x )}", "\r\n\r\na{}\r\n"] {
            let out = roundtrip(css).unwrap_or_else(|| panic!("should parse: {css:?}"));
            assert_eq!(out, css.trim_start_matches('\u{feff}'), "input {css:?}");
        }
    }
}
