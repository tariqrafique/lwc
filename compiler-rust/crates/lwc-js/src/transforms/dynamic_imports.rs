//! Dynamic imports (port of babel-plugin-component/src/dynamic-imports.ts):
//! strictSpecifier -> LWC1121 for non-string-literal sources; loader -> `import(x)` becomes `_load(x)`
//! with `import { load as _load } from "<loader>"`.
use crate::edits::Edit;
use crate::transforms::component::unique;
use crate::transforms::scoped_css::serde_quote;
use lwc_shared::{CompileError, CompileOptions};
use oxc::ast::ast::{Expression, ImportExpression, Program};
use oxc::ast_visit::{walk, Visit};
use oxc::span::GetSpan;

struct V<'s> { instrumentation: Option<&'s lwc_shared::InstrumentationHandle>, src: &'s str, filename: &'s str, strict: bool, load: Option<String>, edits: Vec<Edit>, errs: Vec<CompileError> }
impl<'a> Visit<'a> for V<'_> {
    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if self.strict && !matches!(it.source, Expression::StringLiteral(_)) {
            let sp = it.source.span();
            let text = &self.src[sp.start as usize..sp.end as usize];
            self.errs.push(CompileError::lwc_at(1121,
                lwc_shared::format_message("Invalid import. The argument \"{0}\" must be a stringLiteral for dynamic imports when strict mode is enabled.", &[text]),
                self.filename, self.src, sp.start, sp.end));
        }
        if let Some(l) = &self.load {
            self.edits.push(Edit::replace(it.span.start, it.span.start + 6, l.clone()));
            if let Some(i) = self.instrumentation { i.increment_counter(lwc_shared::CompilerMetric::DynamicImportTransform); }
        }
        walk::walk_import_expression(self, it);
    }
}

pub fn run(program: &Program<'_>, src: &str, filename: &str, o: &CompileOptions, edits: &mut Vec<Edit>) -> Vec<CompileError> {
    let loader = &o.dynamic_import_loader;
    let load = (!loader.is_empty()).then(|| unique(src, "load"));
    let mut v = V { instrumentation: o.instrumentation.as_ref(), src, filename, strict: o.dynamic_import_strict_specifier, load: load.clone(), edits: Vec::new(), errs: Vec::new() };
    v.visit_program(program);
    if let Some(l) = load {
        if !v.edits.is_empty() { edits.push(Edit::insert(0, format!("import {{ load as {l} }} from {};\n", serde_quote(loader)))); }
    }
    edits.append(&mut v.edits);
    v.errs
}
