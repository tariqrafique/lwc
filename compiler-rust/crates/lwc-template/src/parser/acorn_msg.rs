//! Acorn's SyntaxError text for an expression oxc already rejected (AB-D0004 S6).
//!
//! Validity stays with oxc. This module only picks the message and the byte acorn would point at:
//! acorn 8's templates ("Unexpected token", "Unexpected character 'x'", "Unterminated string
//! constant", "Identifier directly after number", the `new.target` / `import.meta` sentences) and
//! acorn's position (start of the offending token). ES2020 legacy mode rejects syntax acorn's
//! `ecmaVersion: 2020` rejects and oxc (ESNext) accepts: `#`, numeric separators, logical assignment.

/// Acorn SyntaxError: body-relative byte and message, without the `(line:col)` suffix.
pub(crate) struct Stop {
    pub at: usize,
    pub msg: String,
}

impl Stop {
    fn new(at: usize, msg: impl Into<String>) -> Self {
        Stop {
            at,
            msg: msg.into(),
        }
    }
}

/// Legacy (`ecmaVersion: 2020`) syntax oxc accepts. `None` means oxc's parse stands.
pub(crate) fn es2020_prescan(body: &str) -> Option<Stop> {
    let b = body.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => i = skip_string(b, i),
            b'`' => i = skip_template(b, i),
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block_comment(b, i),
            b'#' => return Some(Stop::new(i, "Unexpected character '#'")),
            b'&' if b.get(i + 1) == Some(&b'&') && b.get(i + 2) == Some(&b'=') => {
                return Some(Stop::new(i + 2, "Unexpected token"));
            }
            b'|' if b.get(i + 1) == Some(&b'|') && b.get(i + 2) == Some(&b'=') => {
                return Some(Stop::new(i + 2, "Unexpected token"));
            }
            b'?' if b.get(i + 1) == Some(&b'?') && b.get(i + 2) == Some(&b'=') => {
                return Some(Stop::new(i + 2, "Unexpected token"));
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_' || b[i] == b'.') {
                    i += 1;
                }
                if let Some(u) = body[start..i].find('_') {
                    return Some(Stop::new(start + u, "Identifier directly after number"));
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// A SyntaxError acorn raises at tokenize time, before a prefix expression can be accepted.
/// `next` is the first character of the full source after `body` (the closing `}` when the body
/// was sliced out of `{…}`).
pub(crate) fn hard_syntax(
    body: &str,
    at: usize,
    oxc_msg: &str,
    next: Option<char>,
) -> Option<Stop> {
    if oxc_msg == "Unexpected new.target expression" {
        return Some(Stop::new(
            at,
            "'new.target' can only be used in functions and class static block",
        ));
    }
    if oxc_msg.starts_with("Unexpected import.meta") {
        return Some(Stop::new(at, "Cannot use 'import.meta' outside a module"));
    }
    if let Some(ch) = oxc_msg
        .strip_prefix("Invalid Character `")
        .and_then(|s| s.strip_suffix('`'))
    {
        return Some(Stop::new(at, format!("Unexpected character '{ch}'")));
    }
    if oxc_msg == "Invalid characters after number" || oxc_msg.starts_with("Invalid Number") {
        return Some(Stop::new(at, "Identifier directly after number"));
    }
    if oxc_msg.starts_with("Expected `in`") && body.starts_with('#') {
        return Some(Stop::new(0, "Unexpected token"));
    }
    if let Some(stop) = unterminated_at(body, at) {
        return Some(stop);
    }
    if oxc_msg == "Unexpected end of file"
        && body.as_bytes().last() == Some(&b'#')
        && let Some(c) = next
    {
        return Some(Stop::new(body.len(), format!("Unexpected character '{c}'")));
    }
    // `@` is not a token for acorn here; oxc reports it as "Unexpected token".
    if body.as_bytes().get(at) == Some(&b'@') {
        return Some(Stop::new(at, "Unexpected character '@'"));
    }
    None
}

/// `yield` / legacy `await` / `Identifier as` — acorn parses the identifier and stops.
/// The returned end is the body byte where that identifier ends.
pub(crate) fn ident_recovery(body: &str, at: usize, oxc_msg: &str, es2020: bool) -> Option<usize> {
    let rest = body.get(at..)?;
    if oxc_msg.contains("yield") && starts_word(rest, "yield") {
        return Some(at + "yield".len());
    }
    if es2020 && oxc_msg.contains("await") && starts_word(rest, "await") {
        return Some(at + "await".len());
    }
    if oxc_msg.contains("Type assertion") {
        let end = ident_len(body.trim_start());
        let lead = body.len() - body.trim_start().len();
        if end > 0 {
            return Some(lead + end);
        }
    }
    None
}

/// A `//` or `/*` begins before `limit` (body bytes), outside a string or template.
pub(crate) fn comment_before(body: &str, limit: usize) -> bool {
    let b = body.as_bytes();
    let limit = limit.min(b.len());
    let mut i = 0;
    while i + 1 < limit {
        match b[i] {
            b'"' | b'\'' => {
                let n = skip_string(b, i);
                if n > limit {
                    return false;
                }
                i = n;
            }
            b'`' => {
                let n = skip_template(b, i);
                if n > limit {
                    return false;
                }
                i = n;
            }
            b'/' if b[i + 1] == b'/' || b[i + 1] == b'*' => return true,
            _ => i += 1,
        }
    }
    false
}

fn starts_word(rest: &str, word: &str) -> bool {
    rest.starts_with(word) && !rest[word.len()..].starts_with(|c: char| ident_char(c))
}

fn ident_len(s: &str) -> usize {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if ident_start(c) => {}
        _ => return 0,
    }
    let mut n = s.chars().next().map(|c| c.len_utf8()).unwrap_or(0);
    for c in s[n..].chars() {
        if !ident_char(c) {
            break;
        }
        n += c.len_utf8();
    }
    n
}

fn ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}
fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn unterminated_at(body: &str, at: usize) -> Option<Stop> {
    let b = body.as_bytes();
    if at >= b.len() {
        return None;
    }
    match b[at] {
        b'"' | b'\'' if !string_closes(b, at) => {
            Some(Stop::new(at, "Unterminated string constant"))
        }
        b'`' if !template_closes(b, at) => Some(Stop::new(at, "Unterminated template")),
        _ => None,
    }
}

