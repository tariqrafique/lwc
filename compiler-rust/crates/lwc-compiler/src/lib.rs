//! CompilerBackend seam. JS reference: `@lwc/compiler` `transformSync` / `transformFile`.
pub use lwc_shared::{CompileError, CompileOptions, CompileResult};

pub trait CompilerBackend {
    fn transform(
        &self,
        src: &str,
        filename: &str,
        options: &CompileOptions,
    ) -> Result<CompileResult, CompileError>;
}

/// Rust backend: routes by extension + `target_ssr` (PROGRAM.md facade rules).
#[derive(Debug, Default, Clone, Copy)]
pub struct RustBackend;

impl CompilerBackend for RustBackend {
    fn transform(
        &self,
        src: &str,
        filename: &str,
        options: &CompileOptions,
    ) -> Result<CompileResult, CompileError> {
        dispatch(src, filename, options)
    }
}

pub fn dispatch(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, CompileError> {
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "css" => lwc_style::transform(src, filename, options),
        // HTML: SSR vs CSR — matches JS transformFile + PROGRAM.md
        "html" if options.target_ssr => lwc_ssr::transform(src, filename, options),
        "html" => transform_template(src, filename, options),
        "js" | "ts" | "mjs" | "jsx" | "tsx" if options.target_ssr => {
            lwc_ssr::transform(src, filename, options)
        }
        "js" | "ts" | "mjs" | "jsx" | "tsx" => lwc_js::transform(src, filename, options),
        _ => Err(CompileError {
            message: format!("unsupported file extension: {filename}"),
            filename: Some(filename.into()),
            diagnostics: vec![],
            name: None,
            line: None,
            column: None,
        }),
    }
}


