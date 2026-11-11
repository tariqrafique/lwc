//! Versioned rewrite rule table (`rules/rt-1.toml`).
//!
//! AB-D0003 (6): loading is fail-closed. A rule whose `arch_review` is `PENDING` is loaded (so Arch can review its
//! tests) but is never applied. A rule whose `arch_review` is an `AB-…` id is applied; that field is a reviewed code
//! change, not a reviews.tsv lookup. The applied-set identity (`applied_table_version`) is `rt-0` while nothing is
//! applied, and `rt-1:<tagged>+…` (sorted) once any listed rule is. The ASI-only set on 681aba0 used the
//! any-applied name `rt-1`; both rules applied must not reuse that string, so the ratchet sees the change.
//!
//! These two rules are not AST→AST passes. They run inside the rewrite-leg token-spelling check, and only after both
//! sides parse, the printed texts match, and `Program::content_eq` holds (see `compare`).
use crate::Tok;
use oxc::ast::ast::{
    ArrayAssignmentTarget, ArrayExpression, ArrayExpressionElement, ArrayPattern, CallExpression,
    ExportNamedDeclaration, FormalParameters, ImportDeclaration, NewExpression,
    ObjectAssignmentTarget, ObjectExpression, ObjectPattern, Program,
};
use oxc::ast_visit::walk::{
    walk_array_assignment_target, walk_array_expression, walk_array_pattern, walk_call_expression,
    walk_export_named_declaration, walk_formal_parameters, walk_import_declaration,
    walk_new_expression, walk_object_assignment_target, walk_object_expression,
    walk_object_pattern,
};
use oxc::ast_visit::Visit;
use oxc::parser::Kind;
use oxc::span::{GetSpan, Span};
use serde_json::{json, Value};
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct RuleMeta {
    pub id: String,
    pub rev: u32,
    pub decision: String,
    pub arch_review: String,
    pub kind: String,
}

impl RuleMeta {
    pub fn tagged(&self) -> String {
        format!("{}@{}", self.id, self.rev)
    }
    pub fn pending(&self) -> bool {
        self.arch_review == "PENDING"
    }
    /// Applied only when the table records an Arch-review id (not PENDING).
    pub fn is_applied(&self) -> bool {
        self.arch_review.starts_with("AB-")
    }
}

const RAW: &str = include_str!("../rules/rt-1.toml");

fn parse_table() -> (String, Vec<RuleMeta>) {
    let version = RAW
        .lines()
        .find_map(|l| l.strip_prefix("version = \"")?.strip_suffix('"'))
        .expect("rt-N.toml: version")
        .to_string();
    let mut rules = Vec::new();
    let mut cur = RuleMeta {
        id: String::new(),
        rev: 0,
        decision: String::new(),
        arch_review: String::new(),
        kind: String::new(),
    };
    let mut in_rule = false;
    let take = |s: &str, k: &str| {
        s.trim()
            .strip_prefix(k)?
            .trim()
            .strip_prefix('=')?
            .trim()
            .strip_prefix('"')?
            .strip_suffix('"')
            .map(str::to_string)
    };
    for line in RAW.lines() {
        if line.trim() == "[[rule]]" {
            if in_rule {
                rules.push(cur.clone());
            }
            in_rule = true;
            cur = RuleMeta {
                id: String::new(),
                rev: 0,
                decision: String::new(),
                arch_review: String::new(),
                kind: String::new(),
            };
            continue;
        }
        if !in_rule {
            continue;
        }
        if let Some(v) = take(line, "id") {
            cur.id = v;
        } else if let Some(v) = take(line, "kind") {
            cur.kind = v;
        } else if let Some(v) = take(line, "decision") {
            cur.decision = v;
        } else if let Some(v) = take(line, "arch_review") {
            cur.arch_review = v;
        } else if let Some(rest) = line.trim().strip_prefix("rev") {
            cur.rev = rest
                .trim()
                .trim_start_matches('=')
                .trim()
                .parse()
                .expect("rev");
        }
    }
    if in_rule {
        rules.push(cur);
    }
    let markers = RAW.matches("[[rule]]").count();
    assert_eq!(rules.len(), markers, "rt-1.toml rule count");
    (version, rules)
}

