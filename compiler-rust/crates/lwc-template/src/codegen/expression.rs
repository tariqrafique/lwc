//! Binder for complex template expressions.
//!
//! Upstream: `packages/@lwc/template-compiler/src/codegen/expression.ts`
//! `bindExpression` (lines 40-88) and `bindComplexExpression` (111-150), with
//! `ExpressionScopes` (157-192) and `collectParams` (194-249). SSR reaches the
//! same function via `bindExpression(node, isLocal, 'instance', true)`
//! (`ssr-compiler/src/compile-template/expression.ts`).
//!
//! `BindPlan` is a set of identifier spans to print as `<inst>.<name>`. It is
//! built only by [`bind`] (AB-D0004 condition 5). The instance name (`$cmp` or
//! `instance`) is applied by the printer in S4, not stored here.

use lwc_shared::template_ir::ComplexExpression;
use oxc_ast::ast::{
    BindingPattern, Expression, FormalParameters, ObjectProperty, PropertyKey,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::Span;
use std::collections::{HashMap, HashSet};

/// Identifier-reference spans that should be printed as `<inst>.<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindPlan {
    prefixed: HashSet<Span>,
}

impl BindPlan {
    /// Whether `span` (an `IdentifierReference` span on `ComplexExpression.ast` / `.source`) is prefixed.
    pub fn contains(&self, span: Span) -> bool { self.prefixed.contains(&span) }
    /// Prefixed spans in arbitrary order.
    pub fn iter(&self) -> impl Iterator<Item = Span> + '_ { self.prefixed.iter().copied() }
    pub fn len(&self) -> usize { self.prefixed.len() }
    pub fn is_empty(&self) -> bool { self.prefixed.is_empty() }
}

/// `INVALID_EXPR_ARROW_FN_PARAM` (1189): `collectParams` hit a pattern upstream rejects
/// (`expression.ts:210`, `:242` default parameters, `:248` member expressions).
///
/// `{0}` for default params is `AssignmentPattern`: upstream `isAssignmentPattern` compares
/// against the misspelling `AssigmentPattern` (estree.ts:46), so the dedicated "default
/// parameters" arm never runs and `collectParams` uses `node.type` (expression.ts:210).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindError {
    ArrowFnParam { kind: &'static str },
}

impl BindError {
    fn assignment_pattern() -> Self { Self::ArrowFnParam { kind: "AssignmentPattern" } }

    /// The `CompilerError` `invariant(false, INVALID_EXPR_ARROW_FN_PARAM, [kind])` would throw
    /// (`generateCompilerError`, no origin).
    pub fn compiler_error(&self) -> lwc_shared::errors::CompilerError {
        match self {
            Self::ArrowFnParam { kind } => lwc_shared::errors::CompilerError::generate(
                &lwc_shared::errors::ParserDiagnostics::INVALID_EXPR_ARROW_FN_PARAM,
                &[kind],
                None,
            ),
        }
    }
}

/// Bind unshadowed, non-local identifier references on `e.ast` (never `.source` / `.raw`).
///
/// Mirrors `bindExpression` + `bindComplexExpression`:
/// - A root `Identifier` is bound unless `is_local` (`expression.ts:46-51`), including `undefined`.
/// - Nested `undefined` is skipped (`expression.ts:134-135`).
/// - Non-computed member properties are skipped (`expression.ts:139`); oxc does not emit them as
///   `IdentifierReference`, so the walk does not see them.
/// - An object-property key is skipped when it *is* the key node (`expression.ts:140`), computed
///   or not: `{[c]: d}` leaves `c` unbound (AB-D0004 Q5 / design §3). Nested identifiers in a
///   computed key (`{[d.e]: f}`) are still walked.
/// - Arrow params are shadow-counted (`ExpressionScopes`, `expression.ts:161-191`).
pub fn bind<'a>(e: &ComplexExpression<'a>, is_local: &dyn Fn(&str) -> bool) -> Result<BindPlan, BindError> {
    if let Expression::Identifier(id) = e.ast {
        let mut prefixed = HashSet::new();
        if !is_local(id.name.as_str()) {
            prefixed.insert(id.span);
        }
        return Ok(BindPlan { prefixed });
    }
    let mut b = Binder { is_local, arrow_params: HashMap::new(), prefixed: HashSet::new(), error: None };
    b.visit_expression(e.ast);
    match b.error {
        Some(err) => Err(err),
        None => Ok(BindPlan { prefixed: b.prefixed }),
    }
}

