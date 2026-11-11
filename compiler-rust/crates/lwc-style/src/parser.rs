//! The one CSS parser: a port of PostCSS 8.5 `lib/parser.js` over the `lib/tokenize.js` port in
//! [`tokenize`]. A single pass builds the lossless tree (`ast`, with PostCSS `raws`) and fails with
//! exactly the `CssSyntaxError` reason and position `postcss.parse` throws.
//!
//! Offsets are UTF-16 code units, as in JS. `Input` drops a leading U+FEFF / U+FFFE before
//! tokenizing and `fromOffset` splits lines on `\n` only (a `\r\n` pair is one line break; a lone
//! `\r` is not), column = offset in line + 1. Node `Loc.offset` is the byte index into the
//! caller's `src` (BOM included) for slicing.
//!
//! Function map (postcss 8.5.9, node_modules/postcss/lib/parser.js in the pinned oracle):
//! `parse`, `other`, `rule`, `emptyRule`, `decl`, `atrule`, `comment`, `end`, `endFile`,
//! `freeSemicolon`, `init`, `raw`, `spacesAndCommentsFrom{Start,End}`, `spacesFromEnd`,
//! `stringFrom`, `checkMissedSemicolon`, `colon`, and the error helpers `unclosedBlock`,
//! `unclosedBracket`, `unexpectedClose`, `unknownWord`, `unnamedAtrule`, `doubleColon`.
mod tokenize;

use crate::ast::*;
use crate::error::StyleError;
use lwc_shared::js::js_ws;
use std::collections::VecDeque;
use tokenize::{Ctl, Kind, PosTok, R, Span, Tok, Tokenizer};

/// `input.error(reason, …)` before the position is resolved: an offset (JS `undefined` = `None`)
/// or, for `unclosedBlock`, the node's start line/column.
struct Fail {
    reason: String,
    at: At,
}
enum At {
    Offset(Option<usize>),
    LineCol(u32, u32),
}

fn fail(reason: impl Into<String>, at: Option<usize>) -> Fail {
    Fail {
        reason: reason.into(),
        at: At::Offset(at),
    }
}
fn unknown_word(t: &Tok) -> Fail {
    fail(format!("Unknown word {}", t.text()), t.js_start())
}

/// `postcss.parse(src)`: the lossless tree, or PostCSS's `CssSyntaxError` reason and position.
pub fn parse(src: &str) -> Result<Root, StyleError> {
    let input = Input::new(src);
    let mut p = Parser {
        input: &input,
        tz: Tokenizer::new(&input.css),
        stack: vec![Frame {
            node: Open::Root,
            block: Block::default(),
            start: 0,
        }],
        spaces: String::new(),
        semicolon: false,
    };
    p.parse().map_err(|Fail { reason, at }| {
        let pos = match at {
            At::LineCol(l, c) => Some((l, c)),
            At::Offset(o) => o.map(|o| input.position(o)),
        };
        StyleError::syntax_at(reason, pos)
    })
}

/// PostCSS `Input`: the UTF-16 css, `fromOffset`, plus a UTF-16 -> byte offset map into `src`.
struct Input {
    css: Vec<u16>,
    line_starts: Vec<usize>,
    /// `bytes[i]` = byte index in `src` of UTF-16 unit `i`; `bytes[css.len()]` = `src.len()`.
    bytes: Vec<usize>,
}

impl Input {
    fn new(src: &str) -> Self {
        let body = src.strip_prefix(['\u{FEFF}', '\u{FFFE}']).unwrap_or(src);
        let bom = src.len() - body.len();
        let mut css = Vec::with_capacity(body.len());
        let mut bytes = Vec::with_capacity(body.len() + 1);
        let mut line_starts = vec![0];
        for (i, ch) in body.char_indices() {
            let mut buf = [0u16; 2];
            for &u in ch.encode_utf16(&mut buf).iter() {
                css.push(u);
                bytes.push(bom + i);
            }
            if ch == '\n' {
                line_starts.push(css.len());
            }
        }
        bytes.push(src.len());
        Input {
            css,
            line_starts,
            bytes,
        }
    }
    /// `Input#fromOffset`: 1-based line and UTF-16 column.
    fn position(&self, offset: usize) -> (u32, u32) {
        let line = self.line_starts.partition_point(|&s| s <= offset);
        (
            line as u32,
            (offset - self.line_starts[line - 1] + 1) as u32,
        )
    }
    fn loc(&self, offset: usize) -> Loc {
        let (line, column) = self.position(offset);
        Loc {
            line,
            column,
            offset: self.bytes[offset.min(self.css.len())],
        }
    }
}

