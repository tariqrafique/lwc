//! Complex template expressions (TS parser/expression-complex): the oxc parse behind
//! [`ComplexExpression`] (index.ts parseComplexExpression) and its validation (`validate`, validate.ts).

pub(crate) mod validate;

use super::{InternalError, Located};
use lwc_shared::template_ir::{ComplexExpression, SourceLocation};
use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::{Expression, Program, Statement};
use oxc_ast_visit::VisitMut;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::{GetSpan, SourceType, Span};
use std::collections::HashMap;

/// One complex expression parsed during validation and reused when the same `{…}` token is lowered.
pub(crate) struct ExprSlot<'a> {
    pub ast: &'a Expression<'a>,
    pub source: &'a str,
}

/// `body` is the text between the braces. Spans on `expr` index `body`; the slot's spans index `body.trim()`,
/// which is what [`build`] stores as `ComplexExpression.source`.
pub(crate) fn slot_of<'a>(
    alloc: &'a Allocator,
    body: &str,
    expr: &Expression<'a>,
) -> Option<ExprSlot<'a>> {
    let lead = body.len() - body.trim_start().len();
    if expr.span().start < lead as u32 {
        return None;
    }
    let mut cloned = expr.clone_in(alloc);
    if lead > 0 {
        Shift(lead as u32).visit_expression(&mut cloned);
    }
    let trimmed = body.trim();
    if cloned.span().end as usize > trimmed.len() {
        return None;
    }
    Some(ExprSlot {
        ast: alloc.alloc(cloned),
        source: alloc.alloc_str(trimmed),
    })
}

pub(crate) type ExprCache<'a> = HashMap<String, ExprSlot<'a>>;

/// [`validate`] accepted an expression this parse rejects. Both parse the text between the braces with
/// the same oxc options, so this is a caller bug (validation and lowering read different tokens).
const OXC_REJECTED: &str = "An internal parsing error occurred while parsing a complex expression; the validated source did not parse.";

/// Every caller hands over a whole `{…}` token (parse_binding / the attribute and text tokenizers
/// only route brace-delimited values here); anything else is a caller bug, not input to recover from.
const NOT_BRACED: &str = "An internal parsing error occurred while parsing a complex expression; the token is not brace-delimited.";

fn parse_options() -> ParseOptions {
    ParseOptions {
        preserve_parens: false,
        ..ParseOptions::default()
    }
}

/// One expression plus the comments acorn's `onComment` would see in `text`. Produced by a single
/// `(text\n)` Program parse (AB-L0070 / AB-L0055 S3): `parse_expression` keeps no trivia, and wrapping
/// as a parenthesized expression statement both makes `{a:1}` an ObjectExpression (not a block) and
/// terminates a trailing line comment. Spans on `expr` and comment starts are body-relative, matching
/// `parse_expression` over `text`.
pub(crate) struct ParsedExpr<'a> {
    pub expr: Expression<'a>,
    pub comments: Vec<u32>,
}

/// oxc's first diagnostic: its primary label's start (body bytes) and its message.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Rejection {
    pub at: Option<usize>,
    pub message: String,
}

/// Program body was not an ExpressionStatement after a clean parse — invariant, not user input.
pub(crate) const NOT_EXPRESSION_STATEMENT: &str = "An internal parsing error occurred while parsing a complex expression; the program body was not an ExpressionStatement.";

/// `parse_expression` accepted source the wrapping Program parse rejected — invariant, not user input.
pub(crate) const EXPRESSION_ACCEPTED: &str = "An internal parsing error occurred while parsing a complex expression; parse_expression accepted source the Program parse rejected.";

/// `parse_expression` failed with an empty diagnostic list — invariant, not user input.
pub(crate) const EXPRESSION_NO_DIAGNOSTIC: &str = "An internal parsing error occurred while parsing a complex expression; parse_expression failed without a diagnostic.";

/// Outcome of [`parse_source`]: success, a real oxc SyntaxError, or an invariant violation.
/// This is a three-arm enum rather than `Result` so `.ok()` cannot collapse [`Internal`]
/// into "no parse" (AB-L0073 r3 / S4).
pub(crate) enum ParseOutcome<'a> {
    Ok(ParsedExpr<'a>),
    Rejected(Rejection),
    Internal(&'static str),
}

impl ParseOutcome<'_> {
    /// Map a non-success outcome to [`InternalError`] at a `Located` boundary. `Rejected` is
    /// unreachable after validation (build) and becomes [`OXC_REJECTED`]; `Internal` keeps its
    /// own message (AB-X0014 / AB-L0073).
    pub(crate) fn into_internal(self) -> InternalError {
        match self {
            ParseOutcome::Ok(_) => {
                InternalError("parse_source succeeded at an InternalError boundary")
            }
            ParseOutcome::Rejected(_) => InternalError(OXC_REJECTED),
            ParseOutcome::Internal(msg) => InternalError(msg),
        }
    }
}

