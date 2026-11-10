//! SD-2a: the execution leg, CSR only, template-module pairs only.
//! Design: notes/arch-board/proposals/semdiff-SD-2a.md (AB-D0003 (i)–(iii), D-0196).
//!
//! Split of work: this file does every decision. It runs the static union analysis of both sides (oxc), generates the
//! seeded input set, orchestrates one fresh node process per (case, side, run), compares the traces and issues the
//! verdict. The JS under `exec/` only observes and runs (`prepare.mjs` once per case, `run.mjs` per side and run).
//!
//! Verdict order (design §5; item 5 here is §5 4b):
//!   1. CRASH exec-harness-failure: prepare/run exited non-zero, timed out, or printed a trace that does not parse
//!      into `Trace` against the input script (`parse_trace`).
//!   2. CRASH exec-nondeterministic: the two runs of one side are not byte-identical.
//!   3. DIFFERENT exec-throw-one-side: a throw/error event exists on exactly one side.
//!   4. DIFFERENT exec-differs: the first divergent step.
//!   5. DIFFERENT exec-static-comments-differ: traces equal, comment texts differ (D-0196 observes comments).
//!   6. DIFFERENT static-observed-differs: traces equal, and the rewrite reason is printed-differs,
//!      token-spelling-differs, or printer-equal-ast-differs. The rewrite leg already proved a textual difference
//!      Function#toString observes (D-0196). `first_diff` is the rewrite first_diff; detail adds `trace: "equal"`.
//!      A hoist-only pair is printed-differs, so it is DIFFERENT. Exec still ran: a trace divergence is items 3–4.
//!   7. UNCLASSIFIED exec-uncovered: traces equal and nothing observed was proven (a coverage gap, or the leftover
//!      that used to be EXEC_EQUAL). Coverage does not downgrade a DIFFERENT from item 6.
//!      Reason::ExecEqual and Class::EquivalentByExecution are deleted.
//!
//! DIFFERENT is sound whatever the coverage: an observed difference is a difference. Coverage gates equivalence only.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use oxc::allocator::Allocator;
use oxc::ast::ast::{
    Argument, AssignmentExpression, AssignmentTarget, BindingPattern, CallExpression, Expression, ImportDeclaration,
    ObjectExpression, ObjectProperty, ObjectPropertyKind, PropertyKey, VariableDeclarator,
};
use oxc::ast_visit::{walk, Visit};
use oxc::parser::Parser;
use oxc::span::SourceType;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{sha256_hex, InternalError};

