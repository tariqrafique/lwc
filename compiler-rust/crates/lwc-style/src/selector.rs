//! Minimal lossless selector parser modelled on postcss-selector-parser's node model.
//! Each node keeps its raw text, so `to_string` round-trips the input exactly.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind { Tag, Universal, Nesting, Id, Class, Attribute, Pseudo, Combinator }

#[derive(Debug, Clone)]
pub struct SNode {
    pub kind: Kind,
    /// Raw text as written (combinators include surrounding whitespace).
    pub raw: String,
    /// postcss-selector-parser `value`: tag/class/id name, pseudo name with colons, trimmed combinator.
    pub value: String,
    /// Attribute name (unescaped), for `Kind::Attribute`.
    pub attribute: String,
    /// Byte offset relative to the rule selector start.
    pub index: usize,
    /// Parsed pseudo-class arguments (e.g. `:host(.a, .b)`), plus their raw text.
    pub args: Option<SelectorList>,
    pub args_raw: Option<String>,
}

impl SNode {
    pub fn attr(name: &str) -> Self {
        SNode { kind: Kind::Attribute, raw: format!("[{name}]"), value: String::new(), attribute: name.into(), index: 0, args: None, args_raw: None }
    }
    pub fn combinator(raw: &str) -> Self {
        SNode { kind: Kind::Combinator, raw: raw.into(), value: raw.trim().to_string(), attribute: String::new(), index: 0, args: None, args_raw: None }
    }
    pub fn is_pseudo(&self, name: &str) -> bool { self.kind == Kind::Pseudo && self.value == name }
    pub fn is_pseudo_element(&self) -> bool {
        if self.kind != Kind::Pseudo { return false; }
        let v = self.value.to_ascii_lowercase();
        v.starts_with("::") || matches!(v.as_str(), ":before" | ":after" | ":first-letter" | ":first-line")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Selector { pub before: String, pub nodes: Vec<SNode>, pub after: String }

#[derive(Debug, Clone, Default)]
pub struct SelectorList { pub selectors: Vec<Selector> }

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.before)?;
        for n in &self.nodes { f.write_str(&n.raw)?; }
        f.write_str(&self.after)
    }
}
impl std::fmt::Display for SelectorList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, s) in self.selectors.iter().enumerate() { if i > 0 { f.write_str(",")?; } write!(f, "{s}")?; }
        Ok(())
    }
}

impl SelectorList {
    /// Pre-order walk over every node including pseudo arguments (postcss-selector-parser `walk`).
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a SNode)) {
        for s in &self.selectors {
            for n in &s.nodes {
                f(n);
                if let Some(a) = &n.args { a.walk(f); }
            }
        }
    }
}

fn is_ws(c: char) -> bool { matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0c') }

pub fn parse(s: &str) -> SelectorList { parse_at(s, 0) }

fn parse_at(s: &str, base: usize) -> SelectorList {
    let mut list = SelectorList::default();
    let mut start = 0;
    for (i, _) in split_top_level(s, ',') { list.selectors.push(parse_selector(&s[start..i], base + start)); start = i + 1; }
    list.selectors.push(parse_selector(&s[start..], base + start));
    list
}

/// Byte positions of `sep` at depth 0, outside strings/escapes.
fn split_top_level(s: &str, sep: char) -> Vec<(usize, char)> {
    let mut out = Vec::new();
    let (mut depth, mut quote, mut esc) = (0i32, None::<char>, false);
    for (i, c) in s.char_indices() {
        if esc { esc = false; continue; }
        if c == '\\' { esc = true; continue; }
        if let Some(q) = quote { if c == q { quote = None; } continue; }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ if c == sep && depth == 0 => out.push((i, c)),
            _ => {}
        }
    }
    out
}

/// Index just past the bracket matching the opener at `open`.
fn matching(s: &str, open: usize) -> usize {
    let (mut depth, mut quote, mut esc) = (0i32, None::<char>, false);
    for (i, c) in s[open..].char_indices() {
        if esc { esc = false; continue; }
        if c == '\\' { esc = true; continue; }
        if let Some(q) = quote { if c == q { quote = None; } continue; }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' => depth += 1,
            ')' | ']' => { depth -= 1; if depth == 0 { return open + i + 1; } }
            _ => {}
        }
    }
    s.len()
}

