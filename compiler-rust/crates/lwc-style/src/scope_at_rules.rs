//! Port of `@lwc/style-compiler/src/scope-at-rules/transform.ts`: suffix `@keyframes` names with the
//! shadow attribute and rewrite matching `animation` / `animation-name` values.
use crate::ast::*;
use crate::consts::*;
use std::collections::HashSet;

const VENDOR_PREFIXES: [&str; 4] = ["moz", "ms", "o", "webkit"];
fn has_name(prop: &str, name: &str) -> bool {
    prop == name
        || VENDOR_PREFIXES
            .iter()
            .any(|p| prop == format!("-{p}-{name}"))
}

pub(crate) fn transform(nodes: &mut [Node]) {
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
                if let Some(b) = &mut a.block {
                    collect_keyframes(&mut b.nodes, known);
                }
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
                    // Always reassigned, as in JS; serialize keeps the raw source while it still matches.
                    d.value = d
                        .value
                        .split_whitespace()
                        .map(|t| {
                            if known.contains(t) {
                                format!("{t}-{SHADOW_ATTRIBUTE}")
                            } else {
                                t.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                } else if has_name(&d.prop, "animation-name") && known.contains(&d.value) {
                    d.value = format!("{}-{SHADOW_ATTRIBUTE}", d.value);
                }
            }
            Node::AtRule(a) => {
                if let Some(b) = &mut a.block {
                    rewrite_animation(&mut b.nodes, known, in_rule)
                }
            }
            Node::Rule(r) => rewrite_animation(&mut r.block.nodes, known, true),
            _ => {}
        }
    }
}
