//! compile-js/remove-decorator-import.ts: `api`, `wire` and `track` specifiers leave the
//! `@lwc/ssr-runtime` import (decorators are compiled away).
use lwc_shared::edits::Edit;
use lwc_shared::js_literal::json_string;
use oxc::ast::ast::*;

use crate::runtime_abi::RUNTIME_MODULE;

const DECORATORS: [&str; 3] = ["api", "wire", "track"];

/// `removeDecoratorImport`. `from_lwc`: the source was `lwc` and has already been replaced with
/// `@lwc/ssr-runtime` (this edit spans the whole declaration, so it supersedes that one).
pub(super) fn remove_decorator_import(d: &ImportDeclaration, from_lwc: bool, src: &str, edits: &mut Vec<Edit>) {
    if !from_lwc && d.source.value != RUNTIME_MODULE { return; }
    let Some(specifiers) = &d.specifiers else { return };
    let is_decorator = |s: &ImportDeclarationSpecifier| {
        matches!(s, ImportDeclarationSpecifier::ImportSpecifier(x) if DECORATORS.contains(&x.imported.name().as_str()))
    };
    if !specifiers.iter().any(is_decorator) { return; }
    let (mut leading, mut named) = (Vec::new(), Vec::new());
    for s in specifiers.iter().filter(|s| !is_decorator(s)) {
        match s {
            ImportDeclarationSpecifier::ImportSpecifier(x) => named.push(&src[x.span.start as usize..x.span.end as usize]),
            ImportDeclarationSpecifier::ImportDefaultSpecifier(x) => leading.push(x.local.name.to_string()),
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(x) => leading.push(format!("* as {}", x.local.name)),
        }
    }
    if !named.is_empty() { leading.push(format!("{{ {} }}", named.join(", "))); }
    let source = json_string(RUNTIME_MODULE.encode_utf16());
    // No specifiers left: astring prints the declaration as a bare `import "src";`.
    let text = if leading.is_empty() { format!("import {source};") } else { format!("import {} from {source};", leading.join(", ")) };
    edits.push(Edit::replace(d.span.start, d.span.end, text));
}
