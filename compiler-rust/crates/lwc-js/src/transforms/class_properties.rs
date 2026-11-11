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
use lwc_shared::edits::Edit;
use oxc::ast::ast::*;
use oxc::ast_visit::{Visit, walk};
use oxc::semantic::ScopeFlags;
use oxc::span::{GetSpan, Span};
use std::collections::{HashMap, HashSet};

struct Lower<'s> {
    src: &'s str,
    /// Private-field (and any earlier) edits in `src` coordinates. Slices see them applied;
    /// a private key renamed to the marker is lowered as that marker (no re-parse).
    prior: &'s [Edit],
    edits: Vec<Edit>,
    uids: Uids,
    scopes: Scopes,
    /// class span start -> (statement start, statement end) for class declarations
    stmt: HashMap<u32, (u32, u32)>,
    any: bool,
    /// Enclosing-scope hoists of the class currently being lowered (computed keys, static alias).
    own_lets: Vec<String>,
    own_vars: Vec<String>,
    /// Hoists that belong in the constructor this class is building (nested classes placed into it).
    ctor_lets: Vec<String>,
    ctor_vars: Vec<String>,
    /// When set, this class's enclosing hoists go to the placing class's constructor instead of `scopes`.
    placing_in_ctor: bool,
    seen: std::collections::HashSet<u32>,
    /// Spans whose edits were baked into a moved initializer. Callers drop those edits.
    rendered: Vec<(u32, u32)>,
    /// Class and class-statement ends. Inserts there sort later-pass first inside a rendered slice.
    ends: HashSet<u32>,
}

impl<'s> Lower<'s> {
    /// `src[span]` with `prior` edits that lie inside it applied (boundary inserts included).
    fn slice_span(&self, start: u32, end: u32) -> String {
        let tagged: Vec<(Edit, u8)> = self.prior.iter().cloned().map(|e| (e, 0)).collect();
        apply_in(self.src, &tagged, start, end, &self.ends)
    }
    fn slice(&self, s: Span) -> String {
        self.slice_span(s.start, s.end)
    }

    fn hoist_let(&mut self, name: String) {
        self.own_lets.push(name);
    }
    fn hoist_var(&mut self, name: String) {
        self.own_vars.push(name);
    }
}

/// `this` (outside non-arrow functions / nested classes) and references to `name` inside a static initializer.
/// `super` in a static initializer is the superclass constructor (replaced with the heritage text).
struct Refs<'n> {
    name: Option<&'n str>,
    hits: Vec<Span>,
    supers: Vec<Span>,
    opaque: u32,
}
impl<'n> Refs<'n> {
    fn new(name: Option<&'n str>) -> Self {
        Self {
            name,
            hits: vec![],
            supers: vec![],
            opaque: 0,
        }
    }
}
impl<'a> Visit<'a> for Refs<'_> {
    fn visit_this_expression(&mut self, it: &ThisExpression) {
        if self.opaque == 0 {
            self.hits.push(it.span);
        }
    }
    fn visit_super(&mut self, it: &Super) {
        if self.opaque == 0 {
            self.supers.push(it.span);
        }
    }
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if Some(it.name.as_str()) == self.name {
            self.hits.push(it.span);
        }
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
            Statement::ClassDeclaration(c) => {
                self.stmt.insert(c.span.start, (s.start, s.end));
            }
            Statement::ExportDeclaration(e) => {
                if let Declaration::ClassDeclaration(c) = &e.declaration {
                    self.stmt.insert(c.span.start, (s.start, s.end));
                }
            }
            Statement::ExportDefaultDeclaration(e) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(c) = &e.declaration {
                    self.stmt.insert(c.span.start, (s.start, s.end));
                }
            }
            _ => {}
        }
        walk::walk_statement(self, it);
    }
    fn visit_class(&mut self, class: &Class<'a>) {
        self.ends.insert(class.span.end);
        if !self.seen.insert(class.span.start) {
            return;
        }
        let place = self.placing_in_ctor;
        let saved_l = std::mem::take(&mut self.ctor_lets);
        let saved_v = std::mem::take(&mut self.ctor_vars);
        let lowered = self.lower_class(class);
        self.ctor_lets = saved_l;
        self.ctor_vars = saved_v;
        let lets = std::mem::take(&mut self.own_lets);
        let vars = std::mem::take(&mut self.own_vars);
        if place {
            self.ctor_lets.extend(lets);
            self.ctor_vars.extend(vars);
        } else {
            for l in lets {
                self.scopes.let_(l);
            }
            for v in vars {
                self.scopes.var(v);
            }
        }
        self.placing_in_ctor = place;
        if lowered {
            self.descend_non_fields(class);
        } else {
            walk::walk_class(self, class);
        }
    }
}

