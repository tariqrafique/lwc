//! TS parser/expression-complex/validate.ts (+ index.ts `parseComplexExpression`) on the oxc AST
//! (AB-D0004 S2). Citations are `packages/@lwc/template-compiler/src/parser/` paths in the pinned oracle.
//!
//! acorn `parseExpressionAt(source, start + 1)` (index.ts:45) is oxc `parse_expression` over the text
//! between the braces. acorn stops at the first token that cannot continue the expression; oxc reports
//! that token as an error, so the text up to it is parsed again and that prefix is the expression
//! (Arch AB-D0004 Q3). Message text is acorn's, from [`crate::parser::acorn_msg`] (AB-D0004 S6):
//! oxc decides validity and the message model only names the error.
//!
//! acorn-isms (acorn 8, `ecmaVersion: 2022`, script goal, parser.ts:130 + constants.ts:146; oxc parses as a
//! module so `await` is an expression, as `allowAwaitOutsideFunction: true` makes it): oxc accepts these
//! and acorn throws a SyntaxError while parsing, before validation. Each row has an oracle case in the
//! tests below.
//! - `super` outside a method: "'super' keyword outside a method" (fixture expression-complex/invalid/super).
//! - `import.meta`: "Cannot use 'import.meta' outside a module" (fixture expression-complex/invalid/import-meta).
//! - a private name outside a class: `a.#b` "Private field '#b' must be declared in an enclosing class",
//!   `#b in a` "Unexpected token" (both at the private name; template-compiler oracle runs in the tests).

use crate::parser::Located;
use crate::parser::acorn_msg::{self, Stop};
use crate::parser::expr_check::{self, ExprError};
use lwc_shared::errors::ParserDiagnostics;
use lwc_shared::js::js_ws;
use lwc_shared::{LineIndex, LwcErrorInfo};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionBody, ArrowFunctionExpression, Expression, MethodDefinition, ObjectProperty,
    PrivateFieldExpression, PrivateInExpression, PropertyDefinition, PropertyKind, StaticBlock,
    UnaryOperator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;

#[cfg(test)]
use super::{NOT_EXPRESSION_STATEMENT, classify};
use super::{ParseOutcome, Rejection, parse_source};
use crate::parser::InternalError;

/// TS `parseComplexExpression(ctx, source, …, expressionStart)` for one expression of a text node:
/// `source` is the whole cleaned text (it is acorn's input and the message's `{source}`), and the
/// expression is chars `[start, end)` of it, `{` included.
#[cfg(test)]
pub(crate) fn validate_text(source: &str, start: usize, end: usize) -> Located<Option<ExprError>> {
    let alloc = Allocator::default();
    Ok(validate(&alloc, source, start, end, None)?.err())
}

/// [`validate_text`] parsing into `alloc` and, when the expression is accepted, storing it under `key`
/// (the `{…}` token) so lowering reuses the tree.
pub(crate) fn validate_text_cached<'a>(
    source: &str,
    start: usize,
    end: usize,
    alloc: &'a Allocator,
    key: &str,
    cache: &mut super::ExprCache<'a>,
) -> Located<Option<ExprError>> {
    Ok(validate(alloc, source, start, end, Some((key, cache)))?.err())
}

/// TS getTemplateAttribute → parseComplexExpression for a quoted attribute value `value` (`{…}`), with
/// `raw` the entity-undecoded value: validate.ts:207 validateComplexExpression, then the second parse of
/// the template source (validate.ts:296-306).
#[cfg(test)]
pub(crate) fn validate_attribute(value: &str, raw: &str) -> Located<Option<ExprError>> {
    let alloc = Allocator::default();
    validate_attribute_in(&alloc, value, raw, None)
}

pub(crate) fn validate_attribute_cached<'a>(
    value: &str,
    raw: &str,
    alloc: &'a Allocator,
    cache: &mut super::ExprCache<'a>,
) -> Located<Option<ExprError>> {
    validate_attribute_in(alloc, value, raw, Some(cache))
}

fn validate_attribute_in<'a>(
    alloc: &'a Allocator,
    value: &str,
    raw: &str,
    cache: Option<&mut super::ExprCache<'a>>,
) -> Located<Option<ExprError>> {
    let end = match validate(
        alloc,
        value,
        0,
        value.chars().count(),
        cache.map(|c| (value, c)),
    )? {
        Ok(end) => end,
        Err(e) => return Ok(Some(e)),
    };
    if raw == value {
        return Ok(None);
    }
    // validate.ts:296 parseExpressionAt(templateSource, expressionStart + OPENING_CURLY_LEN, options); a
    // SyntaxError there is reported against `source`, like the first parse.
    let w = Window::new(raw, 0, raw.chars().count());
    let raw_end = match parse_source(alloc, w.body()) {
        ParseOutcome::Ok(p) => p.expr.span().end as usize,
        ParseOutcome::Internal(msg) => return Err(InternalError(msg)),
        ParseOutcome::Rejected(r) => match resolve(
            alloc,
            raw,
            &w,
            value,
            false,
            oxc_span::SourceType::mjs(),
            &r,
        )? {
            Resolved::Expr(e, _) => e.span().end as usize,
            Resolved::Err(e) => return Ok(Some(e)),
        },
    } + w.body_start;
    // validate.ts:302 `expression.end === templateExpression.end` (UTF-16 offsets in the two strings).
    // AB-D0001: LineIndex is the one byte→UTF-16 converter.
    let (value_ix, raw_ix) = (LineIndex::new(value), LineIndex::new(raw));
    Ok(
        (value_ix.utf16_offset(end as u32) != raw_ix.utf16_offset(raw_end as u32)).then(|| {
            invalid(
                value,
                &ParserDiagnostics::TEMPLATE_EXPRESSION_PARSING_ERROR,
                &["expression incorrectly formed"],
            )
        }),
    )
}

