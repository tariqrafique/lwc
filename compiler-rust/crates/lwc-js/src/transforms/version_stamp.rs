//! Walks the pre-print Program; inserts are remapped through the codegen map (see
//! [`crate::helpers::post_codegen_edits`]). Non-empty body: after the last member;
//! empty: inner comment. Both land just before `}`, matching Babel placement.
//! [`version_comment`] right before the closing `}` of every class with a superclass.
use lwc_shared::edits::Edit;
use lwc_shared::meta::version_comment;
use oxc::ast::ast::{Class, Program};
use oxc::ast_visit::{Visit, walk};

struct V<'e>(&'e mut Vec<Edit>);
impl<'a> Visit<'a> for V<'_> {
    fn visit_class(&mut self, it: &Class<'a>) {
        if it.heritage.is_some() {
            self.0.push(Edit::insert(
                it.body.span.end - 1,
                format!(
                    "{}{}",
                    if it.body.body.is_empty() { "" } else { "\t" },
                    version_comment()
                ),
            ));
        }
        walk::walk_class(self, it);
    }
}

pub fn run(program: &Program<'_>, edits: &mut Vec<Edit>) {
    V(edits).visit_program(program);
}
