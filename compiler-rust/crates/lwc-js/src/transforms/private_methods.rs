//! Private-method sandwich (mirrors private-method-transform.ts / reverse-private-method-transform.ts).
//! flag off: any `#m()` method is rejected. flag on: forward renames `#m` (kind method only) and its
//! `x.#m` member uses to `__lwc_component_class_internal_private_m` so class-properties lowering
//! never sees it; reverse restores `#m` and checks every forward rename was reversed.
use crate::constants::PRIVATE_METHOD_PREFIX as P;
use lwc_shared::{CompileError, CompileOptions};
use oxc::ast::ast::{Class, ClassElement, Expression, StaticBlock, 
    MemberExpression, MethodDefinition, MethodDefinitionKind, PrivateFieldExpression, Program, PropertyKey,
};
use oxc::ast_visit::{walk, Visit};
use std::collections::BTreeSet;

use crate::edits::Edit;

/// Babel-thrown errors are normalized by transformSync to `LWC1007` (TransformerErrors.JS_TRANSFORMER_ERROR).
/// Babel-internal SyntaxError (class-features plugin / `buildCodeFrameError` with an uncoded message), wrapped as LWC1007.
fn err(filename: &str, m: impl Into<String>) -> CompileError { CompileError::babel("SyntaxError", m, filename, None, None) }

#[derive(Default)]
/// `decorated`: (first decorator's name, member span start, end); oxc member spans start at the decorator, like Babel's.
/// `in_decorated`: per method, whether its class carries any decorator (own or member).
struct Collect { methods: Vec<(String, Option<(String, u32, u32)>, MethodDefinitionKind)>, in_decorated: Vec<bool>, prefixed: Option<String>, static_block: Option<u32>, class_start: Vec<u32>, decorated: Vec<bool> }
impl<'a> Visit<'a> for Collect {
    fn visit_class(&mut self, it: &Class<'a>) {
        self.class_start.push(it.span.start);
        self.decorated.push(has_decorators(it));
        walk::walk_class(self, it);
        self.decorated.pop();
        self.class_start.pop();
    }
    fn visit_static_block(&mut self, it: &StaticBlock<'a>) {
        if self.static_block.is_none() { self.static_block = self.class_start.last().copied().or(Some(it.span.start)); }
        walk::walk_static_block(self, it);
    }
    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        match &it.key {
            PropertyKey::PrivateIdentifier(id) => {
                let decorated = it.decorators.first().map(|d| {
                    let callee = match &d.expression { Expression::CallExpression(c) => &c.callee, e => e };
                    let name = match callee { Expression::Identifier(i) => i.name.to_string(), _ => String::new() };
                    (name, it.span.start, it.span.end)
                });
                self.methods.push((id.name.to_string(), decorated, it.kind));
                self.in_decorated.push(self.decorated.last().copied().unwrap_or(false));
            }
            PropertyKey::StaticIdentifier(id) if it.kind == MethodDefinitionKind::Method && id.name.starts_with(P) => {
                self.prefixed.get_or_insert_with(|| id.name.to_string());
            }
            _ => {}
        }
        walk::walk_method_definition(self, it);
    }
}

fn has_decorators(c: &Class<'_>) -> bool {
    !c.decorators.is_empty()
        || c.body.body.iter().any(|e| match e {
            ClassElement::MethodDefinition(m) => !m.decorators.is_empty(),
            ClassElement::PropertyDefinition(p) => !p.decorators.is_empty(),
            ClassElement::AccessorProperty(a) => !a.decorators.is_empty(),
            _ => false,
        })
}

const NOT_ENABLED: &str = "Class private methods are not enabled. Please add `@babel/plugin-transform-private-methods` to your configuration.";

/// `Forward::Deferred` (flag off): the first private method sits in a decorated class. Shipped runs the
/// LWC decorator visitor first: either the decorators stay (class-features then throws its decorated-class
/// error, decorators.rs `fatal`) or they are removed and class-features rejects the method afterwards;
/// so the "not enabled" check runs on the step-2 output (`not_enabled`).
pub enum Forward { Done(Vec<Edit>, BTreeSet<String>), Deferred }

/// Post-step-2 half of the flag-off check (see `Forward::Deferred`).
pub fn not_enabled(program: &Program<'_>, filename: &str) -> Result<(), CompileError> {
    let mut c = Collect::default();
    c.visit_program(program);
    if c.methods.is_empty() { Ok(()) } else { Err(err(filename, NOT_ENABLED)) }
}

struct Rename<'s> { names: &'s BTreeSet<String>, edits: Vec<Edit> }
impl<'a> Visit<'a> for Rename<'_> {
    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        if let (PropertyKey::PrivateIdentifier(id), MethodDefinitionKind::Method) = (&it.key, it.kind) {
            self.edits.push(Edit::replace(id.span.start, id.span.end, format!("{P}{}", id.name)));
        }
        walk::walk_method_definition(self, it);
    }
    fn visit_private_field_expression(&mut self, it: &PrivateFieldExpression<'a>) {
        if self.names.contains(it.field.name.as_str()) {
            self.edits.push(Edit::replace(it.field.span.start, it.field.span.end, format!("{P}{}", it.field.name)));
        }
        walk::walk_private_field_expression(self, it);
    }
}

