//! compile-js/index.ts `MethodDefinition` + `Super` visitors for LightningElement subclasses
//! (`state.currentComponent`): decorated `@wire` getters/setters become plain fields, the
//! constructor gains a trailing `propsAvailableAtConstruction` param that is forwarded to
//! `super(...)`, and `renderedCallback` / `disconnectedCallback` / `errorCallback` are removed.
//! Source-span edits over an oxc parse; on a parse failure the source passes through unchanged.
use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast_visit::{walk, Visit};
use oxc::span::{GetSpan, SourceType};
use oxc::syntax::scope::ScopeFlags;

const PROPS_ARG: &str = "propsAvailableAtConstruction";

/// `Err` carries the uncoded error message compile-js throws.
pub(crate) fn rewrite_component_classes(src: &str) -> Result<String, String> {
    let alloc = Allocator::default();
    let ret = oxc::parser::Parser::new(&alloc, src, SourceType::mjs()).parse();
    if ret.diagnostics.has_errors() { return Ok(src.to_string()); }
    let Some(le) = lightning_element_local(&ret.program) else { return Ok(src.to_string()) };
    let mut v = ClassVisitor { src, le, edits: Vec::new(), err: None };
    v.visit_program(&ret.program);
    if let Some(e) = v.err { return Err(e); }
    let mut edits = v.edits;
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut out = src.to_string();
    let mut last = usize::MAX;
    for (s, e, text) in edits {
        if e > last { continue; } // overlapping (e.g. super() inside a removed method): outer wins
        out.replace_range(s..e, &text);
        last = s;
    }
    Ok(out)
}

/// Local binding of `import { LightningElement } from 'lwc'`.
fn lightning_element_local(p: &Program) -> Option<String> {
    for s in &p.body {
        let Statement::ImportDeclaration(d) = s else { continue };
        if d.source.value != "lwc" { continue; }
        for sp in d.specifiers.iter().flatten() {
            if let ImportDeclarationSpecifier::ImportSpecifier(x) = sp {
                if x.imported.name() == "LightningElement" { return Some(x.local.name.to_string()); }
            }
        }
    }
    None
}

struct ClassVisitor<'s> { src: &'s str, le: String, edits: Vec<(usize, usize, String)>, err: Option<String> }

fn is_wire(d: &Decorator) -> bool {
    matches!(&d.expression, Expression::CallExpression(c) if matches!(&c.callee, Expression::Identifier(i) if i.name == "wire"))
}

impl<'a> Visit<'a> for ClassVisitor<'_> {
    fn visit_class(&mut self, c: &Class<'a>) {
        let is_cmp = matches!(c.heritage.as_ref().map(|h| &h.expression), Some(Expression::Identifier(i)) if i.name == self.le.as_str());
        if is_cmp {
            for el in &c.body.body {
                if let ClassElement::MethodDefinition(m) = el { self.method(m); }
            }
        }
        walk::walk_class(self, c);
    }
}

impl ClassVisitor<'_> {
    fn method(&mut self, m: &MethodDefinition) {
        // isKeyIdentifier: Identifier key (computed `[x]` included), not private / literal.
        let name = match &m.key {
            PropertyKey::StaticIdentifier(i) => i.name.to_string(),
            PropertyKey::Identifier(i) => i.name.to_string(),
            _ => return,
        };
        if m.decorators.first().is_some_and(is_wire) {
            if m.computed {
                self.err.get_or_insert_with(|| "@wire cannot be used on computed properties in SSR context.".into());
                return;
            }
            if m.kind != MethodDefinitionKind::Method {
                // Getter/setter → `[static] name;` (decorators kept; later passes catalog them).
                let dec_end = m.decorators.iter().map(|d| d.span.end).max().unwrap_or(m.span.start) as usize;
                let rest = &self.src[dec_end..m.span.end as usize];
                let start = dec_end + (rest.len() - rest.trim_start().len());
                let text = format!("{}{name};", if m.r#static { "static " } else { "" });
                self.edits.push((start, m.span.end as usize, text));
                return;
            }
        }
        if m.computed { return; }
        match name.as_str() {
            "constructor" => {
                let p = &m.value.params;
                let last_end = p.items.last().map(|i| i.span.end).into_iter().chain(p.rest.as_ref().map(|r| r.span.end)).max();
                match last_end {
                    Some(e) => self.edits.push((e as usize, e as usize, format!(", {PROPS_ARG}"))),
                    None => self.edits.push((p.span.end as usize - 1, p.span.end as usize - 1, PROPS_ARG.into())),
                }
                if let Some(body) = &m.value.body {
                    let mut sv = SuperVisitor { edits: &mut self.edits };
                    for s in &body.statements { sv.visit_statement(s); }
                }
            }
            "renderedCallback" | "disconnectedCallback" | "errorCallback" => {
                self.edits.push((m.span.start as usize, m.span.end as usize, String::new()));
            }
            _ => {}
        }
    }
}

/// `super(...)` calls whose function parent is the constructor (nested functions/arrows skipped).
struct SuperVisitor<'e> { edits: &'e mut Vec<(usize, usize, String)> }