/// The expression `{…}` at chars `[start, end)` of `source`: its end (byte offset in `source`) or the
/// diagnostic TS throws.
fn validate<'a>(
    alloc: &'a Allocator,
    source: &str,
    start: usize,
    end: usize,
    mut cache: Option<(&str, &mut super::ExprCache<'a>)>,
) -> Located<Result<usize, ExprError>> {
    let w = Window::new(source, start, end);
    let (expr, comments) = match parse_source(alloc, w.body()) {
        ParseOutcome::Ok(p) => {
            if let Some((key, slot)) = cache.as_mut()
                && let Some(saved) = super::slot_of(alloc, w.body(), &p.expr)
            {
                slot.insert((*key).to_string(), saved);
            }
            (p.expr, p.comments)
        }
        ParseOutcome::Internal(msg) => return Err(InternalError(msg)),
        ParseOutcome::Rejected(r) => match resolve(
            alloc,
            source,
            &w,
            source,
            false,
            oxc_span::SourceType::mjs(),
            &r,
        )? {
            Resolved::Expr(e, comments) => (e, comments),
            Resolved::Err(e) => return Ok(Err(e)),
        },
    };
    // index.ts:40 `onComment: () => invariant(false, INVALID_EXPR_COMMENTS_DISALLOWED)`: acorn lexes every
    // comment up to the token it stops at (the closing `}` or the token that ends the expression), before
    // any validation. Comments come from the same `(text\n)` Program parse as the expression (AB-L0070 S3).
    let syntax = AcornIsms::first(&expr);
    let comment_before = |at: u32| comments.iter().any(|&c| c < at);
    if comment_before(syntax.as_ref().map_or(u32::MAX, |s| s.0)) {
        return Ok(Err(invalid(
            source,
            &ParserDiagnostics::INVALID_EXPR_COMMENTS_DISALLOWED,
            &[],
        )));
    }
    if let Some((at, msg)) = syntax {
        return Ok(Err(syntax_error(
            source,
            w.body_start + at as usize,
            source,
            &msg,
        )));
    }
    // validate.ts:243 validateExpressionAst.
    let mut v = ExpressionAst::default();
    v.visit_expression(&expr);
    if let Some((info, arg)) = v.found {
        return Ok(Err(invalid(source, info, &arg)));
    }
    let (e_start, e_end) = (
        w.body_start + expr.span().start as usize,
        w.body_start + expr.span().end as usize,
    );
    // validate.ts:236-239 leadingChars / getTrailingChars (`/[\s)]*/`) / idxOfClosingBracket.
    let leading = &source[w.open + 1..e_start];
    let trailing_len = source[e_end..]
        .chars()
        .take_while(|&c| js_ws(c) || c == ')')
        .map(char::len_utf8)
        .sum::<usize>();
    let trailing = &source[e_end..e_end + trailing_len];
    // validate.ts:244 validateMatchingExtraParens (validate.ts:183-191).
    if leading.matches('(').count() != trailing.matches(')').count() {
        return Ok(Err(invalid(
            source,
            &ParserDiagnostics::TEMPLATE_EXPRESSION_PARSING_ERROR,
            &["expression must have balanced parentheses."],
        )));
    }
    // validate.ts:245-249 `source.codePointAt(idxOfClosingBracket) === CLOSING_CURLY_BRACKET`.
    if !source[e_end + trailing_len..].starts_with('}') {
        return Ok(Err(invalid(
            source,
            &ParserDiagnostics::TEMPLATE_EXPRESSION_PARSING_ERROR,
            &["expression must end with curly brace."],
        )));
    }
    Ok(Ok(e_end))
}

/// The expression's place in `source`, in bytes: `open` is its `{`, the body runs to its closing `}`
/// (or to the end of the window when it has none).
struct Window<'s> {
    source: &'s str,
    open: usize,
    body_start: usize,
    body_end: usize,
}

impl<'s> Window<'s> {
    fn new(source: &'s str, start: usize, end: usize) -> Self {
        let byte = |ch: usize| {
            source
                .char_indices()
                .nth(ch)
                .map_or(source.len(), |(b, _)| b)
        };
        let (open, stop) = (byte(start), byte(end));
        let body_end = if stop > open + 1 && source[..stop].ends_with('}') {
            stop - 1
        } else {
            stop
        };
        Window {
            source,
            open,
            body_start: (open + 1).min(stop),
            body_end,
        }
    }
    fn body(&self) -> &'s str {
        &self.source[self.body_start..self.body_end]
    }
}