pub fn table() -> &'static (String, Vec<RuleMeta>) {
    static T: OnceLock<(String, Vec<RuleMeta>)> = OnceLock::new();
    T.get_or_init(parse_table)
}

pub fn pending() -> Vec<String> {
    table()
        .1
        .iter()
        .filter(|r| r.pending())
        .map(RuleMeta::tagged)
        .collect()
}

/// Identity of the *applied* set. `rt-0` while every rule is PENDING. Otherwise `rt-1:` plus the sorted tagged
/// ids of applied rules (e.g. `rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1`). That string differs from the ASI-only
/// identity `rt-1` recorded on 681aba0, so a ratchet against that baseline is `baseline-rule-table-mismatch`.
pub fn applied_table_version() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| {
        let mut applied: Vec<String> = table()
            .1
            .iter()
            .filter(|r| r.is_applied())
            .map(RuleMeta::tagged)
            .collect();
        applied.sort();
        if applied.is_empty() {
            "rt-0".into()
        } else {
            format!("rt-1:{}", applied.join("+"))
        }
    })
    .as_str()
}

pub(crate) fn tagged_starting(prefix: &str) -> String {
    table()
        .1
        .iter()
        .find(|r| r.id.starts_with(prefix))
        .unwrap_or_else(|| panic!("rt-1.toml: missing {prefix}"))
        .tagged()
}

pub struct Enable {
    pub comma: bool,
    pub asi: bool,
    pub import_merge: bool,
    pub hoist: bool,
    pub arrow_paren: bool,
    pub numeric: bool,
    pub redundant_paren: bool,
    pub comment_star: bool,
}

fn applied(rules: &[RuleMeta], prefix: &str) -> bool {
    rules
        .iter()
        .any(|r| r.id.starts_with(prefix) && r.is_applied())
}

fn forced(rules: &[RuleMeta], prefix: &str, force: &[&str]) -> bool {
    rules
        .iter()
        .any(|r| r.id.starts_with(prefix) && force.iter().any(|x| *x == r.tagged()))
}

pub fn enabled() -> Enable {
    let rules = &table().1;
    Enable {
        comma: applied(rules, "R-TRAILING-COMMA"),
        asi: applied(rules, "R-ASI-SEMI"),
        import_merge: applied(rules, "R-IMPORT-MERGE"),
        hoist: applied(rules, "R-HOIST-DECL"),
        arrow_paren: applied(rules, "R-ARROW-PAREN"),
        numeric: applied(rules, "R-NUMERIC-SPELLING"),
        redundant_paren: applied(rules, "R-REDUNDANT-PAREN"),
        comment_star: applied(rules, "R-COMMENT-STAR-SPACE"),
    }
}

pub fn enable_forcing(force: &[&str]) -> Enable {
    let rules = &table().1;
    Enable {
        comma: forced(rules, "R-TRAILING-COMMA", force),
        asi: forced(rules, "R-ASI-SEMI", force),
        import_merge: forced(rules, "R-IMPORT-MERGE", force),
        hoist: forced(rules, "R-HOIST-DECL", force),
        arrow_paren: forced(rules, "R-ARROW-PAREN", force),
        numeric: forced(rules, "R-NUMERIC-SPELLING", force),
        redundant_paren: forced(rules, "R-REDUNDANT-PAREN", force),
        comment_star: forced(rules, "R-COMMENT-STAR-SPACE", force),
    }
}

/// A comma at `start` is a D-0258/D-0266 trailing comma when the innermost list-like parent is an allowed
/// context and the comma sits in that node's trailing region: after the last *element* and before the closer.
/// A hole is an `Elision` (ArrayExpression) or a `None` slot (ArrayPattern / ArrayAssignmentTarget). If the last
/// slot is a hole, there is no trailing region — that comma is the elision, not a trailing comma. Token neighbors
/// are not consulted.
struct ListParents {
    /// (node span, trailing region after the last real element — None if the last slot is a hole or the list is empty)
    nodes: Vec<(Span, Option<(u32, u32)>)>,
}

