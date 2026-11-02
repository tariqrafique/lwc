//! Port of postcss-lwc-plugin: keyframes scoping, selector validation, selector scoping,
//! :host transform (native/synthetic rule pair) and :dir() placeholders.
use crate::ast::*;
use crate::consts::*;
use crate::selector::{self, Kind, SNode, Selector, SelectorList};
use crate::{Config, StyleError};
use std::collections::HashSet;

/// Port of StyleCompilerCtx: in recovery mode collect (deduped) errors, else fail fast.
pub struct Ctx { pub recovery: bool, pub errors: Vec<StyleError>, seen: HashSet<String> }
impl Ctx {
    pub fn new(recovery: bool) -> Self { Ctx { recovery, errors: vec![], seen: HashSet::new() } }
    pub fn report(&mut self, e: StyleError) -> Result<(), StyleError> {
        if !self.recovery { return Err(e); }
        if self.seen.insert(format!("{e:?}")) { self.errors.push(e); }
        Ok(())
    }
}

pub fn scope_root(root: &mut Root, config: &Config, src: &str) -> Result<(), StyleError> {
    let mut ctx = Ctx::new(config.experimental_error_recovery_mode);
    transform_imports(&mut root.nodes, &mut root.imports, true, config.scoped, &mut ctx)?;
    scope_at_rules(&mut root.nodes);
    root.nodes = walk_rules(std::mem::take(&mut root.nodes), false, config, src, &mut ctx)?;
    if !ctx.errors.is_empty() { return Err(StyleError::Aggregate(ctx.errors)); }
    Ok(())
}

// ---------- scope-at-rules (keyframes) ----------
const VENDOR_PREFIXES: [&str; 4] = ["moz", "ms", "o", "webkit"];
fn has_name(prop: &str, name: &str) -> bool {
    prop == name || VENDOR_PREFIXES.iter().any(|p| prop == format!("-{p}-{name}"))
}

fn scope_at_rules(nodes: &mut [Node]) {
    let mut known = HashSet::new();
    collect_keyframes(nodes, &mut known);
    rewrite_animation(nodes, &known, false);
}
fn collect_keyframes(nodes: &mut [Node], known: &mut HashSet<String>) {
    for n in nodes {
        match n {
            Node::AtRule(a) => {
                if a.name == "keyframes" {
                    known.insert(a.params.clone());
                    a.params = format!("{}-{SHADOW_ATTRIBUTE}", a.params);
                }
                if let Some(b) = &mut a.block { collect_keyframes(&mut b.nodes, known); }
            }
            Node::Rule(r) => collect_keyframes(&mut r.block.nodes, known),
            _ => {}
        }
    }
}
fn rewrite_animation(nodes: &mut [Node], known: &HashSet<String>, in_rule: bool) {
    for n in nodes {
        match n {
            Node::Decl(d) if in_rule => {
                if has_name(&d.prop, "animation") {
                    // postcss exposes the comment-free value; the raw (with comments) survives only
                    // when the reassigned value equals the clean one.
                    let clean = strip_comments(&d.value);
                    let new = clean.split_whitespace().map(|t| if known.contains(t) { format!("{t}-{SHADOW_ATTRIBUTE}") } else { t.to_string() }).collect::<Vec<_>>().join(" ");
                    if new != clean.trim() { d.value = new; }
                } else if has_name(&d.prop, "animation-name") && known.contains(&d.value) {
                    d.value = format!("{}-{SHADOW_ATTRIBUTE}", d.value);
                }
            }
            Node::AtRule(a) => if let Some(b) = &mut a.block { rewrite_animation(&mut b.nodes, known, in_rule) },
            Node::Rule(r) => rewrite_animation(&mut r.block.nodes, known, true),
            _ => {}
        }
    }
}