fn string_closes(b: &[u8], i: usize) -> bool {
    let q = b[i];
    let mut j = i + 1;
    while j < b.len() {
        if b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == b'\n' {
            return false;
        }
        if b[j] == q {
            return true;
        }
        j += 1;
    }
    false
}

fn template_closes(b: &[u8], i: usize) -> bool {
    skip_template(b, i) <= b.len() && i < b.len() && scan_template_closed(b, i)
}

fn scan_template_closed(b: &[u8], mut i: usize) -> bool {
    i += 1;
    let mut depth = 0i32;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'`' if depth == 0 => return true,
            b'$' if depth == 0 && b.get(i + 1) == Some(&b'{') => {
                depth = 1;
                i += 2;
            }
            b'{' if depth > 0 => {
                depth += 1;
                i += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    false
}

fn skip_string(b: &[u8], i: usize) -> usize {
    let q = b[i];
    let mut j = i + 1;
    while j < b.len() {
        if b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == q || b[j] == b'\n' {
            return j + 1;
        }
        j += 1;
    }
    b.len()
}

fn skip_template(b: &[u8], mut i: usize) -> usize {
    i += 1;
    let mut depth = 0i32;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'`' if depth == 0 => return i + 1,
            b'$' if depth == 0 && b.get(i + 1) == Some(&b'{') => {
                depth = 1;
                i += 2;
            }
            b'{' if depth > 0 => {
                depth += 1;
                i += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    b.len()
}

fn skip_block_comment(b: &[u8], mut i: usize) -> usize {
    i += 2;
    while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
        i += 1;
    }
    (i + 2).min(b.len())
}
