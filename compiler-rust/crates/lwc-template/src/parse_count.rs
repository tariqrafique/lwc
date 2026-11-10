//! Crate-local count of oxc `Parser::new` calls. The static hoist must not add one: it walks the
//! AST built while emitting, so a template whose only object/array literals come from codegen
//! parses zero times.
use std::cell::Cell;

thread_local! {
    static PARSE_COUNT: Cell<u32> = const { Cell::new(0) };
}

/// Test hook: oxc `Parser::new` invocations in this crate since the last [`reset_parse_count`].
pub fn reset_parse_count() {
    PARSE_COUNT.set(0);
}

/// See [`reset_parse_count`].
pub fn parse_count() -> u32 {
    PARSE_COUNT.get()
}

pub(crate) fn bump() {
    PARSE_COUNT.with(|n| n.set(n.get() + 1));
}