pub const INPUT_SCHEMA: &str = "semdiff-exec-input/v0";
pub const TRACE_SCHEMA: &str = "semdiff-exec-trace/v0";
/// The oracle every CSR run uses (D-0195/SD-0 §5): engine-dom from this checkout, at this commit.
pub const ORACLE_COMMIT: &str = "b9e8007f9810edb59596536b596f80ca6c0d3fab";
/// The oracle checkout: derived at build time (build.rs) from `LWC` in harness/src/ssrRender.ts, the path the JS side
/// imports engine-dom and @lwc/compiler through (AB-L0051 (2): one source). Preflight verifies HEAD and cleanliness here.
pub const ORACLE_DIR: &str = env!("SEMDIFF_ORACLE_DIR");
pub const EXEC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/exec");
/// Runner sources, hashed into `runner_sha256` so a summary names the observer it was produced with.
pub const RUNNER_FILES: [&str; 6] = ["hooks.mjs", "observer.mjs", "prepare.mjs", "register.mjs", "rt.mjs", "run.mjs"];
pub const STEP_CAP: usize = 48;
pub const TIMEOUT: Duration = Duration::from_secs(60);
const LEAF_A: [&str; 7] = [r#""a""#, r#""a b""#, "1", "-1", "true", "[]", "{}"];
const LEAF_B: [&str; 5] = ["null", "undefined", r#""""#, "0", "false"];
const LIST_A: usize = 2;
const LEN_STEPS: [usize; 2] = [3, 1];

// ---------------------------------------------------------------------------------------------------------------------
// Static analysis (design §3). Paths are segment lists: a name, or "[]" for "an element of this list".

const K_LEAF: u8 = 1;
const K_FN: u8 = 2;
const K_LIST: u8 = 4;
const K_CTOR: u8 = 8;

#[derive(Default, Clone, Debug)]
struct Node {
    kinds: u8,
    kids: BTreeMap<String, Node>,
}

impl Node {
    fn mark(&mut self, path: &[String], kind: u8) {
        match path.split_first() {
            None => self.kinds |= kind,
            Some((h, t)) => self.kids.entry(h.clone()).or_default().mark(t, kind),
        }
    }
    fn merge(&mut self, o: &Node) {
        self.kinds |= o.kinds;
        for (k, v) in &o.kids {
            self.kids.entry(k.clone()).or_default().merge(v);
        }
    }
}

/// One side's facts.
#[derive(Default, Debug)]
pub struct Analysis {
    tree: Node,
    pub events: BTreeSet<String>,
    pub stubs: BTreeSet<String>,
    /// $api keys whose local alias is referenced (the helpers this side can call).
    pub helpers: BTreeSet<String>,
    pub light: bool,
    pub gaps: BTreeSet<String>,
    /// Comment texts in source order. D-0196 observes comments statically (Function#toString: the engine's
    /// version-banner check reads `tmpl.toString()`), and no trace step sees them, so `decide` compares them itself.
    pub comments: Vec<String>,
}

#[derive(Clone, Debug)]
enum Binding {
    Path(Vec<String>),
    /// for:of record `const x = {value: xValue, index: …}`: field → path.
    Rec(BTreeMap<String, Vec<String>>),
}

enum Res {
    Path(Vec<String>),
    Rec(BTreeMap<String, Vec<String>>),
    Gap(&'static str),
}

struct Walker {
    a: Analysis,
    cmp: String,
    /// $api local alias → key.
    alias: BTreeMap<String, String>,
    frames: Vec<BTreeMap<String, Binding>>,
    refs: BTreeSet<String>,
}

fn key_name(k: &PropertyKey) -> Option<String> {
    match k {
        PropertyKey::StaticIdentifier(i) => Some(i.name.to_string()),
        PropertyKey::StringLiteral(s) => s.value.as_str().map(String::from),
        _ => None,
    }
}

fn arg_expr<'b, 'a>(a: &'b Argument<'a>) -> Option<&'b Expression<'a>> {
    a.as_expression()
}

impl Walker {
    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.frames.iter().rev().find_map(|f| f.get(name))
    }

    fn chain(&self, e: &Expression) -> Option<Res> {
        match e.without_parentheses() {
            Expression::Identifier(id) if id.name == self.cmp.as_str() => Some(Res::Path(vec![])),
            Expression::Identifier(id) => match self.lookup(&id.name)? {
                Binding::Path(p) => Some(Res::Path(p.clone())),
                Binding::Rec(r) => Some(Res::Rec(r.clone())),
            },
            Expression::StaticMemberExpression(m) => match self.chain(&m.object)? {
                Res::Path(mut p) => {
                    p.push(m.property.name.to_string());
                    Some(Res::Path(p))
                }
                Res::Rec(r) => r.get(m.property.name.as_str()).map(|p| Res::Path(p.clone())),
                g => Some(g),
            },
            Expression::ComputedMemberExpression(m) => match self.chain(&m.object)? {
                Res::Path(mut p) => match &m.expression {
                    Expression::StringLiteral(s) => match s.value.as_str() {
                        Some(v) if !v.is_empty() && !v.bytes().all(|b| b.is_ascii_digit()) => {
                            p.push(v.to_string());
                            Some(Res::Path(p))
                        }
                        _ => Some(Res::Gap("computed-member")),
                    },
                    _ => Some(Res::Gap("computed-member")),
                },
                Res::Rec(_) => Some(Res::Gap("computed-member")),
                g => Some(g),
            },
            _ => None,
        }
    }

    fn path_of(&mut self, e: &Expression) -> Option<Vec<String>> {
        match self.chain(e)? {
            Res::Path(p) if p.is_empty() => {
                self.a.gaps.insert("bare-cmp".into());
                None
            }
            Res::Path(p) => Some(p),
            Res::Rec(_) => None,
            Res::Gap(g) => {
                self.a.gaps.insert(g.into());
                None
            }
        }
    }

    fn mark_arg(&mut self, call: &CallExpression, i: usize, kind: u8) {
        if let Some(p) = call.arguments.get(i).and_then(arg_expr).and_then(|e| self.path_of(e)) {
            self.a.tree.mark(&p, kind);
        }
    }

    fn on_object(&mut self, e: &Expression) {
        match e.without_parentheses() {
            Expression::ObjectExpression(o) => {
                for p in &o.properties {
                    match p {
                        ObjectPropertyKind::ObjectProperty(p) => match key_name(&p.key) {
                            Some(k) if !p.computed => {
                                self.a.events.insert(k);
                            }
                            _ => {
                                self.a.gaps.insert("dynamic-on".into());
                            }
                        },
                        ObjectPropertyKind::SpreadProperty(_) => {
                            self.a.gaps.insert("dynamic-on".into());
                        }
                    }
                }
            }
            Expression::LogicalExpression(l) => {
                self.on_object(&l.left);
                self.on_object(&l.right);
            }
            Expression::AssignmentExpression(a) => self.on_object(&a.right),
            // `_m0` (the cached listener object) or a `$ctx` read: its literal is the other operand.
            Expression::Identifier(_) | Expression::StaticMemberExpression(_) => {}
            _ => {
                self.a.gaps.insert("dynamic-on".into());
            }
        }
    }
}

impl<'a> Visit<'a> for Walker {
    fn visit_expression(&mut self, it: &Expression<'a>) {
        if matches!(
            it.without_parentheses(),
            Expression::Identifier(_) | Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
        ) {
            if let Some(r) = self.chain(it) {
                match r {
                    Res::Path(p) if !p.is_empty() => {
                        self.a.tree.mark(&p, K_LEAF);
                        if let Expression::ComputedMemberExpression(m) = it.without_parentheses() {
                            self.visit_expression(&m.expression);
                        }
                        return;
                    }
                    Res::Path(_) => {
                        self.a.gaps.insert("bare-cmp".into());
                    }
                    Res::Gap(g) => {
                        self.a.gaps.insert(g.into());
                    }
                    Res::Rec(_) => {}
                }
            }
            if let Expression::Identifier(id) = it.without_parentheses() {
                self.refs.insert(id.name.to_string());
            }
        }
        walk::walk_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let helper = match &it.callee {
            Expression::Identifier(id) => self.alias.get(id.name.as_str()).cloned(),
            _ => None,
        };
        match helper.as_deref() {
            Some("i") => {
                self.visit_expression(&it.callee);
                let list = it.arguments.first().and_then(arg_expr).and_then(|e| self.path_of(e));
                if let Some(p) = &list {
                    self.a.tree.mark(p, K_LIST);
                }
                for (i, a) in it.arguments.iter().enumerate() {
                    let param = |ps: &oxc::ast::ast::FormalParameters| match ps.items.first().map(|p| &p.pattern) {
                        Some(BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
                        _ => None,
                    };
                    let bound = match (i, a) {
                        (1, Argument::FunctionExpression(f)) => param(&f.params),
                        (1, Argument::ArrowFunctionExpression(f)) => param(&f.params),
                        _ => None,
                    };
                    match (bound, &list) {
                        (Some(name), Some(p)) => {
                            let mut el = p.clone();
                            el.push("[]".into());
                            self.frames.push(BTreeMap::from([(name, Binding::Path(el))]));
                            self.visit_argument(a);
                            self.frames.pop();
                        }
                        _ if i == 0 => {}
                        _ => self.visit_argument(a),
                    }
                }
                return;
            }
            Some("b") => self.mark_arg(it, 0, K_FN),
            Some("dc") => self.mark_arg(it, 0, K_CTOR),
            Some("ddc") => self.mark_arg(it, 1, K_CTOR),
            _ => {
                if let Some(p) = self.path_of_callee(&it.callee) {
                    self.a.tree.mark(&p, K_FN);
                }
            }
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        match key_name(&it.key).as_deref() {
            Some("on") if !it.computed => self.on_object(&it.value),
            Some("dynamicOn") if !it.computed => {
                self.a.gaps.insert("dynamic-on".into());
            }
            _ => {}
        }
        walk::walk_object_property(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        match (&it.id, &it.init) {
            // const {k: api_key, …} = $api;
            (BindingPattern::ObjectPattern(o), Some(Expression::Identifier(init))) if init.name == "$api" => {
                for p in &o.properties {
                    if let (Some(k), BindingPattern::BindingIdentifier(local)) = (key_name(&p.key), &p.value) {
                        self.alias.insert(local.name.to_string(), k);
                    }
                }
            }
            // for:of record
            (BindingPattern::BindingIdentifier(id), Some(Expression::ObjectExpression(o))) => {
                if let Some(rec) = self.record_of(o) {
                    if let Some(f) = self.frames.last_mut() {
                        f.insert(id.name.to_string(), Binding::Rec(rec));
                    }
                }
            }
            _ => {}
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let (AssignmentTarget::StaticMemberExpression(m), Expression::StringLiteral(s)) = (&it.left, &it.right) {
            if m.property.name == "renderMode" && s.value == "light" {
                self.a.light = true;
            }
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let Some(s) = it.source.value.as_str() else {
            self.a.gaps.insert("import-unsupported:non-utf8".into());
            return;
        };
        if s == "lwc" || s.ends_with(".css") || s.ends_with(".css?scoped=true") {
        } else if is_stub_spec(s) {
            self.a.stubs.insert(s.to_string());
        } else {
            self.a.gaps.insert(format!("import-unsupported:{s}"));
        }
    }
}

impl Walker {
    fn path_of_callee(&mut self, callee: &Expression) -> Option<Vec<String>> {
        match callee.without_parentheses() {
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => self.path_of(callee),
            _ => None,
        }
    }
    fn record_of(&self, o: &ObjectExpression) -> Option<BTreeMap<String, Vec<String>>> {
        let mut rec = BTreeMap::new();
        for p in &o.properties {
            let ObjectPropertyKind::ObjectProperty(p) = p else { return None };
            if let (Some(k), Expression::Identifier(v)) = (key_name(&p.key), &p.value) {
                if let Some(Binding::Path(path)) = self.lookup(&v.name) {
                    rec.insert(k, path.clone());
                }
            }
        }
        (!rec.is_empty()).then_some(rec)
    }
}

pub fn is_stub_spec(s: &str) -> bool {
    let ok = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    match s.split_once('/') {
        Some((ns, name)) => ns.as_bytes().first().is_some_and(u8::is_ascii_lowercase) && ok(ns) && ok(name),
        None => false,
    }
}

/// Analyze one template module. The `$cmp` name is the second parameter of the function whose first is `$api`.
pub fn analyze(src: &str) -> Analysis {
    let alloc = Allocator::default();
    let ret = Parser::new(&alloc, src, SourceType::mjs()).parse();
    let mut cmp = None;
    for s in &ret.program.body {
        if let oxc::ast::ast::Statement::FunctionDeclaration(f) = s {
            let names: Vec<_> = f
                .params
                .items
                .iter()
                .map(|p| match &p.pattern {
                    BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
                    _ => None,
                })
                .collect();
            if names.first().cloned().flatten().as_deref() == Some("$api") {
                cmp = names.get(1).cloned().flatten();
            }
        }
    }
    let mut w = Walker {
        a: Analysis::default(),
        cmp: cmp.clone().unwrap_or_default(),
        alias: BTreeMap::new(),
        frames: vec![BTreeMap::new()],
        refs: BTreeSet::new(),
    };
    if ret.diagnostics.errors().count() > 0 {
        w.a.gaps.insert("parse-error".into());
    }
    if cmp.is_none() {
        w.a.gaps.insert("no-tmpl".into());
    }
    w.visit_program(&ret.program);
    w.a.comments = ret.program.comments.iter().map(|c| c.span.source_text(src).to_string()).collect();
    let refs = std::mem::take(&mut w.refs);
    w.a.helpers = w.alias.iter().filter(|(l, _)| refs.contains(*l)).map(|(_, k)| k.clone()).collect();
    w.a
}

// ---------------------------------------------------------------------------------------------------------------------
// Generator (design §3).

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn hash_u64(parts: &[&str]) -> u64 {
    let h = sha256_hex(parts.join("\u{0}").as_bytes());
    u64::from_str_radix(&h[..16], 16).expect("hex")
}

pub fn path_str(p: &[String]) -> String {
    let mut s = String::new();
    for seg in p {
        if seg == "[]" {
            s.push_str("[]");
        } else {
            if !s.is_empty() {
                s.push('.');
            }
            s.push_str(seg);
        }
    }
    s
}

struct Gen<'g> {
    seed: u64,
    case: &'g str,
    gaps: BTreeSet<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    A,
    B,
}

impl Gen<'_> {
    fn pick(&self, path: &[String], idx: Option<usize>, mode: Mode) -> Value {
        let tag = match mode {
            Mode::A => "A",
            Mode::B => "B",
        };
        let mut st = hash_u64(&[&self.seed.to_string(), self.case, &path_str(path), tag]);
        let r = splitmix64(&mut st);
        let set: &[&str] = if mode == Mode::A { &LEAF_A } else { &LEAF_B };
        let lit = set[(r % set.len() as u64) as usize];
        if lit == "undefined" {
            return json!({"u": 1});
        }
        let mut v: Value = serde_json::from_str(lit).expect("leaf literal");
        if let Some(i) = idx {
            v = match v {
                Value::String(s) => Value::String(format!("{s}{i}")),
                Value::Number(n) => json!(n.as_i64().expect("int leaf") + i as i64),
                v => v,
            };
        }
        json!({"l": v})
    }

    /// `idx`: the index of the innermost list element this node is under (string/number leaves get it as a suffix,
    /// so keys derived from items differ).
    fn spec(&mut self, n: &Node, path: &mut Vec<String>, mode: Mode, idx: Option<usize>, lens: &BTreeMap<String, usize>) -> Value {
        let conflict = |g: &mut BTreeSet<String>, p: &[String]| {
            g.insert(format!("kind-conflict:{}", path_str(p)));
        };
        if n.kinds & K_FN != 0 {
            if !n.kids.is_empty() || n.kinds & (K_LIST | K_CTOR) != 0 {
                conflict(&mut self.gaps, path);
            }
            return json!({"f": 1});
        }
        if n.kinds & K_CTOR != 0 {
            if !n.kids.is_empty() || n.kinds & K_LIST != 0 {
                conflict(&mut self.gaps, path);
            }
            return if mode == Mode::A { json!({"c": 1}) } else { json!({"l": null}) };
        }
        if n.kinds & K_LIST != 0 {
            if n.kids.keys().any(|k| k != "[]" && k != "length") {
                conflict(&mut self.gaps, path);
            }
            let len = lens.get(&path_str(path)).copied().unwrap_or(if mode == Mode::A { LIST_A } else { 0 });
            let el = n.kids.get("[]").cloned().unwrap_or_default();
            path.push("[]".into());
            let items: Vec<Value> = (0..len).map(|i| self.spec(&el, path, Mode::A, Some(i), lens)).collect();
            path.pop();
            return json!({"a": items});
        }
        if n.kids.contains_key("[]") {
            conflict(&mut self.gaps, path);
        }
        if !n.kids.is_empty() {
            let mut o = Vec::new();
            for (k, kid) in &n.kids {
                path.push(k.clone());
                o.push(json!([k, self.spec(kid, path, mode, idx, lens)]));
                path.pop();
            }
            return json!({"o": o});
        }
        self.pick(path, idx, mode)
    }
}

fn walk_paths(n: &Node, path: &mut Vec<String>, out: &mut BTreeSet<String>, lists: &mut Vec<String>) {
    if !path.is_empty() {
        out.insert(path_str(path));
        if n.kinds & K_LIST != 0 && n.kinds & K_FN == 0 {
            lists.push(path_str(path));
        }
    }
    for (k, kid) in &n.kids {
        path.push(k.clone());
        walk_paths(kid, path, out, lists);
        path.pop();
    }
}

/// The union input set (AB-D0003 (i)), deterministic in (seed, case_key, the two sources).
pub struct InputSet {
    pub json: Value,
    pub o: Analysis,
    pub r: Analysis,
    pub slots: BTreeSet<String>,
    pub events: BTreeSet<String>,
    pub gaps: BTreeSet<String>,
    pub rewrite_reason: crate::Reason,
    pub rewrite_first_diff: Value,
}

impl InputSet {
    /// Record the rewrite-leg reason so `decide` can emit static-observed-differs when Function#toString would see a difference.
    pub fn observe_rewrite(&mut self, reason: crate::Reason, first_diff: &Value) {
        self.rewrite_reason = reason;
        self.rewrite_first_diff = first_diff.clone();
    }
}

/// Rewrite reasons that mean the printed (or printer-equal-but-not-token-equal) texts still differ by something
/// D-0196 observes via Function#toString. Equal traces are not evidence those texts agree.
pub fn rewrite_is_statically_observed(reason: crate::Reason) -> bool {
    matches!(reason, crate::Reason::PrintedDiffers | crate::Reason::TokensDiffer | crate::Reason::AstDiffers)
}

pub fn generate(seed: u64, case_key: &str, oracle: &str, rust: &str) -> InputSet {
    let (o, r) = (analyze(oracle), analyze(rust));
    let mut tree = o.tree.clone();
    tree.merge(&r.tree);
    let events: BTreeSet<String> = o.events.union(&r.events).cloned().collect();
    let stubs: BTreeSet<String> = o.stubs.union(&r.stubs).cloned().collect();
    let mut g = Gen { seed, case: case_key, gaps: o.gaps.union(&r.gaps).cloned().collect() };
    let none = BTreeMap::new();
    let mut slots = BTreeSet::new();
    let mut lists = Vec::new();
    walk_paths(&tree, &mut Vec::new(), &mut slots, &mut lists);
    let dyn_ctor = has_kind(&tree, K_CTOR);

    let (mut roots, mut props_a, mut props_b) = (Vec::new(), Vec::new(), Vec::new());
    for (name, n) in &tree.kids {
        let is_fn = n.kinds & K_FN != 0;
        roots.push(json!({"name": name, "kind": if is_fn { "fn" } else { "prop" }}));
        if is_fn {
            g.spec(n, &mut vec![name.clone()], Mode::A, None, &none); // kind-conflict check only
            continue;
        }
        props_a.push((name.clone(), g.spec(n, &mut vec![name.clone()], Mode::A, None, &none)));
        props_b.push((name.clone(), g.spec(n, &mut vec![name.clone()], Mode::B, None, &none)));
    }
    let props = |v: &[(String, Value)]| Value::Array(v.iter().map(|(k, s)| json!([k, s])).collect());
    let mut steps = vec![json!({"kind": "load", "label": "load"}), json!({"kind": "mount", "label": "mount(A)", "props": props(&props_a)})];
    for t in &events {
        steps.push(json!({"kind": "event", "label": format!("event:{t}"), "type": t}));
    }
    for (k, s) in &props_b {
        steps.push(json!({"kind": "set", "label": format!("set:{k}=B"), "props": props(&[(k.clone(), s.clone())])}));
    }
    for l in &lists {
        let root = l.split(['.', '[']).next().unwrap_or(l).to_string();
        let Some(n) = tree.kids.get(&root) else { continue };
        if n.kinds & K_FN != 0 {
            continue;
        }
        for len in LEN_STEPS {
            let lens = BTreeMap::from([(l.clone(), len)]);
            let s = g.spec(n, &mut vec![root.clone()], Mode::A, None, &lens);
            steps.push(json!({"kind": "set", "label": format!("len:{l}={len}"), "props": props(&[(root.clone(), s)])}));
        }
    }
    steps.push(json!({"kind": "set", "label": "reset(A)", "props": props(&props_a)}));
    steps.push(json!({"kind": "unmount", "label": "unmount"}));
    if steps.len() > STEP_CAP {
        let tail = steps.split_off(STEP_CAP - 1);
        steps.push(tail.last().cloned().expect("unmount"));
        g.gaps.insert("script-truncated".into());
    }
    let mut st = hash_u64(&[&seed.to_string(), case_key, "rng"]);
    let rng_seed = splitmix64(&mut st);
    let json = json!({
        "schema": INPUT_SCHEMA,
        "seed": seed,
        "rng_seed": rng_seed.to_string(),
        "light": o.light || r.light,
        "roots": roots,
        "stubs": stubs,
        "dyn_ctor": dyn_ctor,
        "events": events,
        "slots": slots,
        "gaps": g.gaps,
        "steps": steps,
    });
    let v = crate::compare(oracle, rust);
    InputSet {
        json,
        slots,
        events,
        gaps: g.gaps,
        o,
        r,
        rewrite_reason: v.reason,
        rewrite_first_diff: v.json["first_diff"].clone(),
    }
}

fn has_kind(n: &Node, k: u8) -> bool {
    n.kinds & k != 0 || n.kids.values().any(|c| has_kind(c, k))
}

// ---------------------------------------------------------------------------------------------------------------------
// Runner (design §4).

pub struct ExecCfg {
    pub seed: u64,
    pub jobs: usize,
    pub oracle_dir: PathBuf,
    pub work: PathBuf,
}

/// Environment facts recorded in the summary; also the preflight: wrong oracle commit or no node = internal error.
pub fn preflight(cfg: &ExecCfg) -> Result<Value, InternalError> {
    let git = |args: &[&str]| -> Result<String, InternalError> {
        let o = Command::new("git").arg("-C").arg(&cfg.oracle_dir).args(args).output()
            .map_err(|e| InternalError(format!("exec: git: {e}")))?;
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let head = git(&["rev-parse", "HEAD"])?;
    if head != ORACLE_COMMIT {
        return Err(InternalError(format!("exec: oracle {} is at {head:?}, need {ORACLE_COMMIT}", cfg.oracle_dir.display())));
    }
    if !git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Err(InternalError(format!("exec: oracle {} has tracked modifications", cfg.oracle_dir.display())));
    }
    let node = Command::new("node").arg("--version").output().map_err(|e| InternalError(format!("exec: node: {e}")))?;
    let node = String::from_utf8_lossy(&node.stdout).trim().to_string();
    let mut runner = String::new();
    for f in RUNNER_FILES {
        let t = fs::read_to_string(Path::new(EXEC_DIR).join(f)).map_err(|e| InternalError(format!("exec: read {f}: {e}")))?;
        runner.push_str(&format!("{f}\0{}\0", sha256_hex(t.as_bytes())));
    }
    Ok(json!({
        "runner_sha256": sha256_hex(runner.as_bytes()),
        "oracle_commit": head,
        "engine": "@lwc/engine-dom (oracle dist), jsdom, CSR",
        "node": node,
        "seed": cfg.seed,
    }))
}

enum Proc {
    Ok(String),
    Fail(String),
}

fn node(dir: &Path, script: &str, template: Option<&Path>, out_name: &str) -> Proc {
    let exec = Path::new(EXEC_DIR);
    let out = dir.join(out_name);
    let err = dir.join(format!("{out_name}.stderr"));
    let (Ok(fo), Ok(fe)) = (fs::File::create(&out), fs::File::create(&err)) else { return Proc::Fail("create output".into()) };
    let mut cmd = Command::new("node");
    let oracle = std::env::var_os("LWC_ORACLE")
        .unwrap_or_else(|| std::ffi::OsString::from(env!("SEMDIFF_ORACLE_DIR")));
    cmd.env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .env("NODE_ENV", "test")
        .env("TZ", "UTC")
        .env("LANG", "C")
        .env("LWC_ORACLE", &oracle)
        .env("LWC_ROOT", &oracle)
        .env("SD_EXEC_CASE", dir)
        .arg("--import")
        .arg(exec.join("register.mjs"))
        .arg(exec.join(script))
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(fo)
        .stderr(fe);
    if let Some(t) = template {
        cmd.env("SD_EXEC_TEMPLATE", t);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Proc::Fail(format!("spawn node: {e}")),
    };
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Proc::Fail(format!("{script}: timeout after {}s", TIMEOUT.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Proc::Fail(format!("{script}: wait: {e}")),
        }
    };
    let mut stderr = String::new();
    let _ = fs::File::open(&err).and_then(|mut f| f.read_to_string(&mut stderr));
    if !status.success() {
        let tail: String = stderr.lines().filter(|l| !l.trim().is_empty()).take(3).collect::<Vec<_>>().join(" | ");
        return Proc::Fail(format!("{script}: exit {status}: {}", tail.chars().take(400).collect::<String>()));
    }
    match fs::read_to_string(&out) {
        Ok(s) => Proc::Ok(s),
        Err(e) => Proc::Fail(format!("{script}: read output: {e}")),
    }
}

/// Outcome of one case's execution leg.
pub enum Outcome {
    HarnessFailure(String),
    Nondeterministic(&'static str),
    Gap(String),
    Traces { o: Box<Trace>, r: Box<Trace> },
}

// Trace types (design §2). The JSON is parsed into these at the `parse_trace` boundary and nowhere else; a trace that
// does not fit them is a harness failure (AB-L0051 (1)). Field order is alphabetical so that serializing a parsed
// trace yields the same bytes serde_json's sorted maps did (trace_sha256 is a hash of that serialization).

/// One run's trace: `run.mjs` `trace`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub handled: Vec<String>,
    /// observer.mjs `S.helpers`: $api key → call count.
    pub helpers: BTreeMap<String, u64>,
    pub reads: Vec<String>,
    pub schema: String,
    pub steps: Vec<Step>,
}

/// One scripted step as it ran (or did not). `kind`/`label` match the input script position for position.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "RawStep", into = "RawStep")]
pub struct Step {
    pub kind: String,
    pub label: String,
    pub body: StepBody,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StepBody {
    Ran { events: Vec<Event>, dom: Dom },
    /// Not run (`load` threw); the string is run.mjs's reason.
    Skipped(String),
}

/// The wire shape of a step: `{kind, label, events, dom}` or `{kind, label, skipped}`, nothing else.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    #[serde(skip_serializing_if = "Option::is_none")]
    dom: Option<Dom>,
    #[serde(skip_serializing_if = "Option::is_none")]
    events: Option<Vec<Event>>,
    kind: String,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<String>,
}

impl TryFrom<RawStep> for Step {
    type Error = String;
    fn try_from(r: RawStep) -> Result<Self, String> {
        let body = match (r.events, r.dom, r.skipped) {
            (Some(events), Some(dom), None) => StepBody::Ran { events, dom },
            (None, None, Some(why)) if r.kind != "load" => StepBody::Skipped(why),
            (e, d, sk) => {
                return Err(format!(
                    "step {:?}: want {{events, dom}} or {{skipped}} (not on load), got events:{} dom:{} skipped:{}",
                    r.label,
                    e.is_some(),
                    d.is_some(),
                    sk.is_some()
                ))
            }
        };
        Ok(Step { kind: r.kind, label: r.label, body })
    }
}

impl From<Step> for RawStep {
    fn from(s: Step) -> Self {
        let (events, dom, skipped) = match s.body {
            StepBody::Ran { events, dom } => (Some(events), Some(dom), None),
            StepBody::Skipped(why) => (None, None, Some(why)),
        };
        RawStep { dom, events, kind: s.kind, label: s.label, skipped }
    }
}

/// run.mjs `snapshot()`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dom {
    pub body: Value,
    pub head: Vec<Value>,
}

/// One observer event: an object whose `e` is a string (rt.mjs/observer.mjs `record`). The other fields vary by `e`
/// and are compared as JSON.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "Map<String, Value>")]
pub struct Event(Map<String, Value>);

impl TryFrom<Map<String, Value>> for Event {
    type Error = String;
    fn try_from(m: Map<String, Value>) -> Result<Self, String> {
        match m.get("e") {
            Some(Value::String(_)) => Ok(Event(m)),
            _ => Err(format!("event without a string `e`: {}", Value::Object(m))),
        }
    }
}

impl Serialize for Event {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl Event {
    pub fn kind(&self) -> &str {
        self.0["e"].as_str().expect("validated in TryFrom")
    }
    fn to_value(&self) -> Value {
        Value::Object(self.0.clone())
    }
}

/// The script's (kind, label) per step; `generate` always writes both as strings.
fn script(input: &Value) -> Vec<(&str, &str)> {
    input["steps"]
        .as_array()
        .expect("generate: steps")
        .iter()
        .map(|s| (s["kind"].as_str().expect("generate: kind"), s["label"].as_str().expect("generate: label")))
        .collect()
}

/// The only place trace JSON is read. `input` is the input set the run was given: the trace must have one step per
/// scripted step, same kind and label, so it starts `load`, `mount(A)` (generate always emits that prefix; the load
/// step is where module-evaluation throws land, so the first step that mounts anything is step 1).
pub fn parse_trace(s: &str, input: &Value) -> Result<Trace, String> {
    let t: Trace = serde_json::from_str(s).map_err(|e| format!("trace: {e}"))?;
    if t.schema != TRACE_SCHEMA {
        return Err(format!("trace schema must be {TRACE_SCHEMA}, got {:?}", t.schema));
    }
    if t.steps.is_empty() {
        return Err("trace has no steps".into());
    }
    let at = |i: usize| t.steps.get(i).map(|s| (s.kind.as_str(), s.label.as_str()));
    if at(0) != Some(("load", "load")) || at(1) != Some(("mount", "mount(A)")) {
        return Err(format!("trace must start load, mount(A); got {:?}, {:?}", at(0), at(1)));
    }
    let want = script(input);
    if t.steps.len() != want.len() {
        return Err(format!("trace has {} steps, the script {}", t.steps.len(), want.len()));
    }
    if let Some(i) = (0..want.len()).find(|&i| at(i) != Some(want[i])) {
        return Err(format!("trace step {i} is {:?}, the script's is {:?}", at(i), want[i]));
    }
    Ok(t)
}

/// Parse and check the four runs (oracle 0, 1, rust 0, 1). `run(side, n)` yields run n's stdout or a failure message;
/// it is called in that order and not again after the first failure.
pub fn collect(input: &Value, mut run: impl FnMut(&str, usize) -> Result<String, String>) -> Outcome {
    let mut traces = Vec::new();
    for side in ["oracle", "rust"] {
        let mut runs = Vec::new();
        for n in 0..2 {
            match run(side, n).and_then(|s| parse_trace(&s, input)) {
                Ok(t) => runs.push(t),
                Err(m) => return Outcome::HarnessFailure(format!("{side} run {n}: {m}")),
            }
        }
        if runs[0] != runs[1] {
            return Outcome::Nondeterministic(if side == "oracle" { "oracle" } else { "rust" });
        }
        traces.push(runs.swap_remove(0));
    }
    let r = traces.pop().expect("rust");
    let o = traces.pop().expect("oracle");
    Outcome::Traces { o: Box::new(o), r: Box::new(r) }
}

pub fn run_case(dir: &Path, input: &Value, oracle: &str, rust: &str) -> Outcome {
    let hf = |m: String| Outcome::HarnessFailure(m);
    if let Err(e) = fs::create_dir_all(dir)
        .and_then(|_| fs::write(dir.join("input.json"), serde_json::to_string(input).expect("json")))
        .and_then(|_| fs::write(dir.join("oracle.js"), oracle))
        .and_then(|_| fs::write(dir.join("rust.js"), rust))
    {
        return hf(format!("write case dir: {e}"));
    }
    if let Proc::Fail(m) = node(dir, "prepare.mjs", None, "prepare.out") {
        return hf(m);
    }
    let map: Value = match fs::read_to_string(dir.join("map.json")).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
        Ok(v) => v,
        Err(e) => return hf(format!("map.json: {e}")),
    };
    if let Some(g) = map.get("gap").and_then(Value::as_str) {
        return Outcome::Gap(g.to_string());
    }
    collect(input, |side, n| match node(dir, "run.mjs", Some(&dir.join(format!("{side}.js"))), &format!("{side}.{n}.json")) {
        Proc::Ok(s) => Ok(s),
        Proc::Fail(m) => Err(m),
    })
}

/// Run `f` over `items` on `jobs` threads; results come back in input order.
pub fn par_map<T: Sync, R: Send>(items: &[T], jobs: usize, f: impl Fn(usize, &T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..jobs.max(1).min(items.len().max(1)) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= items.len() {
                    break;
                }
                let r = f(i, &items[i]);
                out.lock().expect("lock")[i] = Some(r);
            });
        }
    });
    out.into_inner().expect("lock").into_iter().map(|r| r.expect("every item ran")).collect()
}

