//! Lossless PostCSS-like tree. `raws` keep original whitespace so stringify is verbatim.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Loc { pub line: u32, pub column: u32, pub offset: usize }

#[derive(Debug, Clone, Default)]
pub struct Root { pub nodes: Vec<Node>, pub after: String, /// `@import` ids collected by the import transform.
    pub imports: Vec<String> }

#[derive(Debug, Clone)]
pub enum Node {
    Rule(Rule),
    AtRule(AtRule),
    Decl(Decl),
    Comment(Comment),
}
impl Node {
    pub fn before(&self) -> &str {
        match self { Node::Rule(r) => &r.before, Node::AtRule(a) => &a.before, Node::Decl(d) => &d.before, Node::Comment(c) => &c.before }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Block { pub nodes: Vec<Node>, pub after: String, pub semicolon: bool }

#[derive(Debug, Clone, Default)]
pub struct Rule { pub before: String, pub selector: String, pub between: String, pub block: Block, pub loc: Loc, pub end_offset: usize, pub host: HostKind }

/// Mirrors `_isNativeHost` / `_isSyntheticHost` markers set by postcss-lwc-plugin.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum HostKind { #[default] None, Native, Synthetic }

#[derive(Debug, Clone, Default)]
pub struct AtRule { pub before: String, pub name: String, pub after_name: String, pub params: String, pub between: String, pub block: Option<Block>, pub loc: Loc }

#[derive(Debug, Clone, Default)]
pub struct Decl { pub before: String, pub prop: String, pub between: String, pub value: String, pub loc: Loc }

#[derive(Debug, Clone, Default)]
pub struct Comment { pub before: String, pub text: String }
