//! Template-expression validation without a JS parser: a small tokenizer plus a precedence
//! classifier that reproduces what upstream gets from acorn + `validateExpression`
//! (expression.ts, legacy path) and `validateComplexExpression` (expression-complex/validate.ts, CTE path).
//!
//! Message shapes (TS `withErrorWrapping(…, (err) => \`Invalid expression ${source} - ${err.message}\`)`):
//! - acorn SyntaxError → `LWC1083: Error parsing template expression: Invalid expression {src} - <msg> (line:col)`
//! - `invariant` CompilerError → `Invalid expression {src} - LWC<code>: <msg>`, code kept.

use super::messages::message;

/// First API version with complex template expressions (TS `minApiVersion(ENABLE_COMPLEX_TEMPLATE_EXPRESSIONS)`).
pub(crate) const MIN_CTE_API_VERSION: u32 = 66;

/// TS `isExpression`: `{...}` with length > 2.
pub(crate) fn is_expression(s: &str) -> bool { s.len() > 2 && s.starts_with('{') && s.ends_with('}') }

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExprError { pub code: u32, pub message: String }

impl ExprError {
    /// Error-level diagnostic at `l`; `message` already carries the upstream prefix/wrapping.
    pub(crate) fn into_diagnostic(self, l: lwc_shared::SourceLocation) -> lwc_shared::CompilerDiagnostic {
        lwc_shared::CompilerDiagnostic::new(Some(self.code), self.message, lwc_shared::DiagnosticLevel::Error, None, Some(l))
    }
}

const RESERVED: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else", "export",
    "extends", "finally", "for", "function", "if", "import", "in", "instanceof", "new", "return", "super", "switch",
    "this", "throw", "try", "typeof", "var", "void", "while", "with", "yield", "await", "let", "enum",
];

pub(crate) fn is_reserved(w: &str) -> bool { RESERVED.contains(&w) }

/// Keywords acorn rejects in expression position ("Unexpected token").
const STATEMENT_KEYWORDS: &[&str] = &[
    "break", "case", "catch", "const", "continue", "debugger", "default", "do", "else", "export", "extends",
    "finally", "for", "if", "return", "switch", "throw", "try", "var", "while", "with", "enum",
];

/// Validation options (TS ParserCtx config + call-site flags).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Opts {
    /// `experimentalComplexExpressions`.
    pub complex: bool,
    /// `experimentalComputedMemberExpression`.
    pub computed_ok: bool,
    pub api_version: u32,
    /// TS `unquotedAttributeExpression` (attribute value written as `attr={…}`).
    pub unquoted: bool,
}

impl Opts {
    /// TS `isComplexTemplateExpressionEnabled`.
    fn cte(&self) -> bool { self.complex && self.api_version >= MIN_CTE_API_VERSION }
}

#[cfg(test)]
pub(crate) fn check(src: &str, complex: bool) -> Option<ExprError> { check_with(src, complex, false) }

/// Text-node semantics at the default API version (tests).
#[cfg(test)]
pub(crate) fn check_with(src: &str, complex: bool, computed_ok: bool) -> Option<ExprError> {
    check_opts(src, &Opts { complex, computed_ok, api_version: crate::config::DEFAULT_API_VERSION, unquoted: false })
}

/// `src` is one `{…}` expression. CTE (quoted / text) goes through the complex validator, everything
/// else through TS `parseExpression`.
pub(crate) fn check_opts(src: &str, o: &Opts) -> Option<ExprError> {
    let n = src.chars().count();
    if o.cte() && !o.unquoted { check_complex(src, 0, n, o) } else { check_legacy(src, o) }
}

/// TS `parseComplexExpression(ctx, source, …, expressionStart)`: `source` is the whole message/acorn
/// source (for text nodes the full cleaned text), the expression spans chars `[start, end)` of it.
pub(crate) fn check_complex(source: &str, start: usize, end: usize, _o: &Opts) -> Option<ExprError> {
    let cs: Vec<char> = source.chars().collect();
    let p = Parsed::new(&cs, start, end, true);
    if let Some(e) = p.syntax_error(source) { return Some(e); }
    let inv = |code: u32, args: &[&str]| Some(ExprError { code, message: format!("Invalid expression {source} - {}", message(code, args)) });
    if let Some((code, arg)) = p.complex_offense() { return inv(code, &arg.iter().map(|s| s.as_str()).collect::<Vec<_>>()); }
    match p.cut {
        Cut::End => None,
        Cut::Extra(i) if p.toks[i].t == ")" => inv(1083, &["expression must have balanced parentheses."]),
        _ => inv(1083, &["expression must end with curly brace."]),
    }
}

