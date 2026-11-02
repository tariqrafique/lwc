//! Class-properties lowering equivalent to Babel `{ loose: true }` (what shipped @lwc/compiler runs):
//! - instance `x = v;` -> `this.x = v;` in the constructor (after the top-level `super(...)` call;
//!   `constructor(...args) { super(...args); ... }` is synthesized when the class has none)
//! - private marker fields (see private_fields.rs) -> `Object.defineProperty(this, _x, { writable: true, value: v });`
//! - static fields -> `A.x = v;` / `Object.defineProperty(A, _x, {...});` right after the class statement,
//!   or `(_Class = class {...}, _Class.x = v, _Class)` for class expressions
//! - a static initializer's `this` / own class name -> the `_A` alias (`var _A;` hoisted, `_A = A;` after the class)
//! - non-literal computed field keys are evaluated once in class-definition order: `let _k;` hoisted, `_k = k;`
//!   before the class (or as a sequence prefix), then `this[_k]` / `A[_k]`.
//! `accessor` fields are left as-is.
use super::hoist::{Scopes, Uids};
use super::private_fields::MARKER;
use crate::edits::Edit;
use oxc::ast::ast::*;
use oxc::ast_visit::{walk, Visit};
use oxc::semantic::ScopeFlags;
use oxc::span::{GetSpan, Span};
use std::collections::HashMap;

struct Lower<'s> {
    src: &'s str,
    edits: Vec<Edit>,
    uids: Uids,
    scopes: Scopes,
    /// class span start -> (statement start, statement end) for class declarations
    stmt: HashMap<u32, (u32, u32)>,
    any: bool,
}

impl<'s> Lower<'s> {
    fn slice(&self, s: Span) -> &'s str { &self.src[s.start as usize..s.end as usize] }
}

/// `this` (outside non-arrow functions / nested classes) and references to `name` inside a static initializer.
struct Refs<'n> { name: Option<&'n str>, hits: Vec<Span>, opaque: u32 }
impl<'n> Refs<'n> { fn new(name: Option<&'n str>) -> Self { Self { name, hits: vec![], opaque: 0 } } }
impl<'a> Visit<'a> for Refs<'_> {
    fn visit_this_expression(&mut self, it: &ThisExpression) { if self.opaque == 0 { self.hits.push(it.span); } }
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if Some(it.name.as_str()) == self.name { self.hits.push(it.span); }
    }
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        self.opaque += 1;
        walk::walk_function(self, it, flags);
        self.opaque -= 1;
    }
    fn visit_class(&mut self, it: &Class<'a>) {
        self.opaque += 1;
        walk::walk_class(self, it);
        self.opaque -= 1;
    }
}