/// `this.current` and its ancestors: an open container and the offset it was `init`ed at.
struct Frame {
    node: Open,
    block: Block,
    start: usize,
}
enum Open {
    Root,
    Rule(Rule),
    AtRule(AtRule),
}

struct Parser<'a> {
    input: &'a Input,
    tz: Tokenizer<'a>,
    stack: Vec<Frame>,
    spaces: String,
    semicolon: bool,
}

impl Parser<'_> {
    fn parse(&mut self) -> R<Root> {
        while !self.tz.end_of_file() {
            let Some(token) = self.tz.next_token()? else {
                break;
            };
            let token = match token {
                Tok::Space(text) => {
                    self.spaces += &text;
                    continue;
                }
                Tok::Pos(p) => p,
            };
            match token {
                PosTok::Ctl {
                    ctl: Ctl::Semicolon,
                    ..
                } => self.free_semicolon(),
                PosTok::Ctl {
                    ctl: Ctl::CloseCurly,
                    start,
                } => self.end(start)?,
                PosTok::Span {
                    span: Span::Comment,
                    text,
                    ..
                } => self.comment(&text),
                PosTok::Span {
                    span: Span::AtWord,
                    text,
                    start,
                    ..
                } => self.atrule(&text, start)?,
                PosTok::Ctl {
                    ctl: Ctl::OpenCurly,
                    start,
                } => self.empty_rule(start),
                _ => self.other(token)?,
            }
        }
        self.end_file()
    }

    fn current(&mut self) -> &mut Block {
        &mut self.stack.last_mut().expect("root frame").block
    }
    fn push(&mut self, node: Node) {
        self.current().nodes.push(node)
    }
    fn open(&mut self, node: Open, start: usize) {
        self.stack.push(Frame {
            node,
            block: Block::default(),
            start,
        })
    }

    /// `init(node)`: takes `raws.before` from the pending spaces; non-comments reset `semicolon`.
    fn init(&mut self, comment: bool) -> String {
        if !comment {
            self.semicolon = false;
        }
        std::mem::take(&mut self.spaces)
    }

    fn comment(&mut self, token: &str) {
        let before = self.init(true);
        let text = token[2..token.len() - 2].to_string();
        self.push(Node::Comment(Comment { before, text }));
    }

    fn empty_rule(&mut self, start: usize) {
        let before = self.init(false);
        let loc = self.input.loc(start);
        self.open(
            Open::Rule(Rule {
                before,
                loc,
                ..Rule::default()
            }),
            start,
        );
    }

    fn free_semicolon(&mut self) {
        self.spaces += ";";
        let block = &mut self.stack.last_mut().expect("root frame").block;
        if let Some(Node::Rule(prev)) = block.nodes.last_mut() {
            if prev.own_semicolon.is_empty() {
                prev.own_semicolon = std::mem::take(&mut self.spaces);
            }
        }
    }

    /// `end(token)` for the `}` at `start`.
    fn end(&mut self, start: usize) -> R<()> {
        let semicolon = std::mem::replace(&mut self.semicolon, false);
        let spaces = std::mem::take(&mut self.spaces);
        let block = self.current();
        if !block.nodes.is_empty() {
            block.semicolon = semicolon;
        }
        block.after += &spaces;
        if self.stack.len() == 1 {
            return Err(fail("Unexpected }", Some(start)));
        }
        let Frame { node, block, .. } = self.stack.pop().expect("open frame");
        let node = match node {
            Open::Rule(mut r) => {
                r.block = block;
                r.end_offset = self.input.bytes[start + 1];
                Node::Rule(r)
            }
            Open::AtRule(mut a) => {
                a.block = Some(block);
                Node::AtRule(a)
            }
            Open::Root => unreachable!("root is never popped"),
        };
        self.push(node);
        Ok(())
    }

    fn end_file(&mut self) -> R<Root> {
        if self.stack.len() > 1 {
            let (l, c) = self
                .input
                .position(self.stack.last().expect("open frame").start);
            return Err(Fail {
                reason: "Unclosed block".into(),
                at: At::LineCol(l, c),
            });
        }
        let semicolon = self.semicolon;
        let spaces = std::mem::take(&mut self.spaces);
        let mut block = std::mem::take(self.current());
        if !block.nodes.is_empty() {
            block.semicolon = semicolon;
        }
        block.after += &spaces;
        Ok(Root {
            nodes: block.nodes,
            after: block.after,
            semicolon: block.semicolon,
            imports: Vec::new(),
        })
    }

    fn atrule(&mut self, token: &str, start: usize) -> R<()> {
        let name = token[1..].to_string();
        if name.is_empty() {
            return Err(fail("At-rule without name", Some(start)));
        }
        let before = self.init(false);
        let loc = self.input.loc(start);

        let (mut last, mut open, mut close) = (false, false, None);
        let mut params: Vec<Tok> = Vec::new();
        let mut brackets: Vec<Kind> = Vec::new();
        while !self.tz.end_of_file() {
            let Some(t) = self.tz.next_token()? else {
                break;
            };
            match t.kind() {
                Kind::OpenParen => brackets.push(Kind::CloseParen),
                Kind::OpenSquare => brackets.push(Kind::CloseSquare),
                Kind::OpenCurly if !brackets.is_empty() => brackets.push(Kind::CloseCurly),
                k if Some(&k) == brackets.last() => {
                    brackets.pop();
                }
                _ => {}
            }
            if brackets.is_empty() {
                match t {
                    Tok::Pos(PosTok::Ctl {
                        ctl: Ctl::Semicolon,
                        ..
                    }) => {
                        self.semicolon = true;
                        break;
                    }
                    Tok::Pos(PosTok::Ctl {
                        ctl: Ctl::OpenCurly,
                        ..
                    }) => {
                        open = true;
                        break;
                    }
                    Tok::Pos(PosTok::Ctl {
                        ctl: Ctl::CloseCurly,
                        start,
                    }) => {
                        close = Some(start);
                        break;
                    }
                    _ => params.push(t),
                }
            } else {
                params.push(t);
            }
            if self.tz.end_of_file() {
                last = true;
                break;
            }
        }

        let mut node = AtRule {
            before,
            name,
            loc,
            ..AtRule::default()
        };
        node.between = spaces_and_comments_from_end(&mut params);
        if !params.is_empty() {
            node.after_name = spaces_and_comments_from_start(&mut params);
            (node.params, node.raw_params) = raw(&params, false);
            if last {
                self.spaces = std::mem::take(&mut node.between);
            }
        }
        if open {
            self.open(Open::AtRule(node), start);
        } else {
            // parser.js calls `end(token)` before filling raws; nothing it reads depends on them.
            self.push(Node::AtRule(node));
            if let Some(at) = close {
                self.end(at)?;
            }
        }
        Ok(())
    }

    fn other(&mut self, start: PosTok) -> R<()> {
        let mut end = false;
        let mut colon = false;
        let mut bracket: Option<usize> = None;
        let mut brackets: Vec<Kind> = Vec::new();
        let custom_property = start.text().starts_with("--");
        let first = start.start();
        let mut tokens: Vec<Tok> = Vec::new();
        let mut token = Some(Tok::Pos(start));
        while let Some(t) = token {
            let kind = t.kind();
            let at = t.js_start();
            tokens.push(t);
            if kind == Kind::OpenParen || kind == Kind::OpenSquare {
                if bracket.is_none() {
                    bracket = at;
                }
                brackets.push(if kind == Kind::OpenParen {
                    Kind::CloseParen
                } else {
                    Kind::CloseSquare
                });
            } else if custom_property && colon && kind == Kind::OpenCurly {
                if bracket.is_none() {
                    bracket = at;
                }
                brackets.push(Kind::CloseCurly);
            } else if brackets.is_empty() {
                match kind {
                    Kind::Semicolon => {
                        if colon {
                            return self.decl(tokens, custom_property);
                        }
                        break;
                    }
                    Kind::OpenCurly => return self.rule(first, tokens),
                    Kind::CloseCurly => {
                        let t = tokens.pop().expect("just pushed");
                        self.tz.back(t);
                        end = true;
                        break;
                    }
                    Kind::Colon => colon = true,
                    _ => {}
                }
            } else if Some(&kind) == brackets.last() {
                brackets.pop();
                if brackets.is_empty() {
                    bracket = None;
                }
            }
            token = self.tz.next_token()?;
        }
        if self.tz.end_of_file() {
            end = true;
        }
        if let Some(b) = bracket.filter(|_| !brackets.is_empty()) {
            return Err(fail("Unclosed bracket", Some(b)));
        }
        if end && colon {
            if !custom_property {
                while tokens
                    .last()
                    .is_some_and(|t| matches!(t.kind(), Kind::Space | Kind::Comment))
                {
                    let t = tokens.pop().expect("non-empty");
                    self.tz.back(t);
                }
            }
            self.decl(tokens, custom_property)
        } else {
            Err(unknown_word(&tokens[0]))
        }
    }

    /// `rule(tokens)`; `start` is `tokens[0][2]`, the positioned token `other` began with.
    fn rule(&mut self, start: usize, mut tokens: Vec<Tok>) -> R<()> {
        tokens.pop();
        let before = self.init(false);
        let loc = self.input.loc(start);
        let between = spaces_and_comments_from_end(&mut tokens);
        let (selector, raw_selector) = raw(&tokens, false);
        self.open(
            Open::Rule(Rule {
                before,
                selector,
                raw_selector,
                between,
                loc,
                ..Rule::default()
            }),
            start,
        );
        Ok(())
    }

    fn decl(&mut self, mut tokens: Vec<Tok>, custom_property: bool) -> R<()> {
        let mut before = self.init(false);
        if tokens.last().is_some_and(|t| t.kind() == Kind::Semicolon) {
            self.semicolon = true;
            tokens.pop();
        }
        let mut tokens: VecDeque<Tok> = tokens.into();
        let start = loop {
            if let Tok::Pos(
                p @ PosTok::Span {
                    span: Span::Word, ..
                },
            ) = &tokens[0]
            {
                break p.start();
            }
            if tokens.len() == 1 {
                return Err(unknown_word(&tokens[0]));
            }
            before += tokens.pop_front().expect("len > 1").text();
        };
        let loc = self.input.loc(start);

        let mut prop = String::new();
        while let Some(t) = tokens.front() {
            if matches!(t.kind(), Kind::Colon | Kind::Space | Kind::Comment) {
                break;
            }
            prop += tokens.pop_front().expect("front").text();
        }

        let mut between = String::new();
        while let Some(t) = tokens.pop_front() {
            if t.kind() == Kind::Colon {
                between += &t.text();
                break;
            }
            // `/\w/.test(token[1])`
            if t.kind() == Kind::Word
                && t.text()
                    .bytes()
                    .any(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(unknown_word(&t));
            }
            between += &t.text();
        }

        if prop.starts_with(['_', '*']) {
            before.push(prop.remove(0));
        }

        let mut first_spaces: Vec<Tok> = Vec::new();
        while tokens
            .front()
            .is_some_and(|t| matches!(t.kind(), Kind::Space | Kind::Comment))
        {
            first_spaces.push(tokens.pop_front().expect("front"));
        }
        let mut tokens: Vec<Tok> = tokens.into();

        let mut important = None;
        let mut i = tokens.len();
        while i > 0 {
            i -= 1;
            let token = tokens[i].clone();
            let lower = token.text().to_lowercase();
            if lower == "!important" {
                let string = string_from(&mut tokens, i);
                important = Some(spaces_from_end(&mut tokens) + &string);
                break;
            } else if lower == "important" {
                let mut cache = tokens.clone();
                let mut s = String::new();
                let mut j = i;
                while j > 0 {
                    let kind = cache[j].kind();
                    if s.trim_matches(js_ws).starts_with('!') && kind != Kind::Space {
                        break;
                    }
                    s = cache.pop().expect("j < len").into_text() + &s;
                    j -= 1;
                }
                if s.trim_matches(js_ws).starts_with('!') {
                    important = Some(s);
                    tokens = cache;
                }
            }
            if token.kind() != Kind::Space && token.kind() != Kind::Comment {
                break;
            }
        }

        if tokens
            .iter()
            .any(|t| !matches!(t.kind(), Kind::Space | Kind::Comment))
        {
            for t in first_spaces.drain(..) {
                between += &t.text();
            }
        }
        let mut value_tokens = first_spaces;
        value_tokens.extend(tokens.iter().cloned());
        let (value, raw_value) = raw(&value_tokens, custom_property);

        if value.contains(':') && !custom_property {
            check_missed_semicolon(&tokens)?;
        }
        self.push(Node::Decl(Decl {
            before,
            prop,
            between,
            value,
            raw_value,
            important,
            loc,
        }));
        Ok(())
    }
}

