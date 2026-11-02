//! Port of `@locker/babel-plugin-transform-unforgeables` 0.22.0 (run by @lwc/compiler when
//! `enableLightningWebSecurityTransforms` is set). Rewrites reads/writes of the unforgeable
//! `location` / `top` so sandboxed code cannot reach the real window through them:
//!
//! - `location = V` / `location += V` (global `location`) -> `(location.assign(V))` / `(location.assign(location.href + V))`
//! - `N.location` -> `(N === globalThis || N === document ? location : N.location)`
//! - `N.location = V` / `+= V` -> `(N === globalThis || N === document ? location.assign(...) : N.location = V)`
//! - `N.top` (not an assignment target, not inside `++`/`--`) -> `(N === globalThis ? top : N.top)`
//!
//! Faithful to the plugin's traversal: top-down, a replaced node's children are not revisited; the
//! nested `.top` chain of `X.top.top.location` collapses into one test against the chain base;
//! already-transformed shapes (validators, incl. top-level `const g = globalThis` aliases) are left alone;
//! replacement identifiers are `location`/`document`/`globalThis`/`top` unless that name is bound in
//! ANY scope, in which case a `_name` (`_name2`, ...) alias is injected once as a `var` at program top.
//!
//! Fail closed (never pass an unforgeable access through untransformed):
//! - `for (X.location in/of o)` / `X.top`: shipped throws a Babel TypeError (LWC1007); we throw the same message.
//! - destructuring member targets (`({a: X.location} = o)`, `[X.top] = o`) and `super.location` / `super.top`:
//!   shipped emits invalid JS (a conditional as an assignment target / `super === globalThis`); we raise LWC1007.
//! Matched as shipped (valid output, access left native): `X.location++` / `X.top--` (update expressions).
use crate::edits::Edit;
use lwc_shared::CompileError;
use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast_visit::{Visit, walk};
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::parser::{ParseOptions, Parser};
use oxc::semantic::{Scoping, SemanticBuilder};
use oxc::span::{GetSpan, SourceType, Span};
use std::collections::{HashMap, HashSet};

const GLOBALS: [&str; 4] = ["location", "document", "globalThis", "top"];

pub fn run(code: &str, filename: &str) -> Result<Vec<Edit>, CompileError> {
    // Cheap exit: nothing to rewrite.
    if !code.contains("location") && !code.contains("top") { return Ok(vec![]); }
    let alloc = Allocator::default();
    let st = SourceType::from_path(filename).unwrap_or_else(|_| SourceType::mjs()).with_module(true);
    // Babel (createParenthesizedExpressions: false) has no paren nodes; validators compare parents directly.
    let ret = Parser::new(&alloc, code, st).with_options(ParseOptions { preserve_parens: false, ..Default::default() }).parse();
    if ret.diagnostics.has_errors() {
        let msg = ret.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>().join("; ");
        return Err(CompileError::new(format!("internal: LWS input failed to parse: {filename}: {msg}"), filename));
    }
    let program = ret.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let mut v = Unforgeables::new(code, &program, &scoping);
    v.filename = filename.to_string();
    v.visit_program(&program);
    if let Some(e) = v.error.take() { return Err(e); }
    let mut edits = std::mem::take(&mut v.edits);
    if !v.injected.is_empty() {
        let decls: Vec<String> = v.injected.iter().map(|g| format!("{} = {g}", v.repl[g])).collect();
        edits.push(Edit::insert(0, format!("var {};\n", decls.join(", "))));
    }
    Ok(edits)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind { Member, MemberAssign, Top }

struct Unforgeables<'s> {
    src: &'s str,
    scoping: &'s Scoping,
    /// global name -> replacement identifier name
    repl: HashMap<&'static str, String>,
    /// global name -> names accepted as that global by validators (replacement + top-level aliases)
    ids: HashMap<&'static str, Vec<String>>,
    /// renamed replacement ids, in first-use order (globals of a builder in its placeholder order)
    injected: Vec<&'static str>,
    in_update: u32,
    transformed: HashSet<(u32, u32, Kind)>,
    edits: Vec<Edit>,
    filename: String,
    /// First fail-closed diagnostic; the walk stops descending once set.
    error: Option<CompileError>,
}

