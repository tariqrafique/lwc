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
    let alloc = lwc_template::Allocator::default();
    let parsing = lwc_template::parse(&alloc, src, config).map_err(|e| CompileError {
        message: e.message,
        filename: Some(filename.into()),
        diagnostics: vec![],
        name: None,
        position: lwc_shared::ProducerPosition::Diagnostics,
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
            position: lwc_shared::ProducerPosition::Diagnostics,
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
            assert_eq!((e.filename.as_deref(), e.line(), e.column(), e.diagnostics.len()), (None, None, None, 0), "{src}");
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

    /// AB-L0058: the parser now keeps non-preserved comments as `ChildNode::Comment`, as upstream's parser
    /// always did (template-compiler parser/index.ts:466-468). Every consumer below matches upstream
    /// ssr-compiler with the comment present (checked by rendering each shape with both compilers in
    /// sync/async/asyncYield); these pin the cases where a comment child changes what is emitted.
    #[test]
    fn transform_html_comment_children_match_upstream() {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), target_ssr: true, ..Default::default() };
        let code = |body: &str, pc: bool| {
            let src = format!("<template{}>{body}</template>", if pc { " lwc:preserve-comments" } else { "" });
            transform(&src, "x/c/c.html", &opts).unwrap_or_else(|e| panic!("{src}: {e:?}")).code
        };
        // element.ts:269: a foreign element self-closes when its *emitted* children are empty.
        let c = code("<svg><!--c--></svg>", false);
        assert!(c.contains("'/>'") && !c.contains("'</svg>'"), "{c}");
        let c = code("<svg><!--c--></svg>", true);
        // The preserved comment is a yield between the open and close tags, so shared.ts
        // folds it onto the right spine: one literal `'><!--c--></svg>'`, not a self-close.
        assert!(c.contains("'><!--c--></svg>'") && !c.contains("'/>'"), "{c}");
        // element.ts:256 tests `node.children.length` (comments count), so lwc:inner-html is not rendered;
        // the parser allows it (parser/index.ts:1775 effectiveChildren drops non-preserved comments).
        let c = code("<div lwc:inner-html={h}><!--c--></div>", false);
        assert!(!c.contains("sanitizeHtmlContent"), "{c}");
        assert!(code("<div lwc:inner-html={h}></div>", false).contains("sanitizeHtmlContent("));
        // adjacent-text-nodes.ts:33-39: a non-preserved comment joins the text run.
        let c = code("{a}<!--c-->{b}", false);
        assert!(c.contains("renderTextContent(normalizeTextContent(instance.a) + normalizeTextContent(instance.b))"), "{c}");
        let c = code("{a}<!--c-->{b}", true);
        assert!(c.contains("renderTextContent(normalizeTextContent(instance.a))") && c.contains("'<!--c-->'"), "{c}");
        // slotted-content.ts: a comment-only child yields no shadow content and is not a light-slot leaf.
        let c = code("<x-d><!--c--></x-d>", false);
        assert!(c.contains("const shadowSlottedContent = null;") && c.contains("const lightSlottedContentMap = null;"), "{c}");
        assert!(!c.contains("addSlottedContent"), "{c}");
        let c = code("<x-d><!--c--></x-d>", true);
        assert!(c.contains("__lwcGenerateShadowSlottedContent_xD_0") && c.contains("const lightSlottedContentMap = null;"), "{c}");
        // getLightSlottedContent: a text before a comment is not the last of its run, so it is skipped, and the
        // pruned lwc:if clone holds only the final text: upstream renders "b" (not "ab") into the light slot.
        let c = code("<x-d><template lwc:if={t}>a<!--c-->b</template></x-d>", false);
        let light = &c[c.find("lightSlottedContentMap = Object.create(null);").expect(&c)..];
        assert!(light.contains("normalizeTextContent('b')") && !light.contains("normalizeTextContent('a')"), "{c}");
        // lwcIf.ts: the if/else chain is built by the parser across the comment, then
        // optimizeIfStatement collapses the two single-yield branches into one ternary.
        let c = code("<template lwc:if={f}>A</template><!--c--><template lwc:else>B</template>", false);
        assert!(c.contains("(instance.f ? '<!---->' + renderTextContent(normalizeTextContent('A')) + '<!---->' : '<!---->' + renderTextContent(normalizeTextContent('B')) + '<!---->')"), "{c}");
    }

    /// ir-to-es.ts defaultTransformer: a `<template lwc:slot-data>` that is not a direct component child (here
    /// under lwc:elseif, integration-wtr light-dom/scoped-slot/if-block mixedSlotParent) reaches irToEs and
    /// throws a plain Error at compile time; as a direct child it is scoped slotted content.
    #[test]
    fn transform_html_nested_scoped_slot_fragment_is_unimplemented() {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), target_ssr: true, ..Default::default() };
        let src = "<template><x-child><template lwc:if={a}><span>A</span></template><template lwc:elseif={b}>\
            <template lwc:slot-data=\"data\"><span>{data.id}</span></template></template></x-child></template>";
        let e = transform(src, "x/c/c.html", &opts).expect_err("nested lwc:slot-data");
        assert_eq!(e.message, "Unimplemented IR node: { type: 'ScopedSlotFragment' }");
        assert_eq!((e.filename.as_deref(), e.line(), e.column(), e.diagnostics.len()), (None, None, None, 0));
        let ok = "<template><x-child><template lwc:slot-data=\"data\"><span>{data.id}</span></template></x-child></template>";
        let c = transform(ok, "x/c/c.html", &opts).expect("direct lwc:slot-data").code;
        assert!(c.contains("scopedSlottedContentMap = Object.create(null)"), "{c}");
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
        assert!(r.code.contains("__setStaticInternals(TextStatic, \"x-text-static\""), "{}", r.code);
        assert!(r.code.contains("./text-static.html"), "{}", r.code);
    }
}
