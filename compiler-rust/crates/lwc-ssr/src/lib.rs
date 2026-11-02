use lwc_shared::{CompileError, CompileOptions, CompileResult};
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
/// `Err` = a transformer's compile-time throw (e.g. element.ts:234-235), surfaced as-is.
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
        )?,
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
        ..lwc_template::Config::default()
    };

    // Complex-expression gating (experimentalComplexExpressions + apiVersion ENABLE_COMPLEX_TEMPLATE_EXPRESSIONS)
    // and lwc:preserve-comments both come from lwc-template via `config` / Root.directives.
    let parsing = lwc_template::parse(src, config).map_err(|e| CompileError {
        message: e.message,
        filename: Some(filename.into()),
        diagnostics: vec![],
        name: None,
        line: None,
        column: None,
    })?;

    // template-compiler parser/index.ts parse(): any Error-level diagnostic from parseHTML (parse5 errors →
    // INVALID_HTML_SYNTAX; TREAT_ALL_PARSE5_ERRORS_AS_ERRORS widens the set from API 59) returns no root, and
    // ssr-compiler compile-template/index.ts then throws on `!root`.
    let Some(root) = parsing.root.as_ref() else {
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
    let preserve_comments = root.directives.iter().any(|d| matches!(d, lwc_template::RootDirective::PreserveComments(true)));
    let mut result = compile_template_for_ssr(
        root,
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
    // Parse warnings are not returned, matching upstream: ssr-compiler compile-template/index.ts:101-105
    // only `console.error`s each warning (throwing just on `!root`, :110-112), and compileTemplateForSSR
    // returns `{ code, map: undefined }` (ssr-compiler index.ts:35-36). The console.error side effect is
    // not reproduced (stderr only; no value reaches the caller).
    let _template_warnings = parsing.warnings;
    result.warnings = vec![];
    result.css_scope_tokens = Some(tokens.css_scope_tokens);
    Ok(result)
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

    // Regression: attribute-global-html/tabindex. Invalid static tabindex values ("true", "false",
    // "truthy") make lwc-template report LWC1061 as an Error, but JS still renders those <x-child>
    // nodes (tabIndex prop normalized to -1), so SSR must keep all 13 children.
    #[test]
    fn transform_html_tabindex_invalid_static_values_kept() {
        let opts = CompileOptions { name: "component".into(), namespace: "x".into(), target_ssr: true, ..Default::default() };
        let mut src = String::from("<template>\n");
        for v in ["{undefined}", "{aNull}", "{string}", "{number}", "{zero}", "{minusOne}", "{truthy}", "{falsy}"] {
            src.push_str(&format!("    <x-child tabindex={v}></x-child>\n"));
        }
        for v in ["true", "false", "truthy", "0", "-1"] {
            src.push_str(&format!("    <x-child tabindex=\"{v}\"></x-child>\n"));
        }
        src.push_str("</template>\n");
        let r = transform(&src, "x/component/component.html", &opts).expect("transform");
        assert_eq!(r.code.matches("'tabIndex'").count(), 13, "{}", r.code);
        for v in ["'true'", "'false'", "'truthy'"] {
            assert!(r.code.contains(v), "missing {v}: {}", r.code);
        }
    }

    /// ssr-compiler element.ts:234-235: inner/outer-h-t-m-l *attributes* on Element, ExternalComponent and
    /// the `<slot>` element throw a plain Error (message only); props (Component, lwc:component) do not.
    #[test]
    fn transform_html_inner_outer_html_attribute_rejected() {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), target_ssr: true, ..Default::default() };
        let err = |src: &str| transform(src, "x/c/c.html", &opts).expect_err(src);
        for (src, msg) in [
            ("<template><div inner-h-t-m-l=\"x\">o</div></template>", "Cannot set attribute \"inner-h-t-m-l\" on <div>."),
            ("<template><div outer-h-t-m-l={x}></div></template>", "Cannot set attribute \"outer-h-t-m-l\" on <div>."),
            ("<template><omg-whatever lwc:external outer-h-t-m-l={x}></omg-whatever></template>", "Cannot set attribute \"outer-h-t-m-l\" on <omg-whatever>."),
            // slot.ts:90-91: the slot's own attributes are checked before its children.
            ("<template><slot inner-h-t-m-l=\"x\"><p outer-h-t-m-l=\"y\"></p></slot></template>", "Cannot set attribute \"inner-h-t-m-l\" on <slot>."),
            ("<template><section><template if:true={a}><span inner-h-t-m-l=\"x\"></span></template></section></template>", "Cannot set attribute \"inner-h-t-m-l\" on <span>."),
            ("<template><x-c><b inner-h-t-m-l=\"x\"></b></x-c></template>", "Cannot set attribute \"inner-h-t-m-l\" on <b>."),
        ] {
            let e = err(src);
            assert_eq!(e.message, msg, "{src}");
            // Plain `Error`: no filename, location, code, or diagnostics.
            assert_eq!((e.filename.as_deref(), e.line, e.column, e.diagnostics.len()), (None, None, None, 0), "{src}");
        }
        // A light-DOM `<slot inner-h-t-m-l>` never reaches the transformer: the parser drops it with LWC1134
        // (template-compiler parser/index.ts:1494), a warning SSR only console.errors.
        for src in [
            "<template><x-c inner-h-t-m-l=\"x\"></x-c></template>",
            "<template><lwc:component lwc:is={ctor} inner-h-t-m-l=\"x\"></lwc:component></template>",
            "<template lwc:render-mode=\"light\"><slot inner-h-t-m-l=\"x\"></slot></template>",
        ] {
            transform(src, "x/c/c.html", &opts).unwrap_or_else(|e| panic!("{src}: {e:?}"));
        }
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