/// acorn's parseExpressionAt stops at the first token that cannot continue the expression, which is
/// where oxc's `parse_expression` reports its first error: the body before it, if it parses, is the
/// expression, and the token is at body byte `.1` (Arch AB-D0004 Q3). Comments come from the same parse.
///
/// [`ParseOutcome::Rejected`] on the prefix is "no usable expression before the stop token" → `Ok(None)`.
/// [`ParseOutcome::Internal`] propagates as [`InternalError`] (AB-L0073).
fn prefix<'a>(
    alloc: &'a Allocator,
    body: &str,
    r: &Rejection,
    goal: oxc_span::SourceType,
) -> Result<Option<(Expression<'a>, usize, Vec<u32>)>, InternalError> {
    let Some(at) =
        r.at.filter(|&at| at > 0 && at <= body.len() && body.is_char_boundary(at))
    else {
        return Ok(None);
    };
    prefix_parse_result(super::parse_goal(alloc, &body[..at], goal), at)
}

/// Map a [`parse_source`] outcome for a prefix slice. Factored so the Internal→InternalError path is
/// unit-testable without needing an input that triggers an invariant in oxc (AB-L0073).
fn prefix_parse_result<'a>(
    parsed: ParseOutcome<'a>,
    at: usize,
) -> Result<Option<(Expression<'a>, usize, Vec<u32>)>, InternalError> {
    match parsed {
        ParseOutcome::Ok(p) => Ok(Some((p.expr, at, p.comments))),
        ParseOutcome::Rejected(_) => Ok(None),
        fail @ ParseOutcome::Internal(_) => Err(fail.into_internal()),
    }
}

/// What an oxc rejection resolves to: a prefix expression acorn would have returned, or the error acorn throws.
enum Resolved<'a> {
    Expr(Expression<'a>, Vec<u32>),
    Err(ExprError),
}

/// oxc rejected `w.body()`. A hard lexical error (or an ES2020-only one the caller already applied) wins;
/// otherwise the prefix before oxc's stop token is the expression, and a failed prefix is acorn's
/// "Unexpected token" — except `yield` / legacy `await` / `as`, which acorn parses as an identifier.
fn resolve<'a>(
    alloc: &'a Allocator,
    column_text: &str,
    w: &Window,
    named: &str,
    es2020: bool,
    goal: oxc_span::SourceType,
    r: &Rejection,
) -> Result<Resolved<'a>, InternalError> {
    let body = w.body();
    let at = r.at.unwrap_or(0);
    let next = w.source[w.body_end..].chars().next();
    let syntax = |stop: &Stop| -> ExprError {
        if acorn_msg::comment_before(body, stop.at) {
            invalid(
                named,
                &ParserDiagnostics::INVALID_EXPR_COMMENTS_DISALLOWED,
                &[],
            )
        } else {
            syntax_error(column_text, w.body_start + stop.at, named, &stop.msg)
        }
    };
    if let Some(stop) = acorn_msg::hard_syntax(body, at, &r.message, next) {
        return Ok(Resolved::Err(syntax(&stop)));
    }
    if let Some((e, _, comments)) = prefix(alloc, body, r, goal)? {
        return Ok(Resolved::Expr(e, comments));
    }
    if let Some(end) = acorn_msg::ident_recovery(body, at, &r.message, es2020)
        && end > 0
        && end <= body.len()
        && body.is_char_boundary(end)
        && let ParseOutcome::Ok(p) = super::parse_goal(alloc, &body[..end], goal)
    {
        return Ok(Resolved::Expr(p.expr, p.comments));
    }
    Ok(Resolved::Err(syntax(&Stop {
        at,
        msg: "Unexpected token".into(),
    })))
}

/// `Invalid expression {source} - LWCnnnn: …` (an `invariant` CompilerError through withErrorWrapping).
fn invalid(source: &str, info: &LwcErrorInfo, args: &[&str]) -> ExprError {
    ExprError::invariant(source, info, args)
}

/// acorn SyntaxError at byte `at` of `text` (acorn's input): `LWC1083: … Invalid expression {source} -
/// {msg} (line:col)`, acorn's 1-based line and UTF-16 column (line breaks `\r\n`, `\r`, `\n`, U+2028, U+2029).
fn syntax_error(text: &str, at: usize, source: &str, msg: &str) -> ExprError {
    // AB-D0001: LineIndex::babel_line_col is the one byte→(1-based line, UTF-16 column) converter.
    let (line, col) = LineIndex::new(text).babel_line_col(at as u32);
    ExprError::syntax(source, msg, line as usize, col as usize)
}

/// validate.ts:175-222 validateNode / validateExpressionAst: the first offending node in estree-walker
/// pre-order (a node before its children, children in ESTree field order).
#[derive(Default)]
struct ExpressionAst {
    arrow_depth: u32,
    found: Option<(&'static LwcErrorInfo, Vec<&'static str>)>,
}

impl ExpressionAst {
    fn hit(&mut self, info: &'static LwcErrorInfo, args: &[&'static str]) -> bool {
        if self.found.is_none() {
            self.found = Some((info, args.to_vec()));
        }
        true
    }
    /// validate.ts:89-103 ALWAYS_INVALID_TYPES.
    fn prohibited(&mut self, what: &'static str) -> bool {
        self.hit(
            &ParserDiagnostics::INVALID_EXPR_PROHIBITED_NODE_TYPE,
            &[what],
        )
    }
}

