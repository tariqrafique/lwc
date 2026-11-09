//! Push-only diagnostics sink (TS `ParserCtx.addDiagnostic`).
//!
//! Parser code can only append: the field is private to this module, so nothing in lowering can
//! index, count or scan what was reported to decide what to do next. Whether a parse threw is a
//! return value (`Thrown`), not a property of the diagnostics list. The list is drained once, at
//! the `parse_template` boundary.

use lwc_shared::CompilerDiagnostic;

pub(crate) struct Diags(Vec<CompilerDiagnostic>);

impl Diags {
    pub(super) fn new(seed: Vec<CompilerDiagnostic>) -> Self { Diags(seed) }
    pub(super) fn push(&mut self, d: CompilerDiagnostic) { self.0.push(d); }
    pub(super) fn into_vec(self) -> Vec<CompilerDiagnostic> { self.0 }
}
