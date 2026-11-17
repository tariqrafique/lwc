//! Port of `@lwc/style-compiler/src/selector-scoping/transform.ts`: validate, insert the shadow
//! attribute into each compound, and (synthetic processor only) rewrite `:host` / `:host(...)`.
use crate::ast::Rule;
use crate::consts::*;
use crate::postcss_lwc_plugin::Ctx;
use crate::selector::{Kind, SNode, Selector, SelectorList};
use crate::{Config, StyleError};

pub(crate) fn transform(
    list: &mut SelectorList,
    rule: &Rule,
    synthetic: bool,
    config: &Config,
    src: &str,
    ctx: &mut Ctx,
) -> Result<(), StyleError> {
    let native_mode = config.disable_synthetic_shadow_support && !config.scoped;
    super::validate::validate(list, rule, src, native_mode, ctx)?;
    for s in &mut list.selectors {
        scope_selector(s);
    }
    if synthetic {
        list.selectors = std::mem::take(&mut list.selectors)
            .into_iter()
            .flat_map(transform_host)
            .collect();
    }
    Ok(())
}

fn scope_selector(sel: &mut Selector) {
    // Split into compounds (ranges of node indices between combinators).
    let mut compounds: Vec<(usize, usize)> = vec![];
    let mut start = 0;
    for (i, n) in sel.nodes.iter().enumerate() {
        if n.kind == Kind::Combinator {
            compounds.push((start, i));
            start = i + 1;
        }
    }
    compounds.push((start, sel.nodes.len()));
    // Insert from the back so earlier indices stay valid.
    for &(s, e) in compounds.iter().rev() {
        let c = &sel.nodes[s..e];
        let single_dir = c.len() == 1 && c[0].is_pseudo(":dir");
        let has_host = c.iter().any(|n| n.is_pseudo(":host"));
        if single_dir || has_host {
            continue;
        }
        let at = c
            .iter()
            .rposition(|n| !n.is_pseudo_element())
            .map(|i| s + i + 1)
            .unwrap_or(s);
        sel.nodes.insert(at, SNode::attr(SHADOW_ATTRIBUTE));
    }
}

fn transform_host(sel: Selector) -> Vec<Selector> {
    let Some(h) = sel.nodes.iter().position(|n| n.is_pseudo(":host")) else {
        return vec![sel];
    };
    let mut base = sel;
    let host = std::mem::replace(&mut base.nodes[h], SNode::attr(HOST_ATTRIBUTE));
    // postcss-selector-parser stores a selector's leading whitespace on its first node; JS replaces
    // that node, so the whitespace is dropped (`.a, :host .b` -> `.a,[x-host] .b`).
    if h == 0 {
        base.before.clear();
    }
    let Some(args) = host.args else {
        return vec![base];
    };
    if args.selectors.is_empty() {
        return vec![base];
    }
    args.selectors
        .into_iter()
        .map(|ctx_sel| {
            let mut cloned = base.clone();
            // JS inserts each context node right after the host node, in iteration order.
            for n in ctx_sel.nodes {
                let mut n = n;
                if n.kind == Kind::Combinator {
                    n.raw = n.value.clone();
                }
                cloned.nodes.insert(h + 1, n);
            }
            cloned
        })
        .collect()
}
