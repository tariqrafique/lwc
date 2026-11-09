//! Port of PostCSS 8.5 `lib/tokenize.js` over UTF-16 code units (offsets are JS string indices).
//! Token = `['space', text]`, `[ctl, ctl, start]` or `[type, text, start, end]` ([`Tok`]); `back()` / `returned` mirror the tokenizer's `back(token)`.
use super::{At, Fail};

/// Token type, for classification only; positions live in [`Tok`] / [`PosTok`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind { Space, Word, Str, AtWord, Brackets, Comment, OpenSquare, CloseSquare, OpenCurly, CloseCurly, Colon, Semicolon, OpenParen, CloseParen }

/// Control tokens `[char, char, pos]`: tokenize.js gives these a start and no end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ctl { OpenSquare, CloseSquare, OpenCurly, CloseCurly, Colon, Semicolon, OpenParen, CloseParen }

/// Spanning tokens `[type, text, pos, next]`: `brackets`, `string`, `at-word`, `word`, `comment`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Span { Word, Str, AtWord, Brackets, Comment }

/// A PostCSS token. `['space', text]` carries no position; every other token has a start.
#[derive(Clone, Debug)]
pub(super) enum Tok { Space(String), Pos(PosTok) }

/// A token with a position: `start` always, `end` only where tokenize.js sets `token[3]`.
#[derive(Clone, Debug)]
pub(super) enum PosTok {
    Ctl { ctl: Ctl, start: usize },
    Span { span: Span, text: String, start: usize, end: usize },
}

impl Ctl {
    fn kind(self) -> Kind {
        match self {
            Ctl::OpenSquare => Kind::OpenSquare, Ctl::CloseSquare => Kind::CloseSquare,
            Ctl::OpenCurly => Kind::OpenCurly, Ctl::CloseCurly => Kind::CloseCurly,
            Ctl::Colon => Kind::Colon, Ctl::Semicolon => Kind::Semicolon,
            Ctl::OpenParen => Kind::OpenParen, Ctl::CloseParen => Kind::CloseParen,
        }
    }
    fn text(self) -> &'static str {
        match self {
            Ctl::OpenSquare => "[", Ctl::CloseSquare => "]", Ctl::OpenCurly => "{", Ctl::CloseCurly => "}",
            Ctl::Colon => ":", Ctl::Semicolon => ";", Ctl::OpenParen => "(", Ctl::CloseParen => ")",
        }
    }
}

impl Span {
    fn kind(self) -> Kind {
        match self {
            Span::Word => Kind::Word, Span::Str => Kind::Str, Span::AtWord => Kind::AtWord,
            Span::Brackets => Kind::Brackets, Span::Comment => Kind::Comment,
        }
    }
}

impl PosTok {
    pub fn start(&self) -> usize { match self { PosTok::Ctl { start, .. } | PosTok::Span { start, .. } => *start } }
    pub fn kind(&self) -> Kind { match self { PosTok::Ctl { ctl, .. } => ctl.kind(), PosTok::Span { span, .. } => span.kind() } }
    pub fn text(&self) -> &str { match self { PosTok::Ctl { ctl, .. } => ctl.text(), PosTok::Span { text, .. } => text } }
}

impl Tok {
    pub fn kind(&self) -> Kind { match self { Tok::Space(_) => Kind::Space, Tok::Pos(p) => p.kind() } }
    pub fn text(&self) -> &str { match self { Tok::Space(t) => t, Tok::Pos(p) => p.text() } }
    pub fn into_text(self) -> String {
        match self { Tok::Space(t) | Tok::Pos(PosTok::Span { text: t, .. }) => t, Tok::Pos(PosTok::Ctl { ctl, .. }) => ctl.text().into() }
    }
    /// JS `token[2]` for an error offset: `undefined` (`None`) on a space token.
    pub fn js_start(&self) -> Option<usize> { match self { Tok::Space(_) => None, Tok::Pos(p) => Some(p.start()) } }
}

pub(super) type R<T> = Result<T, Fail>;

fn is_space(c: u16) -> bool { matches!(c, 0x20 | 0x0A | 0x09 | 0x0D | 0x0C) }

pub(super) struct Tokenizer<'a> { pub css: &'a [u16], pos: usize, buffer: Vec<String>, returned: Vec<Tok> }