/// Validates and (flag on) returns rename edits plus the set of prefixed names produced.
pub fn forward(program: &Program<'_>, filename: &str, o: &CompileOptions) -> Result<Forward, CompileError> {
    let mut c = Collect::default();
    c.visit_program(program);
    if let Some(at) = c.static_block {
        // Shipped 9.5.3 has no class-static-block plugin: babel's class-features plugin rejects them
        // (our class_properties would otherwise reorder static fields after the block).
        let _ = at; // shipped carries no location for this one
        return Err(err(filename, "Static class blocks are not enabled. Please add `@babel/plugin-transform-class-static-block` to your configuration."));
    }
    if !o.enable_private_methods {
        // Shipped: the LWC decorator visitor runs first and rejects `@api/@track/@wire #m()` as a non-property
        // (LWC1103 IS_NOT_CLASS_PROPERTY_OR_CLASS_METHOD at the member) before class-features sees the method.
        if let Some((d, s, e)) = c.methods.iter().find_map(|(_, d, _)| d.as_ref().filter(|d| matches!(d.0.as_str(), "api" | "track" | "wire"))) {
            return Err(CompileError::lwc_at(1103, format!("\"@{d}\" can only be applied on class properties"), filename, program.source_text, *s, *e));
        }
        if !c.methods.is_empty() {
            if c.in_decorated[0] { return Ok(Forward::Deferred); }
            // Shipped 9.5.3: class-properties (loose) rejects any private method/accessor when the private-methods plugin is absent.
            return Err(err(filename, NOT_ENABLED));
        }
        return Ok(Forward::Done(Vec::new(), BTreeSet::new()));
    }
    if let Some(n) = c.prefixed {
        return Err(err(filename, format!("Method '{n}' cannot start with reserved prefix `__lwc_`.")));
    }
    let mut names = BTreeSet::new();
    for (name, decorated, kind) in &c.methods {
        match kind {
            MethodDefinitionKind::Get | MethodDefinitionKind::Set => {
                return Err(err(filename, NOT_ENABLED));
            }
            _ if decorated.is_some() => {
                let (_, s, e) = decorated.as_ref().unwrap();
                return Err(CompileError::lwc_at(1212, "Decorators cannot be applied to private methods. Private methods are not part of the component API.", filename, program.source_text, *s, *e));
            }
            _ => { names.insert(name.clone()); }
        }
    }
    let mut r = Rename { names: &names, edits: Vec::new() };
    r.visit_program(program);
    Ok(Forward::Done(r.edits, names.into_iter().map(|n| format!("{P}{n}")).collect()))
}

struct Restore<'s> { fwd: &'s BTreeSet<String>, done: BTreeSet<String>, edits: Vec<Edit>, collision: Option<String> }
impl<'a> Visit<'a> for Restore<'_> {
    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        if let (PropertyKey::StaticIdentifier(id), MethodDefinitionKind::Method) = (&it.key, it.kind) {
            if id.name.starts_with(P) && !it.computed {
                if self.fwd.contains(id.name.as_str()) {
                    self.edits.push(Edit::replace(id.span.start, id.span.end, format!("#{}", &id.name[P.len()..])));
                    self.done.insert(id.name.to_string());
                } else {
                    self.collision.get_or_insert_with(|| id.name.to_string());
                }
            }
        }
        walk::walk_method_definition(self, it);
    }
    fn visit_member_expression(&mut self, it: &MemberExpression<'a>) {
        if let MemberExpression::StaticMemberExpression(m) = it {
            if self.fwd.contains(m.property.name.as_str()) {
                let p = &m.property;
                self.edits.push(Edit::replace(p.span.start, p.span.end, format!("#{}", &p.name[P.len()..])));
            }
        }
        walk::walk_member_expression(self, it);
    }
}

pub fn reverse(program: &Program<'_>, filename: &str, fwd: &BTreeSet<String>) -> Result<Vec<Edit>, CompileError> {
    let mut r = Restore { fwd, done: BTreeSet::new(), edits: Vec::new(), collision: None };
    r.visit_program(program);
    if let Some(n) = r.collision {
        return Err(err(filename, format!("Method '{n}' cannot start with reserved prefix `__lwc_`.")));
    }
    let missing: Vec<_> = fwd.difference(&r.done).cloned().collect();
    if !missing.is_empty() {
        // Shipped: a plain `new Error(...)` thrown from the reverse pass.
        return Err(CompileError::babel("Error", format!(
            "Private method transform count mismatch: forward transformed {} method(s), but reverse transformed {}. Missing reverse transforms for: {}",
            fwd.len(), r.done.len(), missing.join(", ")), filename, None, None));
    }
    Ok(r.edits)
}
