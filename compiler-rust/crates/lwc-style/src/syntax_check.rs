//! PostCSS 8.5 `tokenize.js` + `parser.js` control flow, ported to decide one thing: does
//! `postcss.parse` throw a CssSyntaxError for this input, and with which reason and position?
//!
//! The tree is still built by the cssparser-based [`crate::parser`], which auto-closes blocks,
//! strings, comments and brackets at EOF (CSS Syntax §5) where PostCSS throws. This pass runs first
//! and builds no nodes: it keeps only the state PostCSS errors depend on (the open-block start
//! positions, the url `(` word buffer, the token stream). Offsets are UTF-16 code units, as in JS;
//! positions are `Input#fromOffset` (lines split on `\n` only, column = offset in line + 1).
//!
//! Error sites covered (postcss 8.5.9 in /workspace/lwc/node_modules/postcss/lib):
//! tokenize.js `unclosed('bracket' | 'string' | 'comment')`; parser.js `unclosedBlock`,
//! `unclosedBracket`, `unexpectedClose`, `unknownWord`, `unnamedAtrule`, `doubleColon`,
//! `checkMissedSemicolon`.
use crate::error::StyleError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind { Space, Word, Str, AtWord, Brackets, Comment, OpenSquare, CloseSquare, OpenCurly, CloseCurly, Colon, Semicolon, OpenParen, CloseParen }

/// A PostCSS token `[type, text, start?, end?]`. `start` is `None` only for `space` tokens.
#[derive(Clone, Debug)]
struct Tok { kind: Kind, text: String, start: Option<usize>, end: Option<usize> }

/// Error with a PostCSS offset (or none, JS `undefined`).
struct Fail { reason: String, at: At }
enum At { Offset(Option<usize>), LineCol(u32, u32) }
type R<T> = Result<T, Fail>;

fn is_space(c: u16) -> bool { matches!(c, 0x20 | 0x0A | 0x09 | 0x0D | 0x0C) }

struct Tokenizer<'a> { css: &'a [u16], pos: usize, buffer: Vec<String>, returned: Vec<Tok> }

impl<'a> Tokenizer<'a> {
    fn at(&self, i: usize) -> Option<u16> { self.css.get(i).copied() }
    fn text(&self, a: usize, b_incl: usize) -> String { String::from_utf16_lossy(&self.css[a..(b_incl + 1).min(self.css.len())]) }
    fn index_of(&self, needle: &[u16], from: usize) -> Option<usize> {
        if from > self.css.len() { return None; }
        self.css[from..].windows(needle.len()).position(|w| w == needle).map(|i| i + from)
    }
    fn unclosed(&self, what: &str) -> Fail { Fail { reason: format!("Unclosed {what}"), at: At::Offset(Some(self.pos)) } }
    fn end_of_file(&self) -> bool { self.returned.is_empty() && self.pos >= self.css.len() }
    fn back(&mut self, t: Tok) { self.returned.push(t); }
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