fn option_list_trailing<T: GetSpan>(
    elements: &[Option<T>],
    rest_end: Option<u32>,
    span: Span,
) -> Option<(u32, u32)> {
    if let Some(end) = rest_end {
        return Some((end, span.end));
    }
    match elements.last() {
        None => None,
        // Last slot is a hole (None/elision entry). Not a trailing comma.
        Some(None) => None,
        Some(Some(el)) => Some((el.span().end, span.end)),
    }
}

fn array_expr_trailing(it: &ArrayExpression<'_>) -> Option<(u32, u32)> {
    match it.elements.last() {
        None => None,
        Some(ArrayExpressionElement::Elision(_)) => None,
        Some(el) => Some((el.span().end, it.span.end)),
    }
}

impl ListParents {
    fn push(&mut self, span: Span, trailing: Option<(u32, u32)>) {
        self.nodes.push((span, trailing));
    }
    fn allows(&self, start: u32) -> bool {
        let mut best: Option<(u32, Option<(u32, u32)>)> = None;
        for &(span, trailing) in &self.nodes {
            if span.start <= start && start < span.end {
                let w = span.end - span.start;
                if best.map_or(true, |(bw, _)| w < bw) {
                    best = Some((w, trailing));
                }
            }
        }
        match best {
            Some((_, Some((lo, hi)))) => start >= lo && start < hi,
            _ => false,
        }
    }
}

impl<'a> Visit<'a> for ListParents {
    fn visit_array_expression(&mut self, it: &ArrayExpression<'a>) {
        self.push(it.span, array_expr_trailing(it));
        walk_array_expression(self, it);
    }
    fn visit_object_expression(&mut self, it: &ObjectExpression<'a>) {
        let last = it.properties.last().map(|p| p.span().end);
        self.push(it.span, last.map(|e| (e, it.span.end)));
        walk_object_expression(self, it);
    }
    fn visit_formal_parameters(&mut self, it: &FormalParameters<'a>) {
        let last = it
            .rest
            .as_ref()
            .map(|r| r.span.end)
            .or_else(|| it.items.last().map(|p| p.span.end));
        self.push(it.span, last.map(|e| (e, it.span.end)));
        walk_formal_parameters(self, it);
    }
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let last = it.arguments.last().map(|a| a.span().end);
        self.push(it.span, last.map(|e| (e, it.span.end)));
        walk_call_expression(self, it);
    }
    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        let last = it.arguments.last().map(|a| a.span().end);
        self.push(it.span, last.map(|e| (e, it.span.end)));
        walk_new_expression(self, it);
    }
    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let last = it
            .specifiers
            .as_ref()
            .and_then(|s| s.last())
            .map(|s| s.span().end);
        self.push(it.span, last.map(|e| (e, it.span.end)));
        walk_import_declaration(self, it);
    }
    fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'a>) {
        let last = it.specifiers.last().map(|s| s.span().end);
        self.push(it.span, last.map(|e| (e, it.span.end)));
        walk_export_named_declaration(self, it);
    }
    fn visit_array_pattern(&mut self, it: &ArrayPattern<'a>) {
        self.push(
            it.span,
            option_list_trailing(&it.elements, it.rest.as_ref().map(|r| r.span.end), it.span),
        );
        walk_array_pattern(self, it);
    }
    fn visit_object_pattern(&mut self, it: &ObjectPattern<'a>) {
        let last = it
            .rest
            .as_ref()
            .map(|r| r.span.end)
            .or_else(|| it.properties.last().map(|p| p.span().end));
        self.push(it.span, last.map(|e| (e, it.span.end)));
        walk_object_pattern(self, it);
    }
    fn visit_array_assignment_target(&mut self, it: &ArrayAssignmentTarget<'a>) {
        self.push(
            it.span,
            option_list_trailing(&it.elements, it.rest.as_ref().map(|r| r.span.end), it.span),
        );
        walk_array_assignment_target(self, it);
    }
    fn visit_object_assignment_target(&mut self, it: &ObjectAssignmentTarget<'a>) {
        let last = it
            .rest
            .as_ref()
            .map(|r| r.span.end)
            .or_else(|| it.properties.last().map(|p| p.span().end));
        self.push(it.span, last.map(|e| (e, it.span.end)));
        walk_object_assignment_target(self, it);
    }
}

pub(crate) fn trailing_comma_in_d0258_or_d0266_context(program: &Program<'_>, start: u32) -> bool {
    let mut p = ListParents { nodes: Vec::new() };
    p.visit_program(program);
    p.allows(start)
}