// ---------- rule walk ----------
fn walk_rules(nodes: Vec<Node>, in_keyframes: bool, config: &Config, src: &str, ctx: &mut Ctx) -> Result<Vec<Node>, StyleError> {
    let mut out = Vec::with_capacity(nodes.len());
    for n in nodes {
        match n {
            Node::Rule(mut r) => {
                r.block.nodes = walk_rules(std::mem::take(&mut r.block.nodes), false, config, src, ctx)?;
                if in_keyframes { out.push(Node::Rule(r)); continue; }
                let synthetic = process_selector(&r, true, config, src, ctx)?;
                let native = process_selector(&r, false, config, src, ctx)?;
                if synthetic != native {
                    let mut clone = r.clone();
                    clone.selector = native;
                    clone.host = HostKind::Native;
                    out.push(Node::Rule(clone));
                    r.host = HostKind::Synthetic;
                }
                r.selector = synthetic;
                out.push(Node::Rule(r));
            }
            Node::AtRule(mut a) => {
                let kf = a.name == "keyframes";
                if let Some(b) = &mut a.block { b.nodes = walk_rules(std::mem::take(&mut b.nodes), kf, config, src, ctx)?; }
                out.push(Node::AtRule(a));
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// postcss Node#error({index, word}) position: first occurrence of `word` in the rule source,
/// else the rule start.
fn rule_error(rule: &Rule, src: &str, reason: String, word: &str) -> StyleError {
    let text = src.get(rule.loc.offset..rule.end_offset).unwrap_or("");
    let (mut line, mut column) = (rule.loc.line, rule.loc.column);
    if let Some(idx) = text.find(word) {
        for ch in text[..idx].chars() { if ch == '\n' { line += 1; column = 1; } else { column += 1; } }
    }
    StyleError::syntax(reason, line, column)
}

const TEMPLATE_DIRECTIVE_PREFIXES: [&str; 4] = ["lwc", "if", "for", "iterator"];
fn is_template_directive(name: &str) -> bool {
    // JS: [/^key$/, /^lwc:*/, /^if:*/, /^for:*/, /^iterator:*/] — `:*` means zero or more colons.
    name == "key" || TEMPLATE_DIRECTIVE_PREFIXES.iter().any(|p| name.starts_with(p))
}

fn process_selector(rule: &Rule, synthetic: bool, config: &Config, src: &str, ctx: &mut Ctx) -> Result<String, StyleError> {
    let mut list = selector::parse(&rule.selector);
    let native_mode = config.disable_synthetic_shadow_support && !config.scoped;
    // no-id-selectors
    let mut errs = Vec::new();
    list.walk(&mut |n| if n.kind == Kind::Id {
        errs.push(rule_error(rule, src, format!("Invalid usage of id selector '#{}'. Try using a class selector or some other selector.", n.value), &n.value));
    });
    // selector-scoping/validate: deprecated + unsupported
    list.walk(&mut |n| {
        let v = n.value.as_str();
        if v.is_empty() { return; }
        if matches!(v, "/deep/" | "::shadow" | ">>>") {
            errs.push(rule_error(rule, src, format!("Invalid usage of deprecated selector \"{v}\"."), v));
        } else if !native_mode && matches!(v, ":root" | ":host-context") {
            errs.push(rule_error(rule, src, format!("Invalid usage of unsupported selector \"{v}\". This selector is only supported in non-scoped CSS where the `disableSyntheticShadowSupport` flag is set to true."), v));
        }
    });
    list.walk(&mut |n| if n.kind == Kind::Attribute && is_template_directive(&n.attribute) {
        let a = &n.attribute;
        errs.push(rule_error(rule, src, format!("Invalid usage of attribute selector \"{a}\". \"{a}\" is a template directive and therefore not supported in css rules."), a));
    });
    for e in errs { ctx.report(e)?; }

    for s in &mut list.selectors { scope_selector(s); }
    if synthetic {
        list.selectors = list.selectors.into_iter().flat_map(transform_host).collect();
    }
    transform_dir(&mut list, rule, src, ctx)?;
    Ok(list.to_string())
}

fn scope_selector(sel: &mut Selector) {
    // Split into compounds (ranges of node indices between combinators).
    let mut compounds: Vec<(usize, usize)> = vec![];
    let mut start = 0;
    for (i, n) in sel.nodes.iter().enumerate() {
        if n.kind == Kind::Combinator { compounds.push((start, i)); start = i + 1; }
    }
    compounds.push((start, sel.nodes.len()));
    // Insert from the back so earlier indices stay valid.
    for &(s, e) in compounds.iter().rev() {
        let c = &sel.nodes[s..e];
        let single_dir = c.len() == 1 && c[0].is_pseudo(":dir");
        let has_host = c.iter().any(|n| n.is_pseudo(":host"));
        if single_dir || has_host { continue; }
        let at = c.iter().rposition(|n| !n.is_pseudo_element()).map(|i| s + i + 1).unwrap_or(s);
        sel.nodes.insert(at, SNode::attr(SHADOW_ATTRIBUTE));
    }
}

fn transform_host(sel: Selector) -> Vec<Selector> {
    let Some(h) = sel.nodes.iter().position(|n| n.is_pseudo(":host")) else { return vec![sel] };
    let mut base = sel;
    let host = std::mem::replace(&mut base.nodes[h], SNode::attr(HOST_ATTRIBUTE));
    // postcss-selector-parser stores a selector's leading whitespace on its first node; JS replaces
    // that node, so the whitespace is dropped (`.a, :host .b` -> `.a,[x-host] .b`).
    if h == 0 { base.before.clear(); }
    let Some(args) = host.args else { return vec![base] };
    if args.selectors.is_empty() { return vec![base]; }
    args.selectors.into_iter().map(|ctx_sel| {
        let mut cloned = base.clone();
        // JS inserts each context node right after the host node, in iteration order.
        for n in ctx_sel.nodes {
            let mut n = n;
            if n.kind == Kind::Combinator { n.raw = n.value.clone(); }
            cloned.nodes.insert(h + 1, n);
        }
        cloned
    }).collect()
}

fn transform_dir(list: &mut SelectorList, rule: &Rule, src: &str, ctx: &mut Ctx) -> Result<(), StyleError> {
    for sel in &mut list.selectors {
        let n0 = sel.nodes.len();
        let mut i = 0;
        while i < n0 && i < sel.nodes.len() {
            if sel.nodes[i].is_pseudo(":dir") {
                let value = sel.nodes[i].args_raw.clone().unwrap_or_default().trim().to_string();
                if value != "ltr" && value != "rtl" {
                    ctx.report(rule_error(rule, src, format!(":dir() pseudo class expects \"ltr\" or \"rtl\" for value, but received \"{value}\"."), ":dir"))?;
                    i += 1;
                    continue;
                }
                let ltr = value == "ltr";
                sel.nodes[i] = SNode::attr(if ltr { DIR_ATTRIBUTE_NATIVE_LTR } else { DIR_ATTRIBUTE_NATIVE_RTL });
                let synth = SNode::attr(if ltr { DIR_ATTRIBUTE_SYNTHETIC_LTR } else { DIR_ATTRIBUTE_SYNTHETIC_RTL });
                let first = &sel.nodes[0];
                if first.kind != Kind::Combinator && first.value != " " {
                    sel.nodes.insert(0, SNode::combinator(" "));
                    sel.nodes.insert(0, synth);
                } else {
                    sel.nodes.insert(i, synth);
                }
            }
            i += 1;
        }
    }
    Ok(())
}

fn strip_comments(v: &str) -> String {
    let mut out = String::new();
    let mut rest = v;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = rest[i + 2..].find("*/").map_or("", |j| &rest[i + 2 + j + 2..]);
    }
    out.push_str(rest);
    out
}

// ---------- css-import/transform ----------
fn node_loc(n: &Node) -> (u32, u32) {
    let l = match n { Node::Rule(r) => &r.loc, Node::AtRule(a) => &a.loc, Node::Decl(d) => &d.loc, Node::Comment(_) => return (1, 1) };
    (l.line, l.column)
}

fn transform_imports(nodes: &mut Vec<Node>, imports: &mut Vec<String>, is_root: bool, scoped: bool, ctx: &mut Ctx) -> Result<(), StyleError> {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::Rule(r) = &mut nodes[i] { transform_imports(&mut r.block.nodes, imports, false, scoped, ctx)?; }
        if let Node::AtRule(a) = &mut nodes[i] {
            if a.name != "import" {
                if let Some(b) = &mut a.block { transform_imports(&mut b.nodes, imports, false, scoped, ctx)?; }
                i += 1;
                continue;
            }
            let (line, col) = (a.loc.line, a.loc.column);
            let check = || -> Result<String, StyleError> {
                if scoped { return Err(StyleError::syntax("Invalid import statement, imports are not allowed in *.scoped.css files.", line, col)); }
                if let Some(prev) = nodes[..i].iter().rev().find(|p| !matches!(p, Node::Comment(_)) && !matches!(p, Node::AtRule(x) if x.name == "import")) {
                    let (l, c) = node_loc(prev);
                    return Err(StyleError::syntax("@import must precede all other statements", l, c));
                }
                let Node::AtRule(a) = &nodes[i] else { unreachable!() };
                let p = a.params.trim();
                let missing = || StyleError::syntax("Invalid import statement, unable to find imported module.", line, col);
                let q = p.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or_else(missing)?;
                let close = p[1..].find(q).map(|j| j + 1).unwrap_or(p.len());
                let value = &p[1..close.max(1).min(p.len())];
                if value.is_empty() { return Err(missing()); }
                if !p[(close + 1).min(p.len())..].trim().is_empty() {
                    return Err(StyleError::syntax("Invalid import statement, import statement only support a single parameter.", line, col));
                }
                Ok(value.to_string())
            };
            match check() {
                Ok(id) => {
                    imports.push(id);
                    let removed = nodes.remove(i);
                    if is_root && i == 0 {
                        if let (Some(next), Node::AtRule(old)) = (nodes.get_mut(0), &removed) { set_before(next, old.before.clone()); }
                    }
                    continue;
                }
                Err(e) => ctx.report(e)?,
            }
        }
        i += 1;
    }
    Ok(())
}

fn set_before(n: &mut Node, b: String) {
    match n { Node::Rule(r) => r.before = b, Node::AtRule(a) => a.before = b, Node::Decl(d) => d.before = b, Node::Comment(c) => c.before = b }
}
