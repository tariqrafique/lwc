//! compile-js/errors.ts `generateError`: the error at a node, through `lwc_shared`'s constructor.
use lwc_shared::{CompileError, LwcErrorInfo};
use oxc::span::Span;

/// `generateError(node, error, ...messageArgs)`: `node`'s span gives the location.
pub(super) fn generate_error(
    src: &str,
    filename: &str,
    node: Span,
    error: &LwcErrorInfo,
    args: &[&str],
) -> CompileError {
    CompileError::generate_at(error, args, filename, src, node.start, node.end)
}