impl std::fmt::Debug for ParseOutcome<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok(_) => write!(f, "Ok(..)"),
            Self::Rejected(r) => write!(f, "Rejected({r:?})"),
            Self::Internal(m) => write!(f, "Internal({m:?})"),
        }
    }
}

/// Parse `text` once as the program `(text\n)`, then [`classify`] the oxc [`Program`].
/// The wrapping `(` is stripped from the AST (`preserve_parens: false`) and all spans are shifted
/// back by 1 so they index `text`. If the wrap is dishonest — the `(` pairs with a `)` inside
/// `text`, as in `a) + (b`, which would parse as `(a) + (b)` — the Program parse succeeds with an
/// expression that starts at byte 0; [`classify`] then recovers via `parse_expression` so
/// prefix-stop (AB-D0004 Q3) still sees the same stop token.
pub(crate) fn parse_source<'a>(alloc: &'a Allocator, text: &str) -> ParseOutcome<'a> {
    parse_goal(alloc, text, SourceType::mjs())
}

/// Same parse as [`parse_source`] under a different goal. Legacy template expressions are scripts
/// (`allowAwaitOutsideFunction: false`); complex expressions are modules.
pub(crate) fn parse_goal<'a>(alloc: &'a Allocator, text: &str, goal: SourceType) -> ParseOutcome<'a> {
    let wrapped = alloc.alloc_str(&format!("({text}\n)"));
    crate::parse_count::bump();
    let ret = Parser::new(alloc, wrapped, goal)
        .with_options(parse_options())
        .parse();
    classify(alloc, &ret.program, &ret.diagnostics, text, goal)
}

/// Classify an already-parsed Program. Every [`ParseOutcome::Internal`] return lives here.
/// `recovery_text` is the original expression source, handed to `parse_expression` when the wrap
/// is unusable (Program diagnostics, or a dishonest wrap whose expression starts at byte 0).
pub(crate) fn classify<'a, D>(
    alloc: &'a Allocator,
    program: &Program<'a>,
    diagnostics: &[D],
    recovery_text: &str,
    goal: SourceType,
) -> ParseOutcome<'a> {
    let recover = !diagnostics.is_empty()
        || matches!(
            program.body.first(),
            Some(Statement::ExpressionStatement(es)) if es.expression.span().start == 0
        );
    if recover {
        // The wrap's closing `)` can become oxc's unexpected token (e.g. `a +` → `(a +\n)`),
        // and a dishonest wrap starts at byte 0. Recover so prefix-stop and 1083 columns stay
        // body-relative. parse_expression accepting that source is an invariant violation.
        let text = alloc.alloc_str(recovery_text);
        crate::parse_count::bump();
        return match Parser::new(alloc, text, goal)
            .with_options(parse_options())
            .parse_expression()
        {
            Ok(_) => ParseOutcome::Internal(EXPRESSION_ACCEPTED),
            Err(ds) => match ds.split_first() {
                // oxc 0.156 `parse_expression` (lib.rs:808-821) returns Err only after a fatal
                // diagnostic or `if !errors.is_empty()`. An empty list cannot be produced even
                // from a crafted Program; it is still Internal, never a fabricated empty Rejection
                // (AB-L0073).
                Some((d, _)) => ParseOutcome::Rejected(Rejection {
                    at: d.labels.first().map(|l| l.offset() as usize),
                    message: d.message.to_string(),
                }),
                None => ParseOutcome::Internal(EXPRESSION_NO_DIAGNOSTIC),
            },
        };
    }
    match program.body.first() {
        Some(Statement::ExpressionStatement(es)) => {
            // Honest wrap: the grouping paren is stripped, so the expression starts at byte >= 1.
            // Honest-wrap spans are >= 1 (AB-L0073); plain subtraction surfaces an off-by-one.
            let comments = program.comments.iter().map(|c| c.span.start - 1).collect();
            let mut expr = es.expression.clone_in(alloc);
            Shift(1).visit_expression(&mut expr);
            ParseOutcome::Ok(ParsedExpr { expr, comments })
        }
        _ => ParseOutcome::Internal(NOT_EXPRESSION_STATEMENT),
    }
}

struct Shift(u32);
impl<'a> VisitMut<'a> for Shift {
    fn visit_span(&mut self, it: &mut Span) {
        // Honest-wrap spans are >= 1 (AB-L0073); plain subtraction surfaces an off-by-one.
        it.start = it.start - self.0;
        it.end = it.end - self.0;
    }
}