impl<'a> Visit<'a> for ExpressionAst {
    fn visit_expression(&mut self, it: &Expression<'a>) {
        if self.found.is_some() {
            return;
        }
        // STATEMENT_TYPES (validate.ts:180) are unreachable: statements only occur inside a function, class
        // or arrow block body, whose node throws first. Node comments (validate.ts:176) are unreachable too:
        // acorn attaches none, and onComment has already thrown.
        let threw = match it {
            // validate.ts:184 MUTATION_TYPES outside an arrow function.
            Expression::AssignmentExpression(_) | Expression::UpdateExpression(_)
                if self.arrow_depth == 0 =>
            {
                self.hit(&ParserDiagnostics::INVALID_EXPR_MUTATION_OUTSIDE_ARROW, &[])
            }
            Expression::AwaitExpression(_) => self.prohibited("await expressions"),
            Expression::ClassExpression(_) => self.prohibited("classes"),
            Expression::FunctionExpression(_) => self.prohibited("function expressions"),
            Expression::ImportExpression(_) => self.prohibited("imports"),
            Expression::ImportMeta(_) => self.prohibited("import.meta"),
            // oxc splits ESTree MetaProperty into ImportMeta and NewTarget; upstream maps both
            // (ALWAYS_INVALID_TYPES MetaProperty → "import.meta", validate.ts:31).
            Expression::NewTarget(_) => self.prohibited("import.meta"),
            Expression::NewExpression(_) => self.prohibited("object instantiation"),
            Expression::SequenceExpression(_) => self.prohibited("comma operators"),
            Expression::Super(_) => self.prohibited("`super`"),
            Expression::ThisExpression(_) => self.prohibited("`this`"),
            Expression::YieldExpression(_) => self.prohibited("`yield`"),
            // validate.ts:156-173 validateLiteral: BigInt first, then RegExp.
            Expression::BigIntLiteral(_) => self.prohibited("BigInts"),
            Expression::RegExpLiteral(_) => self.prohibited("regular expression literals"),
            // validate.ts:152 validateUnaryExpression.
            Expression::UnaryExpression(u) if u.operator == UnaryOperator::Delete => {
                self.hit(&ParserDiagnostics::INVALID_EXPR_DELETE_OP, &[])
            }
            _ => false,
        };
        if !threw {
            walk::walk_expression(self, it);
        }
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        if self.found.is_some() {
            return;
        }
        // validate.ts:144-150 validateArrowFunction (an arrow is never a generator).
        if matches!(it.body, ArrowFunctionBody::FunctionBody(_)) {
            self.hit(&ParserDiagnostics::INVALID_EXPR_ARROW_FN_BODY, &[]);
            return;
        }
        if it.r#async {
            self.hit(&ParserDiagnostics::INVALID_EXPR_ARROW_FN_KIND, &["async"]);
            return;
        }
        // validate.ts:212-219 arrowFnScopeDepth.
        self.arrow_depth += 1;
        walk::walk_arrow_function_expression(self, it);
        self.arrow_depth -= 1;
    }
}

/// The acorn-ism rows (module doc): the first SyntaxError acorn raises while parsing, by source position
/// (body bytes), with its message.
#[derive(Default)]
struct AcornIsms {
    /// acorn `allowSuper`: inside a method (object/class method or accessor, class field, static block).
    in_method: bool,
    /// The next function expression is an object method's value.
    method_value: bool,
    class_depth: u32,
    found: Option<(u32, String)>,
}

impl AcornIsms {
    fn first(expr: &Expression) -> Option<(u32, String)> {
        let mut v = AcornIsms::default();
        v.visit_expression(expr);
        v.found
    }
    fn raise(&mut self, at: u32, msg: impl Into<String>) {
        if self.found.as_ref().is_none_or(|(f, _)| at < *f) {
            self.found = Some((at, msg.into()));
        }
    }
    fn with_method<F: FnOnce(&mut Self)>(&mut self, method: bool, f: F) {
        let saved = std::mem::replace(&mut self.in_method, method);
        f(self);
        self.in_method = saved;
    }
}

impl<'a> Visit<'a> for AcornIsms {
    fn visit_expression(&mut self, it: &Expression<'a>) {
        match it {
            Expression::Super(s) if !self.in_method => {
                self.raise(s.span.start, "'super' keyword outside a method")
            }
            Expression::ImportMeta(m) => {
                self.raise(m.span.start, "Cannot use 'import.meta' outside a module")
            }
            Expression::FunctionExpression(_) => {
                let method = std::mem::take(&mut self.method_value);
                return self.with_method(method, |v| walk::walk_expression(v, it));
            }
            Expression::ClassExpression(_) => {
                self.class_depth += 1;
                walk::walk_expression(self, it);
                self.class_depth -= 1;
                return;
            }
            _ => {}
        }
        walk::walk_expression(self, it);
    }
    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        self.visit_property_key(&it.key);
        self.method_value = it.method || it.kind != PropertyKind::Init;
        self.visit_expression(&it.value);
        self.method_value = false;
    }
    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        self.with_method(true, |v| walk::walk_method_definition(v, it));
    }
    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        self.with_method(true, |v| walk::walk_property_definition(v, it));
    }
    fn visit_static_block(&mut self, it: &StaticBlock<'a>) {
        self.with_method(true, |v| walk::walk_static_block(v, it));
    }
    fn visit_private_field_expression(&mut self, it: &PrivateFieldExpression<'a>) {
        if self.class_depth == 0 {
            self.raise(
                it.field.span.start,
                format!(
                    "Private field '#{}' must be declared in an enclosing class",
                    it.field.name
                ),
            );
        }
        walk::walk_private_field_expression(self, it);
    }
    fn visit_private_in_expression(&mut self, it: &PrivateInExpression<'a>) {
        if self.class_depth == 0 {
            self.raise(it.left.span.start, "Unexpected token");
        }
        walk::walk_private_in_expression(self, it);
    }
}

