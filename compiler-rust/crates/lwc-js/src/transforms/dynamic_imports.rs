//! Dynamic imports (port of babel-plugin-component/src/dynamic-imports.ts):
//! strictSpecifier -> LWC1121 for non-string-literal sources; loader -> `import(x)` becomes `_load(x)`
//! with `import { load as _load } from "<loader>"`.
use crate::transforms::component::unique;
use lwc_shared::edits::Edit;
use lwc_shared::errors::error_info::lwc_class::LWCClassErrors;
use lwc_shared::{CompileError, CompileOptions};
use oxc::ast::ast::{Expression, ImportExpression, Program};
use oxc::ast_visit::{Visit, walk};
use oxc::span::GetSpan;

struct V<'s> {
    src: &'s str,
    filename: &'s str,
    strict: bool,
    load: Option<String>,
    edits: Vec<Edit>,
    /// (traversal sort key, error): the key is the node offset shipped's pre-order visit reaches it at.
    errs: Vec<(u32, CompileError)>,
    /// Sort keys of the `incrementMetricCounter(DynamicImportTransform)` calls (dynamic-imports.ts:72), in
    /// visit order. Applied by [`count`] once every error is known: a thrown error ends Babel's traversal, so
    /// imports past it are never counted.
    counted: Vec<u32>,
}
impl<'a> Visit<'a> for V<'_> {
    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        let key = it.source.span().start;
        if self.strict && !matches!(it.source, Expression::StringLiteral(_)) {
            let sp = it.source.span();
            let text = &self.src[sp.start as usize..sp.end as usize];
            self.errs.push((
                key,
                CompileError::generate_at(
                    &LWCClassErrors::INVALID_DYNAMIC_IMPORT_SOURCE_STRICT,
                    &[text],
                    self.filename,
                    self.src,
                    sp.start,
                    sp.end,
                ),
            ));
        }
        if let Some(l) = &self.load {
            self.edits
                .push(Edit::replace(it.span.start, it.span.start + 6, l.clone()));
            self.counted.push(key);
        }
        walk::walk_import_expression(self, it);
    }
}

/// What [`run`] found: the strict-specifier errors and the pending metric counts.
pub struct Found {
    pub errs: Vec<(u32, CompileError)>,
    pub counted: Vec<u32>,
}

/// Fires the counts Babel reaches before `stop_at` (the sort key of the throw that ends traversal; a count
/// at the same key comes after it, since validateImport runs first). A missing `incrementCounter`
/// (utils.ts:155) is itself a Babel-internal TypeError, wrapped as LWC1007 by @lwc/compiler: returned
/// with its key, and no later import is counted.
pub fn count(
    counted: &[u32],
    stop_at: Option<u32>,
    o: &CompileOptions,
    filename: &str,
) -> Option<(u32, CompileError)> {
    let i = o.instrumentation.as_ref()?;
    for &k in counted {
        if stop_at.is_some_and(|s| k >= s) {
            return None;
        }
        if i.increment_counter(lwc_shared::CompilerMetric::DynamicImportTransform)
            .is_err()
        {
            return Some((
                k,
                CompileError::babel(
                    "TypeError",
                    "state.opts.instrumentation?.incrementCounter is not a function",
                    filename,
                    None,
                    None,
                ),
            ));
        }
    }
    None
}

pub fn run(
    program: &Program<'_>,
    src: &str,
    filename: &str,
    o: &CompileOptions,
    edits: &mut Vec<Edit>,
) -> Found {
    let loader = &o.dynamic_import_loader;
    let load = (!loader.is_empty()).then(|| unique(src, "load"));
    let mut v = V {
        src,
        filename,
        strict: o.dynamic_import_strict_specifier,
        load: load.clone(),
        edits: Vec::new(),
        errs: Vec::new(),
        counted: Vec::new(),
    };
    v.visit_program(program);
    if let Some(l) = load {
        if !v.edits.is_empty() {
            edits.push(Edit::insert(
                0,
                format!(
                    "import {{ load as {l} }} from {};\n",
                    lwc_shared::js::string_literal(loader.encode_utf16())
                ),
            ));
        }
    }
    edits.append(&mut v.edits);
    Found {
        errs: v.errs,
        counted: v.counted,
    }
}