/// The `{…}` token `raw` as a [`ComplexExpression`]: `source` is the trimmed text between the braces,
/// parsed with oxc as one expression (`preserve_parens: false`, as acorn/ESTree), all arena-owned.
pub(crate) fn build<'a>(
    alloc: &'a Allocator,
    raw: &str,
    location: SourceLocation,
) -> Located<ComplexExpression<'a>> {
    let inner = raw
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or(InternalError(NOT_BRACED))?;
    let source: &'a str = alloc.alloc_str(inner.trim());
    let parsed = match parse_source(alloc, source) {
        ParseOutcome::Ok(p) => p,
        other => return Err(other.into_internal()),
    };
    Ok(ComplexExpression {
        ast: alloc.alloc(parsed.expr),
        source,
        raw: alloc.alloc_str(raw),
        location,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The span of `{ (a)(b) }` on line 1.
    const AT: SourceLocation = SourceLocation {
        start_line: 1,
        start_column: 1,
        end_line: 1,
        end_column: 11,
        start: 0,
        end: 10,
    };

    #[test]
    fn builds_typed_ast_with_arena_source() {
        let alloc = Allocator::default();
        let c = build(&alloc, "{ (a)(b) }", AT).ok();
        let c = c.as_ref();
        assert_eq!(c.map(|c| (c.source, c.raw)), Some(("(a)(b)", "{ (a)(b) }")));
        // preserve_parens: false — no ParenthesizedExpression, as in ESTree.
        assert!(matches!(
            c.map(|c| c.ast),
            Some(oxc_ast::ast::Expression::CallExpression(_))
        ));
        // Spans index `source`, not the `(…\n)` wrap.
        if let Some(c) = c {
            assert_eq!(c.ast.span(), oxc_span::Span::new(0, c.source.len() as u32));
        }
    }

    #[test]
    fn oxc_rejection_is_an_internal_error() {
        let alloc = Allocator::default();
        assert_eq!(
            build(&alloc, "{}", AT).err(),
            Some(InternalError(OXC_REJECTED))
        );
    }

    #[test]
    fn unbraced_token_is_an_internal_error() {
        let alloc = Allocator::default();
        for raw in ["a", "{a", "a}", "", "}"] {
            assert_eq!(
                build(&alloc, raw, AT).err(),
                Some(InternalError(NOT_BRACED)),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn program_parse_returns_comments_and_expression() {
        let alloc = Allocator::default();
        let ParseOutcome::Ok(p) = parse_source(&alloc, "a /* c */") else {
            panic!("expected Ok")
        };
        assert!(matches!(p.expr, Expression::Identifier(_)));
        assert_eq!(p.expr.span(), oxc_span::Span::new(0, 1));
        assert_eq!(p.comments, vec![2]);
        let ParseOutcome::Ok(p) = parse_source(&alloc, "foo // c") else {
            panic!("expected Ok")
        };
        assert_eq!(p.comments, vec![4]);
        assert!(matches!(
            parse_source(&alloc, "a) + (b"),
            ParseOutcome::Rejected(_)
        ));
    }

    /// Real oxc rejection through the dishonest-wrap / Program-diagnostics path.
    #[test]
    fn parse_fail_rejected_from_input() {
        let alloc = Allocator::default();
        for text in ["a) + (b", "a +"] {
            match parse_source(&alloc, text) {
                ParseOutcome::Rejected(r) => {
                    assert!(!r.message.is_empty(), "{text:?}: empty Rejection message");
                }
                other => panic!("{text:?}: expected Rejected, got {other:?}"),
            }
        }
    }

    /// Rejected at the build boundary becomes the validated-source-did-not-parse internal.
    #[test]
    fn parse_fail_rejected_maps_at_located_boundary() {
        assert_eq!(
            ParseOutcome::Rejected(Rejection {
                at: Some(0),
                message: "x".into()
            })
            .into_internal(),
            InternalError(OXC_REJECTED),
        );
    }

    /// Every `parse_source` rejection of these inputs is a real oxc SyntaxError (not an Internal
    /// constant dressed as Rejected).
    #[test]
    fn parse_source_rejected_is_a_real_oxc_error() {
        let alloc = Allocator::default();
        for text in [
            "",
            " ",
            "\n",
            ";",
            "a) + (b",
            "a +",
            "let x",
            "debugger",
            "export default 1",
            "/* */",
            "}",
            "(",
            ")",
            "new.target",
        ] {
            match parse_source(&alloc, text) {
                ParseOutcome::Ok(_) => {}
                ParseOutcome::Internal(m) => panic!("{text:?}: unexpected Internal({m})"),
                ParseOutcome::Rejected(r) => {
                    assert!(
                        !r.message.is_empty(),
                        "{text:?}: empty Rejection (Internal dressed as Rejected)"
                    );
                    for internal in [
                        NOT_EXPRESSION_STATEMENT,
                        EXPRESSION_ACCEPTED,
                        EXPRESSION_NO_DIAGNOSTIC,
                    ] {
                        assert_ne!(
                            r.message, internal,
                            "{text:?}: Internal constant leaked as Rejection"
                        );
                    }
                }
            }
        }
    }
    fn parse_program<'a>(alloc: &'a Allocator, text: &'a str) -> oxc_parser::ParserReturn<'a> {
        Parser::new(alloc, text, SourceType::mjs())
            .with_options(parse_options())
            .parse()
    }

    /// Unwrapped `let x = 1` parses as a Program whose body is a VariableDeclaration, not an
    /// ExpressionStatement — the NOT_EXPRESSION_STATEMENT Internal arm.
    #[test]
    fn classify_let_declaration_is_internal_not_expression_statement() {
        let alloc = Allocator::default();
        let ret = parse_program(&alloc, "let x = 1");
        assert!(
            ret.diagnostics.is_empty(),
            "expected a clean Program parse, got {:?}",
            ret.diagnostics
        );
        assert!(
            !matches!(
                ret.program.body.first(),
                Some(Statement::ExpressionStatement(_))
            ),
            "expected a non-ExpressionStatement body, got {:?}",
            ret.program.body.first(),
        );
        match classify(&alloc, &ret.program, &ret.diagnostics, "let x = 1", SourceType::mjs()) {
            ParseOutcome::Internal(NOT_EXPRESSION_STATEMENT) => {}
            other => panic!("expected Internal(NOT_EXPRESSION_STATEMENT), got {other:?}"),
        }
    }

    /// Unwrapped `function f(){}` parses as a FunctionDeclaration Program body.
    #[test]
    fn classify_function_declaration_is_internal_not_expression_statement() {
        let alloc = Allocator::default();
        let ret = parse_program(&alloc, "function f(){}");
        assert!(
            ret.diagnostics.is_empty(),
            "expected a clean Program parse, got {:?}",
            ret.diagnostics
        );
        assert!(
            !matches!(
                ret.program.body.first(),
                Some(Statement::ExpressionStatement(_))
            ),
            "expected a non-ExpressionStatement body, got {:?}",
            ret.program.body.first(),
        );
        match classify(&alloc, &ret.program, &ret.diagnostics, "function f(){}", SourceType::mjs()) {
            ParseOutcome::Internal(NOT_EXPRESSION_STATEMENT) => {}
            other => panic!("expected Internal(NOT_EXPRESSION_STATEMENT), got {other:?}"),
        }
    }

    /// Unwrapped `a` is an ExpressionStatement starting at byte 0. classify recovers via
    /// `parse_expression`, which accepts it — the EXPRESSION_ACCEPTED Internal arm.
    #[test]
    fn classify_unwrapped_identifier_is_internal_expression_accepted() {
        let alloc = Allocator::default();
        let ret = parse_program(&alloc, "a");
        assert!(
            ret.diagnostics.is_empty(),
            "expected a clean Program parse, got {:?}",
            ret.diagnostics
        );
        let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
            panic!(
                "expected ExpressionStatement, got {:?}",
                ret.program.body.first()
            );
        };
        assert_eq!(
            es.expression.span().start,
            0,
            "unwrapped expression should start at byte 0"
        );
        match classify(&alloc, &ret.program, &ret.diagnostics, "a", SourceType::mjs()) {
            ParseOutcome::Internal(EXPRESSION_ACCEPTED) => {}
            other => panic!("expected Internal(EXPRESSION_ACCEPTED), got {other:?}"),
        }
    }

    /// Same recover path with a binary expression (`1+1`).
    #[test]
    fn classify_unwrapped_binary_is_internal_expression_accepted() {
        let alloc = Allocator::default();
        let ret = parse_program(&alloc, "1+1");
        assert!(
            ret.diagnostics.is_empty(),
            "expected a clean Program parse, got {:?}",
            ret.diagnostics
        );
        let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
            panic!(
                "expected ExpressionStatement, got {:?}",
                ret.program.body.first()
            );
        };
        assert_eq!(
            es.expression.span().start,
            0,
            "unwrapped expression should start at byte 0"
        );
        match classify(&alloc, &ret.program, &ret.diagnostics, "1+1", SourceType::mjs()) {
            ParseOutcome::Internal(EXPRESSION_ACCEPTED) => {}
            other => panic!("expected Internal(EXPRESSION_ACCEPTED), got {other:?}"),
        }
    }
}
