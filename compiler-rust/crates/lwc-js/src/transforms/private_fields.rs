//! Babel `@babel/plugin-transform-class-properties` / private-methods in LOOSE mode, private-FIELD
//! half (shipped @lwc/compiler 9.5.3 always runs it, whatever `enablePrivateMethods` says), so `#x`
//! survives a Proxy-wrapped `this` (native brand checks throw there):
//!
//! - `#x` field             -> marker public field `__lwc_private_field__<key>` (class_properties turns it
//!                             into `Object.defineProperty(this|Class, _x, { writable: true, value })`)
//! - key                    -> `var _x = /*#__PURE__*/babelHelpers.classPrivateFieldLooseKey("x");` before the
//!                             class statement, or `(_x = ..., class {...})` with `var _x` hoisted for expressions
//! - `o.#x`                 -> `babelHelpers.classPrivateFieldLooseBase(o, _x)[_x]` (reads, writes, calls)
//! - `#x in o`              -> `Object.prototype.hasOwnProperty.call(babelHelpers.checkInRHS(o), _x)`
//! - `o?.#x` / `a?.b.#x`    -> `o === null || o === void 0 ? void 0 : ...`, temps `_o$a` hoisted as `var`
//!
//! Private METHODS are untouched: with the flag on, private_methods::forward already renamed them, so
//! only `PropertyDefinition`s with a private key are lowered (the shipped "sandwich" result).
use super::hoist::{Scopes, Uids};
use lwc_shared::edits::Edit;
use oxc::ast::ast::*;
use oxc::ast_visit::{walk, Visit};
use oxc::semantic::ScopeFlags;
use oxc::span::GetSpan;
use std::collections::HashMap;

pub const MARKER: &str = "__lwc_private_field__";
const BASE: &str = "babelHelpers.classPrivateFieldLooseBase";

/// `wrapped_as_expr` holds class starts that an earlier edit turns into expressions
/// (`registerComponent(class …)`). The AST still calls those declarations; the re-parse used to
/// see the call argument and take the expression path (sequence wrap, `var` hoisted at scope start).
pub fn run(program: &Program<'_>, src: &str, wrapped_as_expr: &std::collections::HashSet<u32>) -> Vec<Edit> {
    let mut v = Pf { src, wrapped: wrapped_as_expr, uids: Uids::new(program), scopes: Scopes::default(), edits: vec![], classes: vec![], stmt: HashMap::new(), names: HashMap::new(), any: false };
    v.visit_program(program);
    if v.any { v.edits } else { vec![] }
}

struct Pf<'s> {
    src: &'s str,
    uids: Uids,
    scopes: Scopes,
    edits: Vec<Edit>,
    classes: Vec<HashMap<String, String>>,
    /// class span start -> start of the statement that declares it (export included)
    stmt: HashMap<u32, u32>,
    /// anonymous declarator-init class span start -> binding name (babel names transformed classes)
    names: HashMap<u32, String>,
    /// class span starts that LWC wraps as `registerComponent(class …)`
    wrapped: &'s std::collections::HashSet<u32>,
    any: bool,
}

enum Link<'b, 'a> {
    Static(bool, &'b str),
    Computed(bool, &'b Expression<'a>),
    Private(bool, &'b str),
    Call(bool, &'b oxc::allocator::Vec<'a, Argument<'a>>),
}

impl<'s> Pf<'s> {
    fn key(&self, name: &str) -> Option<String> {
        self.classes.iter().rev().find_map(|m| m.get(name).cloned())
    }
    /// Visit `f`'s nodes into a scratch edit list and return `span`'s text with those edits applied.
    fn render(&mut self, start: u32, end: u32, f: impl FnOnce(&mut Self)) -> String {
        let mark = self.edits.len();
        f(self);
        let child: Vec<Edit> = self.edits.split_off(mark).into_iter()
            .map(|e| Edit::replace(e.start - start, e.end - start, e.text)).collect();
        lwc_shared::edits::apply(&self.src[start as usize..end as usize], child).expect("lwc-js edits never partially overlap")
    }
    fn render_expr<'a>(&mut self, e: &Expression<'a>) -> String {
        let s = e.span();
        self.render(s.start, s.end, |v| v.visit_expression(e))
    }
    fn render_args<'a>(&mut self, args: &oxc::allocator::Vec<'a, Argument<'a>>) -> String {
        let mut out = Vec::new();
        for a in args.iter() {
            let s = a.span();
            out.push(self.render(s.start, s.end, |v| v.visit_argument(a)));
        }
        out.join(", ")
    }
    fn visit_link<'a>(&mut self, l: &Link<'_, 'a>) {
        match l {
            Link::Computed(_, e) => self.visit_expression(e),
            Link::Call(_, args) => for a in args.iter() { self.visit_argument(a) },
            _ => {}
        }
    }