pub(crate) fn is_semicolon(toks: &[Tok], i: usize) -> bool {
    toks.get(i).is_some_and(|t| t.kind == Kind::Semicolon)
}

fn skip_trailing(toks: &[Tok], i: usize, program: &Program<'_>) -> bool {
    toks.get(i).is_some_and(|t| {
        t.kind == Kind::Comma && trailing_comma_in_d0258_or_d0266_context(program, t.start)
    })
}

/// Two-pointer comparison: `Tok::same`, plus skips allowed by the enabled D-0258 rules. `applied` collects the
/// `id@rev` of each rule that skipped at least one token, in first-seen order.
pub(crate) fn first_token_diff(
    a: &[Tok],
    b: &[Tok],
    applied: &mut Vec<String>,
    prog_a: &Program<'_>,
    prog_b: &Program<'_>,
) -> Option<Value> {
    first_token_diff_with(a, b, applied, &enabled(), prog_a, prog_b)
}

pub(crate) fn first_token_diff_forcing(
    a: &[Tok],
    b: &[Tok],
    applied: &mut Vec<String>,
    force: &[&str],
    prog_a: &Program<'_>,
    prog_b: &Program<'_>,
) -> Option<Value> {
    first_token_diff_with(a, b, applied, &enable_forcing(force), prog_a, prog_b)
}

fn first_token_diff_with(
    a: &[Tok],
    b: &[Tok],
    applied: &mut Vec<String>,
    en: &Enable,
    prog_a: &Program<'_>,
    prog_b: &Program<'_>,
) -> Option<Value> {
    let clip = |t: Option<&Tok>| t.map(|t| t.text.chars().take(200).collect::<String>());
    let note = |applied: &mut Vec<String>, id: String| {
        if !applied.iter().any(|x| x == &id) {
            applied.push(id);
        }
    };
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) if x.same(y) => {
                i += 1;
                j += 1;
            }
            (None, None) => return None,
            _ => {
                if en.comma && skip_trailing(a, i, prog_a) {
                    note(applied, tagged_starting("R-TRAILING-COMMA"));
                    i += 1;
                    continue;
                }
                if en.comma && skip_trailing(b, j, prog_b) {
                    note(applied, tagged_starting("R-TRAILING-COMMA"));
                    j += 1;
                    continue;
                }
                if en.asi && is_semicolon(a, i) {
                    note(applied, tagged_starting("R-ASI-SEMI"));
                    i += 1;
                    continue;
                }
                if en.asi && is_semicolon(b, j) {
                    note(applied, tagged_starting("R-ASI-SEMI"));
                    j += 1;
                    continue;
                }
                if en.arrow_paren {
                    if let Some((ni, nj)) = align_arrow_paren(a, i, b, j) {
                        note(applied, tagged_starting("R-ARROW-PAREN"));
                        i = ni;
                        j = nj;
                        continue;
                    }
                }
                if en.numeric && same_number(a.get(i), b.get(j)) {
                    note(applied, tagged_starting("R-NUMERIC-SPELLING"));
                    i += 1;
                    j += 1;
                    continue;
                }
                return Some(
                    json!({"coords": "tokens", "index": i.max(j), "oracle": clip(a.get(i)), "rust": clip(b.get(j))}),
                );
            }
        }
    }
}

/// `(ident) =>` and `ident =>` are the same arrow. The paren side's index moves to the `=>`,
/// and the bare side's index moves to its `=>`. A parameter list, default, rest, or pattern is not this shape.
fn align_arrow_paren(a: &[Tok], i: usize, b: &[Tok], j: usize) -> Option<(usize, usize)> {
    fn name_token(t: &Tok) -> bool {
        t.text
            .starts_with(|c: char| c == '_' || c == '$' || c.is_ascii_alphabetic())
    }
    fn wrapped(toks: &[Tok], i: usize) -> bool {
        matches!(
            (toks.get(i), toks.get(i + 1), toks.get(i + 2), toks.get(i + 3)),
            (Some(p), Some(id), Some(c), Some(ar))
                if p.kind == Kind::LParen && name_token(id) && c.kind == Kind::RParen && ar.kind == Kind::Arrow
        )
    }
    fn bare(toks: &[Tok], i: usize) -> bool {
        matches!((toks.get(i), toks.get(i + 1)), (Some(id), Some(ar)) if name_token(id) && ar.kind == Kind::Arrow)
    }
    if wrapped(a, i) && bare(b, j) && a[i + 1].text == b[j].text {
        return Some((i + 3, j + 1));
    }
    if wrapped(b, j) && bare(a, i) && b[j + 1].text == a[i].text {
        return Some((i + 1, j + 3));
    }
    None
}

