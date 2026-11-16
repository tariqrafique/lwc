//! Ordered LWC JS pipeline (mirrors javascript.ts plugin order):
//!   1. LwcPrivateMethodTransform      (private_methods::forward; lib.rs pass)
//!   2. LwcClassTransform              (`run` below: scoped_css, dynamic_imports, decorators, component;
//!                                      version_stamp runs post-print in lib.rs via the codegen map)
//!   3. class-properties (loose)       (class_properties on the same Program as private fields)
//!   4. LwcReversePrivateMethodTransform (text undo of the forward rename; not applied — `#m` is kept)
//!   5. object-rest-spread             (lower::apply_object_rest_spread on the pre-print AST)
//!   6. LWS: unforgeables and async on that AST (lib.rs, enableLightningWebSecurityTransforms)
use crate::JsError;
use crate::Origin;
use lwc_shared::edits::Edit;
use lwc_shared::{CompileError, CompileOptions};
use oxc::ast::ast::Program;

pub mod class_properties;
pub mod class_self_ref;
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

/// `origin`: the pre-rename source, for Babel-style code frames (see `crate::Origin`).
pub fn run(
    program: &Program<'_>,
    src: &str,
    filename: &str,
    o: &CompileOptions,
    origin: Origin<'_>,
) -> Result<Vec<Edit>, JsError> {
    let mut edits = Vec::new();
    scoped_css::run(program, &mut edits);
    let found = dynamic_imports::run(program, src, filename, o, &mut edits);
    // decorators before component: same-offset inserts keep push order (registerDecorators before registerComponent)
    let dec = decorators::run(program, src, filename, o, origin, &mut edits);
    // Shipped visit order: Program enter (decorator import usage), then pre-order `Class` / `Import` nodes,
    // so errors interleave by source position (stable sort: same-key errors keep push order).
    let (late_import, mut all) = match dec {
        Ok(late) => (late, Vec::new()),
        Err(e) => (None, e),
    };
    all.extend(found.errs);
    all.sort_by_key(|(k, _)| *k);
    // The first throw ends Babel's traversal: any error outside recovery mode, a Babel-internal one in it.
    let stop_at = all
        .iter()
        .find(|(_, e)| !o.experimental_error_recovery_mode || !e.is_unframed_lwc())
        .map(|(k, _)| *k);
    if let Some((k, e)) = dynamic_imports::count(&found.counted, stop_at, o, filename) {
        let at = all.partition_point(|(j, _)| *j <= k);
        all.insert(at, (k, e));
    }
    if !all.is_empty() {
        let mut errs: Vec<CompileError> = all.into_iter().map(|(_, e)| e).collect();
        if !o.experimental_error_recovery_mode {
            return Err(errs.swap_remove(0).into());
        }
        // A Babel-internal throw (class-features `Decorators are not enabled.`, a missing
        // `incrementCounter`) aborts the traversal: shipped surfaces it alone, never inside the recovery
        // aggregate.
        if let Some(i) = errs.iter().position(|e| !e.is_unframed_lwc()) {
            return Err(errs.swap_remove(i).into());
        }
        return Err(JsError::LwcErrors(
            errs.into_iter().flat_map(|e| e.diagnostics).collect(),
        ));
    }
    // version_stamp: runs post-codegen in lib.rs (oxc codegen drops class-body comments)
    component::run(program, src, filename, o, &mut edits);
    if let Some(i) = late_import {
        edits.push(Edit::insert(0, i));
    }
    Ok(edits)
}
