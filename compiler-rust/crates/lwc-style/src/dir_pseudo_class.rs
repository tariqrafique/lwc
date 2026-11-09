//! Port of `@lwc/style-compiler/src/dir-pseudo-class/transform.ts`: `:dir(ltr|rtl)` to the native
//! and synthetic dir attribute placeholders.
use crate::ast::Rule;
use crate::consts::*;
use crate::postcss_lwc_plugin::Ctx;
use crate::selector::{Kind, SNode, SelectorList};
use crate::utils::rule_error;
use crate::StyleError;

pub(crate) fn transform(list: &mut SelectorList, rule: &Rule, src: &str, ctx: &mut Ctx) -> Result<(), StyleError> {
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