fn babel_uid(names: &HashSet<&str>, name: &str) -> String {
    let mut uid = name.to_string();
    let mut i = 1;
    while names.contains(uid.as_str()) {
        uid = if i > 1 { format!("_{name}{i}") } else { format!("_{name}") };
        i += 1;
    }
    uid
}

impl<'s> Unforgeables<'s> {
    fn new(src: &'s str, program: &Program<'_>, scoping: &'s Scoping) -> Self {
        let names: HashSet<&str> = scoping.symbol_names().collect();
        let mut repl = HashMap::new();
        let mut ids = HashMap::new();
        for g in GLOBALS {
            let r = babel_uid(&names, g);
            ids.insert(g, vec![r.clone()]);
            repl.insert(g, r);
        }
        // collectGlobalAliases: top-level `X = <global>` declarators not shadowed by an earlier top-level binding.
        for stmt in &program.body {
            let decl = match stmt {
                Statement::VariableDeclaration(d) => Some(&**d),
                Statement::ExportDeclaration(e) => match &e.declaration { Declaration::VariableDeclaration(d) => Some(&**d), _ => None },
                _ => None,
            };
            let Some(decl) = decl else { continue };
            for d in &decl.declarations {
                let (BindingPattern::BindingIdentifier(id), Some(Expression::Identifier(init))) = (&d.id, &d.init) else { continue };
                let Some(g) = GLOBALS.iter().copied().find(|g| *g == init.name.as_str()) else { continue };
                let shadow = scoping.get_root_binding(g.into()).map(|s| scoping.symbol_span(s).start).unwrap_or(u32::MAX);
                if id.span.start < shadow { ids.get_mut(g).unwrap().push(id.name.to_string()); }
            }
        }
        Self { src, scoping, repl, ids, injected: vec![], in_update: 0, transformed: HashSet::new(), edits: vec![], filename: String::new(), error: None }
    }

    fn get(&mut self, g: &'static str) -> String {
        let r = self.repl[g].clone();
        if r != g && !self.injected.contains(&g) { self.injected.push(g); }
        r
    }

    fn is_repl(&self, g: &str, name: &str) -> bool { name == g || self.ids[g].iter().any(|n| n == name) }

    fn text(&self, span: oxc::span::Span) -> &'s str { &self.src[span.start as usize..span.end as usize] }