/// TS validateComplexExpression's second acorn pass: `raw` is the entity-undecoded template source of
/// the expression `source` (which parsed cleanly). Any difference is an error; messages name `source`.
pub(crate) fn check_raw_source(source: &str, raw: &str) -> Option<ExprError> {
    let cs: Vec<char> = raw.chars().collect();
    let p = Parsed::new(&cs, 0, cs.len(), true);
    p.syntax_error(source).or_else(|| Some(ExprError { code: 1083, message: format!("Invalid expression {source} - {}", message(1083, &["expression incorrectly formed"])) }))
}

/// TS `parseExpression`: acorn, `validateSourceIsParsedExpression`, `validateExpression`.
fn check_legacy(src: &str, o: &Opts) -> Option<ExprError> {
    let cs: Vec<char> = src.chars().collect();
    let p = Parsed::new(&cs, 0, cs.len(), false);
    if let Some(e) = p.syntax_error(src) { return Some(e); }
    let inv = |code: u32, args: &[&str]| Some(ExprError { code, message: format!("Invalid expression {src} - {}", message(code, args)) });
    let (a, b) = p.unwrap_parens(0, p.cut_idx());
    // validateSourceIsParsedExpression
    let node_start = p.toks[a].s;
    let node_end = p.toks[b - 1].e;
    if node_end != cs.len() - 1 {
        let mut open = cs[..node_start].iter().filter(|c| **c == '(').count() as i64;
        for &c in &cs[node_end..cs.len() - 1] {
            if c == ')' { open -= 1; }
            else if c == ';' { return inv(1074, &[]); }
            else if !c.is_whitespace() { return inv(1083, &["Unexpected end of expression"]); }
        }
        if open != 0 { return inv(1083, &["Unexpected end of expression"]); }
    }
    // validateExpression
    let api = o.api_version.to_string();
    let min = MIN_CTE_API_VERSION.to_string();
    match p.validate_legacy(a, b) {
        None => None,
        Some(Legacy::Node(ty)) => {
            if !o.complex { inv(1060, &[ty]) }
            else if !o.cte() { inv(1210, &[ty, &api, &min]) }
            else if o.unquoted { inv(1208, &[ty, src]) }
            else { None }
        }
        Some(Legacy::Computed) if o.computed_ok => None,
        Some(Legacy::Computed) => {
            if !o.complex { inv(1038, &[src]) }
            else if !o.cte() { inv(1209, &[src, &api, &min]) }
            else if o.unquoted { inv(1207, &[src]) }
            else { None }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum K { Ident, Num, Str, Tmpl, Regex, Punct, Comment }

#[derive(Debug, Clone)]
struct Tok { k: K, s: usize, e: usize, t: String, bigint: bool }

/// acorn-style lexer error: (position, message).
type LexErr = (usize, String);

const PUNCT: &[&str] = &[
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=",
    "=>", "==", "!=", "<=", ">=", "&&", "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=",
    "**", "<<", ">>",
    "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "/", "%", "&", "|", "^", "!", "~", "?", ":", "=", ".", "@",
];
const ASSIGN_OPS: &[&str] = &["=", "+=", "-=", "*=", "/=", "%=", "**=", "<<=", ">>=", ">>>=", "&=", "|=", "^=", "&&=", "||=", "??="];
const BINARY_OPS: &[&str] = &[
    "==", "!=", "===", "!==", "<", ">", "<=", ">=", "<<", ">>", ">>>", "+", "-", "*", "/", "%", "**", "|", "^", "&", "in", "instanceof",
];

fn ident_start(c: char) -> bool { c.is_alphabetic() || c == '_' || c == '$' }
fn ident_char(c: char) -> bool { c.is_alphanumeric() || c == '_' || c == '$' }

/// Does a token of this kind end an operand (so a following `/` is division, `(` a call, …)?
fn ends_operand(t: &Tok) -> bool {
    match t.k {
        K::Num | K::Str | K::Tmpl | K::Regex => true,
        K::Ident => !matches!(t.t.as_str(), "typeof" | "void" | "delete" | "new" | "in" | "instanceof" | "return" | "throw" | "case" | "do" | "else")
            && !STATEMENT_KEYWORDS.contains(&t.t.as_str()) || matches!(t.t.as_str(), "this" | "super" | "true" | "false" | "null"),
        K::Punct => matches!(t.t.as_str(), ")" | "]" | "}"),
        K::Comment => false,
    }
}

/// Lex `cs[from..to)`. Stops at the first lexer error.
fn lex(cs: &[char], from: usize, to: usize) -> (Vec<Tok>, Option<LexErr>) {
    let mut toks: Vec<Tok> = Vec::new();
    let mut i = from;
    let tok = |k, s, e, cs: &[char]| Tok { k, s, e, t: cs[s..e].iter().collect(), bigint: false };
    while i < to {
        let c = cs[i];
        if c.is_whitespace() { i += 1; continue; }
        let s = i;
        let next = cs.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < to && cs[i] != '\n' { i += 1; }
            toks.push(tok(K::Comment, s, i, cs));
            continue;
        }
        if c == '/' && next == Some('*') {
            i += 2;
            while i + 1 < to && !(cs[i] == '*' && cs[i + 1] == '/') { i += 1; }
            if i + 1 >= to { return (toks, Some((s, "Unterminated comment".into()))); }
            i += 2;
            toks.push(tok(K::Comment, s, i, cs));
            continue;
        }
        if ident_start(c) {
            while i < to && ident_char(cs[i]) { i += 1; }
            toks.push(tok(K::Ident, s, i, cs));
            continue;
        }
        if c == '#' {
            match next { Some(n) if ident_start(n) => { i += 1; while i < to && ident_char(cs[i]) { i += 1; } toks.push(tok(K::Ident, s, i, cs)); continue; }
                Some(n) => return (toks, Some((i + 1, format!("Unexpected character '{n}'")))),
                None => return (toks, Some((i + 1, "Unexpected character".into()))) }
        }
        if c.is_ascii_digit() || (c == '.' && next.is_some_and(|n| n.is_ascii_digit())) {
            while i < to && (cs[i].is_alphanumeric() || cs[i] == '.' || cs[i] == '_'
                || ((cs[i] == '+' || cs[i] == '-') && matches!(cs[i - 1], 'e' | 'E') && !cs[s..i].iter().any(|c| matches!(c, 'x' | 'X')))) { i += 1; }
            let mut t = tok(K::Num, s, i, cs);
            t.bigint = t.t.ends_with('n');
            toks.push(t);
            continue;
        }
        if c == '\'' || c == '"' {
            i += 1;
            loop {
                if i >= cs.len() || cs[i] == '\n' { return (toks, Some((s, "Unterminated string constant".into()))); }
                if cs[i] == '\\' { i += 2; continue; }
                if cs[i] == c { i += 1; break; }
                i += 1;
            }
            toks.push(tok(K::Str, s, i, cs));
            continue;
        }
        if c == '`' {
            i += 1;
            let mut depth = 0;
            loop {
                if i >= cs.len() { return (toks, Some((s, "Unterminated template".into()))); }
                match cs[i] {
                    '\\' => { i += 2; continue; }
                    '`' if depth == 0 => { i += 1; break; }
                    '$' if depth == 0 && cs.get(i + 1) == Some(&'{') => { depth = 1; i += 2; continue; }
                    '{' if depth > 0 => depth += 1,
                    '}' if depth > 0 => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            toks.push(tok(K::Tmpl, s, i, cs));
            continue;
        }
        if c == '/' && !toks.iter().rev().find(|t| t.k != K::Comment).is_some_and(ends_operand) {
            i += 1;
            let mut class = false;
            loop {
                if i >= cs.len() || cs[i] == '\n' { return (toks, Some((s, "Unterminated regular expression".into()))); }
                match cs[i] { '\\' => { i += 2; continue; } '[' => class = true, ']' => class = false, '/' if !class => { i += 1; break; } _ => {} }
                i += 1;
            }
            while i < to && ident_char(cs[i]) { i += 1; }
            toks.push(tok(K::Regex, s, i, cs));
            continue;
        }
        let rest: String = cs[i..to.min(i + 4)].iter().collect();
        match PUNCT.iter().find(|p| rest.starts_with(**p) && !(**p == "?." && rest[2..].starts_with(|c: char| c.is_ascii_digit()))) {
            Some(p) => { i += p.chars().count(); toks.push(tok(K::Punct, s, i, cs)); }
            None => return (toks, Some((s, format!("Unexpected character '{c}'")))),
        }
    }
    (toks, None)
}

/// Where acorn's `parseExpressionAt` stopped.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cut {
    /// Stopped at the expression's own closing `}` (token index = last).
    End,
    /// Stopped at a closer with no opener (token index).
    Extra(usize),
    /// Stopped at a top-level `;`.
    Semi(usize),
    /// Two operands with no operator between them (token index of the second).
    Adjacent(usize),
}

enum Legacy { Node(&'static str), Computed }

struct Parsed<'a> {
    cs: &'a [char],
    /// Non-comment tokens, including the closing `}` when present.
    toks: Vec<Tok>,
    comments: Vec<Tok>,
    /// Matching bracket index (same for non-brackets).
    mate: Vec<usize>,
    depth: Vec<usize>,
    cut: Cut,
    /// First syntax error at or before the cut: (position, message).
    syntax: Option<LexErr>,
    complex: bool,
}

impl<'a> Parsed<'a> {
    /// `cs[start]` is the opening `{`; the expression text is `cs[start..end)`.
    fn new(cs: &'a [char], start: usize, end: usize, complex: bool) -> Self {
        let (all, lex_err) = lex(cs, start + 1, end);
        let (comments, toks): (Vec<Tok>, Vec<Tok>) = all.into_iter().partition(|t| t.k == K::Comment);
        let n = toks.len();
        let mut p = Parsed { cs, toks, comments, mate: (0..n).collect(), depth: vec![0; n], cut: Cut::End, syntax: None, complex };
        // EOF (no closing `}`): acorn reports at the end of input.
        let eof = end;
        let has_close = n > 0 && p.toks[n - 1].t == "}" && p.toks[n - 1].e == end && cs[end - 1] == '}';
        let mut stack: Vec<usize> = Vec::new();
        let mut prev_operand = false;
        let mut block_expected = false; // after `function` / `class`, until their body `{`
        let mut syntax: Option<LexErr> = None;
        let mut cut: Option<Cut> = None;
        let mut i = 0;
        while i < n {
            let t = p.toks[i].clone();
            p.depth[i] = stack.len();
            let after_dot = i > 0 && matches!(p.toks[i - 1].t.as_str(), "." | "?.") && p.toks[i - 1].k == K::Punct;
            let next = p.toks.get(i + 1).map(|t| t.t.as_str());
            let is_key = next == Some(":") && stack.last().is_some_and(|&o| p.toks[o].t == "{");
            if stack.is_empty() && has_close && i == n - 1 { cut = Some(Cut::End); break; }
            match t.k {
                K::Punct => match t.t.as_str() {
                    "(" | "[" | "{" => {
                        if t.t == "{" && stack.is_empty() && prev_operand && !block_expected { cut = Some(Cut::Adjacent(i)); break; }
                        if t.t == "{" { block_expected = false; }
                        stack.push(i);
                        prev_operand = false;
                    }
                    ")" | "]" | "}" => {
                        match stack.pop() {
                            None => { cut = Some(Cut::Extra(i)); break; }
                            Some(o) => {
                                let want = match p.toks[o].t.as_str() { "(" => ")", "[" => "]", _ => "}" };
                                if want != t.t { syntax = Some((t.s, "Unexpected token".into())); break; }
                                p.mate[o] = i; p.mate[i] = o;
                            }
                        }
                        prev_operand = true;
                    }
                    ";" if stack.is_empty() => { cut = Some(Cut::Semi(i)); break; }
                    "++" | "--" => {} // postfix keeps operand state; prefix keeps it false
                    _ => prev_operand = false,
                },
                K::Ident if after_dot || is_key => prev_operand = true,
                K::Ident => {
                    let w = t.t.as_str();
                    let starts_operand = !matches!(w, "in" | "instanceof");
                    if starts_operand && prev_operand && stack.is_empty() && !block_expected { cut = Some(Cut::Adjacent(i)); break; }
                    match w {
                        "super" => { syntax = Some((t.s, "'super' keyword outside a method".into())); break; }
                        "import" if next == Some(".") => { syntax = Some((t.s, "Cannot use 'import.meta' outside a module".into())); break; }
                        "import" => prev_operand = true,
                        "class" | "function" => {
                            match p.toks.get(i + 1) {
                                Some(nt) if nt.k == K::Ident || nt.t == "{" || nt.t == "(" || nt.t == "*" => {}
                                Some(nt) => { syntax = Some((nt.s, "Unexpected token".into())); break; }
                                None => { syntax = Some((eof, "Unexpected token".into())); break; }
                            }
                            block_expected = true;
                            prev_operand = false;
                        }
                        "async" if matches!(next, Some("(") | Some("function")) || p.toks.get(i + 1).is_some_and(|t| t.k == K::Ident && p.toks.get(i + 2).is_some_and(|t| t.t == "=>")) => prev_operand = false,
                        "await" if !complex => prev_operand = true,
                        "typeof" | "void" | "delete" | "new" | "await" | "in" | "instanceof" => prev_operand = false,
                        "extends" if block_expected => prev_operand = false,
                        _ if STATEMENT_KEYWORDS.contains(&w) => { syntax = Some((t.s, "Unexpected token".into())); break; }
                        _ => prev_operand = !block_expected,
                    }
                }
                K::Tmpl => prev_operand = true, // tagged template continues the operand
                K::Num | K::Str | K::Regex => {
                    if prev_operand && stack.is_empty() { cut = Some(Cut::Adjacent(i)); break; }
                    prev_operand = true;
                }
                K::Comment => {}
            }
            i += 1;
        }
        let cut_pos = match cut { Some(Cut::End) => p.toks[n - 1].s, Some(Cut::Extra(i) | Cut::Semi(i) | Cut::Adjacent(i)) => p.toks[i].s, None => eof };
        if syntax.is_none() && cut.is_none() {
            // Ran out of tokens: an empty expression, or an opener left unclosed.
            syntax = Some((if n > 0 && has_close { p.toks[n - 1].s } else { eof }, "Unexpected token".into()));
        }
        if syntax.is_none() && n > 0 && matches!(cut, Some(Cut::End)) && n == 1 {
            syntax = Some((p.toks[0].s, "Unexpected token".into())); // `{ }`
        }
        if syntax.is_none() && matches!(cut, Some(Cut::Extra(0) | Cut::Semi(0))) {
            syntax = Some((p.toks[0].s, "Unexpected token".into()));
        }
        // A lexer error before the stopping point wins only if it comes first in the source.
        if let Some(le) = lex_err {
            let stop = syntax.as_ref().map(|s| s.0).unwrap_or(cut_pos);
            if le.0 <= stop || cut.is_none() { syntax = Some(le); }
        }
        p.cut = cut.unwrap_or(Cut::End);
        p.syntax = syntax;
        p
    }

    /// Token index where the parsed expression stops (exclusive).
    fn cut_idx(&self) -> usize {
        match self.cut { Cut::End => self.toks.len() - 1, Cut::Extra(i) | Cut::Semi(i) | Cut::Adjacent(i) => i }
    }

    fn line_col(&self, pos: usize) -> (usize, usize) {
        let before = &self.cs[..pos.min(self.cs.len())];
        let line = 1 + before.iter().filter(|c| **c == '\n').count();
        let col = before.iter().rev().take_while(|c| **c != '\n').count();
        (line, col)
    }

    /// acorn SyntaxError (1083, outer prefix) or the onComment invariant (1197), whichever comes first.
    fn syntax_error(&self, source: &str) -> Option<ExprError> {
        let stop = self.syntax.as_ref().map(|s| s.0)
            .unwrap_or_else(|| self.toks.get(self.cut_idx()).map(|t| t.s).unwrap_or(usize::MAX));
        if self.comments.iter().any(|c| c.s < stop) {
            return Some(ExprError { code: 1197, message: format!("Invalid expression {source} - {}", message(1197, &[])) });
        }
        let (pos, msg) = self.syntax.as_ref()?;
        let (l, c) = self.line_col(*pos);
        Some(ExprError { code: 1083, message: message(1083, &[&format!("Invalid expression {source} - {msg} ({l}:{c})")]) })
    }

    fn is(&self, i: usize, s: &str) -> bool { self.toks.get(i).is_some_and(|t| t.t == s && t.k != K::Str) }

    /// Strip wrapping parentheses (acorn drops them from the node).
    fn unwrap_parens(&self, mut a: usize, mut b: usize) -> (usize, usize) {
        while b > a + 1 && self.is(a, "(") && self.mate[a] == b - 1 { a += 1; b -= 1; }
        (a, b)
    }

    /// Indices in `[a, b)` at the range's own nesting level.
    fn top(&self, a: usize, b: usize) -> Vec<usize> {
        let mut v = Vec::new();
        let mut i = a;
        while i < b {
            v.push(i);
            if matches!(self.toks[i].t.as_str(), "(" | "[" | "{") && self.toks[i].k == K::Punct && self.mate[i] > i { i = self.mate[i]; }
            i += 1;
        }
        v
    }

    /// Start index of the arrow function whose `=>` is at `m`.
    fn arrow_start(&self, m: usize) -> usize {
        let mut s = if self.is(m - 1, ")") { self.mate[m - 1] } else { m - 1 };
        if s > 0 && self.is(s - 1, "async") { s -= 1; }
        s
    }

    /// ESTree root node type of tokens `[a, b)`.
    fn node_type(&self, a: usize, b: usize) -> &'static str {
        let (a, b) = self.unwrap_parens(a, b);
        let top = self.top(a, b);
        let punct = |i: &usize, ops: &[&str]| { let t = &self.toks[*i]; ops.contains(&t.t.as_str()) && (t.k == K::Punct || t.k == K::Ident) };
        if top.iter().any(|i| punct(i, &[","])) { return "SequenceExpression"; }
        if let Some(&m) = top.iter().find(|i| punct(i, &["=>"])) { if self.arrow_start(m) == a { return "ArrowFunctionExpression"; } }
        if top.iter().any(|i| punct(i, ASSIGN_OPS)) { return "AssignmentExpression"; }
        if top.iter().any(|i| punct(i, &["?"])) { return "ConditionalExpression"; }
        if top.iter().any(|i| punct(i, &["||", "&&", "??"])) { return "LogicalExpression"; }
        if top.iter().any(|&i| i > a && punct(&i, BINARY_OPS) && ends_operand(&self.toks[i - 1])) { return "BinaryExpression"; }
        let first = &self.toks[a];
        match first.t.as_str() {
            "!" | "~" | "+" | "-" | "typeof" | "void" | "delete" => return "UnaryExpression",
            "++" | "--" => return "UpdateExpression",
            "await" if self.complex => return "AwaitExpression",
            "class" => return "ClassExpression",
            "function" => return "FunctionExpression",
            _ => {}
        }
        if matches!(self.toks[b - 1].t.as_str(), "++" | "--") { return "UpdateExpression"; }
        if top.iter().any(|i| punct(i, &["?."])) { return "ChainExpression"; }
        if first.t == "new" && self.toks[b - 1].t == ")" && self.mate[b - 1] > a {
            // `new X(...)` with nothing after the arguments.
            let o = self.mate[b - 1];
            if top.iter().filter(|&&i| i > a && i < o && (self.is(i, "(") || self.is(i, "["))).count() == 0 { return "NewExpression"; }
        }
        if top.len() > 1 {
            let last = *top.last().unwrap();
            let lt = &self.toks[last];
            if lt.k == K::Tmpl { return "TaggedTemplateExpression"; }
            if self.is(last, "(") { return "CallExpression"; }
            return "MemberExpression";
        }
        match first.k {
            K::Ident => match first.t.as_str() {
                "this" => "ThisExpression",
                "true" | "false" | "null" => "Literal",
                _ => "Identifier",
            },
            K::Num | K::Str | K::Regex => "Literal",
            K::Tmpl => "TemplateLiteral",
            _ => match first.t.as_str() { "[" => "ArrayExpression", "{" => "ObjectExpression", _ => "Identifier" },
        }
    }

    /// TS `validateExpression` (legacy): first non Identifier/Member node, or a computed member.
    fn validate_legacy(&self, a: usize, b: usize) -> Option<Legacy> {
        let ty = self.node_type(a, b);
        if ty == "Identifier" { return None; }
        if ty != "MemberExpression" { return Some(Legacy::Node(ty)); }
        let (a, b) = self.unwrap_parens(a, b);
        let top = self.top(a, b);
        let last = *top.last().unwrap();
        let (obj_end, computed) = if self.is(last, "[") { (last, true) } else { (last - 1, false) }; // `.prop` → drop `.` too
        if computed { return Some(Legacy::Computed); }
        if self.node_type(a, obj_end) != "Identifier" { if let Some(e) = self.validate_legacy(a, obj_end) { return Some(e); } }
        None
    }

    /// TS validateExpressionAst (pre-order walk): the first offending node by (start, outermost).
    fn complex_offense(&self) -> Option<(u32, Vec<String>)> {
        let b = self.cut_idx();
        let (a, b) = self.unwrap_parens(0, b);
        // (token start index, rank: lower = outer node at the same start, code, args)
        let mut found: Vec<(usize, u8, u32, Vec<String>)> = Vec::new();
        let prohibited = |what: &str| (1196u32, vec![what.to_string()]);
        // Arrow bodies: (=> index, body end exclusive).
        let mut arrows: Vec<(usize, usize, usize)> = Vec::new(); // (start, =>, end)
        for m in a..b {
            if !(self.is(m, "=>") && self.toks[m].k == K::Punct) { continue; }
            let mut e = m + 1;
            let d = self.depth[m];
            while e < b && !(self.depth[e] < d || (self.depth[e] == d && (self.is(e, ",") || self.is(e, ":")))) {
                if self.depth[e] == d && matches!(self.toks[e].t.as_str(), "(" | "[" | "{") && self.mate[e] > e { e = self.mate[e]; }
                e += 1;
            }
            let s = self.arrow_start(m);
            arrows.push((s, m, e));
            if self.is(m + 1, "{") { found.push((s, 2, 1193, vec![])); }
            else if self.is(s, "async") { found.push((s, 2, 1194, vec!["async".into()])); }
        }
        let in_arrow = |i: usize| arrows.iter().any(|&(_, m, e)| i > m && i < e);
        let in_params = |i: usize| arrows.iter().any(|&(s, m, _)| i >= s && i < m);
        // Sequences: commas at the top level or directly inside a parenthesized expression.
        let paren_expr = |o: usize| self.is(o, "(") && !(o > 0 && ends_operand(&self.toks[o - 1]) && !self.is(o - 1, "}"))
            && !self.is(self.mate[o] + 1, "=>");
        for i in a..b {
            if !(self.is(i, ",") && self.toks[i].k == K::Punct) { continue; }
            let opener = (0..i).rev().find(|&o| matches!(self.toks[o].t.as_str(), "(" | "[" | "{") && self.toks[o].k == K::Punct && self.mate[o] > i);
            match opener {
                None => found.push((a, 0, 1196, vec!["comma operators".into()])),
                Some(o) if o < a => found.push((a, 0, 1196, vec!["comma operators".into()])),
                Some(o) if paren_expr(o) => found.push((o + 1, 0, 1196, vec!["comma operators".into()])),
                _ => {}
            }
        }
        for i in a..b {
            let t = &self.toks[i];
            let after_dot = i > 0 && (self.is(i - 1, ".") || self.is(i - 1, "?."));
            let is_key = self.is(i + 1, ":") && self.depth[i] > 0 && (0..i).rev().find(|&o| self.mate[o] > i && self.toks[o].k == K::Punct && self.depth[o] < self.depth[i] + 1).is_some_and(|o| self.is(o, "{"));
            match t.k {
                K::Punct if ASSIGN_OPS.contains(&t.t.as_str()) && !in_arrow(i) && !in_params(i) => found.push((self.lhs_start(i, a), 1, 1191, vec![])),
                K::Punct if (t.t == "++" || t.t == "--") && !in_arrow(i) => {
                    let postfix = i > a && ends_operand(&self.toks[i - 1]);
                    found.push((if postfix { self.lhs_start(i, a) } else { i }, 1, 1191, vec![]));
                }
                K::Ident if after_dot || is_key => {}
                K::Ident => {
                    let (code, args) = match t.t.as_str() {
                        "delete" => (1192, vec![]),
                        "await" => prohibited("await expressions"),
                        "class" => prohibited("classes"),
                        "function" => prohibited("function expressions"),
                        "import" if self.is(i + 1, "(") => prohibited("imports"),
                        "new" => prohibited("object instantiation"),
                        "this" => prohibited("`this`"),
                        _ => continue,
                    };
                    found.push((i, 3, code, args));
                }
                K::Regex => found.push((i, 3, 1196, vec!["regular expression literals".into()])),
                K::Num if t.bigint => found.push((i, 3, 1196, vec!["BigInts".into()])),
                _ => {}
            }
        }
        found.into_iter().min_by_key(|f| (self.toks[f.0].s, f.1)).map(|f| (f.2, f.3))
    }

    /// Start of the assignment/update target ending just before token `op`.
    fn lhs_start(&self, op: usize, lo: usize) -> usize {
        let mut k = op - 1;
        loop {
            if matches!(self.toks[k].t.as_str(), ")" | "]") && self.toks[k].k == K::Punct { k = self.mate[k]; }
            if self.is(k, "[") && k > lo && ends_operand(&self.toks[k - 1]) { k -= 1; continue; }
            if k >= lo + 2 && (self.is(k - 1, ".") || self.is(k - 1, "?.")) { k -= 2; continue; }
            if self.is(k, "(") && k > lo && ends_operand(&self.toks[k - 1]) { k -= 1; continue; }
            return k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn o(complex: bool) -> Opts { Opts { complex, computed_ok: false, api_version: 66, unquoted: false } }
    fn msg(s: &str, complex: bool) -> Option<String> { check_opts(s, &o(complex)).map(|e| format!("{} {}", e.code, e.message)) }
    fn code(s: &str, cte: bool) -> Option<u32> { check(s, cte).map(|e| e.code) }

    #[test]
    fn legacy_upstream_fixtures() {
        assert_eq!(msg("{ }", false).unwrap(), "1083 LWC1083: Error parsing template expression: Invalid expression { } - Unexpected token (1:2)");
        assert_eq!(msg("{ (foo }", false).unwrap(), "1083 LWC1083: Error parsing template expression: Invalid expression { (foo } - Unexpected token (1:7)");
        assert_eq!(msg("{ ((foo.bar) }", false).unwrap(), "1083 LWC1083: Error parsing template expression: Invalid expression { ((foo.bar) } - Unexpected token (1:13)");
        assert_eq!(msg("{ foo) }", false).unwrap(), "1083 Invalid expression { foo) } - LWC1083: Error parsing template expression: Unexpected end of expression");
        assert_eq!(msg("{class}", false).unwrap(), "1083 LWC1083: Error parsing template expression: Invalid expression {class} - Unexpected token (1:6)");
        assert_eq!(msg("{foo;title}", false).unwrap(), "1074 Invalid expression {foo;title} - LWC1074: Multiple expressions found");
        assert_eq!(msg("{   foo;      }", false).unwrap(), "1074 Invalid expression {   foo;      } - LWC1074: Multiple expressions found");
        assert_eq!(msg("{this.title}", false).unwrap(), "1060 Invalid expression {this.title} - LWC1060: Template expression doesn't allow ThisExpression");
        assert_eq!(msg("{ Math.random() }", false).unwrap(), "1060 Invalid expression { Math.random() } - LWC1060: Template expression doesn't allow CallExpression");
        assert_eq!(msg("{classNames[0]}", false).unwrap(), "1038 Invalid expression {classNames[0]} - LWC1038: Template expression doesn't allow computed property access");
        for ok in ["{ (foo) }", "{ ((foo)) }", "{ ((foo.bar)) }", "{ ((foo).bar) }", "{  foo    }", "{a.b.c}"] { assert_eq!(msg(ok, false), None, "{ok}"); }
        assert_eq!(code("{Component as default}", false), Some(1083));
    }

    #[test]
    fn api_version_and_unquoted() {
        let old = Opts { complex: true, computed_ok: false, api_version: 59, unquoted: false };
        assert_eq!(check_opts("{foo()}", &old).unwrap().message, "Invalid expression {foo()} - LWC1210: Template expression doesn't allow CallExpression. The current component API version (59) is insufficient and must be increased to at least 66 for this type of expression.");
        assert_eq!(check_opts("{val[state.foo]}", &old).unwrap().code, 1209);
        let unq = Opts { unquoted: true, ..o(true) };
        assert_eq!(check_opts("{bar()}", &unq).unwrap().message, "Invalid expression {bar()} - LWC1208: Template expression doesn't allow CallExpression unless the expression is surrounded by quotes: \"{bar()}\"");
        assert_eq!(check_opts("{list[0].name}", &unq).unwrap().code, 1207);
        assert_eq!(check_opts("{val[state.foo]}", &Opts { computed_ok: true, ..o(false) }), None);
    }

    #[test]
    fn complex_upstream_fixtures() {
        let c = |s: &str| check_opts(s, &o(true)).map(|e| e.message);
        assert_eq!(c("{async () => doSomething()}").unwrap(), "Invalid expression {async () => doSomething()} - LWC1194: Arrow functions in template expressions cannot be async.");
        assert_eq!(code("{() => { doSomething(); }}", true), Some(1193));
        assert_eq!(c("{myField = 'foo'}").unwrap(), "Invalid expression {myField = 'foo'} - LWC1191: Field mutations are only permitted within arrow functions.");
        assert_eq!(code("{foo++}", true), Some(1191));
        assert_eq!(code("{count += 1}", true), Some(1191));
        assert_eq!(code("{() => count += 1}", true), None);
        assert_eq!(c("{await foo()}").unwrap(), "Invalid expression {await foo()} - LWC1196: Use of await expressions is disallowed within template expressions.");
        assert_eq!(code("{transformBigInt(1n)}", true), Some(1196));
        assert_eq!(c("{\n    class Bar {\n        method() {}\n    }\n}").map(|m| m.ends_with("Use of classes is disallowed within template expressions.")), Some(true));
        assert_eq!(c("{one(), two(), three(), iAmTheCount()}").unwrap(), "Invalid expression {one(), two(), three(), iAmTheCount()} - LWC1196: Use of comma operators is disallowed within template expressions.");
        assert_eq!(code("{\n /* what do you think ? */\n someValue\n }", true), Some(1197));
        assert_eq!(c("{() => delete baz}").unwrap(), "Invalid expression {() => delete baz} - LWC1192: Use of the delete operator is prohibited within template expressions.");
        assert_eq!(c("{function Awesomeness (foo) { bar(foo); }}").unwrap(), "Invalid expression {function Awesomeness (foo) { bar(foo); }} - LWC1196: Use of function expressions is disallowed within template expressions.");
        assert_eq!(c("{foo &#60; bar}").unwrap(), "LWC1083: Error parsing template expression: Invalid expression {foo &#60; bar} - Unexpected character '6' (1:7)");
        assert_eq!(c("{() => call(").unwrap(), "LWC1083: Error parsing template expression: Invalid expression {() => call( - Unexpected token (1:12)");
        assert_eq!(c("{() => foo(\"}").unwrap(), "LWC1083: Error parsing template expression: Invalid expression {() => foo(\"} - Unterminated string constant (1:11)");
        assert_eq!(code("{() => import('foo').then(doThingWithFoo)}", true), Some(1196));
        assert_eq!(c("{() => doThing(import.meta.env.SSR)}").unwrap(), "LWC1083: Error parsing template expression: Invalid expression {() => doThing(import.meta.env.SSR)} - Cannot use 'import.meta' outside a module (1:15)");
        assert_eq!(c("{() => new Day()}").unwrap(), "Invalid expression {() => new Day()} - LWC1196: Use of object instantiation is disallowed within template expressions.");
        assert_eq!(code("{/wannabe/g}", true), Some(1196));
        assert_eq!(c("{super('duper')}").unwrap(), "LWC1083: Error parsing template expression: Invalid expression {super('duper')} - 'super' keyword outside a method (1:1)");
        assert_eq!(c("{this.bar}").unwrap(), "Invalid expression {this.bar} - LWC1196: Use of `this` is disallowed within template expressions.");
        assert_eq!(c("{() => throw 'oh no!'}").unwrap(), "LWC1083: Error parsing template expression: Invalid expression {() => throw 'oh no!'} - Unexpected token (1:7)");
        assert_eq!(c("{yield bar}").unwrap(), "Invalid expression {yield bar} - LWC1083: Error parsing template expression: expression must end with curly brace.");
        assert_eq!(c("{foo())}").unwrap(), "Invalid expression {foo())} - LWC1083: Error parsing template expression: expression must have balanced parentheses.");
        for ok in ["{foo(bar, baz)}", "{a ? b : c}", "{a in b}", "{tag`x`}", "{typeof a}", "{(foo.bar)}", "{((foo ?? bar)())}",
                   "{(foo ?? bar).baz}", "{x => x.y}", "{() => foo.bar = 1}", "{a / b}", "{[1, 2]}", "{({a: 1, b: 2})}", "{a?.b}", "{-1}"] {
            assert_eq!(c(ok), None, "{ok}");
        }
    }
}