impl<'a> Visit<'a> for Lower<'_> {
    fn visit_program(&mut self, it: &Program<'a>) {
        self.scopes.enter_program(it, &mut self.edits);
        walk::walk_program(self, it);
        self.scopes.exit(&mut self.edits);
    }
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        match &it.body {
            Some(b) => {
                self.scopes.enter_body(b, &mut self.edits);
                walk::walk_function(self, it, flags);
                self.scopes.exit(&mut self.edits);
            }
            None => walk::walk_function(self, it, flags),
        }
    }
    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.scopes.enter_arrow(it, &mut self.edits);
        walk::walk_arrow_function_expression(self, it);
        self.scopes.exit(&mut self.edits);
    }
    fn visit_statement(&mut self, it: &Statement<'a>) {
        let s = it.span();
        match it {
            Statement::ClassDeclaration(c) => { self.stmt.insert(c.span.start, (s.start, s.end)); }
            Statement::ExportDeclaration(e) => if let Declaration::ClassDeclaration(c) = &e.declaration { self.stmt.insert(c.span.start, (s.start, s.end)); },
            Statement::ExportDefaultDeclaration(e) => if let ExportDefaultDeclarationKind::ClassDeclaration(c) = &e.declaration { self.stmt.insert(c.span.start, (s.start, s.end)); },
            _ => {}
        }
        walk::walk_statement(self, it);
    }
    fn visit_class(&mut self, class: &Class<'a>) {
        let decl = if class.is_declaration() { self.stmt.get(&class.span.start).copied() } else { None };
        // anonymous class declarations (only `export default class {}`) cannot carry trailing statics
        let lower_static = decl.is_some() && class.id.is_some() || !class.is_declaration();
        let mut inits = Vec::new();
        let mut statics = Vec::new();
        let mut keys = Vec::new(); // `_k = k`
        let mut alias_needed = false;
        let own = class.id.as_ref().map(|i| i.name.as_str());
        let mut alias: Option<String> = None;
        let mut pending_static: Vec<(String, Span, Option<&Expression<'a>>, bool)> = Vec::new(); // (target-suffix, ..)
        for el in &class.body.body {
            let ClassElement::PropertyDefinition(p) = el else { continue };
            if p.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition { continue; }
            if p.r#static && !lower_static { continue; }
            self.edits.push(Edit::replace(p.span.start, p.span.end, ""));
            if p.declare { continue; }
            // key: `.x`, `["lit"]`, `[_k]`, or the private marker
            let (suffix, private) = match &p.key {
                PropertyKey::StaticIdentifier(id) if !p.computed => match id.name.strip_prefix(MARKER) {
                    Some(k) => (k.to_string(), true),
                    None => (format!(".{}", id.name), false),
                },
                PropertyKey::PrivateIdentifier(_) => continue, // untouched (should not happen after private_fields)
                k => {
                    let e = k.as_expression();
                    match e {
                        Some(e) if p.computed && !self.uids.is_constant_expression(e) => {
                            let t = self.uids.generate_for(e, "ref");
                            self.scopes.let_(t.clone());
                            keys.push(format!("{t} = {}", self.slice(e.span())));
                            (format!("[{t}]"), false)
                        }
                        _ => (format!("[{}]", self.slice(k.span())), false),
                    }
                }
            };
            if p.r#static {
                if let Some(v) = &p.value {
                    let mut r = Refs::new(own);
                    r.visit_expression(v);
                    if !r.hits.is_empty() { alias_needed = true; }
                }
                pending_static.push((suffix, p.span, p.value.as_ref(), private));
            } else {
                let value = p.value.as_ref().map_or("void 0".to_string(), |v| self.slice(v.span()).to_string());
                inits.push(if private {
                    format!("Object.defineProperty(this, {suffix}, {{\nwritable: true,\nvalue: {value}\n}});")
                } else {
                    format!("this{suffix} = {value};")
                });
            }
        }
        if inits.is_empty() && pending_static.is_empty() && keys.is_empty() && !class.body.body.iter().any(|e| matches!(e, ClassElement::PropertyDefinition(p) if p.declare)) {
            return walk::walk_class(self, class);
        }
        self.any = true;
        // reference used by the trailing statics, and the alias used inside static initializers
        let reference = match decl {
            Some(_) => own.unwrap_or_default().to_string(),
            None if !pending_static.is_empty() => {
                let a = self.uids.generate(own.unwrap_or("Class"));
                self.scopes.var(a.clone());
                alias = Some(a.clone());
                a
            }
            None => String::new(),
        };
        if decl.is_some() && alias_needed {
            let a = self.uids.generate(own.unwrap_or("Class"));
            self.scopes.var(a.clone());
            alias = Some(a);
        }
        for (suffix, _, value, private) in pending_static {
            let value = match value {
                None => "void 0".to_string(),
                Some(v) => {
                    let s = v.span();
                    let mut r = Refs::new(own);
                    r.visit_expression(v);
                    let a = alias.clone().unwrap_or_else(|| reference.clone());
                    let es = r.hits.into_iter().map(|h| Edit::replace(h.start - s.start, h.end - s.start, a.clone())).collect();
                    crate::edits::apply(self.slice(s), es)
                }
            };
            statics.push(if private {
                format!("Object.defineProperty({reference}, {suffix}, {{\nwritable: true,\nvalue: {value}\n}})")
            } else {
                format!("{reference}{suffix} = {value}")
            });
        }
        // instance initializers -> constructor
        if !inits.is_empty() {
            let body = inits.join("\n");
            let ctor = class.body.body.iter().find_map(|e| match e {
                ClassElement::MethodDefinition(m) if m.kind == MethodDefinitionKind::Constructor => Some(m),
                _ => None,
            });
            match ctor.and_then(|m| m.value.body.as_ref()) {
                Some(b) => {
                    let after_super = b.statements.iter().find_map(|s| match s {
                        Statement::ExpressionStatement(es) => match &es.expression {
                            Expression::CallExpression(c) if matches!(c.callee, Expression::Super(_)) => Some(es.span.end),
                            _ => None,
                        },
                        _ => None,
                    });
                    let at = after_super.unwrap_or(b.span.start + 1);
                    self.edits.push(Edit::insert(at, format!("\n{body}\n")));
                }
                None => {
                    let text = if class.heritage.is_some() {
                        format!("constructor(...args) {{ super(...args);\n{body}\n}}\n")
                    } else {
                        format!("constructor() {{\n{body}\n}}\n")
                    };
                    self.edits.push(Edit::insert(class.body.span.start + 1, text));
                }
            }
        }
        match decl {
            Some((start, end)) => {
                if !keys.is_empty() {
                    self.edits.push(Edit::insert(start, keys.iter().map(|k| format!("{k};\n")).collect::<String>()));
                }
                let mut after = String::new();
                if let Some(a) = &alias { after += &format!("\n{a} = {reference};"); }
                for s in &statics { after += &format!("\n{s};"); }
                if !after.is_empty() { self.edits.push(Edit::insert(end, after)); }
            }
            None => {
                let mut pre: Vec<String> = keys.clone();
                if let Some(a) = &alias { pre.push(format!("{a} = ")); }
                let wrap = !pre.is_empty() || !statics.is_empty();
                if wrap {
                    let head = match &alias {
                        Some(_) => { let last = pre.pop().unwrap(); pre.iter().map(|p| format!("{p}, ")).collect::<String>() + &last }
                        None => pre.iter().map(|p| format!("{p}, ")).collect::<String>(),
                    };
                    self.edits.push(Edit::insert(class.span.start, format!("({head}")));
                    let mut tail = String::new();
                    for s in &statics { tail += &format!(", {s}"); }
                    if let Some(a) = &alias { tail += &format!(", {a}"); }
                    self.edits.push(Edit::insert(class.span.end, format!("{tail})")));
                }
            }
        }
        // Do not descend: nested classes inside moved initializers are lowered on the next fixpoint pass.
    }
}

/// Returns edits for one pass; caller re-parses and repeats until empty.
pub fn run(program: &Program<'_>, src: &str) -> Vec<Edit> {
    let mut l = Lower { src, edits: Vec::new(), uids: Uids::new(program), scopes: Scopes::default(), stmt: HashMap::new(), any: false };
    l.visit_program(program);
    if l.any { l.edits } else { vec![] }
}