/// Legacy `parseExpression` (`ecmaVersion: 2020`, `allowAwaitOutsideFunction: false`) on the oxc AST.
/// Callers pass one `{…}` token. `None` means acorn accepted an identifier or member chain.
pub(crate) fn legacy(src: &str, o: &expr_check::Opts) -> Option<ExprError> {
    let alloc = Allocator::default();
    let w = Window::new(src, 0, src.chars().count());
    // The complex flag selects acorn's ecma edition (2022 vs 2020). It does not switch the goal:
    // parseExpression keeps allowAwaitOutsideFunction false, so this stays a script either way.
    if !o.complex
        && let Some(stop) = acorn_msg::es2020_prescan(w.body())
    {
        return Some(stop_error(src, &w, src, &stop));
    }
    match super::parse_goal(&alloc, w.body(), oxc_span::SourceType::script()) {
        ParseOutcome::Ok(p) => legacy_parsed(src, &w, &p.expr, &p.comments, o),
        ParseOutcome::Rejected(r) => match resolve(
            &alloc,
            src,
            &w,
            src,
            true,
            oxc_span::SourceType::script(),
            &r,
        ) {
            Ok(Resolved::Expr(e, c)) => legacy_parsed(src, &w, &e, &c, o),
            Ok(Resolved::Err(e)) => Some(e),
            Err(_) => Some(syntax_error(src, 1, src, "Unexpected token")),
        },
        ParseOutcome::Internal(_) => Some(syntax_error(src, 1, src, "Unexpected token")),
    }
}

fn stop_error(column_text: &str, w: &Window, named: &str, stop: &Stop) -> ExprError {
    if acorn_msg::comment_before(w.body(), stop.at) {
        invalid(
            named,
            &ParserDiagnostics::INVALID_EXPR_COMMENTS_DISALLOWED,
            &[],
        )
    } else {
        syntax_error(column_text, w.body_start + stop.at, named, &stop.msg)
    }
}

fn legacy_parsed(
    src: &str,
    w: &Window,
    expr: &Expression,
    comments: &[u32],
    o: &expr_check::Opts,
) -> Option<ExprError> {
    if !comments.is_empty() {
        return Some(invalid(
            src,
            &ParserDiagnostics::INVALID_EXPR_COMMENTS_DISALLOWED,
            &[],
        ));
    }
    // acorn throws these while parsing, before validateExpression, so they stay 1083 rather than 1060/1210.
    if let Some((at, msg)) = AcornIsms::first(expr) {
        return Some(syntax_error(src, w.body_start + at as usize, src, &msg));
    }
    let (e_start, e_end) = (
        w.body_start + expr.span().start as usize,
        w.body_start + expr.span().end as usize,
    );
    if let Some(e) = source_is_parsed(src, e_start, e_end) {
        return Some(e);
    }
    legacy_node(src, o, violation(expr, o.computed_ok)?)
}

/// validate.ts `validateSourceIsParsedExpression` (expression.ts). Indices are bytes of `src`.
fn source_is_parsed(src: &str, start: usize, end: usize) -> Option<ExprError> {
    if end == src.len().saturating_sub(1) {
        return None;
    }
    let inv = |m: &str| {
        ExprError::invariant(
            src,
            &ParserDiagnostics::TEMPLATE_EXPRESSION_PARSING_ERROR,
            &[m],
        )
    };
    let mut open = src
        .get(..start)
        .unwrap_or("")
        .chars()
        .filter(|&c| c == '(')
        .count() as i64;
    let last = src.len().saturating_sub(1);
    let mut i = end;
    while i < last {
        let c = src[i..].chars().next()?;
        if c == ')' {
            open -= 1;
        } else if c == ';' {
            return Some(ExprError::invariant(
                src,
                &ParserDiagnostics::MULTIPLE_EXPRESSIONS,
                &[],
            ));
        } else if !js_ws(c) {
            return Some(inv("Unexpected end of expression"));
        }
        i += c.len_utf8();
    }
    (open != 0).then(|| inv("Unexpected end of expression"))
}

