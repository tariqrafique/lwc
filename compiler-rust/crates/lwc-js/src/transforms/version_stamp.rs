//! Runs on printed code after codegen (see lib.rs). Non-empty body: after the last member;
//! empty: inner comment. Both land just before `}`, matching Babel placement.
//! `/*LWC compiler vX*/` right before the closing `}` of every class with a superclass.
use crate::{constants::version_comment, edits::Edit};
use oxc::ast::ast::{Class, Program};
use oxc::ast_visit::{walk, Visit};

struct V<'e>(&'e mut Vec<Edit>);
impl<'a> Visit<'a> for V<'_> {
    fn visit_class(&mut self, it: &Class<'a>) {
        if it.heritage.is_some() {
            self.0.push(Edit::insert(it.body.span.end - 1, format!("{}{}", if it.body.body.is_empty() { "" } else { "\t" }, version_comment())));
        }
        walk::walk_class(self, it);
    }
}

pub fn run(program: &Program<'_>, edits: &mut Vec<Edit>) { V(edits).visit_program(program); }