    /// `true` when the chain was lowered here (children already handled).
    fn chain<'a>(&mut self, it: &ChainExpression<'a>) -> bool {
        // links root -> outward, each with the span end of its node
        let mut links: Vec<(Link<'_, 'a>, u32)> = Vec::new();
        let mut node: &Expression<'a>;
        let first_end = it.expression.span().end;
        match &it.expression {
            ChainElement::CallExpression(c) => { links.push((Link::Call(c.optional, &c.arguments), first_end)); node = &c.callee; }
            ChainElement::StaticMemberExpression(m) => { links.push((Link::Static(m.optional, m.property.name.as_str()), first_end)); node = &m.object; }
            ChainElement::ComputedMemberExpression(m) => { links.push((Link::Computed(m.optional, &m.expression), first_end)); node = &m.object; }
            ChainElement::PrivateFieldExpression(m) => { links.push((Link::Private(m.optional, m.field.name.as_str()), first_end)); node = &m.object; }
            _ => return false,
        }
        loop {
            let end = node.span().end;
            match node {
                Expression::CallExpression(c) => { links.push((Link::Call(c.optional, &c.arguments), end)); node = &c.callee; }
                Expression::StaticMemberExpression(m) => { links.push((Link::Static(m.optional, m.property.name.as_str()), end)); node = &m.object; }
                Expression::ComputedMemberExpression(m) => { links.push((Link::Computed(m.optional, &m.expression), end)); node = &m.object; }
                Expression::PrivateFieldExpression(m) => { links.push((Link::Private(m.optional, m.field.name.as_str()), end)); node = &m.object; }
                _ => break,
            }
        }
        links.reverse();
        let root = node;
        let opt = |l: &Link| match l { Link::Static(o, _) | Link::Computed(o, _) | Link::Private(o, _) | Link::Call(o, _) => *o };
        let Some(k) = links.iter().rposition(|(l, _)| matches!(l, Link::Private(_, n) if self.key(n).is_some())) else { return false };
        if !links[..=k].iter().any(|(l, _)| opt(l)) { return false; }

        let mut parts = super::hoist::parts(root);
        let mut cur = self.render_expr(root);
        let mut simple = matches!(root.get_inner_expression(), Expression::Identifier(_) | Expression::ThisExpression(_));
        let mut recv: Option<(String, String)> = None; // (receiver, accessor) when cur is a member access
        let mut conds = String::new();
        for (l, _) in &links[..=k] {
            if opt(l) {
                match (l, recv.take()) {
                    (Link::Call(..), Some((r, acc))) => {
                        let f = self.uids.generate_parts(&parts, "ref");
                        self.scopes.var(f.clone());
                        let r = if recv_simple(&r) { r } else {
                            let t = self.uids.generate_parts(&parts[..parts.len().saturating_sub(1)], "ref");
                            self.scopes.var(t.clone());
                            conds += "";
                            let assigned = format!("({t} = {r})");
                            cur = format!("{assigned}{acc}");
                            t
                        };
                        let fetch = if cur.starts_with('(') { cur.clone() } else { format!("{r}{acc}") };
                        conds += &format!("({f} = {fetch}) === null || {f} === void 0 ? void 0 : ");
                        cur = format!("{f}.call({r}");
                        let Link::Call(_, args) = l else { unreachable!() };
                        let a = self.render_args(args);
                        cur = if a.is_empty() { format!("{cur})") } else { format!("{cur}, {a})") };
                        simple = false;
                        continue;
                    }
                    _ => {
                        if simple {
                            conds += &format!("{cur} === null || {cur} === void 0 ? void 0 : ");
                        } else {
                            let t = self.uids.generate_parts(&parts, "ref");
                            self.scopes.var(t.clone());
                            conds += &format!("({t} = {cur}) === null || {t} === void 0 ? void 0 : ");
                            cur = t;
                        }
                    }
                }
            }
            recv = None;
            match l {
                Link::Static(_, n) => { recv = Some((cur.clone(), format!(".{n}"))); cur = format!("{cur}.{n}"); parts.push(n.to_string()); }
                Link::Computed(_, e) => {
                    let t = self.render_expr(e);
                    recv = Some((cur.clone(), format!("[{t}]")));
                    cur = format!("{cur}[{t}]");
                    parts.extend(super::hoist::parts(e));
                }
                Link::Private(_, n) => {
                    let key = self.key(n).unwrap_or_else(|| format!("#{n}"));
                    let b = format!("{BASE}({cur}, {key})");
                    recv = Some((b.clone(), format!("[{key}]")));
                    cur = format!("{b}[{key}]");
                    parts.push(n.to_string());
                }
                Link::Call(_, args) => { let a = self.render_args(args); cur = format!("{cur}({a})"); }
            }
            simple = false;
        }
        // tail after the last private link: kept as source text (inner edits applied), split at its first `?.`
        let end_k = links[k].1;
        let split = links[k + 1..].iter().position(|(l, _)| opt(l)).map(|j| links[k + j].1).unwrap_or(it.span.end);
        let (inner, outer) = links[k + 1..].split_at(links[k + 1..].iter().position(|(l, _)| opt(l)).unwrap_or(links.len() - k - 1));
        let inner_t = self.render(end_k, split, |v| for (l, _) in inner { v.visit_link(l) });
        let outer_t = self.render(split, it.span.end, |v| for (l, _) in outer { v.visit_link(l) });
        self.edits.push(Edit::replace(it.span.start, it.span.end, format!("({conds}{cur}{inner_t}){outer_t}")));
        self.any = true;
        true
    }
}

