//! Port of `@lwc/style-compiler/src/no-id-selectors/validate.ts`: reject `#id` selectors.
use crate::StyleError;
use crate::ast::Rule;
use crate::postcss_lwc_plugin::Ctx;
use crate::selector::{Kind, SelectorList};
use crate::utils::rule_error;

pub(crate) fn validate(
    list: &SelectorList,
    rule: &Rule,
    src: &str,
    ctx: &mut Ctx,
) -> Result<(), StyleError> {
    let mut errs = Vec::new();
    list.walk(&mut |n| if n.kind == Kind::Id {
        errs.push(rule_error(rule, src, format!("Invalid usage of id selector '#{}'. Try using a class selector or some other selector.", n.value), &n.value));
    });
    for e in errs {
        ctx.report(e)?;
    }
    Ok(())
}
