//! Lossless PostCSS tree, built by [`crate::parser::parse`]. Fields mirror PostCSS node props and
//! `raws`, so `serialize` reproduces `postcss.stringify` byte for byte.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Loc {
    pub line: u32,
    pub column: u32,
    pub offset: usize,
}

/// PostCSS `raws.{selector,value,params} = { value, raw }`: set when the source text differs from
/// the clean prop (comments / trailing space dropped). The stringifier's `rawValue` emits `raw` only
/// while the prop still equals `value`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawValue {
    pub value: String,
    pub raw: String,
}

/// `Stringifier#rawValue(node, prop)`.
pub fn raw_value<'a>(value: &'a str, raw: &'a Option<RawValue>) -> &'a str {
    match raw {
        Some(r) if r.value == value => &r.raw,
        _ => value,
    }
}

#[derive(Debug, Clone, Default)]
pub struct Root {
    pub nodes: Vec<Node>,
    pub after: String,
    pub semicolon: bool,
    /// `@import` ids collected by the import transform.
    pub imports: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Node {
    Rule(Rule),
    AtRule(AtRule),
    Decl(Decl),
    Comment(Comment),
}
impl Node {
    pub fn before(&self) -> &str {
        match self {
            Node::Rule(r) => &r.before,
            Node::AtRule(a) => &a.before,
            Node::Decl(d) => &d.before,
            Node::Comment(c) => &c.before,
        }
    }
}

/// Container children plus `raws.after` / `raws.semicolon`.
#[derive(Debug, Clone, Default)]
pub struct Block {
    pub nodes: Vec<Node>,
    pub after: String,
    pub semicolon: bool,
}

/// `end_offset` is the byte index just past the closing `}` (PostCSS `source.end.offset`).
#[derive(Debug, Clone, Default)]
pub struct Rule {
    pub before: String,
    pub selector: String,
    pub raw_selector: Option<RawValue>,
    pub between: String,
    pub block: Block,
    /// `raws.ownSemicolon`: a free `;` (plus spaces) right after the rule.
    pub own_semicolon: String,
    pub loc: Loc,
    pub end_offset: usize,
    pub host: HostKind,
}

/// Mirrors `_isNativeHost` / `_isSyntheticHost` markers set by postcss-lwc-plugin.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum HostKind {
    #[default]
    None,
    Native,
    Synthetic,
}

#[derive(Debug, Clone, Default)]
pub struct AtRule {
    pub before: String,
    pub name: String,
    pub after_name: String,
    pub params: String,
    pub raw_params: Option<RawValue>,
    pub between: String,
    pub block: Option<Block>,
    pub loc: Loc,
}

/// `important` holds the source text of the flag (`raws.important || ' !important'`).
#[derive(Debug, Clone, Default)]
pub struct Decl {
    pub before: String,
    pub prop: String,
    pub between: String,
    pub value: String,
    pub raw_value: Option<RawValue>,
    pub important: Option<String>,
    pub loc: Loc,
}

/// `text` is everything between `/*` and `*/` (PostCSS `raws.left + text + raws.right`).
#[derive(Debug, Clone, Default)]
pub struct Comment {
    pub before: String,
    pub text: String,
}
