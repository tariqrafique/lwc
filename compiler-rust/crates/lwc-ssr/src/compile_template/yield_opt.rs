//! Port of `@lwc/ssr-compiler` `optimizeAdjacentYieldStmts` (`compile-template/shared.ts`, b9e8007).
//!
//! The pass runs on the statements the oracle passes to that function:
//! the template body, and the bodies built by legacy `if`, `lwc:if`, `for:each`,
//! `for:of`, and slotted content. An `if` whose consequent and alternate each
//! reduce to one non-delegate yield becomes `yield test ? cons : alt` (a missing
//! alternate is `''`). Adjacent non-delegate yields become one yield of a `+`
//! chain, and a string literal on the right spine is concatenated at compile time.
//! `yield*` is not merged. When an `if` does not collapse, the attempt is discarded
//! and the statements that were already optimized at construction are kept.
use super::js_str;
use lwc_shared::SsrMode;

#[derive(Clone, Debug)]
enum Expr {
    /// Decoded JS string literal. Printed with [`js_str`].
    Str(String),
    /// Any other yield argument, already valid to splice into source.
    Atom(String),
    Add(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug)]
enum Stmt {
    Line(String),
    /// `{ ... }` block. shared.ts does not walk arbitrary blocks, so the lines stay as written.
    Raw(Vec<String>),
    Yield {
        expr: Expr,
        delegate: bool,
    },
    For {
        header: String,
        body: Vec<Stmt>,
    },
    If {
        test: String,
        cons: Vec<Stmt>,
        alt: Alt,
    },
}

#[derive(Clone, Debug)]
enum Alt {
    None,
    Block(Vec<Stmt>),
    ElseIf(Box<Stmt>),
}

pub(crate) fn optimize_yields(lines: Vec<String>, mode: SsrMode) -> Vec<String> {
    let mut out = Vec::new();
    print_stmts(&optimize(parse(&lines)), mode, &mut out);
    out
}

fn optimize(stmts: Vec<Stmt>) -> Vec<Stmt> {
    let mut result = Vec::new();
    for stmt in stmts {
        let stmt = match stmt {
            Stmt::For { header, body } => Stmt::For {
                header,
                body: optimize(body),
            },
            Stmt::If { test, cons, alt } => match try_collapse(&test, &cons, &alt) {
                Some(yielded) => yielded,
                None => Stmt::If { test, cons, alt },
            },
            other => other,
        };
        push_merged(&mut result, stmt);
    }
    result
}

/// shared.ts `optimizeIfStatement`. `None` discards the optimized branches.
fn try_collapse(test: &str, cons: &[Stmt], alt: &Alt) -> Option<Stmt> {
    let cons_expr = single_yield(&optimize(cons.to_vec()))?;
    let alt_expr = match alt {
        Alt::None => Expr::Str(String::new()),
        Alt::Block(b) => single_yield(&optimize(b.clone()))?,
        Alt::ElseIf(_) => return None,
    };
    let expr = Expr::Atom(format!(
        "({test} ? {} : {})",
        print_expr(&cons_expr),
        print_expr(&alt_expr)
    ));
    Some(Stmt::Yield {
        expr,
        delegate: false,
    })
}

fn single_yield(stmts: &[Stmt]) -> Option<Expr> {
    match stmts {
        [
            Stmt::Yield {
                expr,
                delegate: false,
            },
        ] => Some(expr.clone()),
        _ => None,
    }
}

fn push_merged(result: &mut Vec<Stmt>, stmt: Stmt) {
    let Stmt::Yield {
        expr,
        delegate: false,
    } = stmt
    else {
        result.push(stmt);
        return;
    };
    // bare `yield` and `yield ""` amount to nothing
    if is_empty(&expr) {
        return;
    }
    if let Some(Stmt::Yield {
        expr: prev,
        delegate: false,
    }) = result.last_mut()
    {
        merge_into(prev, expr);
        return;
    }
    result.push(Stmt::Yield {
        expr,
        delegate: false,
    });
}

fn is_empty(expr: &Expr) -> bool {
    matches!(expr, Expr::Str(s) if s.is_empty())
}

fn merge_into(left: &mut Expr, right: Expr) {
    let folded = match &right {
        Expr::Str(extra) => append_to_right_string(left, extra),
        Expr::Atom(_) | Expr::Add(_, _) => false,
    };
    if folded {
        return;
    }
    let prev = std::mem::replace(left, Expr::Str(String::new()));
    *left = Expr::Add(Box::new(prev), Box::new(right));
}

/// shared.ts walks the right spine of `+` and, when it is a string literal, appends in place.
fn append_to_right_string(expr: &mut Expr, extra: &str) -> bool {
    match expr {
        Expr::Add(_, right) => append_to_right_string(right, extra),
        Expr::Str(s) => {
            s.push_str(extra);
            true
        }
        Expr::Atom(_) => false,
    }
}