    /// Source text of `e`, parenthesized unless it can stand as a member object / binary operand as-is.
    fn wrapped(&self, e: &Expression<'_>) -> String {
        let t = self.text(e.span());
        match e {
            Expression::Identifier(_) | Expression::ThisExpression(_) | Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_) | Expression::PrivateFieldExpression(_) | Expression::CallExpression(_)
            | Expression::StringLiteral(_) | Expression::ArrayExpression(_) | Expression::ObjectExpression(_)
            | Expression::TemplateLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral(_) => t.to_string(),
            _ => format!("({t})"),
        }
    }

    // ---- builders (getGlobalReplacement order = each builder's placeholder order) ----
    fn build_location_assign(&mut self, value: &str, concat: bool) -> String {
        let loc = self.get("location");
        if concat { format!("({loc}.assign({loc}.href + {value}))") } else { format!("({loc}.assign({value}))") }
    }
    fn build_member(&mut self, node: &str) -> String {
        let (doc, gt, loc) = (self.get("document"), self.get("globalThis"), self.get("location"));
        format!("({node} === {gt} || {node} === {doc} ? {loc} : {node}.location)")
    }
    fn build_member_assign(&mut self, node: &str, value: &str, concat: bool) -> String {
        let (doc, gt, loc) = (self.get("document"), self.get("globalThis"), self.get("location"));
        if concat {
            format!("({node} === {gt} || {node} === {doc} ? {loc}.assign({loc}.href + {value}) : {node}.location += {value})")
        } else {
            format!("({node} === {gt} || {node} === {doc} ? {loc}.assign({value}) : {node}.location = {value})")
        }
    }
    fn build_top(&mut self, base: &str, expr: &str) -> String {
        let (gt, top) = (self.get("globalThis"), self.get("top"));
        format!("({base} === {gt} ? {top} : {expr})")
    }

    /// `X.location` with `X` a `.top` chain: the location builder's NODE is the top builder's output.
    fn location_object(&mut self, object: &Expression<'_>) -> (String, bool) {
        if prop_name(object) == Some("top") && member_object(object).is_some() {
            let base = top_base(object);
            let b = self.wrapped(base);
            let e = self.text(object.span()).to_string();
            (self.build_top(&b, &e), true)
        } else {
            (self.wrapped(object), false)
        }
    }

    fn is_global_ref(&self, id: &IdentifierReference<'_>) -> bool {
        id.reference_id.get().map(|r| self.scoping.get_reference(r).symbol_id().is_none()).unwrap_or(true)
    }

    fn equals(&self, a: &[&Expression<'_>]) -> bool {
        let print = |e: &Expression<'_>| {
            let mut c = Codegen::new().with_options(CodegenOptions { comments: oxc::codegen::CommentOptions::disabled(), ..CodegenOptions::minify() });
            c.print_expression(e);
            c.into_source_text()
        };
        let first = print(a[0]);
        a[1..].iter().all(|e| print(e) == first)
    }

    /// `(N === GT || N === DOC)` with both N of `object`'s type and equal to it.
    fn location_test_ok(&self, test: &Expression<'_>, object: &Expression<'_>) -> bool {
        let Expression::LogicalExpression(l) = test else { return false };
        if l.operator != LogicalOperator::Or { return false; }
        let side = |e: &'_ Expression<'_>, g: &str| -> Option<()> {
            let Expression::BinaryExpression(b) = e else { return None };
            if !eqeq(b.operator) || !same_type(&b.left, object) { return None; }
            let Expression::Identifier(r) = &b.right else { return None };
            self.is_repl(g, r.name.as_str()).then_some(())
        };
        if side(&l.left, "globalThis").is_none() || side(&l.right, "document").is_none() { return false; }
        let (Expression::BinaryExpression(a), Expression::BinaryExpression(b)) = (&l.left, &l.right) else { return false };
        self.equals(&[&a.left, &b.left, object])
    }

    /// `LOCATION.assign(...)` callee check.
    fn is_location_assign_call<'b>(&self, e: &'b Expression<'b>) -> Option<&'b CallExpression<'b>> {
        let Expression::CallExpression(c) = e else { return None };
        if c.arguments.len() != 1 || prop_name(&c.callee) != Some("assign") { return None; }
        let Some(Expression::Identifier(o)) = member_object(&c.callee) else { return None };
        self.is_repl("location", o.name.as_str()).then_some(c)
    }

    /// Validators: mark `alternate` as already transformed when `cond` has the builder's shape.
    fn check_transformed(&mut self, c: &ConditionalExpression<'_>) {
        let alt = &c.alternate;
        match alt {
            Expression::AssignmentExpression(a) if matches!(a.operator, AssignmentOperator::Assign | AssignmentOperator::Addition) => {
                let Some(object) = target_location_object(&a.left) else { return };
                let Some(call) = self.is_location_assign_call(&c.consequent) else { return };
                let Some(arg) = call.arguments[0].as_expression() else { return };
                let value_ok = if a.operator == AssignmentOperator::Assign {
                    same_type(arg, &a.right) && self.equals(&[arg, &a.right])
                } else {
                    let Expression::BinaryExpression(b) = arg else { return };
                    let href_ok = b.operator == BinaryOperator::Addition
                        && prop_name(&b.left) == Some("href")
                        && matches!(member_object(&b.left), Some(Expression::Identifier(o)) if self.is_repl("location", o.name.as_str()))
                        && same_type(&b.right, &a.right);
                    href_ok && self.equals(&[&b.right, &a.right])
                };
                if value_ok && self.location_test_ok(&c.test, object) {
                    self.transformed.insert((a.span.start, a.span.end, Kind::MemberAssign));
                }
            }
            _ if prop_name(alt) == Some("location") && member_object(alt).is_some() => {
                let object = member_object(alt).unwrap();
                let Expression::Identifier(cons) = &c.consequent else { return };
                if self.is_repl("location", cons.name.as_str()) && self.location_test_ok(&c.test, object) {
                    let s = alt.span();
                    self.transformed.insert((s.start, s.end, Kind::Member));
                }
            }
            _ if prop_name(alt) == Some("top") && member_object(alt).is_some() => {
                let base = top_base(alt);
                let Expression::Identifier(cons) = &c.consequent else { return };
                let Expression::BinaryExpression(b) = &c.test else { return };
                let Expression::Identifier(r) = &b.right else { return };
                if self.is_repl("top", cons.name.as_str()) && eqeq(b.operator) && same_type(&b.left, base)
                    && self.is_repl("globalThis", r.name.as_str()) && self.equals(&[&b.left, base])
                {
                    let s = alt.span();
                    self.transformed.insert((s.start, s.end, Kind::Top));
                }
            }
            _ => {}
        }
    }

    fn is_marked(&self, span: oxc::span::Span, k: Kind) -> bool { self.transformed.contains(&(span.start, span.end, k)) }

    /// Visit a member's children (what Babel does when it leaves the member in place without `skip()`).
    fn walk_member_children(&mut self, e: &Expression<'_>) {
        match e {
            Expression::StaticMemberExpression(m) => self.visit_expression(&m.object),
            Expression::ComputedMemberExpression(m) => { self.visit_expression(&m.object); self.visit_expression(&m.expression); }
            _ => {}
        }
    }
}