impl<'a> Tokenizer<'a> {
    pub fn new(css: &'a [u16]) -> Self { Tokenizer { css, pos: 0, buffer: Vec::new(), returned: Vec::new() } }
    fn at(&self, i: usize) -> Option<u16> { self.css.get(i).copied() }
    fn text(&self, a: usize, b_incl: usize) -> String { String::from_utf16_lossy(&self.css[a..(b_incl + 1).min(self.css.len())]) }
    fn index_of(&self, needle: &[u16], from: usize) -> Option<usize> {
        if from > self.css.len() { return None; }
        self.css[from..].windows(needle.len()).position(|w| w == needle).map(|i| i + from)
    }
    fn unclosed(&self, what: &str) -> Fail { Fail { reason: format!("Unclosed {what}"), at: At::Offset(Some(self.pos)) } }
    pub fn end_of_file(&self) -> bool { self.returned.is_empty() && self.pos >= self.css.len() }
    pub fn back(&mut self, t: Tok) { self.returned.push(t); }
    /// `RE.lastIndex = from; RE.test(css)` then `next = lastIndex ? lastIndex - 2 : css.length - 1`.
    fn scan_end(&self, from: usize, stop: impl Fn(&[u16], usize) -> bool) -> usize {
        (from..self.css.len()).find(|&k| stop(self.css, k)).map_or(self.css.len().wrapping_sub(1), |k| k - 1)
    }
    /// Backslash run before `i` has odd length.
    fn escaped_at(&self, i: usize) -> bool {
        let mut e = i; let mut escaped = false;
        while e > 0 && self.css[e - 1] == b'\\' as u16 { e -= 1; escaped = !escaped; }
        escaped
    }

    pub fn next_token(&mut self) -> R<Option<Tok>> {
        if let Some(t) = self.returned.pop() { return Ok(Some(t)); }
        if self.pos >= self.css.len() { return Ok(None); }
        let pos = self.pos;
        let code = self.css[pos];
        let tok: Tok;
        let ctl = |ctl: Ctl| Tok::Pos(PosTok::Ctl { ctl, start: pos });
        let span = |span: Span, text: String, end: usize| Tok::Pos(PosTok::Span { span, text, start: pos, end });
        match code {
            0x0A | 0x20 | 0x09 | 0x0D | 0x0C => {
                let mut next = pos + 1;
                while self.at(next).is_some_and(is_space) { next += 1; }
                tok = Tok::Space(self.text(pos, next - 1));
                self.pos = next - 1;
            }
            0x5B => tok = ctl(Ctl::OpenSquare),
            0x5D => tok = ctl(Ctl::CloseSquare),
            0x7B => tok = ctl(Ctl::OpenCurly),
            0x7D => tok = ctl(Ctl::CloseCurly),
            0x3A => tok = ctl(Ctl::Colon),
            0x3B => tok = ctl(Ctl::Semicolon),
            0x29 => tok = ctl(Ctl::CloseParen),
            0x28 => {
                let url = self.buffer.pop().as_deref() == Some("url");
                let n = self.at(pos + 1);
                let quote_or_space = n.is_some_and(|n| n == b'\'' as u16 || n == b'"' as u16 || is_space(n));
                if url && !quote_or_space {
                    let mut next = pos;
                    loop {
                        next = match self.index_of(&[0x29], next + 1) { Some(i) => i, None => return Err(self.unclosed("bracket")) };
                        if !self.escaped_at(next) { break; }
                    }
                    tok = span(Span::Brackets, self.text(pos, next), next);
                    self.pos = next;
                } else {
                    match self.index_of(&[0x29], pos + 1) {
                        Some(next) if !bad_bracket(&self.css[pos..=next]) => {
                            tok = span(Span::Brackets, self.text(pos, next), next);
                            self.pos = next;
                        }
                        _ => tok = ctl(Ctl::OpenParen),
                    }
                }
            }
            0x27 | 0x22 => {
                let mut next = pos;
                loop {
                    next = match self.index_of(&[code], next + 1) { Some(i) => i, None => return Err(self.unclosed("string")) };
                    if !self.escaped_at(next) { break; }
                }
                tok = span(Span::Str, self.text(pos, next), next);
                self.pos = next;
            }
            0x40 => {
                // RE_AT_END = /[\t\n\f\r "#'()/;[\\\]{}]/g
                let next = self.scan_end(pos + 1, |c, k| matches!(c[k], 0x09 | 0x0A | 0x0C | 0x0D | 0x20 | 0x22 | 0x23 | 0x27 | 0x28 | 0x29 | 0x2F | 0x3B | 0x5B | 0x5C | 0x5D | 0x7B | 0x7D));
                tok = span(Span::AtWord, self.text(pos, next), next);
                self.pos = next;
            }
            0x5C => {
                let mut next = pos;
                let mut escape = true;
                while self.at(next + 1) == Some(0x5C) { next += 1; escape = !escape; }
                let c = self.at(next + 1);
                if escape && !c.is_some_and(|c| c == b'/' as u16 || is_space(c)) {
                    next += 1;
                    let hex = |c: Option<u16>| c.is_some_and(|c| c < 128 && (c as u8).is_ascii_hexdigit());
                    if hex(self.at(next)) {
                        while hex(self.at(next + 1)) { next += 1; }
                        if self.at(next + 1) == Some(0x20) { next += 1; }
                    }
                }
                tok = span(Span::Word, self.text(pos, next), next);
                self.pos = next;
            }
            _ => {
                if code == b'/' as u16 && self.at(pos + 1) == Some(b'*' as u16) {
                    let next = match self.index_of(&[0x2A, 0x2F], pos + 2) { Some(i) => i + 1, None => return Err(self.unclosed("comment")) };
                    tok = span(Span::Comment, self.text(pos, next), next);
                    self.pos = next;
                } else {
                    // RE_WORD_END = /[\t\n\f\r !"#'():;@[\\\]{}]|\/(?=\*)/g
                    let next = self.scan_end(pos + 1, |c, k| {
                        matches!(c[k], 0x09 | 0x0A | 0x0C | 0x0D | 0x20 | 0x21 | 0x22 | 0x23 | 0x27 | 0x28 | 0x29 | 0x3A | 0x3B | 0x40 | 0x5B | 0x5C | 0x5D | 0x7B | 0x7D)
                            || (c[k] == 0x2F && c.get(k + 1) == Some(&0x2A))
                    });
                    let text = self.text(pos, next);
                    self.buffer.push(text.clone());
                    tok = span(Span::Word, text, next);
                    self.pos = next;
                }
            }
        }
        self.pos += 1;
        Ok(Some(tok))
    }
}

/// `RE_BAD_BRACKET = /.[\r\n"'(/\\]/`: some non-line-terminator followed by one of `\r\n"'(/\`.
fn bad_bracket(s: &[u16]) -> bool {
    s.windows(2).any(|w| !matches!(w[0], 0x0A | 0x0D | 0x2028 | 0x2029) && matches!(w[1], 0x0D | 0x0A | 0x22 | 0x27 | 0x28 | 0x2F | 0x5C))
}