fn print_stmts(stmts: &[Stmt], mode: SsrMode, out: &mut Vec<String>) {
    for stmt in stmts {
        print_stmt(stmt, mode, out);
    }
}

fn print_stmt(stmt: &Stmt, mode: SsrMode, out: &mut Vec<String>) {
    match stmt {
        Stmt::Line(s) => out.push(s.clone()),
        Stmt::Raw(lines) => out.extend(lines.iter().cloned()),
        Stmt::Yield { expr, delegate } => out.push(yield_line(expr, *delegate, mode)),
        Stmt::For { header, body } => {
            out.push(header.clone());
            print_stmts(body, mode, out);
            out.push("}".into());
        }
        Stmt::If { test, cons, alt } => {
            out.push(format!("if ({test}) {{"));
            print_stmts(cons, mode, out);
            match alt {
                Alt::None => out.push("}".into()),
                Alt::Block(b) => {
                    out.push("} else {".into());
                    print_stmts(b, mode, out);
                    out.push("}".into());
                }
                Alt::ElseIf(inner) => {
                    let mut alt_lines = Vec::new();
                    print_stmt(inner, mode, &mut alt_lines);
                    if let Some(first) = alt_lines.first_mut() {
                        *first = format!("}} else {first}");
                    }
                    out.extend(alt_lines);
                }
            }
        }
    }
}

fn print_expr(expr: &Expr) -> String {
    match expr {
        Expr::Str(s) => js_str(s),
        Expr::Atom(s) => s.clone(),
        Expr::Add(left, right) => format!("{} + {}", print_expr(left), print_add_right(right)),
    }
}

fn print_add_right(expr: &Expr) -> String {
    match expr {
        Expr::Add(_, _) => format!("({})", print_expr(expr)),
        _ => print_expr(expr),
    }
}

fn yield_line(expr: &Expr, delegate: bool, mode: SsrMode) -> String {
    if delegate {
        return format!("yield* {};", print_expr(expr));
    }
    let printed = print_expr(expr);
    match mode {
        // `await` binds tighter than `+`, so a merged argument is parenthesized
        // (`__lwcYield += await (a + b)`), matching transmogrify's AwaitExpression.
        SsrMode::Async if matches!(expr, Expr::Add(_, _)) => {
            format!("__lwcYield += await ({printed});")
        }
        SsrMode::Sync => format!("__lwcYield += {printed};"),
        SsrMode::Async => format!("__lwcYield += await {printed};"),
        SsrMode::AsyncYield => format!("yield {printed};"),
    }
}

fn expr_from_src(src: &str) -> Expr {
    let src = src.trim();
    match parse_js_str_literal(src) {
        Some(value) => Expr::Str(value),
        None => Expr::Atom(src.to_string()),
    }
}

fn parse(lines: &[String]) -> Vec<Stmt> {
    let mut i = 0;
    parse_seq(lines, &mut i)
}

fn parse_seq(lines: &[String], i: &mut usize) -> Vec<Stmt> {
    let mut out = Vec::new();
    while *i < lines.len() {
        let line = lines[*i].as_str();
        if line == "}" || line == "} else {" || line.starts_with("} else if (") {
            break;
        }
        out.push(parse_one(lines, i));
    }
    out
}

fn parse_one(lines: &[String], i: &mut usize) -> Stmt {
    let line = lines[*i].clone();
    if let Some(y) = parse_yield_stmt(&line) {
        *i += 1;
        return y;
    }
    if for_header(&line) {
        *i += 1;
        let body = parse_seq(lines, i);
        if *i < lines.len() && lines[*i] == "}" {
            *i += 1;
        }
        return Stmt::For { header: line, body };
    }
    if let Some(iff) = (line.starts_with("if (") || line.starts_with("} else if ("))
        .then(|| parse_if(lines, i))
        .flatten()
    {
        return iff;
    }
    if line == "{" {
        let mut raw = vec![line];
        *i += 1;
        let mut depth = 1i32;
        while *i < lines.len() && depth > 0 {
            let next = lines[*i].clone();
            depth += brace_delta(&next);
            raw.push(next);
            *i += 1;
        }
        return Stmt::Raw(raw);
    }
    *i += 1;
    Stmt::Line(line)
}