fn eqeq(op: BinaryOperator) -> bool { matches!(op, BinaryOperator::StrictEquality | BinaryOperator::Equality) }

/// Babel node-type equality (`MemberExpression` covers static/computed/private).
fn same_type(a: &Expression<'_>, b: &Expression<'_>) -> bool {
    let member = |e: &Expression<'_>| matches!(e, Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) | Expression::PrivateFieldExpression(_));
    if member(a) || member(b) { return member(a) && member(b); }
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Property `Identifier` name of a (non-optional) member expression: `x.p` or `x[p]`.
fn prop_name<'b>(e: &'b Expression<'_>) -> Option<&'b str> {
    match e {
        Expression::StaticMemberExpression(m) if !m.optional => Some(m.property.name.as_str()),
        Expression::ComputedMemberExpression(m) if !m.optional => match &m.expression { Expression::Identifier(i) => Some(i.name.as_str()), _ => None },
        _ => None,
    }
}

fn member_object<'b, 'a>(e: &'b Expression<'a>) -> Option<&'b Expression<'a>> {
    match e {
        Expression::StaticMemberExpression(m) if !m.optional => Some(&m.object),
        Expression::ComputedMemberExpression(m) if !m.optional => Some(&m.object),
        _ => None,
    }
}

/// `getTopObject`: strip a `.top.top...` chain (starting at `e`, itself a `.top` member) to its base.
fn top_base<'b, 'a>(e: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut cur = member_object(e).unwrap();
    while prop_name(cur) == Some("top") {
        match member_object(cur) { Some(o) => cur = o, None => break }
    }
    cur
}

/// Object of an assignment target `X.location` / `X[location]`.
fn target_location_object<'b, 'a>(t: &'b AssignmentTarget<'a>) -> Option<&'b Expression<'a>> {
    match t {
        AssignmentTarget::StaticMemberExpression(m) if m.property.name == "location" => Some(&m.object),
        AssignmentTarget::ComputedMemberExpression(m) => match &m.expression { Expression::Identifier(i) if i.name == "location" => Some(&m.object), _ => None },
        _ => None,
    }
}

/// Babel `OptionalMemberExpression`/`OptionalCallExpression`: some link of the object/callee chain is `?.`.
fn in_optional_chain(e: &Expression<'_>) -> bool {
    let mut cur = e;
    loop {
        cur = match cur {
            Expression::StaticMemberExpression(m) => { if m.optional { return true; } &m.object }
            Expression::ComputedMemberExpression(m) => { if m.optional { return true; } &m.object }
            Expression::PrivateFieldExpression(m) => { if m.optional { return true; } &m.object }
            Expression::CallExpression(c) => { if c.optional { return true; } &c.callee }
            _ => return false,
        }
    }
}

fn is_super(e: &Expression<'_>) -> bool { matches!(e, Expression::Super(_)) }

/// `X.location` / `X[location]` / `X.top` / `X[top]` as a simple target (non-optional by grammar).
fn unforgeable_target<'b>(t: &'b SimpleAssignmentTarget<'_>) -> Option<Span> {
    match t {
        SimpleAssignmentTarget::StaticMemberExpression(m) if matches!(m.property.name.as_str(), "location" | "top") => Some(m.span),
        SimpleAssignmentTarget::ComputedMemberExpression(m) if matches!(&m.expression, Expression::Identifier(i) if matches!(i.name.as_str(), "location" | "top")) => Some(m.span),
        _ => None,
    }
}

/// First unforgeable member target nested in a destructuring pattern.
fn pattern_unforgeable(t: &AssignmentTarget<'_>) -> Option<Span> {
    fn maybe(t: &AssignmentTargetMaybeDefault<'_>) -> Option<Span> {
        match t {
            AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => any(&d.binding),
            _ => t.as_assignment_target().and_then(any),
        }
    }
    fn any(t: &AssignmentTarget<'_>) -> Option<Span> {
        match t {
            AssignmentTarget::ArrayAssignmentTarget(a) => a.elements.iter().flatten().find_map(maybe).or_else(|| a.rest.as_ref().and_then(|r| any(&r.target))),
            AssignmentTarget::ObjectAssignmentTarget(o) => o.properties.iter().find_map(|p| match p {
                AssignmentTargetProperty::AssignmentTargetPropertyProperty(p) => maybe(&p.binding),
                AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(_) => None,
            }).or_else(|| o.rest.as_ref().and_then(|r| any(&r.target))),
            _ => t.as_simple_assignment_target().and_then(unforgeable_target),
        }
    }
    match t {
        AssignmentTarget::ArrayAssignmentTarget(_) | AssignmentTarget::ObjectAssignmentTarget(_) => any(t),
        _ => None,
    }
}

impl<'a> Visit<'a> for Unforgeables<'_> {
    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        if self.for_left_fails(&it.left, "ForInStatement") { return; }
        walk::walk_for_in_statement(self, it);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        if self.for_left_fails(&it.left, "ForOfStatement") { return; }
        walk::walk_for_of_statement(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        self.in_update += 1;
        walk::walk_update_expression(self, it);
        self.in_update -= 1;
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.check_transformed(it);
        walk::walk_conditional_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if self.error.is_some() { return; }
        if let Some(sp) = pattern_unforgeable(&it.left) { return self.fail_destructuring(sp); }
        if let Some(sp) = it.left.as_simple_assignment_target().and_then(unforgeable_target) {
            if member_object_target(&it.left).is_some_and(is_super) { return self.fail_super(sp); }
        }
        let assign_or_concat = matches!(it.operator, AssignmentOperator::Assign | AssignmentOperator::Addition);
        let concat = it.operator == AssignmentOperator::Addition;
        match &it.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) if id.name == "location" && self.is_global_ref(id) && assign_or_concat => {
                let v = if concat { self.wrapped(&it.right) } else { self.arg_text(&it.right) };
                let t = self.build_location_assign(&v, concat);
                self.edits.push(Edit::replace(it.span.start, it.span.end, t));
                return;
            }
            AssignmentTarget::StaticMemberExpression(_) | AssignmentTarget::ComputedMemberExpression(_) => {
                if let Some(object) = target_location_object(&it.left) {
                    if is_super(object) { walk::walk_assignment_expression(self, it); return; }
                    // The top builder runs (and may inject its ids) before the assignment checks, like Babel.
                    let (node, is_top_obj) = self.location_object(object);
                    if assign_or_concat && !self.is_marked(it.span, Kind::MemberAssign) {
                        let v = if concat { self.wrapped(&it.right) } else { self.arg_text(&it.right) };
                        let t = self.build_member_assign(&node, &v, concat);
                        self.edits.push(Edit::replace(it.span.start, it.span.end, t));
                        return;
                    }
                    // Left in place: the member's children are visited unless it was `skip()`ped (top-chain object).
                    if !is_top_obj {
                        self.visit_expression(object);
                        if let AssignmentTarget::ComputedMemberExpression(m) = &it.left { self.visit_expression(&m.expression); }
                    }
                    self.visit_expression(&it.right);
                    return;
                }
            }
            _ => {}
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_expression(&mut self, it: &Expression<'a>) {
        if self.error.is_some() { return; }
        let prop = prop_name(it);
        if matches!(prop, Some("location" | "top")) && self.in_update == 0 && member_object(it).is_some_and(is_super) {
            return self.fail_super(it.span());
        }
        if matches!(prop, Some("location" | "top")) && !in_optional_chain(it) && !member_object(it).is_some_and(is_super) {
            let object = member_object(it).unwrap();
            let span = it.span();
            if prop == Some("location") {
                let (node, is_top_obj) = self.location_object(object);
                if self.in_update == 0 && !self.is_marked(span, Kind::Member) {
                    let t = self.build_member(&node);
                    self.edits.push(Edit::replace(span.start, span.end, t));
                    return;
                }
                if !is_top_obj { self.walk_member_children(it); }
                return;
            }
            // `.top`
            if self.in_update == 0 {
                if !self.is_marked(span, Kind::Top) {
                    let base = self.wrapped(top_base(it));
                    let e = self.text(span).to_string();
                    let t = self.build_top(&base, &e);
                    self.edits.push(Edit::replace(span.start, span.end, t));
                }
                return; // whole top chain skipped
            }
        }
        walk::walk_expression(self, it);
    }
}

