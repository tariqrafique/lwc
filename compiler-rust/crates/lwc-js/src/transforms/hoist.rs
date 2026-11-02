//! Babel-scope emulation shared by the class-features passes: program-wide unique identifiers
//! (`scope.generateUid`) and per-function hoisted declarations (`scope.push`).
use crate::edits::Edit;
use oxc::ast::ast::*;
use oxc::ast_visit::Visit;
use std::collections::HashSet;

/// Every identifier name the program references, binds or labels, plus names generated so far.
pub struct Uids { used: HashSet<String>, violated: HashSet<String>, bound: HashSet<String> }

impl Uids {
    pub fn new(program: &Program<'_>) -> Self {
        #[derive(Default)]
        struct C { used: HashSet<String>, violated: HashSet<String>, bound: HashSet<String> }
        impl<'a> Visit<'a> for C {
            fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) { self.used.insert(it.name.to_string()); }
            fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) { self.used.insert(it.name.to_string()); self.bound.insert(it.name.to_string()); }
            fn visit_label_identifier(&mut self, it: &LabelIdentifier<'a>) { self.used.insert(it.name.to_string()); }
            fn visit_simple_assignment_target(&mut self, it: &SimpleAssignmentTarget<'a>) {
                if let SimpleAssignmentTarget::AssignmentTargetIdentifier(i) = it { self.violated.insert(i.name.to_string()); }
                oxc::ast_visit::walk::walk_simple_assignment_target(self, it);
            }
            fn visit_assignment_target_property_identifier(&mut self, it: &AssignmentTargetPropertyIdentifier<'a>) {
                self.violated.insert(it.binding.name.to_string());
                oxc::ast_visit::walk::walk_assignment_target_property_identifier(self, it);
            }
        }
        let mut c = C::default();
        c.visit_program(program);
        Self { used: c.used, violated: c.violated, bound: c.bound }
    }
    pub fn is_used(&self, name: &str) -> bool { self.used.contains(name) }
    /// Babel `path.isConstantExpression()` (bindings resolved by name, not scope).
    pub fn is_constant_expression(&self, e: &Expression<'_>) -> bool {
        match e.get_inner_expression() {
            Expression::Identifier(i) => self.bound.contains(i.name.as_str()) && !self.violated.contains(i.name.as_str()),
            Expression::RegExpLiteral(_) => false,
            Expression::StringLiteral(_) | Expression::NumericLiteral(_) | Expression::BigIntLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral(_) => true,
            Expression::TemplateLiteral(t) => t.expressions.iter().all(|x| self.is_constant_expression(x)),
            Expression::UnaryExpression(u) => u.operator == UnaryOperator::Void && self.is_constant_expression(&u.argument),
            Expression::BinaryExpression(b) => !matches!(b.operator, BinaryOperator::In | BinaryOperator::Instanceof) && self.is_constant_expression(&b.left) && self.is_constant_expression(&b.right),
            Expression::StaticMemberExpression(m) => matches!(&m.object, Expression::Identifier(o) if o.name == "Symbol") && !self.bound.contains("Symbol"),
            Expression::CallExpression(c) => c.arguments.len() == 1 && matches!(&c.arguments[0], Argument::StringLiteral(_))
                && matches!(&c.callee, Expression::StaticMemberExpression(m) if m.property.name == "for" && matches!(&m.object, Expression::Identifier(o) if o.name == "Symbol"))
                && !self.bound.contains("Symbol"),
            _ => false,
        }
    }
    /// `name` itself when free, else [`Self::generate`]`(name)`.
    pub fn reserve(&mut self, name: &str) -> String {
        if self.used.insert(name.to_string()) { name.to_string() } else { self.generate(name) }
    }
    /// Babel `generateUid(name)`: `_name`, `_name2`, ... after stripping leading `_` and trailing digits.
    pub fn generate(&mut self, name: &str) -> String {
        let base = name.trim_start_matches('_').trim_end_matches(|c: char| c.is_ascii_digit());
        let mut i = 1;
        loop {
            let id = if i > 1 { format!("_{base}{i}") } else { format!("_{base}") };
            i += 1;
            if self.used.insert(id.clone()) { return id; }
        }
    }
    /// Babel `generateUidBasedOnNode`.
    pub fn generate_for(&mut self, e: &Expression<'_>, default: &str) -> String {
        self.generate_parts(&parts(e), default)
    }
    pub fn generate_parts(&mut self, parts: &[String], default: &str) -> String {
        let mut id = parts.join("$");
        id = id.trim_start_matches('_').to_string();
        if id.is_empty() { id = default.to_string(); }
        let id: String = id.chars().take(20).collect();
        self.generate(&id)
    }
}