impl<'s> Lower<'s> {
    fn lower_class(&mut self, class: &Class<'_>) -> bool {
        // LWC turns `export default class extends …` into `registerComponent(class extends …)`.
        // After that edit the class is an expression, so static fields lower and the `_Class`
        // assignment gives the constructor its name. The AST still says declaration.
        let wrapped_as_expr = self
            .prior
            .iter()
            .any(|e| e.end == class.span.start && e.text.contains("registerComponent("));
        let is_decl = class.is_declaration() && !wrapped_as_expr;
        let decl = if is_decl {
            self.stmt.get(&class.span.start).copied()
        } else {
            None
        };
        // anonymous class declarations (only `export default class {}`) cannot carry trailing statics
        let lower_static = decl.is_some() && class.id.is_some() || !is_decl;
        let mut inits = Vec::new();
        let mut statics = Vec::new();
        let mut keys = Vec::new(); // `_k = k`
        let mut alias_needed = false;
        // Private fields may have named an anonymous `const C = class` in `prior` (` C` at `class`.len()).
        let named = inserted_name(self.prior, class.span.start);
        let own_owned = class.id.as_ref().map(|i| i.name.to_string()).or(named);
        let own = own_owned.as_deref();
        let mut alias: Option<String> = None;
        let mut pending_static: Vec<(String, usize, bool)> = Vec::new(); // suffix, element index, private
        for (el_idx, el) in class.body.body.iter().enumerate() {
            let ClassElement::PropertyDefinition(p) = el else {
                continue;
            };
            if p.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition {
                continue;
            }
            if p.r#static && !lower_static {
                continue;
            }
            if p.declare {
                self.edits.push(Edit::replace(p.span.start, p.span.end, ""));
                continue;
            }
            // key: `.x`, `["lit"]`, `[_k]`, or the private marker
            let (suffix, private) = match &p.key {
                PropertyKey::StaticIdentifier(id) if !p.computed => {
                    match id.name.strip_prefix(MARKER) {
                        Some(k) => (k.to_string(), true),
                        None => (format!(".{}", id.name), false),
                    }
                }
                PropertyKey::PrivateIdentifier(id) => {
                    match prior_replacement(self.prior, id.span) {
                        Some(rep) => match rep.strip_prefix(MARKER) {
                            Some(k) => (k.to_string(), true),
                            None => continue,
                        },
                        None => continue,
                    }
                }
                k => {
                    let e = k.as_expression();
                    match e {
                        Some(e) if p.computed && !self.uids.is_constant_expression(e) => {
                            let t = self.uids.generate_for(e, "ref");
                            self.hoist_let(t.clone());
                            keys.push(format!("{t} = {}", self.slice(e.span())));
                            (format!("[{t}]"), false)
                        }
                        _ => (format!("[{}]", self.slice(k.span())), false),
                    }
                }
            };
            self.edits.push(Edit::replace(p.span.start, p.span.end, ""));
            if p.r#static {
                if let Some(v) = &p.value {
                    let mut r = Refs::new(own);
                    r.visit_expression(v);
                    if !r.hits.is_empty() {
                        alias_needed = true;
                    }
                }
                pending_static.push((suffix, el_idx, private));
            } else {
                let value = p
                    .value
                    .as_ref()
                    .map_or("void 0".to_string(), |v| self.render_expr(v));
                inits.push(if private {
                    format!("Object.defineProperty(this, {suffix}, {{\nwritable: true,\nvalue: {value}\n}});")
                } else {
                    format!("this{suffix} = {value};")
                });
            }
        }
        if inits.is_empty()
            && pending_static.is_empty()
            && keys.is_empty()
            && !class
                .body
                .body
                .iter()
                .any(|e| matches!(e, ClassElement::PropertyDefinition(p) if p.declare))
        {
            return false;
        }
        self.any = true;
        // Babel's class-features uid is reserved for a class that has instance fields even when
        // the alias is not emitted. A later class expression in the same program then gets `_Class2`.
        if pending_static.is_empty() && !inits.is_empty() && decl.is_none() {
            let _reserved = self.uids.generate(own.unwrap_or("Class"));
        }
        // reference used by the trailing statics, and the alias used inside static initializers
        let reference = match decl {
            Some(_) => own.unwrap_or_default().to_string(),
            None if !pending_static.is_empty() => {
                let a = self.uids.generate(own.unwrap_or("Class"));
                self.hoist_var(a.clone());
                alias = Some(a.clone());
                a
            }
            None => String::new(),
        };
        if decl.is_some() && alias_needed {
            let a = self.uids.generate(own.unwrap_or("Class"));
            self.hoist_var(a.clone());
            alias = Some(a);
        }
        for (suffix, el_idx, private) in pending_static {
            let ClassElement::PropertyDefinition(p) = &class.body.body[el_idx] else {
                continue;
            };
            let value = match &p.value {
                None => "void 0".to_string(),
                Some(v) => {
                    let s = v.span();
                    let mut r = Refs::new(own);
                    r.visit_expression(v);
                    let a = alias.clone().unwrap_or_else(|| reference.clone());
                    let heritage = class
                        .heritage
                        .as_ref()
                        .map(|h| self.slice(h.span()))
                        .unwrap_or_else(|| "Object".to_string());
                    let mut es: Vec<Edit> = r
                        .hits
                        .into_iter()
                        .map(|h| Edit::replace(h.start - s.start, h.end - s.start, a.clone()))
                        .collect();
                    es.extend(r.supers.into_iter().map(|h| {
                        Edit::replace(h.start - s.start, h.end - s.start, heritage.clone())
                    }));
                    lwc_shared::edits::apply(&self.slice(s), es)
                        .expect("lwc-js edits never partially overlap")
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
            let hoist = hoist_text(&self.ctor_lets, &self.ctor_vars);
            self.ctor_lets.clear();
            self.ctor_vars.clear();
            let ctor = class.body.body.iter().find_map(|e| match e {
                ClassElement::MethodDefinition(m)
                    if m.kind == MethodDefinitionKind::Constructor =>
                {
                    Some(m)
                }
                _ => None,
            });
            match ctor.and_then(|m| m.value.body.as_ref()) {
                Some(b) => {
                    if !hoist.is_empty() {
                        self.edits.push(Edit::insert(b.span.start + 1, hoist));
                    }
                    let after_super = b.statements.iter().find_map(|s| match s {
                        Statement::ExpressionStatement(es) => match &es.expression {
                            Expression::CallExpression(c)
                                if matches!(c.callee, Expression::Super(_)) =>
                            {
                                Some(es.span.end)
                            }
                            _ => None,
                        },
                        _ => None,
                    });
                    let at = after_super.unwrap_or(b.span.start + 1);
                    self.edits.push(Edit::insert(at, format!("\n{body}\n")));
                }
                None => {
                    let text = if class.heritage.is_some() {
                        format!("constructor(...args) {{{hoist} super(...args);\n{body}\n}}\n")
                    } else {
                        format!("constructor() {{{hoist}\n{body}\n}}\n")
                    };
                    self.edits
                        .push(Edit::insert(class.body.span.start + 1, text));
                }
            }
        } else {
            self.ctor_lets.clear();
            self.ctor_vars.clear();
        }
        match decl {
            Some((start, end)) => {
                if !keys.is_empty() {
                    self.edits.push(Edit::insert(
                        start,
                        keys.iter().map(|k| format!("{k};\n")).collect::<String>(),
                    ));
                }
                let mut after = String::new();
                if let Some(a) = &alias {
                    after += &format!("\n{a} = {reference};");
                }
                for s in &statics {
                    after += &format!("\n{s};");
                }
                if !after.is_empty() {
                    self.edits.push(Edit::insert(end, after));
                }
            }
            None => {
                let mut pre: Vec<String> = keys.clone();
                if let Some(a) = &alias {
                    pre.push(format!("{a} = "));
                }
                let wrap = !pre.is_empty() || !statics.is_empty();
                if wrap {
                    let head = match &alias {
                        Some(_) => {
                            let last = pre.pop().unwrap();
                            pre.iter().map(|p| format!("{p}, ")).collect::<String>() + &last
                        }
                        None => pre.iter().map(|p| format!("{p}, ")).collect::<String>(),
                    };
                    self.edits
                        .push(Edit::insert(class.span.start, format!("({head}")));
                    let mut tail = String::new();
                    for s in &statics {
                        tail += &format!(", {s}");
                    }
                    if let Some(a) = &alias {
                        tail += &format!(", {a}");
                    }
                    self.edits
                        .push(Edit::insert(class.span.end, format!("{tail})")));
                }
            }
        }
        true
    }

    /// Field values are already rendered. Methods and static blocks still hold nested classes.
    fn descend_non_fields(&mut self, class: &Class<'_>) {
        for el in &class.body.body {
            match el {
                ClassElement::PropertyDefinition(_) => {}
                ClassElement::MethodDefinition(m) => {
                    self.visit_function(&m.value, ScopeFlags::empty())
                }
                ClassElement::StaticBlock(s) => walk::walk_static_block(self, s),
                _ => {}
            }
        }
    }

    /// Text of `e` after private-field edits and after lowering any class nested in it.
    /// A class expression that sits directly in `e` hoists into the constructor that receives `e`.
    /// A class nested in a function or arrow inside `e` hoists into that function: an arrow with an
    /// expression body becomes a block (`{ var _Class; return ... }`), matching Babel's scope.
    fn render_expr(&mut self, e: &Expression<'_>) -> String {
        let span = e.span();
        let me = self as *mut Lower<'s>;
        struct V<'s> {
            lower: *mut Lower<'s>,
            /// Function and arrow bodies entered while rendering this initializer.
            fns: u32,
        }
        impl<'s, 'a> Visit<'a> for V<'s> {
            fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
                let lower = unsafe { &mut *self.lower };
                let entered = if let Some(b) = &it.body {
                    lower.scopes.enter_body(b, &mut lower.edits);
                    true
                } else {
                    false
                };
                self.fns += 1;
                walk::walk_function(self, it, flags);
                self.fns -= 1;
                if entered {
                    let lower = unsafe { &mut *self.lower };
                    lower.scopes.exit(&mut lower.edits);
                }
            }
            fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
                let lower = unsafe { &mut *self.lower };
                lower.scopes.enter_arrow(it, &mut lower.edits);
                self.fns += 1;
                walk::walk_arrow_function_expression(self, it);
                self.fns -= 1;
                let lower = unsafe { &mut *self.lower };
                lower.scopes.exit(&mut lower.edits);
            }
            fn visit_class(&mut self, c: &Class<'a>) {
                // Re-enter Lower. The visitor does not hold a reference to it across the walk.
                let lower = unsafe { &mut *self.lower };
                if class_needs_lower(c, lower.prior) && !lower.seen.contains(&c.span.start) {
                    let saved = lower.placing_in_ctor;
                    if self.fns == 0 {
                        lower.placing_in_ctor = true;
                    }
                    lower.visit_class(c);
                    let lower = unsafe { &mut *self.lower };
                    lower.placing_in_ctor = saved;
                } else {
                    walk::walk_class(self, c);
                }
            }
        }
        // SAFETY: `me` points at `self`. `V` dereferences it only inside a visit method, which does
        // not use `V` again until that call returns. Nested `render_expr` calls make their own
        // visitors and re-enter on disjoint class nodes.
        let mut v = V { lower: me, fns: 0 };
        v.visit_expression(e);
        let mut tagged: Vec<(Edit, u8)> = self.prior.iter().cloned().map(|e| (e, 0)).collect();
        tagged.extend(self.edits.iter().cloned().map(|e| (e, 1)));
        let out = apply_in(self.src, &tagged, span.start, span.end, &self.ends);
        // Those edits are in `out`. Leaving them in the global list reapplies them beside the
        // deletion of the field, and an insert at the field's end survives `drop_contained`.
        self.edits
            .retain(|e| !edit_rendered_into(span.start, span.end, e));
        self.rendered.push((span.start, span.end));
        out
    }
}

