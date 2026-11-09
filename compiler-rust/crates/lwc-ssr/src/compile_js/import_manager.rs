//! ssr-compiler estree/builders.ts `bImportDeclaration` and compile-js/imports.ts `ImportManager`,
//! printed as source text: the imports compile-js prepends (generate-markup's `setStaticInternals`
//! and `__lwcTmpl`, index.ts's `registerPublicProperties`, and the ImportExpression visitor's loader).
use lwc_shared::js_literal::json_string;

/// One `bImportDeclaration` specifier: `default` local, or `imported as local`.
pub(super) enum Spec<'a> {
    Default(&'a str),
    Named(&'a str, &'a str),
}

pub(super) fn import_declaration(specs: &[Spec], source: &str) -> String {
    let mut parts = Vec::new();
    let mut named = Vec::new();
    for s in specs {
        match s {
            Spec::Default(local) => parts.push(local.to_string()),
            Spec::Named(imported, local) => named.push(format!("{imported} as {local}")),
        }
    }
    if !named.is_empty() { parts.push(format!("{{ {} }}", named.join(", "))); }
    format!("import {} from {};\n", parts.join(", "), json_string(source.encode_utf16()))
}

/// imports.ts `ImportManager`: `Map<source, Map<imported, local>>`, both in first-insertion order,
/// a later `add` of the same imported name replacing its local (`Map.set`).
#[derive(Debug, Default, PartialEq)]
pub(super) struct ImportManager {
    sources: Vec<(String, Vec<(String, String)>)>,
}

impl ImportManager {
    /// `add({ [imported]: local }, source)`.
    pub(super) fn add(&mut self, imported: &str, local: &str, source: &str) {
        let at = match self.sources.iter().position(|(s, _)| s == source) {
            Some(at) => at,
            None => {
                self.sources.push((source.into(), Vec::new()));
                self.sources.len() - 1
            }
        };
        let specs = &mut self.sources[at].1;
        match specs.iter_mut().position(|(i, _)| i == imported) {
            Some(i) => specs[i].1 = local.into(),
            None => specs.push((imported.into(), local.into())),
        }
    }

    /// `getImportDeclarations()`, one declaration per source.
    pub(super) fn declarations(&self) -> String {
        self.sources
            .iter()
            .map(|(source, specs)| {
                let specs: Vec<Spec> = specs.iter().map(|(i, l)| Spec::Named(i, l)).collect();
                import_declaration(&specs, source)
            })
            .collect()
    }
}