/// Same finite number, including sign of zero. `1000` and `1e3` match. `1000` and `1001` do not.
/// Non-finite values do not match (`1e309` and `1e310` are both infinities).
fn same_number(x: Option<&Tok>, y: Option<&Tok>) -> bool {
    let (Some(x), Some(y)) = (x, y) else {
        return false;
    };
    if !is_decimal_number(x.kind) || !is_decimal_number(y.kind) || x.text == y.text {
        return false;
    }
    let (Ok(a), Ok(b)) = (parse_js_number(&x.text), parse_js_number(&y.text)) else {
        return false;
    };
    a.is_finite() && b.is_finite() && a == b && a.is_sign_negative() == b.is_sign_negative()
}

/// Decimal spellings only. `0b` / `0o` / `0x` stay observed (arch probe `num` / `num-75`).
fn is_decimal_number(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Decimal | Kind::Float | Kind::PositiveExponential | Kind::NegativeExponential
    )
}

fn parse_js_number(s: &str) -> Result<f64, ()> {
    let t = s.replace('_', "");
    if t.len() > 2 {
        let p = t.as_bytes()[1];
        if t.as_bytes()[0] == b'0' && matches!(p, b'x' | b'X' | b'b' | b'B' | b'o' | b'O') {
            return Err(());
        }
    }
    t.parse::<f64>().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc::allocator::Allocator;
    use oxc::parser::Parser;
    use oxc::span::SourceType;

    fn commas_allowed(src: &str) -> Vec<(u32, bool)> {
        let alloc = Allocator::default();
        let ret = Parser::new(&alloc, src, SourceType::mjs()).parse();
        assert_eq!(ret.diagnostics.errors().count(), 0, "parse {src:?}");
        src.char_indices()
            .filter(|(_, c)| *c == ',')
            .map(|(i, _)| {
                let start = i as u32;
                (
                    start,
                    trailing_comma_in_d0258_or_d0266_context(&ret.program, start),
                )
            })
            .collect()
    }

    fn any_trailing(src: &str) -> bool {
        commas_allowed(src).iter().any(|(_, ok)| *ok)
    }

    #[test]
    fn ast_elements_distinguish_trailing_comma_from_elision() {
        // Required shapes in assignment, declarator, and param position.
        for (trail, hole, lead) in [
            ("([a,] = o);", "([a,,] = o);", "([,a] = o);"),
            ("let [a,] = o;", "let [a,,] = o;", "let [,a] = o;"),
            (
                "function f([a,]) {}",
                "function f([a,,]) {}",
                "function f([,a]) {}",
            ),
        ] {
            assert!(any_trailing(trail), "trailing must be recognized: {trail}");
            assert!(
                !any_trailing(hole),
                "a last-slot hole is an elision, not a trailing comma: {hole}"
            );
            assert!(
                !any_trailing(lead),
                "a leading hole stays an elision: {lead}"
            );
        }
        // Same for an array literal (D-0258).
        assert!(any_trailing("const a = [a,];"));
        assert!(!any_trailing("const a = [a,,];"));
        assert!(!any_trailing("const a = [,a];"));
    }

    #[test]
    fn rule_ids_come_from_the_toml() {
        // Ids live in rt-1.toml; Rust looks them up by prefix rather than duplicating the tagged string.
        for r in &table().1 {
            assert!(RAW.contains(&format!("id = \"{}\"", r.id)), "{}", r.id);
            assert!(
                RAW.contains(&format!("rev = {}", r.rev)),
                "{}@{}",
                r.id,
                r.rev
            );
            assert_eq!(r.tagged(), format!("{}@{}", r.id, r.rev));
        }
    }
}