// ---------------------------------------------------------------------------------------------------------------------
// Comparison (design §5).

fn clip(v: &Value) -> String {
    let s = serde_json::to_string(v).expect("json");
    if s.chars().count() > 300 { format!("{}…", s.chars().take(300).collect::<String>()) } else { s }
}

fn window(a: &str, b: &str) -> (usize, String, String) {
    let at = a.char_indices().zip(b.chars()).find(|((_, x), y)| x != y).map(|((i, _), _)| i).unwrap_or(a.len().min(b.len()));
    let at = (0..=at).rev().find(|i| a.is_char_boundary(*i) && b.is_char_boundary(*i)).unwrap_or(0);
    let from = (0..=at.saturating_sub(60)).rev().find(|i| a.is_char_boundary(*i) && b.is_char_boundary(*i)).unwrap_or(0);
    let cut = |s: &str| s[from..].chars().take(200).collect::<String>();
    (at, cut(a), cut(b))
}

/// `None` when the traces are identical. Both traces passed `parse_trace` against the same script, so they have the
/// same steps (kind, label) in the same order; a length difference is still reported, never skipped.
pub fn first_divergence(o: &Trace, r: &Trace) -> Option<Value> {
    for (i, (a, b)) in o.steps.iter().zip(&r.steps).enumerate() {
        if a == b {
            continue;
        }
        let label = json!(a.label);
        if (&a.kind, &a.label) != (&b.kind, &b.label) {
            return Some(json!({"coords": "exec-step", "index": i, "label": label, "kind": "step", "event_index": null,
                "oracle": format!("{}:{}", a.kind, a.label), "rust": format!("{}:{}", b.kind, b.label)}));
        }
        let skipped = |s: &Step| match &s.body {
            StepBody::Skipped(why) => json!(why),
            StepBody::Ran { .. } => Value::Null,
        };
        let (StepBody::Ran { events: ea, dom: da }, StepBody::Ran { events: eb, dom: db }) = (&a.body, &b.body) else {
            return Some(json!({"coords": "exec-step", "index": i, "label": label, "kind": "skipped", "event_index": null,
                "oracle": clip(&skipped(a)), "rust": clip(&skipped(b))}));
        };
        if let Some(j) = (0..ea.len().max(eb.len())).find(|j| ea.get(*j) != eb.get(*j)) {
            let ev = |e: &[Event]| e.get(j).map(|e| clip(&e.to_value()));
            return Some(json!({"coords": "exec-step", "index": i, "label": label, "kind": "event", "event_index": j,
                "oracle": ev(ea), "rust": ev(eb)}));
        }
        let (da, db) = (dom_json(da), dom_json(db));
        let (at, x, y) = window(&da, &db);
        return Some(json!({"coords": "exec-step", "index": i, "label": label, "kind": "dom", "event_index": null,
            "dom_offset": at, "oracle": x, "rust": y}));
    }
    let (n, m) = (o.steps.len(), r.steps.len());
    (n != m).then(|| {
        let i = n.min(m);
        let step = |t: &Trace| t.steps.get(i).map(|s| clip(&serde_json::to_value(s).expect("json")));
        json!({"coords": "exec-step", "index": i, "kind": "missing-step", "event_index": null, "oracle": step(o), "rust": step(r)})
    })
}