fn parse_if(lines: &[String], i: &mut usize) -> Option<Stmt> {
    let raw = lines.get(*i)?.clone();
    let line = raw.strip_prefix("} else ").unwrap_or(raw.as_str());
    let (test, kind) = split_if(line)?;
    *i += 1;
    Some(match kind {
        IfKind::OneLine(body) => match parse_yield_stmt(&body) {
            Some(y) => Stmt::If {
                test,
                cons: vec![y],
                alt: Alt::None,
            },
            None => Stmt::Line(raw),
        },
        IfKind::Header => {
            let cons = parse_seq(lines, i);
            let alt = if *i < lines.len() && lines[*i] == "}" {
                *i += 1;
                Alt::None
            } else if *i < lines.len() && lines[*i] == "} else {" {
                *i += 1;
                let block = parse_seq(lines, i);
                if *i < lines.len() && lines[*i] == "}" {
                    *i += 1;
                }
                Alt::Block(block)
            } else if *i < lines.len() && lines[*i].starts_with("} else if (") {
                Alt::ElseIf(Box::new(parse_if(lines, i)?))
            } else {
                Alt::None
            };
            Stmt::If { test, cons, alt }
        }
    })
}

enum IfKind {
    Header,
    OneLine(String),
}

fn split_if(line: &str) -> Option<(String, IfKind)> {
    let rest = line.strip_prefix("if ")?;
    let (test, after) = take_paren_group(rest)?;
    let after = after.trim_start().strip_prefix('{')?;
    if after.trim().is_empty() {
        return Some((test, IfKind::Header));
    }
    let body = after.trim().strip_suffix('}')?.trim().to_string();
    Some((test, IfKind::OneLine(body)))
}

fn for_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("for ") else {
        return false;
    };
    let Some((_, after)) = take_paren_group(rest) else {
        return false;
    };
    after.trim() == "{"
}

fn parse_yield_stmt(stmt: &str) -> Option<Stmt> {
    let s = stmt.trim().strip_suffix(';').unwrap_or(stmt.trim()).trim();
    // `yield*` before `yield`, and `+= await` before `+=`: the shorter prefix also matches the longer one.
    let (expr, delegate) = s
        .strip_prefix("yield* ")
        .map(|e| (e, true))
        .or_else(|| s.strip_prefix("yield ").map(|e| (e, false)))
        .or_else(|| s.strip_prefix("__lwcYield += await ").map(|e| (e, false)))
        .or_else(|| s.strip_prefix("__lwcYield += ").map(|e| (e, false)))?;
    Some(Stmt::Yield {
        expr: expr_from_src(expr),
        delegate,
    })
}

/// A complete `'...'` / `"..."` literal, decoded. Anything else is not a string literal
/// (shared.ts only folds `Literal` nodes, not template literals).
fn parse_js_str_literal(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if b.len() < 2 || (b[0] != b'\'' && b[0] != b'"') || b[b.len() - 1] != b[0] {
        return None;
    }
    let mut out = String::new();
    let mut i = 1usize;
    let end = b.len() - 1;
    while i < end {
        if b[i] == b'\\' {
            i += 1;
            if i >= end {
                return None;
            }
            match b[i] {
                b'\'' => out.push('\''),
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    if i + 4 >= end {
                        return None;
                    }
                    let hex = std::str::from_utf8(&b[i + 1..i + 5]).ok()?;
                    out.push(char::from_u32(u32::from_str_radix(hex, 16).ok()?)?);
                    i += 4;
                }
                _ => return None,
            }
            i += 1;
            continue;
        }
        if b[i] == b[0] {
            return None;
        }
        let ch = s[i..].chars().next()?;
        out.push(ch);
        i += ch.len_utf8();
    }
    Some(out)
}

struct Scan<'a> {
    s: &'a str,
    i: usize,
}

impl Scan<'_> {
    fn rest(&self) -> &str {
        &self.s[self.i..]
    }
    fn bump(&mut self) -> Option<char> {
        let ch = self.rest().chars().next()?;
        self.i += ch.len_utf8();
        Some(ch)
    }
    fn skip_string(&mut self, quote: char) -> bool {
        while let Some(ch) = self.bump() {
            if ch == '\\' {
                self.bump();
                continue;
            }
            if ch == quote {
                return true;
            }
        }
        false
    }
    fn skip_template(&mut self) -> bool {
        while let Some(ch) = self.bump() {
            if ch == '\\' {
                self.bump();
                continue;
            }
            if ch == '`' {
                return true;
            }
            if ch == '$' && self.rest().starts_with('{') {
                self.bump();
                if !self.skip_braces() {
                    return false;
                }
            }
        }
        false
    }
    fn skip_braces(&mut self) -> bool {
        let mut depth = 1i32;
        while depth > 0 {
            match self.bump() {
                None => return false,
                Some('\'' | '"') => {
                    let q = self.s[self.i - 1..].chars().next().unwrap_or('"');
                    if !self.skip_string(q) {
                        return false;
                    }
                }
                Some('`') => {
                    if !self.skip_template() {
                        return false;
                    }
                }
                Some('{') => depth += 1,
                Some('}') => depth -= 1,
                Some(_) => {}
            }
        }
        true
    }
}