fn ident_end(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() {
        let c = b[i];
        if c == b'\\' { i += 2; continue; }
        if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80 { i += 1; } else { break; }
    }
    i.min(s.len())
}

pub fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            let mut hex = String::new();
            while hex.len() < 6 { let Some(h) = it.next_if(|c| c.is_ascii_hexdigit()) else { break }; hex.push(h); }
            if !hex.is_empty() {
                if it.peek() == Some(&' ') { it.next(); }
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) { out.push(ch); }
            } else if let Some(n) = it.next() { out.push(n); }
        } else { out.push(c); }
    }
    out
}

fn parse_selector(s: &str, base: usize) -> Selector {
    let mut sel = Selector::default();
    let lead = s.len() - s.trim_start_matches(is_ws).len();
    let trail_start = s.trim_end_matches(is_ws).len().max(lead);
    sel.before = s[..lead].to_string();
    sel.after = s[trail_start..].to_string();
    let body_end = trail_start;
    let mut i = lead;
    let b = s.as_bytes();
    let node = |kind, raw: &str, value: String, idx: usize| SNode { kind, raw: raw.to_string(), value, attribute: String::new(), index: base + idx, args: None, args_raw: None };
    while i < body_end {
        let c = b[i] as char;
        let st = i;
        if is_ws(c) || c == '>' || c == '+' || c == '~' || s[i..].starts_with("/deep/") {
            // combinator: ws* op? ws*
            while i < body_end && is_ws(b[i] as char) { i += 1; }
            let rest = &s[i..body_end];
            for op in [">>>", "/deep/", ">", "+", "~"] { if rest.starts_with(op) { i += op.len(); break; } }
            while i < body_end && is_ws(b[i] as char) { i += 1; }
            let raw = &s[st..i];
            let v = raw.trim();
            sel.nodes.push(node(Kind::Combinator, raw, if v.is_empty() { " ".into() } else { v.into() }, st + (raw.len() - raw.trim_start().len())));
            continue;
        }
        match c {
            '#' | '.' => {
                i = ident_end(s, i + 1);
                sel.nodes.push(node(if c == '#' { Kind::Id } else { Kind::Class }, &s[st..i], s[st + 1..i].to_string(), st));
            }
            '[' => {
                i = matching(s, i).min(body_end);
                let inner = &s[st + 1..i.saturating_sub(1).max(st + 1)];
                let inner_t = inner.trim_start();
                let name_end = inner_t.find(|ch: char| matches!(ch, '=' | '~' | '|' | '^' | '$' | '*' | ']') || is_ws(ch)).unwrap_or(inner_t.len());
                let mut n = node(Kind::Attribute, &s[st..i], String::new(), st);
                n.attribute = unescape(&inner_t[..name_end]);
                sel.nodes.push(n);
            }
            ':' => {
                let colons = if s[i..].starts_with("::") { 2 } else { 1 };
                i = ident_end(s, i + colons);
                let value = s[st..i].to_string();
                let mut n = node(Kind::Pseudo, "", value, st);
                if i < body_end && b[i] == b'(' {
                    let close = matching(s, i).min(body_end);
                    let inner_end = if s[..close].ends_with(')') { close - 1 } else { close };
                    let inner = &s[i + 1..inner_end];
                    n.args = Some(parse_at(inner, base + i + 1));
                    n.args_raw = Some(inner.to_string());
                    i = close;
                }
                n.raw = s[st..i].to_string();
                sel.nodes.push(n);
            }
            '*' => { i += 1; sel.nodes.push(node(Kind::Universal, "*", "*".into(), st)); }
            '&' => { i += 1; sel.nodes.push(node(Kind::Nesting, "&", "&".into(), st)); }
            _ => {
                i = ident_end(s, i);
                if i < body_end && b[i] == b'|' { i = ident_end(s, i + 1); }
                if i == st { i += s[st..].chars().next().map_or(1, |c| c.len_utf8()); }
                sel.nodes.push(node(Kind::Tag, &s[st..i], s[st..i].to_string(), st));
            }
        }
    }
    sel
}