struct Binder<'s> {
    is_local: &'s dyn Fn(&str) -> bool,
    arrow_params: HashMap<String, u32>,
    prefixed: HashSet<Span>,
    error: Option<BindError>,
}

impl Binder<'_> {
    fn collect_params(params: &FormalParameters<'_>) -> Result<Vec<String>, BindError> {
        let mut names = Vec::new();
        for item in &params.items {
            // FormalParameter::initializer, not BindingPattern::AssignmentPattern (oxc 0.156).
            if item.initializer.is_some() {
                return Err(BindError::assignment_pattern());
            }
            collect_pattern(&item.pattern, &mut names)?;
        }
        if let Some(rest) = &params.rest {
            collect_pattern(&rest.rest.argument, &mut names)?;
        }
        Ok(names)
    }
}

fn collect_pattern(pat: &BindingPattern<'_>, names: &mut Vec<String>) -> Result<(), BindError> {
    match pat {
        BindingPattern::BindingIdentifier(id) => { names.push(id.name.to_string()); Ok(()) }
        BindingPattern::ObjectPattern(o) => {
            for p in &o.properties { collect_pattern(&p.value, names)?; }
            if let Some(rest) = &o.rest { collect_pattern(&rest.argument, names)?; }
            Ok(())
        }
        BindingPattern::ArrayPattern(a) => {
            for el in &a.elements {
                if let Some(el) = el { collect_pattern(el, names)?; }
            }
            if let Some(rest) = &a.rest { collect_pattern(&rest.argument, names)?; }
            Ok(())
        }
        // expression.ts:241-242 collectParamsFromAssignmentPattern (destructure defaults)
        BindingPattern::AssignmentPattern(_) => Err(BindError::assignment_pattern()),
    }
}