fn take_paren_group(s: &str) -> Option<(String, &str)> {
    let t = s.trim_start();
    let lead = s.len() - t.len();
    if !t.starts_with('(') {
        return None;
    }
    let mut sc = Scan { s, i: lead + 1 };
    let mut depth = 1i32;
    while depth > 0 {
        match sc.bump()? {
            '\'' | '"' => {
                let q = s[sc.i - 1..].chars().next()?;
                if !sc.skip_string(q) {
                    return None;
                }
            }
            '`' => {
                if !sc.skip_template() {
                    return None;
                }
            }
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
    }
    let test = s[lead + 1..sc.i - 1].to_string();
    Some((test, &s[sc.i..]))
}

fn brace_delta(line: &str) -> i32 {
    let mut sc = Scan { s: line, i: 0 };
    let mut d = 0i32;
    while let Some(ch) = sc.bump() {
        match ch {
            '\'' | '"' => {
                if !sc.skip_string(ch) {
                    break;
                }
            }
            '`' => {
                if !sc.skip_template() {
                    break;
                }
            }
            '{' => d += 1,
            '}' => d -= 1,
            _ => {}
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn y(lines: &[&str]) -> Vec<String> {
        optimize_yields(
            lines.iter().map(|s| (*s).to_string()).collect(),
            SsrMode::AsyncYield,
        )
    }

    #[test]
    fn adjacent_static_yields_fold_into_one() {
        // shared.ts: a string literal on the right spine of `+` is concatenated now.
        assert_eq!(
            y(&["yield '<div';", "yield '>';", "yield '</div>';"]),
            vec!["yield '<div></div>';"]
        );
        assert_eq!(
            y(&["yield '</div>';", "yield '<div';", "yield ' class=\"x\"';"]),
            vec!["yield '</div><div class=\"x\"';"]
        );
    }

    #[test]
    fn empty_literal_yield_is_dropped_and_star_breaks_the_chain() {
        assert_eq!(
            y(&["yield '<a';", "yield '';", "yield '</a>';"]),
            vec!["yield '<a</a>';"]
        );
        assert_eq!(
            y(&["yield '<a';", "yield* child();", "yield '</a>';"]),
            vec![
                "yield '<a';".to_string(),
                "yield* child();".into(),
                "yield '</a>';".into()
            ]
        );
    }

    #[test]
    fn single_yield_if_becomes_parenthesized_ternary_and_adjacent_ifs_merge() {
        assert_eq!(
            y(&["if (instance.show) {", "yield 'x';", "}"]),
            vec!["yield (instance.show ? 'x' : '');"]
        );
        assert_eq!(
            y(&[
                "if (instance.a) {",
                "yield 'A';",
                "}",
                "if (instance.b) {",
                "yield 'B';",
                "}",
            ]),
            vec!["yield (instance.a ? 'A' : '') + (instance.b ? 'B' : '');"]
        );
    }

    #[test]
    fn else_if_does_not_collapse_and_a_failed_if_discards_the_inner_merge() {
        // alternate is an IfStatement, so optimizeSingleStatement returns null and the
        // original if is kept. Bodies that were not optimized at construction stay unmerged.
        let kept = y(&[
            "if (isLightDom) {",
            "const x = 1;",
            "yield 'a';",
            "yield 'b';",
            "} else {",
            "yield '<slot';",
            "yield '>';",
            "yield '</slot>';",
            "}",
        ]);
        assert!(kept.iter().any(|l| l == "yield '<slot';"), "{kept:?}");
        assert!(kept.iter().any(|l| l == "yield '>';"), "{kept:?}");
        assert!(kept.iter().any(|l| l == "yield '</slot>';"), "{kept:?}");
        assert!(!kept.iter().any(|l| l.contains("<slot>")), "{kept:?}");
    }

    #[test]
    fn opaque_block_and_for_body() {
        let out = y(&[
            "yield '<div';",
            "{ const suffix = ''; yield ' class'; yield '=\"x\"'; }",
            "yield '>';",
        ]);
        assert_eq!(out[0], "yield '<div';");
        assert!(out[1].starts_with("{ const suffix"), "{out:?}");
        assert_eq!(out.last().map(String::as_str), Some("yield '>';"));
        let out = y(&[
            "yield '<ul>';",
            "for (let [i, item] of Object.entries(instance.list ?? {})) {",
            "yield '<li';",
            "yield '>';",
            "}",
            "yield '</ul>';",
        ]);
        assert_eq!(
            out,
            vec![
                "yield '<ul>';".to_string(),
                "for (let [i, item] of Object.entries(instance.list ?? {})) {".into(),
                "yield '<li>';".into(),
                "}".into(),
                "yield '</ul>';".into(),
            ]
        );
    }
}