fn recv_simple(r: &str) -> bool { r == "this" || (!r.is_empty() && r.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')) }

impl<'a> Visit<'a> for Pf<'_> {
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
        let s = it.span().start;
        match it {
            Statement::ClassDeclaration(c) => { self.stmt.insert(c.span.start, s); }
            Statement::ExportDeclaration(e) => if let Declaration::ClassDeclaration(c) = &e.declaration { self.stmt.insert(c.span.start, s); },
            Statement::ExportDefaultDeclaration(e) => if let ExportDefaultDeclarationKind::ClassDeclaration(c) = &e.declaration { self.stmt.insert(c.span.start, s); },
            _ => {}
        }
        walk::walk_statement(self, it);
    }
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let (BindingPattern::BindingIdentifier(b), Some(Expression::ClassExpression(c))) = (&it.id, it.init.as_ref().map(|e| e.get_inner_expression())) {
            if c.id.is_none() { self.names.insert(c.span.start, b.name.to_string()); }
        }
        walk::walk_variable_declarator(self, it);
    }
    fn visit_class(&mut self, it: &Class<'a>) {
        let mut map = HashMap::new();
        let mut keys = Vec::new();
        let mut has_fields = false;
        for el in &it.body.body {
            let ClassElement::PropertyDefinition(p) = el else { continue };
            if p.declare || p.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition { continue; }
            has_fields = true;
            if let PropertyKey::PrivateIdentifier(id) = &p.key {
                let k = self.uids.generate(&id.name);
                self.edits.push(Edit::replace(id.span.start, id.span.end, format!("{MARKER}{k}")));
                keys.push(format!("{k} = /*#__PURE__*/babelHelpers.classPrivateFieldLooseKey(\"{}\")", id.name));
                map.insert(id.name.to_string(), k);
            }
        }
        // A decorated class expression is wrapped by `registerDecorators(...)` before this text
        // exists. Naming it from `const C = class` would bake `class C` into a call argument;
        // the re-parse used to see the call, not the declarator, and left the class anonymous.
        let decorated = !it.decorators.is_empty() || it.body.body.iter().any(|el| match el {
            ClassElement::PropertyDefinition(p) => !p.decorators.is_empty(),
            ClassElement::MethodDefinition(m) => !m.decorators.is_empty(),
            ClassElement::AccessorProperty(a) => !a.decorators.is_empty(),
            _ => false,
        });
        if has_fields && it.id.is_none() && !decorated && self.src[it.span.start as usize..].starts_with("class")
            && let Some(n) = self.names.get(&it.span.start)
        {
            self.edits.push(Edit::insert(it.span.start + 5, format!(" {n}")));
            self.any = true;
        }
        if !keys.is_empty() {
            self.any = true;
            let stays_decl = it.is_declaration() && !self.wrapped.contains(&it.span.start);
            match self.stmt.get(&it.span.start) {
                Some(&s) if stays_decl => {
                    let decls: String = keys.iter().map(|k| format!("var {k};\n")).collect();
                    self.edits.push(Edit::insert(s, decls));
                }
                _ => {
                    for k in map.values() { let _ = k; }
                    let mut names: Vec<&String> = Vec::new();
                    for el in &it.body.body {
                        if let ClassElement::PropertyDefinition(p) = el {
                            if let PropertyKey::PrivateIdentifier(id) = &p.key { names.push(&map[id.name.as_str()]); }
                        }
                    }
                    for n in names { self.scopes.var(n.clone()); }
                    self.edits.push(Edit::insert(it.span.start, format!("({}, ", keys.join(", "))));
                    self.edits.push(Edit::insert(it.span.end, ")"));
                }
            }
        }
        self.classes.push(map);
        walk::walk_class(self, it);
        self.classes.pop();
    }
    fn visit_private_field_expression(&mut self, it: &PrivateFieldExpression<'a>) {
        if let Some(k) = self.key(&it.field.name) {
            let o = it.object.span();
            self.edits.push(Edit::insert(o.start, format!("{BASE}(")));
            self.edits.push(Edit::replace(o.end, it.span.end, format!(", {k})[{k}]")));
            self.any = true;
        }
        walk::walk_private_field_expression(self, it);
    }
    fn visit_private_in_expression(&mut self, it: &PrivateInExpression<'a>) {
        if let Some(k) = self.key(&it.left.name) {
            let r = it.right.span();
            self.edits.push(Edit::replace(it.left.span.start, r.start, "Object.prototype.hasOwnProperty.call(babelHelpers.checkInRHS("));
            self.edits.push(Edit::insert(r.end, format!("), {k})")));
            self.any = true;
        }
        walk::walk_private_in_expression(self, it);
    }
    fn visit_chain_expression(&mut self, it: &ChainExpression<'a>) {
        if !self.chain(it) { walk::walk_chain_expression(self, it); }
    }
}
