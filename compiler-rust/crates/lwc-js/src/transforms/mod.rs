//! Ordered LWC JS pipeline (mirrors javascript.ts plugin order):
//!   1. LwcPrivateMethodTransform      (private_methods::forward; lib.rs pass)
//!   2. LwcClassTransform              (`run` below: scoped_css, dynamic_imports, decorators, component;
//!                                      version_stamp runs post-codegen in lib.rs)
//!   3. class-properties (loose)       (class_properties; lib.rs fixpoint pass)
//!   4. LwcReversePrivateMethodTransform (private_methods::reverse; lib.rs pass)
//!   5. object-rest-spread             (lower::object_rest_spread; lib.rs, apiVersion < 60 only)
//!   6. LWS: unforgeables, lower::async_functions (lib.rs, enableLightningWebSecurityTransforms)
use crate::edits::Edit;
use lwc_shared::{CompileError, CompileOptions};
use oxc::ast::ast::Program;

pub mod class_properties;
pub mod component;
pub mod decorators;
pub mod dynamic_imports;
pub mod hoist;
pub mod lower;
pub mod private_fields;
pub mod private_methods;
pub mod scoped_css;
pub mod unforgeables;
pub mod version_stamp;

pub fn run(program: &Program<'_>, src: &str, filename: &str, o: &CompileOptions) -> Result<Vec<Edit>, CompileError> {
    let mut edits = Vec::new();
    scoped_css::run(program, &mut edits);
    let dyn_errs = dynamic_imports::run(program, src, filename, o, &mut edits);
    // decorators before component: same-offset inserts keep push order (registerDecorators before registerComponent)
    let dec = decorators::run(program, src, filename, o, &mut edits);
    if !dyn_errs.is_empty() || dec.is_err() {
        // Shipped visit order: Program enter (decorator import usage), then pre-order `Class` / `Import` nodes,
        // so errors interleave by source position; non-recovery throws the first one.
        let mut all = dec.err().unwrap_or_default();
        all.extend(dyn_errs.into_iter().map(|e| (e.diagnostics.first().and_then(|d| d.location).map_or(0, |l| l.start), e)));
        all.sort_by_key(|(k, _)| *k);
        let mut errs: Vec<CompileError> = all.into_iter().map(|(_, e)| e).collect();
        if !o.experimental_error_recovery_mode { return Err(errs.swap_remove(0)); }
        // A Babel-internal throw (class-features `Decorators are not enabled.`) aborts the traversal: shipped
        // surfaces it alone, never inside the recovery aggregate.
        if let Some(i) = errs.iter().position(|e| !e.is_unframed_lwc()) { return Err(errs.swap_remove(i)); }
        return Err(CompileError::aggregate(errs, filename));
    }
    let late_import = dec.unwrap_or_default();
    // version_stamp: runs post-codegen in lib.rs (oxc codegen drops class-body comments)
    component::run(program, src, filename, o, &mut edits);
    if let Some(i) = late_import { edits.push(Edit::insert(0, i)); }
    Ok(edits)
}