impl<'a> Visit<'a> for SuperVisitor<'_> {
    fn visit_call_expression(&mut self, c: &CallExpression<'a>) {
        if matches!(c.callee, Expression::Super(_)) {
            match c.arguments.last() {
                Some(a) => { let e = a.span().end as usize; self.edits.push((e, e, format!(", {PROPS_ARG}"))); }
                None => { let e = c.span.end as usize - 1; self.edits.push((e, e, PROPS_ARG.into())); }
            }
        }
        walk::walk_call_expression(self, c);
    }
    fn visit_function(&mut self, _f: &Function<'a>, _flags: ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _f: &ArrowFunctionExpression<'a>) {}
    fn visit_class(&mut self, _c: &Class<'a>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctor_and_lifecycle() {
        let src = "import { LightningElement } from 'lwc';\nexport default class A extends LightningElement {\n  constructor() { super(); const f = () => super.x; }\n  renderedCallback() {}\n}\n";
        let out = rewrite_component_classes(src).unwrap();
        assert!(out.contains("constructor(propsAvailableAtConstruction) { super(propsAvailableAtConstruction);"), "{out}");
        assert!(!out.contains("renderedCallback"), "{out}");
    }
    #[test]
    fn wire_getter_becomes_field() {
        let src = "import { LightningElement, wire } from 'lwc';\nexport default class A extends LightningElement {\n  @wire(a, {})\n  get x() { return 1; }\n  set y(v) {}\n}\n";
        let out = rewrite_component_classes(src).unwrap();
        assert!(out.contains("@wire(a, {})\n  x;"), "{out}");
        assert!(out.contains("set y(v) {}"), "{out}");
    }
}

/// decorators/wire.ts `catalogWireAdapters` over every LightningElement subclass, in source order:
/// `(adapterId source, reactive config source, field name, is a real method)`. A computed `[x]`
/// key keeps the identifier name (JS `instance.${key}` quirk). Only top-level `'$a.b'` string
/// config values become `instance["a"]["b"]`. `None` when the module does not parse.
pub(crate) fn collect_wire_adapters(src: &str) -> Option<Vec<(String, String, String, bool)>> {
    let alloc = Allocator::default();
    let ret = oxc::parser::Parser::new(&alloc, src, SourceType::mjs()).parse();
    if ret.diagnostics.has_errors() { return None; }
    let le = lightning_element_local(&ret.program)?;
    let mut v = WireCollector { src, le, out: Vec::new() };
    v.visit_program(&ret.program);
    Some(v.out)
}

struct WireCollector<'s> { src: &'s str, le: String, out: Vec<(String, String, String, bool)> }

impl<'a> Visit<'a> for WireCollector<'_> {
    fn visit_class(&mut self, c: &Class<'a>) {
        let is_cmp = matches!(c.heritage.as_ref().map(|h| &h.expression), Some(Expression::Identifier(i)) if i.name == self.le.as_str());
        if is_cmp {
            for el in &c.body.body {
                let (decorators, key, is_method) = match el {
                    ClassElement::PropertyDefinition(p) => (&p.decorators, &p.key, false),
                    ClassElement::MethodDefinition(m) => (&m.decorators, &m.key, m.kind == MethodDefinitionKind::Method),
                    _ => continue,
                };
                let name = match key {
                    PropertyKey::StaticIdentifier(i) => i.name.to_string(),
                    PropertyKey::Identifier(i) => i.name.to_string(),
                    _ => continue,
                };
                let Some(Expression::CallExpression(call)) = decorators.first().filter(|d| is_wire(d)).map(|d| &d.expression) else { continue };
                let Some(id) = call.arguments.first() else { continue };
                let id_src = self.src[id.span().start as usize..id.span().end as usize].to_string();
                let config = match call.arguments.get(1) {
                    Some(Argument::ObjectExpression(o)) => self.reactive_config(o),
                    Some(a) => self.src[a.span().start as usize..a.span().end as usize].to_string(),
                    None => "{}".to_string(),
                };
                self.out.push((id_src, config, name, is_method));
            }
        }
        walk::walk_class(self, c);
    }
}

impl WireCollector<'_> {
    fn reactive_config(&self, o: &ObjectExpression) -> String {
        let (start, end) = (o.span.start as usize, o.span.end as usize);
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        for p in &o.properties {
            let ObjectPropertyKind::ObjectProperty(p) = p else { continue };
            if let Expression::StringLiteral(s) = &p.value {
                if let Some(path) = s.value.as_str().and_then(|v| v.strip_prefix('$')) {
                    let chain: String = path.split('.').map(|seg| format!("[{seg:?}]")).collect();
                    edits.push((s.span.start as usize, s.span.end as usize, format!("instance{chain}")));
                }
            }
        }
        let mut out = String::new();
        let mut at = start;
        for (s, e, t) in edits {
            out.push_str(&self.src[at..s]);
            out.push_str(&t);
            at = e;
        }
        out.push_str(&self.src[at..end]);
        out
    }
}
