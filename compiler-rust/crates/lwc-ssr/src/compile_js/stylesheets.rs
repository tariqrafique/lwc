//! compile-js/stylesheets.ts `catalogAndReplaceStyleImports`: a default import of a
//! `*.scoped.css` file gets `?scoped=true`, so the template compiler marks it `$scoped$`.
//! `cssExplicitImports` / `staticStylesheetIds` are not cataloged: upstream only writes them (generate-markup
//! and the rest of compile-js never read them), so they cannot reach the output.
use lwc_shared::edits::Edit;
use oxc::ast::ast::*;

pub(super) fn catalog_and_replace_style_imports(
    d: &ImportDeclaration,
    src: &str,
    edits: &mut Vec<Edit>,
) {
    let Some(source) = d.source.value.as_str() else {
        return;
    };
    let single_default = matches!(
        d.specifiers.as_ref().map(|v| (v.len(), v.first())),
        Some((
            1,
            Some(ImportDeclarationSpecifier::ImportDefaultSpecifier(_))
        ))
    );
    if !source.ends_with(".css") || !single_default || !source.ends_with(".scoped.css") {
        return;
    }
    let s = d.source.span.start as usize;
    let q = &src[s..s + 1];
    edits.push(Edit::replace(
        d.source.span.start,
        d.source.span.end,
        format!("{q}{source}?scoped=true{q}"),
    ));
}