fn dom_json(d: &Dom) -> String {
    serde_json::to_string(d).expect("json")
}

fn has_throw(t: &Trace) -> bool {
    t.steps.iter().any(|s| match &s.body {
        StepBody::Ran { events, .. } => events.iter().any(|e| matches!(e.kind(), "throw" | "error")),
        StepBody::Skipped(_) => false,
    })
}

/// What equal traces did NOT cover (empty = covered). Design §5 step 5.
pub fn coverage_missing(input: &InputSet, o: &Trace, r: &Trace) -> Vec<String> {
    let mut miss: Vec<String> = input.gaps.iter().map(|g| format!("gap:{g}")).collect();
    for (side, a, t) in [("oracle", &input.o, o), ("rust", &input.r, r)] {
        miss.extend(a.helpers.iter().filter(|h| !t.helpers.contains_key(*h)).map(|h| format!("helper-not-run:{side}:{h}")));
    }
    let reads: BTreeSet<&str> = [o, r].iter().flat_map(|t| t.reads.iter().map(String::as_str)).collect();
    miss.extend(input.slots.iter().filter(|s| !reads.contains(s.as_str())).map(|s| format!("slot-not-read:{s}")));
    let handled: BTreeSet<&str> = [o, r].iter().flat_map(|t| t.handled.iter().map(String::as_str)).collect();
    miss.extend(input.events.iter().filter(|e| !handled.contains(e.as_str())).map(|e| format!("event-not-handled:{e}")));
    miss
}