/// One walk of `program`. `prior` is earlier span edits (private fields) in the same coordinates;
/// nested classes are lowered in this walk, not by re-parsing.
pub fn run(program: &Program<'_>, src: &str) -> Vec<Edit> {
    run_after(program, src, &[]).0
}

pub fn run_after(program: &Program<'_>, src: &str, prior: &[Edit]) -> (Vec<Edit>, Vec<(u32, u32)>) {
    let mut l = Lower {
        src,
        prior,
        edits: Vec::new(),
        uids: Uids::new(program),
        scopes: Scopes::default(),
        stmt: HashMap::new(),
        any: false,
        own_lets: Vec::new(),
        own_vars: Vec::new(),
        ctor_lets: Vec::new(),
        ctor_vars: Vec::new(),
        placing_in_ctor: false,
        seen: std::collections::HashSet::new(),
        rendered: Vec::new(),
        ends: HashSet::new(),
    };
    l.visit_program(program);
    let rendered = l.rendered;
    let edits = if l.any { l.edits } else { vec![] };
    (edits, rendered)
}

/// An edit `apply_in` copies into a rendered initializer: strictly inside `[start, end]`, or an
/// insert on that span's boundary.
pub(crate) fn edit_rendered_into(start: u32, end: u32, e: &Edit) -> bool {
    let inside = e.start >= start && e.end <= end && !(e.start == start && e.end == end);
    let boundary_insert = e.start == e.end && e.start >= start && e.start <= end;
    inside || boundary_insert
}

