//! TS optimize.ts `optimizeStaticExpressions`: hoist every static object/array literal in the
//! template function to a module-level `const stcN = ...;`.
//!
//! TS walks the `tmpl` ESTree pre-order (estree-walker `enter`); the outermost node for which
//! `isStaticObjectOrArray` holds is replaced by an identifier and not descended into. Nodes are
//! deduped by their `astring` print, and `stcN` is numbered by first encounter.
//!
//! Static (optimize.ts isStaticObjectOrArray): an ObjectExpression whose properties are all plain
//! `key: value` (not computed, method, shorthand or spread) and an ArrayExpression with no holes,
//! where every value is a `Literal` (string, number, boolean, null) or itself static. `{}` and `[]`
//! are static. `-1` is a UnaryExpression and template literals are not Literals, so neither is static.
//!
//! The walk is over the oxc AST of the emitted function body (one parse). Grouping parentheses are
//! not nodes (`preserve_parens: false`), so replacing an object drops the parens astring had added
//! around it; call parentheses stay because they belong to the `CallExpression`.

use oxc_allocator::Allocator;
use oxc_ast::ast::{ArrayExpressionElement, Expression, ObjectPropertyKind, PropertyKind};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{ParseOptions, Parser};
use oxc_span::{GetSpan, SourceType, Span};
use std::collections::HashSet;

fn is_literal(e: &Expression) -> bool {
    matches!(
        e,
        Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
    )
}

fn is_static_value(e: &Expression) -> bool {
    is_literal(e)
        || matches!(
            e,
            Expression::ObjectExpression(_) | Expression::ArrayExpression(_)
        ) && is_static(e)
}

/// optimize.ts `isStaticObjectOrArray`.
fn is_static(e: &Expression) -> bool {
    match e {
        Expression::ObjectExpression(o) => o.properties.iter().all(|p| match p {
            ObjectPropertyKind::ObjectProperty(prop) => {
                !prop.computed
                    && !prop.method
                    && !prop.shorthand
                    && prop.kind == PropertyKind::Init
                    && is_static_value(&prop.value)
            }
            ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        Expression::ArrayExpression(a) => a
            .elements
            .iter()
            .all(|el| el.as_expression().is_some_and(is_static_value)),
        _ => false,
    }
}

fn slice(src: &str, span: Span) -> &str {
    src.get(span.start as usize..span.end as usize)
        .unwrap_or("")
}

/// astring print of a static literal (2-space indent at `depth`). Literal spellings are the source
/// text, so `'literal'` and `"a"` keep their quotes.
fn print_static(e: &Expression, src: &str, depth: usize) -> String {
    match e {
        Expression::ArrayExpression(a) => {
            if a.elements.is_empty() {
                return "[]".into();
            }
            let parts: Vec<String> = a
                .elements
                .iter()
                .filter_map(ArrayExpressionElement::as_expression)
                .map(|el| {
                    if is_literal(el) {
                        slice(src, el.span()).to_string()
                    } else {
                        print_static(el, src, depth)
                    }
                })
                .collect();
            format!("[{}]", parts.join(", "))
        }
        Expression::ObjectExpression(o) => {
            if o.properties.is_empty() {
                return "{}".into();
            }
            let pad = "  ".repeat(depth + 1);
            let props: Vec<String> = o
                .properties
                .iter()
                .filter_map(|p| match p {
                    ObjectPropertyKind::ObjectProperty(prop) => {
                        let val = if is_literal(&prop.value) {
                            slice(src, prop.value.span()).to_string()
                        } else {
                            print_static(&prop.value, src, depth + 1)
                        };
                        Some(format!("{pad}{}: {val}", slice(src, prop.key.span())))
                    }
                    ObjectPropertyKind::SpreadProperty(_) => None,
                })
                .collect();
            format!("{{\n{}\n{}}}", props.join(",\n"), "  ".repeat(depth))
        }
        _ => slice(src, e.span()).to_string(),
    }
}

/// Byte offset of each `(` that opens a call or `new` argument list.
struct CallParens<'a> {
    src: &'a str,
    at: HashSet<usize>,
}

impl<'a> Visit<'a> for CallParens<'a> {
    fn visit_call_expression(&mut self, it: &oxc_ast::ast::CallExpression<'a>) {
        self.mark(it.callee.span().end, it.span().end);
        walk::walk_call_expression(self, it);
    }
    fn visit_new_expression(&mut self, it: &oxc_ast::ast::NewExpression<'a>) {
        if !it.arguments.is_empty() {
            self.mark(it.callee.span().end, it.span().end);
        }
        walk::walk_new_expression(self, it);
    }
}

