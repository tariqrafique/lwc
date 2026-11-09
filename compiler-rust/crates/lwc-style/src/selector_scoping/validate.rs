//! Port of `@lwc/style-compiler/src/selector-scoping/validate.ts`: deprecated, unsupported and
//! template-directive attribute selectors.
use crate::ast::Rule;
use crate::postcss_lwc_plugin::Ctx;
use crate::selector::{Kind, SelectorList};
use crate::utils::rule_error;
use crate::StyleError;

const TEMPLATE_DIRECTIVE_PREFIXES: [&str; 4] = ["lwc", "if", "for", "iterator"];
fn is_template_directive(name: &str) -> bool {
    // JS: [/^key$/, /^lwc:*/, /^if:*/, /^for:*/, /^iterator:*/] — `:*` means zero or more colons.
    name == "key" || TEMPLATE_DIRECTIVE_PREFIXES.iter().any(|p| name.starts_with(p))
}

pub(crate) fn validate(list: &SelectorList, rule: &Rule, src: &str, native_mode: bool, ctx: &mut Ctx) -> Result<(), StyleError> {
    let mut errs = Vec::new();
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
    Ok(())
}