enum LegV {
    Node(&'static str),
    Computed,
}

fn violation(e: &Expression, computed_ok: bool) -> Option<LegV> {
    match e {
        Expression::Identifier(_) => None,
        Expression::ParenthesizedExpression(p) => violation(&p.expression, computed_ok),
        Expression::StaticMemberExpression(m) if !m.optional => violation(&m.object, computed_ok),
        Expression::ComputedMemberExpression(m) if !m.optional => {
            if !computed_ok {
                return Some(LegV::Computed);
            }
            violation(&m.object, true).or_else(|| violation(&m.expression, true))
        }
        other => Some(LegV::Node(estree_name(other))),
    }
}

fn legacy_node(src: &str, o: &expr_check::Opts, v: LegV) -> Option<ExprError> {
    use lwc_shared::api_version::{ApiFeature, min_api_version};
    let api = o.api_version.to_string();
    let min = min_api_version(ApiFeature::EnableComplexTemplateExpressions).to_string();
    let inv =
        |info: &lwc_shared::LwcErrorInfo, args: &[&str]| ExprError::invariant(src, info, args);
    match v {
        LegV::Node(ty) => {
            if !o.complex {
                Some(inv(&ParserDiagnostics::INVALID_NODE, &[ty]))
            } else if !o.cte() {
                Some(inv(
                    &ParserDiagnostics::INVALID_NODE_CTE_API_VERSION,
                    &[ty, &api, &min],
                ))
            } else if o.unquoted {
                Some(inv(
                    &ParserDiagnostics::INVALID_NODE_CTE_UNQUOTED,
                    &[ty, src],
                ))
            } else {
                None
            }
        }
        LegV::Computed => {
            if !o.complex {
                Some(inv(
                    &ParserDiagnostics::COMPUTED_PROPERTY_ACCESS_NOT_ALLOWED,
                    &[src],
                ))
            } else if !o.cte() {
                Some(inv(
                    &ParserDiagnostics::COMPUTED_PROPERTY_ACCESS_NOT_ALLOWED_CTE_API_VERSION,
                    &[src, &api, &min],
                ))
            } else if o.unquoted {
                Some(inv(
                    &ParserDiagnostics::COMPUTED_PROPERTY_ACCESS_NOT_ALLOWED_CTE_UNQUOTED,
                    &[src],
                ))
            } else {
                None
            }
        }
    }
}

/// ESTree `node.type` for the root of a legacy expression (expression.ts messages).
fn estree_name(e: &Expression) -> &'static str {
    match e {
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::StringLiteral(_) => "Literal",
        Expression::TemplateLiteral(_) => "TemplateLiteral",
        Expression::Identifier(_) => "Identifier",
        Expression::Super(_) => "Super",
        Expression::ArrayExpression(_) => "ArrayExpression",
        Expression::ArrowFunctionExpression(_) => "ArrowFunctionExpression",
        Expression::AssignmentExpression(_) => "AssignmentExpression",
        Expression::AwaitExpression(_) => "AwaitExpression",
        Expression::BinaryExpression(_) => "BinaryExpression",
        Expression::CallExpression(_) => "CallExpression",
        Expression::ChainExpression(_) => "ChainExpression",
        Expression::ClassExpression(_) => "ClassExpression",
        Expression::ConditionalExpression(_) => "ConditionalExpression",
        Expression::FunctionExpression(_) => "FunctionExpression",
        Expression::ImportExpression(_) => "ImportExpression",
        Expression::LogicalExpression(_) => "LogicalExpression",
        Expression::NewExpression(_) => "NewExpression",
        Expression::ObjectExpression(_) => "ObjectExpression",
        Expression::ParenthesizedExpression(_) => "ParenthesizedExpression",
        Expression::SequenceExpression(_) => "SequenceExpression",
        Expression::TaggedTemplateExpression(_) => "TaggedTemplateExpression",
        Expression::ThisExpression(_) => "ThisExpression",
        Expression::UnaryExpression(_) => "UnaryExpression",
        Expression::UpdateExpression(_) => "UpdateExpression",
        Expression::YieldExpression(_) => "YieldExpression",
        Expression::PrivateInExpression(_) => "BinaryExpression",
        Expression::ImportMeta(_) | Expression::NewTarget(_) => "MetaProperty",
        Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_)
        | Expression::PrivateFieldExpression(_) => "MemberExpression",
        Expression::JSXElement(_) => "JSXElement",
        Expression::JSXFragment(_) => "JSXFragment",
        Expression::TSAsExpression(_) => "TSAsExpression",
        Expression::TSSatisfiesExpression(_) => "TSSatisfiesExpression",
        Expression::TSTypeAssertion(_) => "TSTypeAssertion",
        Expression::TSNonNullExpression(_) => "TSNonNullExpression",
        Expression::TSInstantiationExpression(_) => "TSInstantiationExpression",
        Expression::V8IntrinsicExpression(_) => "V8IntrinsicExpression",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A text node holding just `src` (TS parseTextComplex on the cleaned text).
    fn c(src: &str) -> Option<String> {
        validate_text(src, 0, src.chars().count())
            .ok()
            .flatten()
            .map(|e| e.message)
    }
    fn code(src: &str) -> Option<u32> {
        validate_text(src, 0, src.chars().count())
            .ok()
            .flatten()
            .map(|e| e.code)
    }
    fn attr(value: &str, raw: &str) -> Option<String> {
        validate_attribute(value, raw)
            .ok()
            .flatten()
            .map(|e| e.message)
    }

