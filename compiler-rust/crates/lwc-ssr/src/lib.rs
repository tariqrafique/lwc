use lwc_shared::{CompileError, CompileOptions, CompileResult, DiagnosticLevel};
pub mod compile_js;
pub mod compile_template;
pub mod runtime_abi;

#[derive(Debug, Clone, Default)]
pub struct TemplateTransformOptions {
    pub name: String,
    pub namespace: String,
    pub preserve_comments: bool,
    pub filename: String,
    /// `CompilationMode` (compile-template/index.ts `compilationMode`).
    pub mode: lwc_shared::SsrMode,
}

/// SSR template module emit from IR, in `options.mode` (sync | async | asyncYield).
pub fn compile_template_for_ssr(
    root: &lwc_shared::template_ir::Root,
    options: &TemplateTransformOptions,
) -> Result<CompileResult, CompileError> {
    Ok(CompileResult {
        code: compile_template::compile_module(
            root,
            compile_template::TemplateOptions {
                preserve_comments: options.preserve_comments,
                mode: options.mode,
            },
            &compile_template::ModuleMeta {
                filename: options.filename.clone(),
                namespace: options.namespace.clone(),
                name: options.name.clone(),
            },
        ),
        map: None,
        warnings: vec![],
        css_scope_tokens: None,
    })
}

/// compileComponentForSSR (honors `options.ssr_mode`).
pub fn compile_component_for_ssr(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, CompileError> {
    compile_js::compile_component_for_ssr(src, filename, options)
}

/// Facade entry: `@lwc/compiler` `targetSSR` path.
/// - `.html` → parse with `lwc-template` then `compile_template_for_ssr`
/// - `.js`/`.ts` → `compile_component_for_ssr`
pub fn transform(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, CompileError> {
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "html" => transform_html(src, filename, options),
        "js" | "ts" | "mjs" | "jsx" | "tsx" | "mts" => {
            compile_component_for_ssr(src, filename, options)
        }
        _ => Err(CompileError::not_implemented("lwc-ssr", filename)),
    }
}

fn transform_html(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, CompileError> {
    let config = lwc_template::Config {
        name: Some(options.name.clone()).filter(|s| !s.is_empty()),
        namespace: Some(options.namespace.clone()).filter(|s| !s.is_empty()),
        api_version: options.api_version,
        disable_synthetic_shadow_support: Some(options.disable_synthetic_shadow_support),
        experimental_complex_expressions: options.experimental_complex_expressions,
        preserve_html_comments: options.preserve_html_comments,
        enable_static_content_optimization: options.enable_static_content_optimization,
        enable_dynamic_components: options.enable_dynamic_components,
        enable_lwc_on: options.enable_lwc_on,
        // ssr-compiler compileTemplate passes the raw option (no `?? Boolean(dynamicImports)`): omitted → false.
        experimental_dynamic_directive: options.experimental_dynamic_directive,
        // ssr-compiler compileTemplate forwards customRendererConfig to template-compiler parse().
        custom_renderer_config: options.custom_renderer_config.clone(),
        instrumentation: options.instrumentation.clone(),
        // SSR path: template parser must apply SSR-only rejects (inner-h-t-m-l, etc.).
        ssr: Some(true),
        ..lwc_template::Config::default()
    };

    // Template handoff (a): the Rust parser emits no RootDirective::PreserveComments for
    // `<template lwc:preserve-comments>` and drops comments unless the config flag is set. JS's
    // ssr-compiler reads preserveComments only from that root directive, so detect it here and
    // reparse with comments kept.
    // SHIM(template-gap:preserve-comments-directive): comments-*, adjacent-text-nodes: no RootDirective::PreserveComments from the parser
    let root_preserves = root_has_preserve_comments(src);
    // ssr-compiler expression.ts expressionIrToEs: complex binding only when
    // `experimentalComplexExpressions && isAPIFeatureEnabled(ENABLE_COMPLEX_TEMPLATE_EXPRESSIONS, apiVersion)`.
    let api_version = lwc_shared::api_version::get_api_version_from_number(options.api_version);
    let complex_enabled = options.experimental_complex_expressions == Some(true)
        && lwc_shared::api_version::is_api_feature_enabled(lwc_shared::api_version::ApiFeature::EnableComplexTemplateExpressions, api_version);
    // SHIM(template-gap:complex-for-each): for-each-block/complex: mask complex for:each values before parse
    let (masked, for_each_pairs) = if complex_enabled { // SHIM(template-gap:complex-for-each)
        compile_template::ir_compat::mask_complex_for_each(src) // SHIM(template-gap:complex-for-each)
    } else {
        (src.to_string(), Vec::new())
    };
    let src = masked.as_str(); // SHIM(template-gap:complex-for-each)
    // SHIM(template-gap:preserve-comments-directive): reparse with comments kept: parser drops comments unless the config flag is set
    let config = if root_preserves { lwc_template::Config { preserve_html_comments: Some(true), ..config } } else { config };
    let parsing = lwc_template::parse(src, config).map_err(|e| CompileError {
        message: e.message,
        filename: Some(filename.into()),
        diagnostics: vec![],
        name: None,
        line: None,
        column: None,
    })?;

    let first_err = parsing
        .warnings
        .iter()
        // TS SSR ignores parse diagnostics (bails only on missing root); the one exception is the
        // SSR-only inner/outer-h-t-m-l reject, which TS throws from element.ts as an uncoded Error.
        .find(|w| matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal) && w.code.is_none())
        .cloned();
    if let Some(d) = first_err {
        let errors: Vec<_> = parsing
            .warnings
            .iter()
            .filter(|w| matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal) && w.code.is_none())
            .cloned()
            .collect();
        return Err(CompileError {
            message: d.message.clone(),
            filename: Some(filename.into()),
            diagnostics: errors,
            name: None,
            line: d.location.map(|l| l.start_line),
            column: d.location.map(|l| l.start_column),
        });
    }

    // template-compiler parser/index.ts parse(): any Error-level diagnostic from parseHTML (parse5 errors →
    // INVALID_HTML_SYNTAX; TREAT_ALL_PARSE5_ERRORS_AS_ERRORS widens the set from API 59) returns no root, and
    // ssr-compiler compile-template/index.ts then throws on `!root`.
    // SHIM(template-gap:parse5-error-root): parse5/unclosed-div: parser returns a root despite parse5 errors
    let parse5_failed = parsing.warnings.iter().any(|w| w.code == Some(1058) && matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal)); // SHIM(template-gap:parse5-error-root)
    let Some(root) = parsing.root.as_ref().filter(|_| !parse5_failed) else { // SHIM(template-gap:parse5-error-root)
        return Err(CompileError {
            message: "Template compilation failure; see warnings in the console.".into(),
            filename: Some(filename.into()),
            diagnostics: parsing.warnings,
            name: None,
            line: None,
            column: None,
        });
    };

    // index.ts: `preserveComments: !!root.directives.find(PreserveComments)?.value` (not the option).
    // SHIM(template-gap:preserve-comments-directive): source-detected directive stands in for root.directives PreserveComments
    let preserve_comments = root_preserves;
    // SHIM(template-gap:ir-compat-walk): mutable IR copy for the post-parse shims below
    let mut root = root.clone();
    // SHIM(template-gap:valueless-attr-bool): global-html-attributes, known-boolean-attributes: valueless attrs -> Bool(true)
    compile_template::ir_compat::mark_valueless_attrs(&mut root, src); // SHIM(template-gap:valueless-attr-bool)
    // SHIM(template-gap:complex-expr-literal): attribute-dynamic-complex, text-interpolation-complex, scoped-slots/complex
    if complex_enabled { compile_template::ir_compat::lift_quoted_complex(&mut root); } // SHIM(template-gap:complex-expr-literal)
    // SHIM(template-gap:expr-error-recovery): *-complex fixtures below API 66: drop nodes whose expression errored (JS withErrorRecovery)
    if !complex_enabled { compile_template::ir_compat::drop_errored_expr_nodes(&mut root); } // SHIM(template-gap:expr-error-recovery)
    // SHIM(template-gap:complex-for-each): for-each-block/complex: restore masked for:each expression
    compile_template::ir_compat::unmask_complex_for_each(&mut root, &for_each_pairs); // SHIM(template-gap:complex-for-each)
    let mut result = compile_template_for_ssr(
        &root,
        &TemplateTransformOptions {
            name: options.name.clone(),
            namespace: options.namespace.clone(),
            preserve_comments,
            filename: filename.into(),
            mode: options.ssr_mode,
        },
    )?;

    let tokens = lwc_template::generate_scope_tokens(
        filename,
        Some(options.namespace.as_str()).filter(|s| !s.is_empty()),
        Some(options.name.as_str()).filter(|s| !s.is_empty()),
    );
    // SSR behavioral goldens expect `warnings: []` (JS oracle). Keep errors as hard fails above.
    let _template_warnings = parsing.warnings;
    result.warnings = vec![];
    result.css_scope_tokens = Some(tokens.css_scope_tokens);
    Ok(result)
}