fn transform_template(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, CompileError> {
    use lwc_shared::DiagnosticLevel;
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
        // transformSync default (omitted → true), resolved in lwc_shared, not template's None→false.
        experimental_dynamic_directive: Some(options.resolved_experimental_dynamic_directive()),
        custom_renderer_config: options.custom_renderer_config.clone(),
        instrumentation: options.instrumentation.clone(),
        ..lwc_template::Config::default()
    };
    match lwc_template::compile(src, filename, config) {
        Ok(r) => {
            // JS transformSync: throw CompilerError.from(first Error diag); Fatal treated as error too.
            let first_err = r
                .warnings
                .iter()
                .find(|w| matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal))
                .cloned();
            if let Some(d) = first_err {
                let errors: Vec<_> = r
                    .warnings
                    .into_iter()
                    .filter(|w| matches!(w.level, DiagnosticLevel::Error | DiagnosticLevel::Fatal))
                    .collect();
                // errors[0] first so napi `code` = diagnostics[0].code.
                Err(CompileError {
                    message: d.message.clone(),
                    filename: Some(filename.into()),
                    diagnostics: errors,
                    name: None,
                    line: d.location.map(|l| l.start_line),
                    column: d.location.map(|l| l.start_column),
                })
            } else {
                Ok(CompileResult {
                    code: r.code,
                    map: None,
                    warnings: r.warnings.into_iter().filter(|w| w.level == DiagnosticLevel::Warning).collect(),
                    css_scope_tokens: Some(r.css_scope_tokens),
                })
            }
        }
        Err(e) => Err(CompileError {
            message: format!("lwc-template: {}", e.message),
            filename: Some(filename.into()),
            diagnostics: vec![],
            name: None,
            line: None,
            column: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_css_and_js_csr() {
        let o = CompileOptions::default();
        // empty css → Ok from style stub
        let css = dispatch("", "a.css", &o).expect("empty css");
        assert_eq!(css.map.as_deref(), Some(""));
        // .js routes to lwc-js: empty module compiles; lwc-js parse errors surface.
        assert_eq!(dispatch("", "a.js", &o).unwrap().code, "");
        assert!(dispatch("let = ;", "a.js", &o).is_err());
        let lws = CompileOptions { enable_lightning_web_security_transforms: true, ..CompileOptions::default() };
        // LWS is implemented in lwc-js (no longer fail-closed). Empty module stays empty, and a global
        // read is actually rewritten (shipped JS: `window === globalThis || window === document ? location : window.location`).
        assert_eq!(dispatch("", "a.js", &lws).unwrap().code, "");
        let g = "export const a = window.location;\n";
        let (off, on) = (dispatch(g, "a.js", &o).unwrap().code, dispatch(g, "a.js", &lws).unwrap().code);
        assert!(!off.contains("globalThis"), "{off}");
        assert!(on.contains("window === globalThis || window === document ? location : window.location"), "{on}");
        assert!(dispatch("", "a.txt", &o).is_err());
    }

    #[test]
    fn routes_html_csr_to_template_not_ssr() {
        let o = CompileOptions::default();
        let r = dispatch("<template></template>", "a.html", &o).expect("html CSR should reach template");
        assert!(r.code.contains("registerTemplate"), "{}", r.code);
    }

    /// JS transformSync('', 'a.html'): LWC1072 "Missing root template tag" at line 0, column 0
    /// (fixture parsing-errors/empty-template; location {line:0,column:0,start:0,length:0}).
    #[test]
    fn empty_html_is_1072_at_0_0() {
        for src in ["", " \n "] {
            let e = dispatch(src, "a.html", &CompileOptions::default()).expect_err("1072");
            assert_eq!((e.line, e.column), (Some(0), Some(0)), "{e:?}");
            assert!(e.message.starts_with("LWC1072: Missing root template tag"), "{}", e.message);
            let [d] = &e.diagnostics[..] else { panic!("{:?}", e.diagnostics) };
            assert_eq!(d.code, Some(1072));
            let l = d.location.clone().expect("location");
            assert_eq!((l.start_line, l.start_column, l.start, l.end), (0, 0, 0, 0));
        }
    }

    #[test]
    fn routes_html_and_js_to_ssr_when_target_ssr() {
        let o = CompileOptions {
            target_ssr: true,
            ..CompileOptions::default()
        };
        // html + target_ssr: live emit via lwc-ssr (not CSR template)
        let html = dispatch("<template></template>", "a.html", &o).expect("html + target_ssr");
        assert!(
            html.code.contains("@lwc/ssr-runtime") || html.code.contains("ssr-runtime"),
            "html + target_ssr must emit SSR module, got: {}",
            &html.code[..html.code.len().min(120)]
        );
        // js/ts + target_ssr: live compileComponentForSSR via lwc-ssr
        let src = "import { LightningElement } from 'lwc';\nexport default class A extends LightningElement {}\n";
        for f in ["a.js", "a.ts"] {
            let r = dispatch(src, f, &o).unwrap_or_else(|e| panic!("{f} + target_ssr: {}", e.message));
            assert!(r.code.contains("ssr-runtime"), "{f} + target_ssr must emit SSR module, got: {}", &r.code[..r.code.len().min(120)]);
        }
        // css never routes via target_ssr
        let css = dispatch("", "a.css", &o).expect("css ignores target_ssr");
        assert_eq!(css.map.as_deref(), Some(""));
    }

    #[test]
    fn facade_does_not_forward_computed_member_expression() {
        // Shipped transformSync has no experimentalComputedMemberExpression → LWC1038 via the facade.
        let o = CompileOptions { name: "foo".into(), namespace: "x".into(), ..CompileOptions::default() };
        let e = dispatch("<template>{val[state.foo]}</template>", "foo.html", &o).unwrap_err();
        assert_eq!(e.diagnostics[0].code, Some(1038), "{}", e.message);
    }

    #[test]
    fn forwards_custom_renderer_config() {
        let src = "<template>\n    <div class=\"city\">a</div>\n    <div>b</div>\n</template>";
        let off = CompileOptions { name: "foo".into(), namespace: "x".into(), ..CompileOptions::default() };
        let on = CompileOptions {
            custom_renderer_config: Some(lwc_shared::CustomRendererConfig {
                directives: vec![],
                elements: vec![lwc_shared::CustomRendererElementConfig { tag_name: "div".into(), namespace: None, attributes: None }],
            }),
            ..off.clone()
        };
        let n = |o: &CompileOptions| dispatch(src, "foo.html", o).unwrap().code.matches("renderer").count();
        assert!(n(&on) > n(&off), "on={} off={}", n(&on), n(&off));
    }

    #[test]
    fn experimental_dynamic_directive_defaults_true_via_facade() {
        let src = "<template><x-foo lwc:dynamic={ctor}></x-foo></template>";
        let o = CompileOptions { name: "foo".into(), namespace: "x".into(), ..CompileOptions::default() };
        assert_eq!(o.experimental_dynamic_directive, None);
        let w = |o: &CompileOptions| dispatch(src, "foo.html", o).unwrap().warnings.iter().filter_map(|w| w.code).collect::<Vec<_>>();
        assert_eq!(w(&o), vec![1187]);
        assert_eq!(w(&CompileOptions { experimental_dynamic_directive: Some(true), ..o.clone() }), vec![1187]);
    }

    #[test]
    fn instrumentation_counters_reach_template_and_js() {
        use lwc_shared::{CompilerMetric, Instrumentation, InstrumentationHandle};
        use std::sync::{Arc, Mutex};
        #[derive(Default)]
        struct Rec(Mutex<Vec<CompilerMetric>>);
        impl Instrumentation for Rec { fn increment_counter(&self, m: CompilerMetric) { self.0.lock().unwrap().push(m); } }
        let rec = Arc::new(Rec::default());
        let o = CompileOptions {
            name: "foo".into(),
            namespace: "x".into(),
            dynamic_import_loader: "@x/loader".into(),
            instrumentation: Some(InstrumentationHandle(rec.clone())),
            ..CompileOptions::default()
        };
        dispatch("<template lwc:render-mode=\"light\"><x-foo lwc:dynamic={ctor}></x-foo></template>", "foo.html", &o).unwrap();
        dispatch("export default function f() { return import('a'); }\n", "foo.js", &o).unwrap();
        assert_eq!(
            *rec.0.lock().unwrap(),
            vec![CompilerMetric::LwcRenderModeDirective, CompilerMetric::LwcDynamicDirective, CompilerMetric::DynamicImportTransform]
        );
        // No handle: nothing to call, compile still succeeds.
        dispatch("export default function f() { return import('a'); }\n", "foo.js", &CompileOptions { instrumentation: None, ..o }).unwrap();
    }

    #[test]
    fn js_sourcemap_on_and_inline() {
        let o = CompileOptions::default();
        assert!(dispatch("", "a.js", &o).unwrap().map.is_none());
        let src = "export const a = 1;\n";
        let on = dispatch(src, "a.js", &CompileOptions { sourcemap: lwc_shared::Sourcemap::On, ..o.clone() }).unwrap();
        assert!(on.map.as_deref().is_some_and(|m| m.contains("\"sources\":[\"a.js\"]")), "{:?}", on.map);
        let m = on.map.as_deref().unwrap();
        assert!(m.contains("\"version\":3") && !m.contains("\"mappings\":\"\""), "map must be v3 with non-empty mappings: {m}");
        let inline = dispatch(src, "a.js", &CompileOptions { sourcemap: lwc_shared::Sourcemap::Inline, ..o.clone() }).unwrap();
        assert!(inline.map.is_none());
        assert!(inline.code.contains("\n//# sourceMappingURL=data:application/json;charset=utf-8;base64,"), "{}", inline.code);
    }
}