pub fn parts(e: &Expression<'_>) -> Vec<String> {
    let mut out = Vec::new();
    node_parts(e, &mut out);
    out
}

fn node_parts(e: &Expression<'_>, out: &mut Vec<String>) {
    match e.get_inner_expression() {
        Expression::Identifier(i) => out.push(i.name.to_string()),
        Expression::ThisExpression(_) => out.push("this".into()),
        Expression::Super(_) => out.push("super".into()),
        Expression::StringLiteral(s) => { if let Some(v) = s.value.as_str() { out.push(v.to_string()); } }
        Expression::NumericLiteral(n) => out.push(n.raw.as_ref().map_or_else(|| n.value.to_string(), |r| r.to_string())),
        Expression::StaticMemberExpression(m) => { node_parts(&m.object, out); out.push(m.property.name.to_string()); }
        Expression::ComputedMemberExpression(m) => { node_parts(&m.object, out); node_parts(&m.expression, out); }
        Expression::PrivateFieldExpression(m) => { node_parts(&m.object, out); out.push(m.field.name.to_string()); }
        Expression::CallExpression(c) => node_parts(&c.callee, out),
        Expression::NewExpression(c) => node_parts(&c.callee, out),
        Expression::ChainExpression(c) => match &c.expression {
            ChainElement::CallExpression(c) => node_parts(&c.callee, out),
            ChainElement::StaticMemberExpression(m) => { node_parts(&m.object, out); out.push(m.property.name.to_string()); }
            ChainElement::ComputedMemberExpression(m) => { node_parts(&m.object, out); node_parts(&m.expression, out); }
            ChainElement::PrivateFieldExpression(m) => { node_parts(&m.object, out); out.push(m.field.name.to_string()); }
            _ => {}
        },
        _ => {}
    }
}

enum At { Block(u32), Arrow(u32, u32) }
struct Frame { slot: usize, at: At, vars: Vec<String>, lets: Vec<String> }

/// Stack of function scopes. Entering reserves a placeholder edit so the declaration lands before any
/// same-offset insert emitted by children; leaving fills it (`var a, b;` / arrow body -> block).
#[derive(Default)]
pub struct Scopes { frames: Vec<Frame> }

impl Scopes {
    pub fn enter_program(&mut self, p: &Program<'_>, edits: &mut Vec<Edit>) {
        let at = p.directives.last().map_or(p.span.start, |d| d.span.end);
        self.push(edits, At::Block(at));
    }
    pub fn enter_body(&mut self, b: &FunctionBody<'_>, edits: &mut Vec<Edit>) {
        let at = b.directives.last().map_or(b.span.start + 1, |d| d.span.end);
        self.push(edits, At::Block(at));
    }
    pub fn enter_arrow(&mut self, a: &ArrowFunctionExpression<'_>, edits: &mut Vec<Edit>) {
        match &a.body {
            ArrowFunctionBody::FunctionBody(b) => self.enter_body(b, edits),
            body => {
                let s = oxc::span::GetSpan::span(body);
                self.push(edits, At::Arrow(s.start, s.end));
            }
        }
    }
    fn push(&mut self, edits: &mut Vec<Edit>, at: At) {
        let pos = match at { At::Block(p) | At::Arrow(p, _) => p };
        edits.push(Edit::insert(pos, ""));
        self.frames.push(Frame { slot: edits.len() - 1, at, vars: vec![], lets: vec![] });
    }
    pub fn var(&mut self, name: String) { self.frames.last_mut().expect("scope").vars.push(name); }
    pub fn let_(&mut self, name: String) { self.frames.last_mut().expect("scope").lets.push(name); }
    pub fn exit(&mut self, edits: &mut Vec<Edit>) {
        let f = self.frames.pop().expect("scope");
        let mut decl = String::new();
        if !f.lets.is_empty() { decl += &format!("let {};\n", f.lets.join(", ")); }
        if !f.vars.is_empty() { decl += &format!("var {};\n", f.vars.join(", ")); }
        match f.at {
            At::Block(_) => { if !decl.is_empty() { edits[f.slot].text = format!("\n{decl}"); } }
            At::Arrow(_, end) => {
                if !decl.is_empty() {
                    edits[f.slot].text = format!("{{\n{decl}return (");
                    edits.push(Edit::insert(end, ");\n}"));
                }
            }
        }
    }
}

/// Walk helper so visitors can wrap function bodies with `Scopes` without re-implementing traversal.
pub fn is_literal_key(e: &Expression<'_>) -> bool {
    matches!(e.get_inner_expression(), Expression::StringLiteral(_) | Expression::NumericLiteral(_) | Expression::BigIntLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral(_))
        || matches!(e.get_inner_expression(), Expression::TemplateLiteral(t) if t.expressions.is_empty())
}

