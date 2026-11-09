//! Port of `@lwc/style-compiler/src/postcss-lwc-plugin.ts`: plugin orchestration (imports, at-rules,
//! then per rule the synthetic and native selector processors, duplicating `:host` rules into a
//! Native/Synthetic pair). `Ctx` is the recovery context from `utils/error-recovery.ts`.
use crate::ast::*;
use crate::selector;
use crate::{css_import, dir_pseudo_class, no_id_selectors, scope_at_rules, selector_scoping};
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
    css_import::transform(&mut root.nodes, &mut root.imports, true, config.scoped, &mut ctx)?;
    scope_at_rules::transform(&mut root.nodes);
    root.nodes = walk_rules(std::mem::take(&mut root.nodes), false, config, src, &mut ctx)?;
    if !ctx.errors.is_empty() { return Err(StyleError::Aggregate(ctx.errors)); }
    Ok(())
}

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

/// selectorProcessorFactory: no-id-selectors, selector-scoping (validate + transform), dir-pseudo-class.
fn process_selector(rule: &Rule, synthetic: bool, config: &Config, src: &str, ctx: &mut Ctx) -> Result<String, StyleError> {
    let mut list = selector::parse(&rule.selector);
    no_id_selectors::validate(&list, rule, src, ctx)?;
    selector_scoping::transform::transform(&mut list, rule, synthetic, config, src, ctx)?;
    dir_pseudo_class::transform(&mut list, rule, src, ctx)?;
    Ok(list.to_string())
}