fn hoist_text(lets: &[String], vars: &[String]) -> String {
    let mut decl = String::new();
    if !lets.is_empty() {
        decl += &format!("let {};\n", lets.join(", "));
    }
    if !vars.is_empty() {
        decl += &format!("var {};\n", vars.join(", "));
    }
    if decl.is_empty() {
        String::new()
    } else {
        format!("\n{decl}")
    }
}

fn inserted_name(prior: &[Edit], class_start: u32) -> Option<String> {
    prior
        .iter()
        .find(|e| e.start == class_start + 5 && e.start == e.end && e.text.starts_with(' '))
        .map(|e| e.text.trim().to_string())
}

fn prior_replacement(prior: &[Edit], span: Span) -> Option<&str> {
    prior
        .iter()
        .find(|e| e.start == span.start && e.end == span.end && e.start < e.end)
        .map(|e| e.text.as_str())
}

fn apply_in(src: &str, edits: &[(Edit, u8)], start: u32, end: u32, ends: &HashSet<u32>) -> String {
    let mut local = Vec::new();
    for (e, pass) in edits {
        if edit_rendered_into(start, end, e) {
            local.push((
                Edit {
                    start: e.start - start,
                    end: e.end - start,
                    text: e.text.clone(),
                },
                *pass,
                e.start,
            ));
        }
    }
    // Same rule as `order_same_offset`: at a class end, the later pass's insert is in front.
    local.sort_by(|(a, pa, aa), (b, pb, ba)| {
        (a.start, a.end).cmp(&(b.start, b.end)).then_with(|| {
            if a.start != a.end || b.start != b.end {
                return std::cmp::Ordering::Equal;
            }
            let la = a.text.starts_with('\n') || ends.contains(aa);
            let lb = b.text.starts_with('\n') || ends.contains(ba);
            match (la, lb) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (true, true) => pb.cmp(pa),
                (false, false) => pa.cmp(pb),
            }
        })
    });
    let plain: Vec<Edit> = local.into_iter().map(|(e, _, _)| e).collect();
    lwc_shared::edits::apply(&src[start as usize..end as usize], plain)
        .expect("lwc-js edits never partially overlap")
}