fn member_object_target<'b, 'a>(t: &'b AssignmentTarget<'a>) -> Option<&'b Expression<'a>> {
    match t {
        AssignmentTarget::StaticMemberExpression(m) => Some(&m.object),
        AssignmentTarget::ComputedMemberExpression(m) => Some(&m.object),
        _ => None,
    }
}

impl Unforgeables<'_> {
    fn fail(&mut self, kind: &str, msg: String) {
        if self.error.is_none() { self.error = Some(CompileError::babel(kind, msg, &self.filename, None, None)); }
    }

    /// Shipped: the plugin replaces the member with a (parenthesized) conditional and Babel's
    /// validator throws `TypeError: Property left of ForInStatement ...` (wrapped as LWC1007).
    fn for_left_fails(&mut self, left: &ForStatementLeft<'_>, kind: &str) -> bool {
        let Some(t) = left.as_assignment_target() else { return false };
        // A pattern is a valid LVal for Babel's validator, so shipped emits invalid JS there instead of throwing.
        if let Some(sp) = pattern_unforgeable(t) { self.fail_destructuring(sp); return true; }
        if t.as_simple_assignment_target().and_then(unforgeable_target).is_none() { return false; }
        self.fail("TypeError", format!("Property left of {kind} expected node to be of a type [\"VariableDeclaration\",\"LVal\"] but instead got \"ParenthesizedExpression\""));
        true
    }

    fn fail_destructuring(&mut self, sp: Span) {
        let e = self.text(sp).to_string();
        self.fail("SyntaxError", format!("Lightning Web Security cannot transform the unforgeable access `{e}` used as a destructuring assignment target. Assign it in a separate statement instead."));
    }

    fn fail_super(&mut self, sp: Span) {
        let e = self.text(sp).to_string();
        self.fail("SyntaxError", format!("Lightning Web Security cannot transform the unforgeable access `{e}` on `super`. Read it through a local reference instead."));
    }

    /// A call argument: only a sequence expression needs parentheses.
    fn arg_text(&self, e: &Expression<'_>) -> String {
        let t = self.text(e.span());
        if matches!(e, Expression::SequenceExpression(_)) { format!("({t})") } else { t.to_string() }
    }
}