/// `raw(node, prop, tokens, customProperty)`: the clean value (comments next to spaces / edges
/// and a trailing space dropped) and, when that differs from the source, `{ value, raw }`.
fn raw(tokens: &[Tok], custom_property: bool) -> (String, Option<RawValue>) {
    let safe = |k: Option<Kind>| matches!(k, None | Some(Kind::Space));
    let mut value = String::new();
    let mut clean = true;
    for (i, t) in tokens.iter().enumerate() {
        if t.kind() == Kind::Space && i == tokens.len() - 1 && !custom_property {
            clean = false;
        } else if t.kind() == Kind::Comment {
            let prev = i.checked_sub(1).map(|p| tokens[p].kind());
            let next = tokens.get(i + 1).map(|n| n.kind());
            if !safe(prev) && !safe(next) {
                if value.ends_with(',') {
                    clean = false;
                } else {
                    value += &t.text();
                }
            } else {
                clean = false;
            }
        } else {
            value += &t.text();
        }
    }
    if clean {
        return (value, None);
    }
    let raw = tokens.iter().map(Tok::text).collect();
    (value.clone(), Some(RawValue { value, raw }))
}

fn spaces_and_comments_from_end(tokens: &mut Vec<Tok>) -> String {
    let mut spaces = String::new();
    while tokens
        .last()
        .is_some_and(|t| matches!(t.kind(), Kind::Space | Kind::Comment))
    {
        spaces = tokens.pop().expect("non-empty").into_text() + &spaces;
    }
    spaces
}

