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
    let mut dyn_errs = dynamic_imports::run(program, src, filename, o, &mut edits);
    if !dyn_errs.is_empty() {
        if !o.experimental_error_recovery_mode { return Err(dyn_errs.swap_remove(0)); }
        if let Err(e) = decorators::run(program, src, filename, o, &mut Vec::new()) { dyn_errs.push(e); }
        return Err(CompileError::aggregate(dyn_errs, filename));
    }
    // decorators before component: same-offset inserts keep push order (registerDecorators before registerComponent)
    let late_import = decorators::run(program, src, filename, o, &mut edits)?;
    // version_stamp: runs post-codegen in lib.rs (oxc codegen drops class-body comments)
    component::run(program, src, filename, o, &mut edits);
    if let Some(i) = late_import { edits.push(Edit::insert(0, i)); }
    Ok(edits)
}