fn class_needs_lower(class: &Class<'_>, prior: &[Edit]) -> bool {
    fn walk(class: &Class<'_>, prior: &[Edit]) -> bool {
        for el in &class.body.body {
            match el {
                ClassElement::PropertyDefinition(p)
                    if p.r#type != PropertyDefinitionType::TSAbstractPropertyDefinition
                        && !p.declare =>
                {
                    if !matches!(&p.key, PropertyKey::PrivateIdentifier(id) if prior_replacement(prior, id.span).is_none())
                    {
                        return true;
                    }
                }
                ClassElement::MethodDefinition(m) => {
                    if fn_has_class(&m.value) {
                        return true;
                    }
                }
                ClassElement::StaticBlock(s) => {
                    if block_has_class(s) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    fn fn_has_class(f: &oxc::ast::ast::Function<'_>) -> bool {
        f.body
            .as_ref()
            .is_some_and(|b| b.statements.iter().any(stmt_has_class))
    }
    fn stmt_has_class(s: &Statement<'_>) -> bool {
        struct F(bool);
        impl<'a> Visit<'a> for F {
            fn visit_class(&mut self, c: &Class<'a>) {
                self.0 = true;
                let _ = c;
            }
        }
        let mut f = F(false);
        f.visit_statement(s);
        f.0
    }
    fn block_has_class(s: &oxc::ast::ast::StaticBlock<'_>) -> bool {
        s.body.iter().any(stmt_has_class)
    }
    walk(class, prior)
}