/// The execution leg's decision for one case: (class, reason, first_diff, trace_sha256, detail).
pub struct ExecDecision {
    pub class: crate::Class,
    pub reason: crate::Reason,
    pub first_diff: Value,
    pub trace_sha256: Option<String>,
    pub detail: Value,
}

/// First differing comment (D-0196: statically observed). `None` when the comment lists are identical.
fn comment_divergence(o: &[String], r: &[String]) -> Option<Value> {
    let i = (0..o.len().max(r.len())).find(|&i| o.get(i) != r.get(i))?;
    Some(json!({"coords": "static-comments", "index": i, "oracle": o.get(i), "rust": r.get(i)}))
}

/// Order (design §5): harness failure / nondeterminism → crash; a trace divergence → DIFFERENT at the first divergent
/// step; else a statically observed comment difference → DIFFERENT; else a coverage gap → UNCLASSIFIED; else equal.
/// DIFFERENT does not depend on coverage (a seen difference is a difference); EQUIVALENT does.
pub fn decide(input: &InputSet, out: Outcome) -> ExecDecision {
    use crate::{Class, Reason};
    let d = |class, reason, detail: Value| ExecDecision { class, reason, first_diff: Value::Null, trace_sha256: None, detail };
    let comments = comment_divergence(&input.o.comments, &input.r.comments);
    let static_comment = |fd: Value, trace_sha256, detail| ExecDecision {
        class: Class::Different,
        reason: Reason::ExecStaticCommentsDiffer,
        first_diff: fd,
        trace_sha256,
        detail,
    };
    let refuse_exec_equal = rewrite_is_statically_observed(input.rewrite_reason);
    let static_observed = || ExecDecision {
        class: Class::Different,
        reason: Reason::StaticObservedDiffers,
        first_diff: input.rewrite_first_diff.clone(),
        trace_sha256: None,
        detail: json!({"rewrite_reason": input.rewrite_reason.as_str(), "rewrite_first_diff": input.rewrite_first_diff, "trace": "equal"}),
    };
    match out {
        Outcome::HarnessFailure(m) => d(Class::Crash, Reason::ExecHarnessFailure, json!(m)),
        Outcome::Nondeterministic(side) => d(Class::Crash, Reason::ExecNondeterministic, json!(format!("{side} runs differ"))),
        Outcome::Gap(g) => {
            let miss = json!([format!("gap:{g}")]);
            match comments {
                Some(fd) => static_comment(fd, None, miss),
                None => d(Class::Unclassified, Reason::ExecUncovered, miss),
            }
        }
        Outcome::Traces { o, r } => {
            let sha = |t: &Trace| sha256_hex(serde_json::to_string(t).expect("json").as_bytes());
            if let Some(fd) = first_divergence(&o, &r) {
                let reason = if has_throw(&o) != has_throw(&r) { Reason::ExecThrowOneSide } else { Reason::ExecDiffers };
                return ExecDecision {
                    class: Class::Different,
                    reason,
                    first_diff: fd,
                    trace_sha256: None,
                    detail: json!({"oracle_trace_sha256": sha(&o), "rust_trace_sha256": sha(&r)}),
                };
            }
            let miss = coverage_missing(input, &o, &r);
            let trace_sha256 = Some(sha(&o));
            if let Some(fd) = comments {
                return static_comment(fd, trace_sha256, json!(miss));
            }
            if refuse_exec_equal {
                let mut d = static_observed();
                d.trace_sha256 = trace_sha256;
                return d;
            }
            // No EXEC_EQUAL path. Helper/slot/event coverage may be complete; branch coverage is not proven (F2).
            let detail = if miss.is_empty() { json!({"branch_coverage": "not-proven"}) } else { json!(miss) };
            ExecDecision { class: Class::Unclassified, reason: Reason::ExecUncovered, first_diff: Value::Null, trace_sha256, detail }
        }
    }
}

/// Apply the execution leg to a rewrite verdict JSON (which stays under `rewrite`).
pub fn apply(verdict: &mut Map<String, Value>, input: &InputSet, d: &ExecDecision) {
    let rewrite = json!({"reason": verdict["reason"].clone(), "first_diff": verdict["first_diff"].clone()});
    verdict.insert("class".into(), json!(d.class.as_str()));
    verdict.insert("reason".into(), json!(d.reason.as_str()));
    verdict.insert("first_diff".into(), d.first_diff.clone());
    verdict.insert("input_set_sha256".into(), json!(sha256_hex(serde_json::to_string(&input.json).expect("json").as_bytes())));
    verdict.insert("trace_sha256".into(), json!(d.trace_sha256));
    verdict.insert("rewrite".into(), rewrite);
    verdict.insert("exec".into(), json!({"detail": d.detail, "input_set": input.json}));
}

pub fn case_dir(work: &Path, i: usize) -> PathBuf {
    work.join(format!("c{i:06}"))
}