impl<'a> CallParens<'a> {
    fn mark(&mut self, from: u32, to: u32) {
        let (from, to) = (from as usize, to as usize);
        if let Some(i) = self.src.get(from..to).and_then(|s| s.find('(')) {
            self.at.insert(from + i);
        }
    }
}

/// Extend `start..end` over grouping parentheses. A `(` that opens a call is left in place.
fn expand_grouping(
    src: &str,
    mut start: usize,
    mut end: usize,
    calls: &HashSet<usize>,
) -> (usize, usize) {
    loop {
        let before = src[..start].trim_end();
        if !before.ends_with('(') {
            break;
        }
        let paren = before.len() - 1;
        if calls.contains(&paren) {
            break;
        }
        let after = src[end..].trim_start();
        if !after.starts_with(')') {
            break;
        }
        let ws = src[end..].len() - after.len();
        start = paren;
        end += ws + 1;
    }
    (start, end)
}

struct Hoist<'a> {
    src: &'a str,
    calls: &'a HashSet<usize>,
    repl: Vec<(usize, usize, usize)>,
    decls: &'a mut Vec<(String, String)>,
}

impl<'a> Visit<'a> for Hoist<'a> {
    fn visit_expression(&mut self, it: &Expression<'a>) {
        if is_static(it) {
            let text = print_static(it, self.src, 0);
            let n = match self.decls.iter().position(|(k, _)| *k == text) {
                Some(n) => n,
                None => {
                    self.decls.push((text.clone(), text));
                    self.decls.len() - 1
                }
            };
            let span = it.span();
            let (from, to) =
                expand_grouping(self.src, span.start as usize, span.end as usize, self.calls);
            self.repl.push((from, to, n));
            return;
        }
        walk::walk_expression(self, it);
    }
}

