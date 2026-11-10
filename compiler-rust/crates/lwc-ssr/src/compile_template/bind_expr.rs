//! Port of template-compiler `bindComplexExpression` (codegen/expression.ts), reached via `bindExpression(expr,
//! isLocal, 'instance', experimentalComplexExpressions=true)`: identifier references become `instance.x` unless
//! - the name is a template-local (for:each item/index, iterator, slot data) in `scope`,
//! - it is an arrow-function parameter still in scope at that point (JS `ExpressionScopes`: lexical, shadow-counted),
//! - it is `undefined` (acorn parses it as an Identifier; skipped unless it is the whole expression), or
//! - it is directly the key of an object property, computed or not (JS checks `parent.key === node`; so `{[d]: e}`
//!   leaves `d` unbound, an upstream quirk mirrored for parity).
//! Differential-tested against the JS implementation by `js_bind_corpus` (harness/scripts/gen-bind-corpus.mjs).
use oxc::allocator::Allocator;
use oxc::ast::ast::{ArrowFunctionExpression, BindingIdentifier, Expression, IdentifierReference, ObjectProperty, PropertyKey};
use oxc::ast_visit::{walk, Visit};
use oxc::span::SourceType;
use std::collections::HashMap;

/// A reference to prefix: (byte offset, name, is the value of a shorthand property).
type Ref = (u32, String, bool);

struct Binder<'s> { scope: &'s [String], arrow_params: HashMap<String, u32>, refs: Vec<Ref>, shorthand_value: Option<u32> }

#[derive(Default)]
struct ParamNames(Vec<String>);
impl<'a> Visit<'a> for ParamNames {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) { self.0.push(it.name.to_string()); }
    // Default-param initializers are rejected upstream (INVALID_EXPR_ARROW_FN_PARAM); don't treat their refs as params.
    fn visit_expression(&mut self, _: &Expression<'a>) {}
}

impl<'a> Visit<'a> for Binder<'_> {
    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let mut p = ParamNames::default();
        p.visit_formal_parameters(&it.params);
        for n in &p.0 { *self.arrow_params.entry(n.clone()).or_default() += 1; }
        self.visit_arrow_function_body(&it.body);
        for n in &p.0 { *self.arrow_params.get_mut(n).unwrap() -= 1; }
    }
    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if it.shorthand {
            if let Expression::Identifier(id) = &it.value { self.shorthand_value = Some(id.span.start); }
            self.visit_expression(&it.value);
            self.shorthand_value = None;
            return;
        }
        match &it.key {
            PropertyKey::Identifier(_) => self.visit_expression(&it.value), // `parent.key === node`: never bound
            _ => walk::walk_object_property(self, it),
        }
    }
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        let name = it.name.as_str();
        if name == "undefined" || self.scope.iter().any(|s| s == name) || self.arrow_params.get(name).is_some_and(|c| *c > 0) {
            return;
        }
        self.refs.push((it.span.start, name.to_string(), self.shorthand_value == Some(it.span.start)));
    }
}

/// The identifier references `bind` prefixes with `instance.`, sorted by offset; `None` if oxc can't parse `src`.
pub(crate) fn free_refs(src: &str, scope: &[String]) -> Option<Vec<Ref>> {
    let alloc = Allocator::default();
    let e = oxc::parser::Parser::new(&alloc, src, SourceType::mjs()).parse_expression().ok()?;
    let mut root = &e;
    while let Expression::ParenthesizedExpression(p) = root { root = &p.expression; }
    // bindExpression's Identifier branch: the whole expression is bound unless local (even `undefined`).
    if let Expression::Identifier(id) = root {
        return Some(if scope.iter().any(|s| s == id.name.as_str()) { vec![] } else { vec![(id.span.start, id.name.to_string(), false)] });
    }
    let mut b = Binder { scope, arrow_params: HashMap::new(), refs: vec![], shorthand_value: None };
    b.visit_expression(&e);
    b.refs.sort_by_key(|r| r.0);
    Some(b.refs)
}

/// Returns `(src)` with free refs prefixed. Falls back to `(src)` unchanged if oxc can't parse it.
pub(crate) fn bind(src: &str, scope: &[String]) -> String {
    let Some(refs) = free_refs(src, scope) else { return format!("({src})") };
    let mut out = String::with_capacity(src.len() + 16);
    let mut last = 0usize;
    for (start, name, short) in refs {
        let s = start as usize;
        out.push_str(&src[last..s]);
        if short { out.push_str(&name); out.push_str(": "); }
        out.push_str("instance.");
        last = s;
    }
    out.push_str(&src[last..]);
    format!("({out})")
}

#[cfg(test)]
mod tests {
    use super::{bind, free_refs};
    #[test]
    fn binds() {
        assert_eq!(bind("isTrue()", &[]), "(instance.isTrue())");
        assert_eq!(bind("a + b.c", &["a".into()]), "(a + instance.b.c)");
        assert_eq!(bind("{a, b: c}", &[]), "({a: instance.a, b: instance.c})");
        assert_eq!(bind("xs.map(x => x + y)", &[]), "(instance.xs.map(x => x + instance.y))");
        assert_eq!(bind("(x => x)(x)", &[]), "((x => x)(instance.x))");
        assert_eq!(bind("a === undefined", &[]), "(instance.a === undefined)");
    }

    /// Differential test against JS `bindExpression` (complex-expression mode). Regenerate the corpus with
    /// `node harness/scripts/gen-bind-corpus.mjs` (needs the pinned oracle built), then `cargo test -p lwc-ssr js_bind_corpus`.
    #[test]
    fn js_bind_corpus() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/bind_corpus.json");
        let corpus: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let (mut n, mut bad) = (0, vec![]);
        for c in &corpus {
            let Some(want) = c["prefixed"].as_array() else { continue }; // JS threw: nothing to compare
            let src = c["src"].as_str().unwrap();
            let scope: Vec<String> = c["scope"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
            let want: Vec<(u64, &str)> = want.iter().map(|p| (p[0].as_u64().unwrap(), p[1].as_str().unwrap())).collect();
            n += 1;
            // acorn offsets are UTF-16 code units; oxc spans are UTF-8 bytes.
            let got: Option<Vec<(u64, &str)>> = free_refs(src, &scope).map(|rs| rs.iter().map(|(b, name, _)| {
                (src[..*b as usize].encode_utf16().count() as u64, &src[*b as usize..*b as usize + name.len()])
            }).collect());
            if got.as_ref() != Some(&want) { bad.push(format!("{} {src:?} scope={scope:?}\n   js={want:?}\n rust={got:?}", c["origin"])); }
        }
        assert!(n > 800, "corpus too small: {n}");
        assert!(bad.is_empty(), "{} / {n} divergences:\n{}", bad.len(), bad.join("\n"));
    }
}