    fn next_token(&mut self) -> R<Option<Tok>> {
        if let Some(t) = self.returned.pop() { return Ok(Some(t)); }
        if self.pos >= self.css.len() { return Ok(None); }
        let pos = self.pos;
        let code = self.css[pos];
        let tok: Tok;
        let ctl = |k: Kind, c: u16| Tok { kind: k, text: String::from_utf16_lossy(&[c]), start: Some(pos), end: None };
        match code {
            0x0A | 0x20 | 0x09 | 0x0D | 0x0C => {
                let mut next = pos + 1;
                while self.at(next).is_some_and(is_space) { next += 1; }
                tok = Tok { kind: Kind::Space, text: self.text(pos, next - 1), start: None, end: None };
                self.pos = next - 1;
            }
            0x5B => tok = ctl(Kind::OpenSquare, code),
            0x5D => tok = ctl(Kind::CloseSquare, code),
            0x7B => tok = ctl(Kind::OpenCurly, code),
            0x7D => tok = ctl(Kind::CloseCurly, code),
            0x3A => tok = ctl(Kind::Colon, code),
            0x3B => tok = ctl(Kind::Semicolon, code),
            0x29 => tok = ctl(Kind::CloseParen, code),
            0x28 => {
                let prev = self.buffer.pop().unwrap_or_default();
                let n = self.at(pos + 1);
                let quote_or_space = n.is_some_and(|n| n == b'\'' as u16 || n == b'"' as u16 || is_space(n));
                if prev == "url" && !quote_or_space {
                    let mut next = pos;
                    loop {
                        next = match self.index_of(&[0x29], next + 1) { Some(i) => i, None => return Err(self.unclosed("bracket")) };
                        if !self.escaped_at(next) { break; }
                    }
                    tok = Tok { kind: Kind::Brackets, text: self.text(pos, next), start: Some(pos), end: Some(next) };
                    self.pos = next;
                } else {
                    match self.index_of(&[0x29], pos + 1) {
                        Some(next) if !bad_bracket(&self.css[pos..=next]) => {
                            tok = Tok { kind: Kind::Brackets, text: self.text(pos, next), start: Some(pos), end: Some(next) };
                            self.pos = next;
                        }
                        _ => tok = ctl(Kind::OpenParen, code),
                    }
                }
            }
            0x27 | 0x22 => {
                let mut next = pos;
                loop {
                    next = match self.index_of(&[code], next + 1) { Some(i) => i, None => return Err(self.unclosed("string")) };
                    if !self.escaped_at(next) { break; }
                }
                tok = Tok { kind: Kind::Str, text: self.text(pos, next), start: Some(pos), end: Some(next) };
                self.pos = next;
            }
            0x40 => {
                // RE_AT_END = /[\t\n\f\r "#'()/;[\\\]{}]/g
                let next = self.scan_end(pos + 1, |c, k| matches!(c[k], 0x09 | 0x0A | 0x0C | 0x0D | 0x20 | 0x22 | 0x23 | 0x27 | 0x28 | 0x29 | 0x2F | 0x3B | 0x5B | 0x5C | 0x5D | 0x7B | 0x7D));
                tok = Tok { kind: Kind::AtWord, text: self.text(pos, next), start: Some(pos), end: Some(next) };
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
                tok = Tok { kind: Kind::Word, text: self.text(pos, next), start: Some(pos), end: Some(next) };
                self.pos = next;
            }
            _ => {
                if code == b'/' as u16 && self.at(pos + 1) == Some(b'*' as u16) {
                    let next = match self.index_of(&[0x2A, 0x2F], pos + 2) { Some(i) => i + 1, None => return Err(self.unclosed("comment")) };
                    tok = Tok { kind: Kind::Comment, text: self.text(pos, next), start: Some(pos), end: Some(next) };
                    self.pos = next;
                } else {
                    // RE_WORD_END = /[\t\n\f\r !"#'():;@[\\\]{}]|\/(?=\*)/g
                    let next = self.scan_end(pos + 1, |c, k| {
                        matches!(c[k], 0x09 | 0x0A | 0x0C | 0x0D | 0x20 | 0x21 | 0x22 | 0x23 | 0x27 | 0x28 | 0x29 | 0x3A | 0x3B | 0x40 | 0x5B | 0x5C | 0x5D | 0x7B | 0x7D)
                            || (c[k] == 0x2F && c.get(k + 1) == Some(&0x2A))
                    });
                    tok = Tok { kind: Kind::Word, text: self.text(pos, next), start: Some(pos), end: Some(next) };
                    self.buffer.push(tok.text.clone());
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

struct Parser<'a> {
    tz: Tokenizer<'a>,
    /// `this.current` chain below root: start offset of each open rule / at-rule / empty rule.
    open: Vec<usize>,
}

fn unknown_word(t: &Tok) -> Fail { Fail { reason: format!("Unknown word {}", t.text), at: At::Offset(t.start) } }

impl<'a> Parser<'a> {
    fn parse(&mut self) -> R<()> {
        while !self.tz.end_of_file() {
            let Some(token) = self.tz.next_token()? else { break };
            match token.kind {
                Kind::Space | Kind::Semicolon | Kind::Comment => {}
                Kind::CloseCurly => self.end(&token)?,
                Kind::AtWord => self.atrule(token)?,
                Kind::OpenCurly => self.open.push(token.start.unwrap()),
                _ => self.other(token)?,
            }
        }
        // endFile -> unclosedBlock: `input.error('Unclosed block', start.line, start.column)`.
        if let Some(&start) = self.open.last() {
            let (l, c) = from_offset(self.tz.css, start);
            return Err(Fail { reason: "Unclosed block".into(), at: At::LineCol(l, c) });
        }
        Ok(())
    }

    fn end(&mut self, token: &Tok) -> R<()> {
        if self.open.pop().is_none() {
            return Err(Fail { reason: "Unexpected }".into(), at: At::Offset(token.start) });
        }
        Ok(())
    }

    fn atrule(&mut self, token: Tok) -> R<()> {
        if token.text.len() == 1 {
            return Err(Fail { reason: "At-rule without name".into(), at: At::Offset(token.start) });
        }
        let start = token.start.unwrap();
        let mut brackets: Vec<Kind> = Vec::new();
        while !self.tz.end_of_file() {
            let Some(t) = self.tz.next_token()? else { break };
            match t.kind {
                Kind::OpenParen => brackets.push(Kind::CloseParen),
                Kind::OpenSquare => brackets.push(Kind::CloseSquare),
                Kind::OpenCurly if !brackets.is_empty() => brackets.push(Kind::CloseCurly),
                k if Some(&k) == brackets.last() => { brackets.pop(); }
                _ => {}
            }
            if brackets.is_empty() {
                match t.kind {
                    Kind::Semicolon => break,
                    Kind::OpenCurly => { self.open.push(start); break; }
                    Kind::CloseCurly => { self.end(&t)?; break; }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn other(&mut self, start: Tok) -> R<()> {
        let mut end = false;
        let mut colon = false;
        let mut bracket: Option<Tok> = None;
        let mut brackets: Vec<Kind> = Vec::new();
        let custom_property = start.text.starts_with("--");
        let mut tokens: Vec<Tok> = Vec::new();
        let mut token = Some(start);
        while let Some(t) = token {
            let kind = t.kind;
            tokens.push(t.clone());
            if kind == Kind::OpenParen || kind == Kind::OpenSquare {
                if bracket.is_none() { bracket = Some(t); }
                brackets.push(if kind == Kind::OpenParen { Kind::CloseParen } else { Kind::CloseSquare });
            } else if custom_property && colon && kind == Kind::OpenCurly {
                if bracket.is_none() { bracket = Some(t); }
                brackets.push(Kind::CloseCurly);
            } else if brackets.is_empty() {
                match kind {
                    Kind::Semicolon => {
                        if colon { return self.decl(tokens, custom_property); }
                        break;
                    }
                    Kind::OpenCurly => {
                        // rule(tokens): init(node, tokens[0][2]); this.current = node.
                        self.open.push(tokens[0].start.unwrap());
                        return Ok(());
                    }
                    Kind::CloseCurly => {
                        let t = tokens.pop().unwrap();
                        self.tz.back(t);
                        end = true;
                        break;
                    }
                    Kind::Colon => colon = true,
                    _ => {}
                }
            } else if Some(&kind) == brackets.last() {
                brackets.pop();
                if brackets.is_empty() { bracket = None; }
            }
            token = self.tz.next_token()?;
        }
        if self.tz.end_of_file() { end = true; }
        if let Some(b) = bracket.filter(|_| !brackets.is_empty()) {
            return Err(Fail { reason: "Unclosed bracket".into(), at: At::Offset(b.start) });
        }
        if end && colon {
            if !custom_property {
                while let Some(last) = tokens.last() {
                    if last.kind != Kind::Space && last.kind != Kind::Comment { break; }
                    let t = tokens.pop().unwrap();
                    self.tz.back(t);
                }
            }
            self.decl(tokens, custom_property)
        } else {
            Err(unknown_word(&tokens[0]))
        }
    }

    fn decl(&mut self, mut tokens: Vec<Tok>, custom_property: bool) -> R<()> {
        if tokens.last().is_some_and(|t| t.kind == Kind::Semicolon) { tokens.pop(); }
        let mut tokens: std::collections::VecDeque<Tok> = tokens.into();
        while tokens[0].kind != Kind::Word {
            if tokens.len() == 1 { return Err(unknown_word(&tokens[0])); }
            tokens.pop_front();
        }
        // prop
        while let Some(t) = tokens.front() {
            if matches!(t.kind, Kind::Colon | Kind::Space | Kind::Comment) { break; }
            tokens.pop_front();
        }
        // between
        while let Some(t) = tokens.pop_front() {
            if t.kind == Kind::Colon { break; }
            if t.kind == Kind::Word && t.text.bytes().any(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err(unknown_word(&t));
            }
        }
        while tokens.front().is_some_and(|t| matches!(t.kind, Kind::Space | Kind::Comment)) { tokens.pop_front(); }
        let mut tokens: Vec<Tok> = tokens.into();

        // !important detection (mutates `tokens`, which checkMissedSemicolon then reads).
        let mut i = tokens.len();
        while i > 0 {
            i -= 1;
            let token = tokens[i].clone();
            let lower = token.text.to_lowercase();
            if lower == "!important" {
                tokens.truncate(i);
                while tokens.last().is_some_and(|t| t.kind == Kind::Space) { tokens.pop(); }
                break;
            } else if lower == "important" {
                let mut cache = tokens.clone();
                let mut s = String::new();
                let mut j = i;
                while j > 0 {
                    let kind = cache[j].kind;
                    if s.trim_start_matches(js_ws).starts_with('!') && kind != Kind::Space { break; }
                    let t = cache.pop().unwrap();
                    s = t.text + &s;
                    j -= 1;
                }
                if s.trim_start_matches(js_ws).starts_with('!') { tokens = cache; }
            }
            if token.kind != Kind::Space && token.kind != Kind::Comment { break; }
        }

        // `node.value.includes(':') && !customProperty`: a `:` token always lands in the value, and
        // colon() finds nothing otherwise, so checking unconditionally is equivalent.
        if !custom_property { check_missed_semicolon(&tokens)?; }
        Ok(())
    }
}

fn js_ws(c: char) -> bool { c.is_whitespace() || c == '\u{FEFF}' }

fn check_missed_semicolon(tokens: &[Tok]) -> R<()> {
    let Some(colon) = colon(tokens)? else { return Ok(()) };
    let mut founded = 0;
    let mut token = &tokens[0];
    for j in (0..colon).rev() {
        token = &tokens[j];
        if token.kind != Kind::Space {
            founded += 1;
            if founded == 2 { break; }
        }
    }
    let at = if token.kind == Kind::Word { token.end.map(|e| e + 1) } else { token.start };
    Err(Fail { reason: "Missed semicolon".into(), at: At::Offset(at) })
}

fn colon(tokens: &[Tok]) -> R<Option<usize>> {
    let mut brackets = 0i32;
    let mut prev: Option<&Tok> = None;
    for (i, token) in tokens.iter().enumerate() {
        if token.kind == Kind::OpenParen { brackets += 1; }
        if token.kind == Kind::CloseParen { brackets -= 1; }
        if brackets == 0 && token.kind == Kind::Colon {
            match prev {
                None => return Err(Fail { reason: "Double colon".into(), at: At::Offset(token.start) }),
                Some(p) if p.kind == Kind::Word && p.text == "progid" => continue,
                Some(_) => return Ok(Some(i)),
            }
        }
        prev = Some(token);
    }
    Ok(None)
}

/// `Input#fromOffset`: 1-based line (split on `\n`) and column (UTF-16 units).
fn from_offset(css: &[u16], offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut line_start = 0usize;
    for (i, &c) in css.iter().enumerate().take(offset.min(css.len())) {
        if c == 0x0A { line += 1; line_start = i + 1; }
    }
    (line, (offset - line_start + 1) as u32)
}

/// `postcss.parse(src)` outcome: `Err` with PostCSS's reason and position when it throws.
pub fn check(src: &str) -> Result<(), StyleError> {
    let mut css: Vec<u16> = src.encode_utf16().collect();
    // Input: a leading U+FEFF / U+FFFE is dropped before tokenizing (offsets are after it).
    if matches!(css.first(), Some(0xFEFF | 0xFFFE)) { css.remove(0); }
    let mut p = Parser { tz: Tokenizer { css: &css, pos: 0, buffer: Vec::new(), returned: Vec::new() }, open: Vec::new() };
    match p.parse() {
        Ok(()) => Ok(()),
        Err(Fail { reason, at }) => {
            let pos = match at {
                At::LineCol(l, c) => Some((l, c)),
                At::Offset(o) => o.map(|o| from_offset(&css, o)),
            };
            Err(StyleError::syntax_at(reason, pos))
        }
    }
}