fn spaces_and_comments_from_start(tokens: &mut Vec<Tok>) -> String {
    let n = tokens
        .iter()
        .take_while(|t| matches!(t.kind(), Kind::Space | Kind::Comment))
        .count();
    tokens.drain(..n).map(Tok::into_text).collect()
}

fn spaces_from_end(tokens: &mut Vec<Tok>) -> String {
    let mut spaces = String::new();
    while tokens.last().is_some_and(|t| t.kind() == Kind::Space) {
        spaces = tokens.pop().expect("non-empty").into_text() + &spaces;
    }
    spaces
}

fn string_from(tokens: &mut Vec<Tok>, from: usize) -> String {
    tokens.drain(from..).map(Tok::into_text).collect()
}

fn check_missed_semicolon(tokens: &[Tok]) -> R<()> {
    let Some(colon) = colon(tokens)? else {
        return Ok(());
    };
    let mut founded = 0;
    let mut token = &tokens[0];
    for j in (0..colon).rev() {
        token = &tokens[j];
        if token.kind() != Kind::Space {
            founded += 1;
            if founded == 2 {
                break;
            }
        }
    }
    let at = match token {
        Tok::Pos(PosTok::Span {
            span: Span::Word,
            end,
            ..
        }) => Some(end + 1),
        _ => token.js_start(),
    };
    Err(fail("Missed semicolon", at))
}

fn colon(tokens: &[Tok]) -> R<Option<usize>> {
    let mut brackets = 0i32;
    let mut prev: Option<&Tok> = None;
    for (i, token) in tokens.iter().enumerate() {
        if token.kind() == Kind::OpenParen {
            brackets += 1;
        }
        if token.kind() == Kind::CloseParen {
            brackets -= 1;
        }
        if brackets == 0 && token.kind() == Kind::Colon {
            match prev {
                None => return Err(fail("Double colon", token.js_start())),
                Some(p) if p.kind() == Kind::Word && p.text() == "progid" => continue,
                Some(_) => return Ok(Some(i)),
            }
        }
        prev = Some(token);
    }
    Ok(None)
}
