//! compile-js/lwc-import.ts: `lwc` becomes `@lwc/ssr-runtime` on import, export-named and
//! export-all declarations; the `LightningElement` import's local name goes on the state.
use lwc_shared::edits::Edit;
use lwc_shared::js::string_literal;
use oxc::ast::ast::*;

use super::ComponentMetaState;
use crate::runtime_abi::RUNTIME_MODULE;

/// `replaceLwcImport`. Returns whether the declaration's source was `lwc`.
pub(super) fn replace_lwc_import(
    d: &ImportDeclaration,
    src: &str,
    state: &mut ComponentMetaState,
    edits: &mut Vec<Edit>,
) -> bool {
    if d.source.value != "lwc" {
        return false;
    }
    let le = d.specifiers.iter().flatten().find_map(|s| match s {
        ImportDeclarationSpecifier::ImportSpecifier(x)
            if x.imported.name() == "LightningElement" =>
        {
            Some(x.local.name.to_string())
        }
        _ => None,
    });
    if le.is_some() {
        state.lightning_element_identifier = le;
    }
    edits.push(runtime_source(&d.source, src));
    true
}

/// `replaceNamedLwcExport` (`export { x } from 'lwc'`) / `replaceAllLwcExport`.
pub(super) fn replace_lwc_export(source: &StringLiteral, src: &str, edits: &mut Vec<Edit>) {
    if source.value == "lwc" {
        edits.push(runtime_source(source, src));
    }
}

/// The source literal rewritten to `@lwc/ssr-runtime`.
///
/// Upstream `replaceLwcImport` / `replaceNamedLwcExport` / `replaceAllLwcExport` replace the
/// source with estree-toolkit `b.literal('@lwc/ssr-runtime')` (`lwc-import.ts`), which has no
/// `raw`. astring then prints `JSON.stringify` (`string_literal`), not the quote the source used.
fn runtime_source(lit: &StringLiteral, _src: &str) -> Edit {
    Edit::replace(
        lit.span.start,
        lit.span.end,
        string_literal(RUNTIME_MODULE.encode_utf16()),
    )
}