    /// The expr_check complex-mode block (AB-D0004 design exit check), unchanged, on the oxc path.
    /// Each row is an upstream fixture under template-compiler src/__tests__/fixtures/expression-complex/.
    #[test]
    fn complex_upstream_fixtures() {
        assert_eq!(
            c("{async () => doSomething()}").unwrap(),
            "Invalid expression {async () => doSomething()} - LWC1194: Arrow functions in template expressions cannot be async."
        );
        assert_eq!(code("{() => { doSomething(); }}"), Some(1193));
        assert_eq!(
            c("{myField = 'foo'}").unwrap(),
            "Invalid expression {myField = 'foo'} - LWC1191: Field mutations are only permitted within arrow functions."
        );
        assert_eq!(code("{foo++}"), Some(1191));
        assert_eq!(code("{count += 1}"), Some(1191));
        assert_eq!(code("{() => count += 1}"), None);
        assert_eq!(
            c("{await foo()}").unwrap(),
            "Invalid expression {await foo()} - LWC1196: Use of await expressions is disallowed within template expressions."
        );
        assert_eq!(code("{transformBigInt(1n)}"), Some(1196));
        assert_eq!(
            c("{\n    class Bar {\n        method() {}\n    }\n}")
                .map(|m| m.ends_with("Use of classes is disallowed within template expressions.")),
            Some(true)
        );
        assert_eq!(
            c("{one(), two(), three(), iAmTheCount()}").unwrap(),
            "Invalid expression {one(), two(), three(), iAmTheCount()} - LWC1196: Use of comma operators is disallowed within template expressions."
        );
        assert_eq!(
            code("{\n /* what do you think ? */\n someValue\n }"),
            Some(1197)
        );
        assert_eq!(
            c("{() => delete baz}").unwrap(),
            "Invalid expression {() => delete baz} - LWC1192: Use of the delete operator is prohibited within template expressions."
        );
        assert_eq!(
            c("{function Awesomeness (foo) { bar(foo); }}").unwrap(),
            "Invalid expression {function Awesomeness (foo) { bar(foo); }} - LWC1196: Use of function expressions is disallowed within template expressions."
        );
        assert_eq!(
            c("{foo &#60; bar}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {foo &#60; bar} - Unexpected character '6' (1:7)"
        );
        assert_eq!(
            c("{() => call(").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {() => call( - Unexpected token (1:12)"
        );
        assert_eq!(
            c("{() => foo(\"}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {() => foo(\"} - Unterminated string constant (1:11)"
        );
        assert_eq!(
            code("{() => import('foo').then(doThingWithFoo)}"),
            Some(1196)
        );
        assert_eq!(
            c("{() => doThing(import.meta.env.SSR)}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {() => doThing(import.meta.env.SSR)} - Cannot use 'import.meta' outside a module (1:15)"
        );
        assert_eq!(
            c("{() => new Day()}").unwrap(),
            "Invalid expression {() => new Day()} - LWC1196: Use of object instantiation is disallowed within template expressions."
        );
        assert_eq!(code("{/wannabe/g}"), Some(1196));
        assert_eq!(
            c("{super('duper')}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {super('duper')} - 'super' keyword outside a method (1:1)"
        );
        assert_eq!(
            c("{this.bar}").unwrap(),
            "Invalid expression {this.bar} - LWC1196: Use of `this` is disallowed within template expressions."
        );
        assert_eq!(
            c("{() => throw 'oh no!'}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {() => throw 'oh no!'} - Unexpected token (1:7)"
        );
        assert_eq!(
            c("{yield bar}").unwrap(),
            "Invalid expression {yield bar} - LWC1083: Error parsing template expression: expression must end with curly brace."
        );
        assert_eq!(
            c("{foo())}").unwrap(),
            "Invalid expression {foo())} - LWC1083: Error parsing template expression: expression must have balanced parentheses."
        );
        for ok in [
            "{foo(bar, baz)}",
            "{a ? b : c}",
            "{a in b}",
            "{tag`x`}",
            "{typeof a}",
            "{(foo.bar)}",
            "{((foo ?? bar)())}",
            "{(foo ?? bar).baz}",
            "{x => x.y}",
            "{() => foo.bar = 1}",
            "{a / b}",
            "{[1, 2]}",
            "{({a: 1, b: 2})}",
            "{a?.b}",
            "{-1}",
        ] {
            assert_eq!(c(ok), None, "{ok}");
        }
    }

    /// acorn-ism rows and AST-order cases. Expected strings are @lwc/template-compiler (oracle b9e8007,
    /// dist compile, experimentalComplexExpressions) on `<template>{…}</template>`.
    #[test]
    fn acorn_isms_match_the_oracle() {
        assert_eq!(
            c("{a.#b}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {a.#b} - Private field '#b' must be declared in an enclosing class (1:3)"
        );
        assert_eq!(
            c("{#x in y}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {#x in y} - Unexpected token (1:1)"
        );
        assert_eq!(
            c("{super.x}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {super.x} - 'super' keyword outside a method (1:1)"
        );
        // super inside an object method is allowed by acorn; the function expression throws in validation.
        assert_eq!(
            c("{({a() { return super.x }})}").unwrap(),
            "Invalid expression {({a() { return super.x }})} - LWC1196: Use of function expressions is disallowed within template expressions."
        );
        // A SyntaxError acorn raises while parsing beats a node validation would reject first.
        assert_eq!(
            c("{function f() { super.x }}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {function f() { super.x }} - 'super' keyword outside a method (1:16)"
        );
        // onComment fires while lexing, before validation of the earlier sequence node.
        assert_eq!(
            c("{a, /* c */ b}").unwrap(),
            "Invalid expression {a, /* c */ b} - LWC1197: Use of comments is disallowed within template expressions."
        );
        assert_eq!(
            c("{a /* c */}").unwrap(),
            "Invalid expression {a /* c */} - LWC1197: Use of comments is disallowed within template expressions."
        );
        // acorn stops at `c`; the sequence before it is validated first.
        assert_eq!(code("{a, b c}"), Some(1196));
        assert_eq!(
            c("{a => { return a; }}").unwrap(),
            "Invalid expression {a => { return a; }} - LWC1193: The body of arrow functions in template expressions must be an expression."
        );
    }

    /// ESTree `MetaProperty` is oxc `ImportMeta` | `NewTarget`. Upstream ALWAYS_INVALID_TYPES maps
    /// the type to the argument "import.meta" (validate.ts:31).
    ///
    /// Oracle (b9e8007, experimentalComplexExpressions) on `<template>{…}</template>`:
    /// - `{new.target}` → LWC1083 "'new.target' can only be used in functions and class static block (1:1)"
    /// - `{import.meta}` → LWC1083 "Cannot use 'import.meta' outside a module (1:1)"
    /// - `{() => new.target}` → LWC1083 "'new.target' can only be used in functions and class static block (1:7)"
    /// - `{function() { return new.target }}` → LWC1196 "Use of function expressions is disallowed…"
    ///
    /// oxc `parse_expression` rejects `{new.target}` and `{() => new.target}` (arrows do not enable
    /// NewTarget context). The message is acorn's "'new.target' can only be used in functions and class
    /// static block". The input oxc *accepts* is `new.target` inside a function. validate()
    /// then reports the function expression first (parent before child), matching the oracle.
    /// The NewTarget → "import.meta" arm is proven by visiting that accepted inner node: that is
    /// the walk that would run if a parent did not already prohibit the expression.
    #[test]
    fn new_target_meta_property_maps_to_import_meta() {
        // Parse-time path: acorn's sentence, not oxc's "Unexpected new.target expression".
        assert_eq!(
            c("{new.target}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {new.target} - 'new.target' can only be used in functions and class static block (1:1)"
        );
        assert_eq!(
            c("{() => new.target}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {() => new.target} - 'new.target' can only be used in functions and class static block (1:7)"
        );
        // import.meta is an acorn-ism (AcornIsms), matching the oracle exactly.
        assert_eq!(
            c("{import.meta}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {import.meta} - Cannot use 'import.meta' outside a module (1:1)"
        );
        // oxc accepts new.target inside a function; validation hits FunctionExpression first.
        assert_eq!(
            c("{function() { return new.target }}").unwrap(),
            "Invalid expression {function() { return new.target }} - LWC1196: Use of function expressions is disallowed within template expressions."
        );

        let alloc = Allocator::default();
        let ParseOutcome::Ok(parsed) = parse_source(&alloc, "function () { return new.target }")
        else {
            panic!("oxc should accept new.target inside a function");
        };
        let expr = parsed.expr;
        let inner = match &expr {
            Expression::FunctionExpression(f) => {
                let body = f.body.as_ref().expect("function body");
                match body.statements.first() {
                    Some(oxc_ast::ast::Statement::ReturnStatement(ret)) => {
                        ret.argument.as_ref().expect("return argument")
                    }
                    other => panic!("expected a return statement, got {other:?}"),
                }
            }
            other => panic!("expected FunctionExpression, got {other:?}"),
        };
        assert!(
            matches!(inner, Expression::NewTarget(_)),
            "inner node should be NewTarget, got {inner:?}"
        );
        let mut v = ExpressionAst::default();
        v.visit_expression(inner);
        let (info, args) = v.found.expect("NewTarget arm should fire");
        assert_eq!(
            info.code,
            ParserDiagnostics::INVALID_EXPR_PROHIBITED_NODE_TYPE.code
        );
        assert_eq!(args, ["import.meta"]);
    }

    /// The text node's expression is chars `[start, end)` of the whole text, which messages name.
    #[test]
    fn text_node_expression_inside_text() {
        let full = "a {a +} b {c}";
        assert_eq!(
            validate_text(full, 2, 7)
                .ok()
                .flatten()
                .map(|e| e.message)
                .unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression a {a +} b {c} - Unexpected token (1:6)"
        );
        assert_eq!(validate_text(full, 10, 13).ok().flatten(), None);
    }

    /// validate.ts:296-306, the second parse over the raw template source (oracle rows on
    /// `<x-foo data-a="{…}">`).
    #[test]
    fn raw_template_source_second_parse() {
        assert_eq!(
            attr("{f(\"x\")}", "{f(&quot;x&quot;)}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {f(\"x\")} - Unexpected token (1:3)"
        );
        assert_eq!(
            attr("{a+b}", "{a&#43;b}").unwrap(),
            "LWC1083: Error parsing template expression: Invalid expression {a+b} - Unexpected character '4' (1:4)"
        );
        assert_eq!(
            attr("{a && b c}", "{a &amp;&amp; b c}").unwrap(),
            "Invalid expression {a && b c} - LWC1083: Error parsing template expression: expression must end with curly brace."
        );
        assert_eq!(attr("{a}", "{a}"), None);
    }

    /// AB-L0073: prefix must not collapse ParseOutcome::Internal into "no prefix".
    /// The Internal value comes from classifying a real oxc Program (`let x = 1`), not a planted arm.
    #[test]
    fn prefix_parse_result_propagates_internal() {
        let alloc = Allocator::default();
        let ret = oxc_parser::Parser::new(&alloc, "let x = 1", oxc_span::SourceType::mjs()).parse();
        let outcome = classify(
            &alloc,
            &ret.program,
            &ret.diagnostics,
            "let x = 1",
            oxc_span::SourceType::mjs(),
        );
        assert!(
            matches!(outcome, ParseOutcome::Internal(NOT_EXPRESSION_STATEMENT)),
            "expected Internal(NOT_EXPRESSION_STATEMENT), got {outcome:?}",
        );
        assert_eq!(
            prefix_parse_result(outcome, 1).unwrap_err(),
            InternalError(NOT_EXPRESSION_STATEMENT),
        );
        assert!(
            prefix_parse_result(
                ParseOutcome::Rejected(Rejection {
                    at: Some(0),
                    message: "x".into()
                }),
                1,
            )
            .unwrap()
            .is_none()
        );
    }
}
