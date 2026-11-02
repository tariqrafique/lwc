//! cssparser-driven lossless parser producing the PostCSS-like tree in `ast`.
use crate::ast::*;
use crate::error::StyleError;
use cssparser::{ParseError, ParseErrorKind, Parser, ParserInput, Token};

type PResult<'i, T> = Result<T, ParseError<'i, StyleError>>;

pub fn parse(src: &str) -> Result<Root, StyleError> {
    let mut input = ParserInput::new(src);
    let mut parser = Parser::new(&mut input);
    match parse_block(&mut parser) {
        Ok(b) => Ok(Root { nodes: b.nodes, after: b.after, imports: Vec::new() }),
        Err(e) => Err(match e.kind {
            ParseErrorKind::Custom(se) => se,
            _ => StyleError::syntax("Unknown parse error", e.location.line + 1, e.location.column),
        }),
    }
}

fn err<'i, T>(reason: String, loc: Loc) -> PResult<'i, T> {
    Err(ParseError {
        kind: ParseErrorKind::Custom(StyleError::syntax(reason, loc.line, loc.column)),
        location: cssparser::SourceLocation { line: loc.line.saturating_sub(1), column: loc.column },
    })
}

fn loc(p: &Parser) -> Loc {
    let l = p.current_source_location();
    Loc { line: l.line + 1, column: l.column, offset: p.position().byte_index() }
}

enum End { Semicolon, Block, Eof }

fn parse_block<'i, 't>(p: &mut Parser<'i, 't>) -> PResult<'i, Block> {
    let mut block = Block::default();
    loop {
        // raws.before: whitespace; comments become nodes.
        let mut before = String::new();
        loop {
            let st = p.state();
            let pos = p.position();
            match p.next_including_whitespace_and_comments() {
                Ok(Token::WhiteSpace(_)) => before.push_str(p.slice_from(pos)),
                Ok(Token::Comment(c)) => {
                    let text = c.to_string();
                    block.nodes.push(Node::Comment(Comment { before: std::mem::take(&mut before), text }));
                }
                Ok(_) => { p.reset(&st); break; }
                Err(_) => { block.after = before; return Ok(block); }
            }
        }
        let start = p.position();
        let start_loc = loc(p);
        let mut last_end = start.clone();
        let mut brace_pos = None;
        let end = loop {
            let pos = p.position();
            let tok_loc = loc(p);
            match p.next_including_whitespace_and_comments() {
                Ok(Token::Semicolon) => break End::Semicolon,
                Ok(Token::CurlyBracketBlock) => { brace_pos = Some(pos); break End::Block }
                Ok(Token::WhiteSpace(_)) | Ok(Token::Comment(_)) => {}
                Ok(Token::Function(_)) | Ok(Token::ParenthesisBlock) | Ok(Token::SquareBracketBlock) => {
                    let open_text = p.slice_from(pos.clone()).to_string();
                    let close = if open_text.ends_with('[') { ']' } else { ')' };
                    let mut open_loc = tok_loc;
                    open_loc.column += open_text.len() as u32 - 1;
                    p.parse_nested_block(|np| { while np.next_including_whitespace_and_comments().is_ok() {} Ok::<(), ParseError<'i, StyleError>>(()) })?;
                    // cssparser lets an unclosed `(`/`[` block run to EOF (swallowing `}`); PostCSS throws.
                    if !p.slice_from(pos).ends_with(close) { return err("Unclosed bracket".to_string(), open_loc); }
                    last_end = p.position();
                }
                Ok(_) => last_end = p.position(),
                Err(_) => break End::Eof,
            }
        };
        let full = |p: &Parser<'i, 't>, to| p.slice(start.clone()..to).to_string();
        match end {
            End::Block => {
                let head = full(p, brace_pos.unwrap());
                let trimmed = head.trim_end().to_string();
                let between = head[trimmed.len()..].to_string();
                let inner = p.parse_nested_block(|np| parse_block(np))?;
                if let Some(rest) = trimmed.strip_prefix('@') {
                    let (name, after_name, params) = split_at_name(rest);
                    block.nodes.push(Node::AtRule(AtRule { before, name, after_name, params, between, block: Some(inner), loc: start_loc }));
                } else {
                    block.nodes.push(Node::Rule(Rule { before, selector: trimmed, between, block: inner, loc: start_loc, end_offset: p.position().byte_index(), host: HostKind::None }));
                }
            }
            End::Semicolon | End::Eof => {
                let text = full(p, last_end.clone());
                if text.is_empty() { continue; }
                block.semicolon = matches!(end, End::Semicolon);
                if let Some(rest) = text.strip_prefix('@') {
                    let (name, after_name, params) = split_at_name(rest);
                    block.nodes.push(Node::AtRule(AtRule { before, name, after_name, params, between: String::new(), block: None, loc: start_loc }));
                } else if let Some(idx) = text.find(':') {
                    let prop = text[..idx].trim_end().to_string();
                    let rest = &text[idx + 1..];
                    let value = rest.trim_start().to_string();
                    let between = text[prop.len()..text.len() - value.len()].to_string();
                    block.nodes.push(Node::Decl(Decl { before, prop, between, value, loc: start_loc }));
                } else {
                    return err(format!("Unknown word {}", text), start_loc);
                }
                if matches!(end, End::Eof) { block.after = String::new(); return Ok(block); }
            }
        }
    }
}

fn split_at_name(rest: &str) -> (String, String, String) {
    let n = rest.find(|c: char| c.is_whitespace() || c == '(' || c == '"' || c == '\'').unwrap_or(rest.len());
    let name = rest[..n].to_string();
    let tail = &rest[n..];
    let params = tail.trim_start().to_string();
    let after_name = tail[..tail.len() - params.len()].to_string();
    (name, after_name, params)
}