/// Does the root `<template ...>` start tag carry `lwc:preserve-comments`?
// SHIM(template-gap:preserve-comments-directive): source scan for <template lwc:preserve-comments>
fn root_has_preserve_comments(src: &str) -> bool {
    let mut rest = src;
    loop {
        let t = rest.trim_start();
        if let Some(after) = t.strip_prefix("<!--") {
            match after.find("-->") { Some(e) => { rest = &after[e + 3..]; continue; } None => return false }
        }
        if !t.starts_with("<template") { return false; }
        let end = t.find('>').unwrap_or(t.len());
        return t[..end].split(|c: char| c.is_whitespace() || c == '/').any(|a| a == "lwc:preserve-comments" || a.starts_with("lwc:preserve-comments="));
    }
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    #[test]
    fn transform_html_text_static() {
        let opts = CompileOptions {
            name: "text-static".into(),
            namespace: "x".into(),
            target_ssr: true,
            ..Default::default()
        };
        let src = "<template>\n    Static text\n</template>\n";
        let r = transform(src, "x/text-static/text-static.html", &opts).expect("transform");
        assert!(r.code.contains("function __lwcTmpl"), "{}", r.code);
        assert!(r.code.contains("Static text"), "{}", r.code);
        assert!(r.code.contains("__lwcYield"), "{}", r.code);
        assert!(r.code.contains("__lwcRegisterTemplate"), "{}", r.code);
        assert!(r.code.contains("stylesheetScopeToken"), "{}", r.code);
        assert!(!r.code.contains("yield ") && !r.code.contains("async function"), "{}", r.code);
        assert!(r.css_scope_tokens.is_some());
    }

    #[test]
    fn transform_js_text_static() {
        let opts = CompileOptions {
            name: "text-static".into(),
            namespace: "x".into(),
            target_ssr: true,
            ..Default::default()
        };
        let src = "import { LightningElement } from 'lwc';\n\nexport default class TextStatic extends LightningElement {}\n";
        let r = transform(src, "x/text-static/text-static.js", &opts).expect("transform");
        assert!(r.code.contains("@lwc/ssr-runtime"), "{}", r.code);
        assert!(r.code.contains("__setStaticInternals(TextStatic, 'x-text-static'"), "{}", r.code);
        assert!(r.code.contains("./text-static.html"), "{}", r.code);
    }
}