/// Hoist static literals out of `body` (the tmpl function's return expression, or a statement in tests).
/// `decls` holds the `(dedupe key, astring print)` of each `stcN` in order; returns the rewritten body.
///
/// `return` is only legal inside a function, and the unit inputs include one, so the body is parsed
/// as the body of `function tmpl()`. Spans are shifted back onto `body`.
pub(super) fn hoist_static_expressions(body: &str, decls: &mut Vec<(String, String)>) -> String {
    let prefix = "function tmpl(){\n";
    let wrapped = format!("{prefix}{body}\n}}");
    let base = prefix.len();
    let alloc = Allocator::default();
    let ret = Parser::new(&alloc, &wrapped, SourceType::script())
        .with_options(ParseOptions {
            preserve_parens: false,
            ..ParseOptions::default()
        })
        .parse();
    if !ret.diagnostics.is_empty() {
        return body.to_string();
    }
    let mut calls = CallParens {
        src: &wrapped,
        at: HashSet::new(),
    };
    calls.visit_program(&ret.program);
    let mut hoist = Hoist {
        src: &wrapped,
        calls: &calls.at,
        repl: Vec::new(),
        decls,
    };
    hoist.visit_program(&ret.program);
    hoist.repl.sort_by_key(|r| r.0);
    let mut out = String::with_capacity(body.len());
    let mut last = 0;
    for (from, to, n) in hoist.repl {
        let (from, to) = (from.saturating_sub(base), to.saturating_sub(base));
        if from < last || to > body.len() || from > to {
            continue;
        }
        out.push_str(&body[last..from]);
        out.push_str(&format!("stc{n}"));
        last = to;
    }
    out.push_str(&body[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> (String, Vec<String>) {
        let mut d = Vec::new();
        let out = hoist_static_expressions(src, &mut d);
        (out, d.into_iter().map(|(_, p)| p).collect())
    }

    #[test]
    fn hoists_outermost_static_and_dedupes() {
        let (out, d) = run(
            r#"return [api_element("div", { classMap: { "a": true }, key: 0 }), api_element("p", { key: 1 }, [api_text("x")]), api_element("i", { key: 0, classMap: { "a": true } })];"#,
        );
        assert_eq!(
            out,
            r#"return [api_element("div", stc0), api_element("p", stc1, [api_text("x")]), api_element("i", stc2)];"#
        );
        assert_eq!(
            d,
            vec![
                "{\n  classMap: {\n    \"a\": true\n  },\n  key: 0\n}",
                "{\n  key: 1\n}",
                "{\n  key: 0,\n  classMap: {\n    \"a\": true\n  }\n}"
            ]
        );
        let (out, d) = run(r#"f({ key: 1 }, { key: 1, }, [], {})"#);
        assert_eq!(out, "f(stc0, stc0, stc1, stc2)");
        assert_eq!(d, vec!["{\n  key: 1\n}", "[]", "{}"]);
    }

    #[test]
    fn non_static_shapes_are_kept_and_descended() {
        for src in [
            "f({ a: $cmp.x })",
            "f({ a: -1 })",
            "f({ a })",
            "f({ [k]: 1 })",
            "f({ ...x })",
            "f({ __proto__: null, ...x })",
            "f([1, , 2])",
            "f({ a: `t` })",
            "f([g()])",
            "a[0]",
            "x.y[\"z\"]",
            "function (a) { const {b: c} = a; }",
        ] {
            assert_eq!(run(src), (src.to_string(), vec![]), "{src}");
        }
        let (out, d) = run("f({ a: $cmp.x, b: { c: 1 }, d: [\"e\"] })");
        assert_eq!(out, "f({ a: $cmp.x, b: stc0, d: stc1 })");
        assert_eq!(d, vec!["{\n  c: 1\n}", "[\"e\"]"]);
    }

    #[test]
    fn grouping_parens_around_hoisted_object_are_dropped() {
        let (out, d) =
            run(r#"return [api_text(api_dynamic_text(({ obj: 'literal' }).toString()))];"#);
        assert_eq!(
            out,
            r#"return [api_text(api_dynamic_text(stc0.toString()))];"#
        );
        assert_eq!(d, vec!["{\n  obj: 'literal'\n}"]);
        let (out, d) = run(r#"f({ a: 1 })"#);
        assert_eq!(out, "f(stc0)", "{out}");
        assert_eq!(d, vec!["{\n  a: 1\n}"]);
        assert_eq!(run("typeof ({ a: 1 })").0, "typeof stc0");
        assert_eq!(run("!({ a: 1 })").0, "!stc0");
        assert_eq!(run("x in ({ a: 1 })").0, "x in stc0");
        assert_eq!(run("(({ a: 1 }))").0, "stc0");
        assert_eq!(run("foo({ a: 1 })").0, "foo(stc0)");
    }

    /// A string is opaque. An object inside a template substitution is a real expression, so the
    /// pre-order walk hoists it (oracle `` `t${stc0}` ``).
    #[test]
    fn strings_are_opaque_and_template_substitutions_hoist() {
        let (out, d) = run(r#"f("{ a: 1 }", `${ {a: 1} }`, { "k}": "v]" })"#);
        assert_eq!(out, r#"f("{ a: 1 }", `${ stc0 }`, stc1)"#);
        assert_eq!(d, vec!["{\n  a: 1\n}", "{\n  \"k}\": \"v]\"\n}"]);
    }
}