impl<'a> Visit<'a> for Binder<'_> {
    fn visit_arrow_function_expression(&mut self, it: &oxc_ast::ast::ArrowFunctionExpression<'a>) {
        if self.error.is_some() { return; }
        let names = match Self::collect_params(&it.params) {
            Ok(n) => n,
            Err(e) => { self.error = Some(e); return; }
        };
        for n in &names { *self.arrow_params.entry(n.clone()).or_default() += 1; }
        self.visit_arrow_function_body(&it.body);
        for n in &names {
            if let Some(c) = self.arrow_params.get_mut(n) { *c -= 1; }
        }
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if self.error.is_some() { return; }
        match &it.key {
            // `parent.key === node` (expression.ts:140): static keys and computed identifier keys
            // (`{[c]: d}`) are not bound. Nested computed keys (`{[d.e]: f}`) fall through.
            PropertyKey::StaticIdentifier(_) | PropertyKey::PrivateIdentifier(_) | PropertyKey::Identifier(_) => {
                self.visit_expression(&it.value);
            }
            _ => walk::walk_object_property(self, it),
        }
    }

    fn visit_identifier_reference(&mut self, it: &oxc_ast::ast::IdentifierReference<'a>) {
        if self.error.is_some() { return; }
        let name = it.name.as_str();
        if name == "undefined" { return; }
        if (self.is_local)(name) { return; }
        if self.arrow_params.get(name).is_some_and(|c| *c > 0) { return; }
        self.prefixed.insert(it.span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::expression_complex::build;
    use lwc_shared::template_ir::SourceLocation;
    use oxc_allocator::Allocator;

    const AT: SourceLocation = SourceLocation { start_line: 1, start_column: 1, end_line: 1, end_column: 20, start: 0, end: 19 };

    fn names(raw: &str, locals: &[&str]) -> Vec<String> {
        let alloc = Allocator::default();
        let c = build(&alloc, raw, AT).expect(raw);
        let p = bind(&c, &|n| locals.iter().any(|s| *s == n)).expect(raw);
        let mut got: Vec<(u32, String)> = p.iter().map(|sp| {
            (sp.start, c.source[sp.start as usize..sp.end as usize].to_string())
        }).collect();
        got.sort_by_key(|(s, _)| *s);
        got.into_iter().map(|(_, n)| n).collect()
    }

    #[test]
    fn root_identifier_and_undefined() {
        // bindExpression Identifier branch (expression.ts:46-51): even `undefined` is bound at the root.
        assert_eq!(names("{foo}", &[]), ["foo"]);
        assert_eq!(names("{undefined}", &[]), ["undefined"]);
        assert_eq!(names("{foo}", &["foo"]), [] as [&str; 0]);
        // Nested `undefined` is skipped (expression.ts:134-135).
        assert_eq!(names("{a === undefined}", &[]), ["a"]);
        assert_eq!(names("{x => undefined}", &[]), [] as [&str; 0]);
    }

    #[test]
    fn computed_key_unbound_static_key_unbound() {
        // `{[c]: d}` → `[c]: $cmp.d` (Q5). `{a, b: c}` binds the shorthand value and `c`, not the key `b`.
        assert_eq!(names("{{[c]: d}}", &[]), ["d"]);
        assert_eq!(names("{{a, b: c, [d]: e}}", &[]), ["a", "c", "e"]);
        assert_eq!(names("{{[d.e]: f}}", &[]), ["d", "f"]);
        assert_eq!(names("{a.b.c}", &[]), ["a"]);
    }

    #[test]
    fn arrow_params_are_shadow_counted() {
        assert_eq!(names("{xs.map(x => x + y)}", &[]), ["xs", "y"]);
        assert_eq!(names("{(x => x)(x)}", &[]), ["x"]);
        assert_eq!(names("{({a, b: [c, ...d]}) => a + b + c + d + e}", &[]), ["b", "e"]);
        assert_eq!(names("{item => item.id === selectedId}", &["item"]), ["selectedId"]);
    }

    #[test]
    fn default_params_are_1189() {
        let alloc = Allocator::default();
        let c = build(&alloc, "{(x = 1) => x}", AT).unwrap();
        assert_eq!(bind(&c, &|_| false).unwrap_err(), BindError::assignment_pattern());
    }

    /// Differential test against JS `bindExpression` (complex mode). Oracle:
    /// `crates/lwc-template/tests/fixtures/bind_corpus.json`, generated by
    /// `harness/scripts/gen-bind-corpus.mjs` (UTF-16 offsets into each case's `src`).
    /// `BindPlan` does not store the instance name; both `$cmp` and `instance` prefix the same spans.
    #[test]
    fn js_bind_corpus() {
        let path = lwc_shared::cargo_manifest_dir!().join("tests/fixtures/bind_corpus.json");
        let corpus: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let (mut n, mut skipped, mut bad) = (0usize, 0usize, vec![]);
        let alloc = Allocator::default();
        for c in &corpus {
            let Some(want) = c["prefixed"].as_array() else { skipped += 1; continue };
            let src = c["src"].as_str().unwrap();
            let scope: Vec<String> = c["scope"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
            let want: Vec<(u64, &str)> = want.iter().map(|p| (p[0].as_u64().unwrap(), p[1].as_str().unwrap())).collect();
            n += 1;
            let raw = format!("{{{src}}}");
            let Ok(expr) = build(&alloc, &raw, AT) else {
                bad.push(format!("{} {src:?} scope={scope:?}\n   js={want:?}\n rust=build failed", c["origin"]));
                continue;
            };
            let plan = match bind(&expr, &|n| scope.iter().any(|s| s == n)) {
                Ok(p) => p,
                Err(e) => {
                    bad.push(format!("{} {src:?} scope={scope:?}\n   js={want:?}\n rust=bind error {e:?}", c["origin"]));
                    continue;
                }
            };
            // build() trims; corpus offsets are UTF-16 into the raw `src`.
            let prefix = src.len() - src.trim_start().len();
            let mut got: Vec<(u64, String)> = plan.iter().map(|sp| {
                let byte = prefix + sp.start as usize;
                let name = expr.source[sp.start as usize..sp.end as usize].to_string();
                (src[..byte].encode_utf16().count() as u64, name)
            }).collect();
            got.sort_by_key(|(s, _)| *s);
            let got_ref: Vec<(u64, &str)> = got.iter().map(|(s, n)| (*s, n.as_str())).collect();
            if got_ref != want {
                bad.push(format!("{} {src:?} scope={scope:?}\n   js={want:?}\n rust={got_ref:?}", c["origin"]));
                continue;
            }
            // BindPlan is inst-agnostic; both CSR `$cmp` and SSR `instance` prefix the same spans.
            for inst in ["$cmp", "instance"] {
                let got_m: Vec<(u64, String)> = got.iter().map(|(s, n)| (*s, format!("{inst}.{n}"))).collect();
                let want_m: Vec<(u64, String)> = want.iter().map(|(s, n)| (*s, format!("{inst}.{n}"))).collect();
                if got_m != want_m {
                    bad.push(format!("{} {src:?} inst={inst} scope={scope:?}\n   js={want_m:?}\n rust={got_m:?}", c["origin"]));
                }
            }
        }
        assert!(n > 800, "corpus too small: {n}");
        assert!(bad.is_empty(), "{} / {n} divergences (skipped {skipped} without prefixed):\n{}", bad.len(), bad.join("\n"));
    }
}
