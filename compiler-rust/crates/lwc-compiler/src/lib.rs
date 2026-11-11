//! CompilerBackend seam. JS reference: `@lwc/compiler` `transformSync` / `transformFile`.
//! Each facade surfaces its compiler's error the way the matching `transformers/*.ts` does.
use lwc_shared::errors::TransformerErrors;
pub use lwc_shared::{CompileError, CompileOptions, CompileResult, TransformError};
use lwc_shared::{
    CompilerAggregateError, CompilerError, DiagnosticLevel, normalize_to_compiler_error,
};

pub trait CompilerBackend {
    fn transform(
        &self,
        src: &str,
        filename: &str,
        options: &CompileOptions,
    ) -> Result<CompileResult, TransformError>;
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
    ) -> Result<CompileResult, TransformError> {
        dispatch(src, filename, options)
    }
}

pub fn dispatch(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, TransformError> {
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "css" => transform_style(src, filename, options),
        // HTML: SSR vs CSR — matches JS transformFile + PROGRAM.md
        // SSR errors are surfaced as @lwc/ssr-compiler throws them (shipped `transformSync` does not normalize them,
        // transformer.ts:98-99 / :112-113). compile-template throws a plain `Error` (compile-template/index.ts:111).
        "html" if options.target_ssr => {
            lwc_ssr::transform(src, filename, options).map_err(unnormalized)
        }
        "html" => transform_template(src, filename, options),
        "js" | "ts" | "mjs" | "jsx" | "tsx" if options.target_ssr => {
            lwc_ssr::transform(src, filename, options).map_err(ssr_js_error)
        }
        "js" | "ts" | "mjs" | "jsx" | "tsx" => transform_javascript(src, filename, options),
        _ => Err(unnormalized(CompileError::new(
            format!("unsupported file extension: {filename}"),
            filename,
        ))),
    }
}

fn unnormalized(e: CompileError) -> TransformError {
    TransformError::Unnormalized(e.into())
}

/// @lwc/ssr-compiler compile-js: every coded error is compile-js/errors.ts:21-40 `generateError(node, info, ..args)`
/// = `generateCompilerError(info, { messageArgs, origin: { filename, location } })` (@lwc/errors compiler/errors.ts:77-100):
/// a `CompilerError` with the info's code, level and url, at the node. Its other throws are plain `new Error(..)`
/// (no code): unnormalized.
fn ssr_js_error(e: CompileError) -> TransformError {
    match e.diagnostics.first() {
        Some(d) if d.code.is_some() => TransformError::Compiler(CompilerError {
            level: d.level,
            ..CompilerError::from_diagnostic(d, e.filename.as_deref())
        }),
        _ => unnormalized(e),
    }
}

/// An options validation failure (the binding's `to_compile_options`; today only LWC1021) surfaced as
/// `transformSync` throws it: options.ts:206-211 `validateOutputConfig` -> `invariant(.., INVALID_SOURCEMAP_PROPERTY,
/// [config.sourcemap])` (@lwc/errors compiler/errors.ts:109-119) -> `generateCompilerError(info, { messageArgs })`
/// (:77-100). Code, message, level and url come from the info; with no `origin`, `filename` and `location`
/// are `undefined` (own keys on the JS object), whatever file was being compiled.
pub fn options_error(e: CompileError) -> TransformError {
    let Some(d) = e.diagnostics.first() else {
        return unnormalized(e);
    };
    TransformError::Compiler(CompilerError {
        filename: None,
        location: None,
        level: d.level,
        ..CompilerError::from_diagnostic(d, None)
    })
}

/// `transformers/style.ts` 44-61: a recovery-mode `AggregateError` becomes
/// `CompilerAggregateError(e.errors.map(normalizeToCompilerError(CSS_TRANSFORMER_ERROR, error, { filename })),
/// 'Multiple CSS errors occurred during compilation.')`; anything else is
/// `normalizeToCompilerError(CSS_TRANSFORMER_ERROR, e, { filename })`.
fn transform_style(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, TransformError> {
    let normalize = |e: &lwc_style::StyleError| {
        normalize_to_compiler_error(
            &TransformerErrors::CSS_TRANSFORMER_ERROR,
            e.thrown(filename),
            Some(filename),
        )
    };
    lwc_style::transform(src, filename, options).map_err(|e| match &e {
        lwc_style::StyleError::Aggregate(errors) if options.experimental_error_recovery_mode => {
            TransformError::Aggregate(CompilerAggregateError {
                errors: errors.iter().map(normalize).collect(),
                message: "Multiple CSS errors occurred during compilation.".into(),
            })
        }
        _ => TransformError::Compiler(normalize(&e)),
    })
}

/// `transformers/javascript.ts` 109-135: a Babel throw is `normalizeToCompilerError(JS_TRANSFORMER_ERROR, e,
/// { filename })`, or against JS_TRANSFORMER_DECORATOR_ERROR when it is Babel's own "Decorators are not
/// enabled." transform error naming @track/@api/@wire; recovery-mode `metadata.lwcErrors` become
/// `CompilerAggregateError(errors.map(d => CompilerError.from(d)), 'Multiple errors occurred during compilation.')`.
fn transform_javascript(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, TransformError> {
    lwc_js::transform(src, filename, options).map_err(|e| match e {
        lwc_js::JsError::Thrown { error, code } => {
            let decorator = code == lwc_js::BabelCode::Transform
                && error.message.contains("Decorators are not enabled.")
                && mentions_lwc_decorator(&error.message);
            let info = if decorator {
                TransformerErrors::JS_TRANSFORMER_DECORATOR_ERROR
            } else {
                TransformerErrors::JS_TRANSFORMER_ERROR
            };
            TransformError::Compiler(normalize_to_compiler_error(&info, error, Some(filename)))
        }
        lwc_js::JsError::LwcErrors(ds) => TransformError::Aggregate(CompilerAggregateError {
            errors: ds
                .iter()
                .map(|d| CompilerError::from_diagnostic(d, None))
                .collect(),
            message: "Multiple errors occurred during compilation.".into(),
        }),
    })
}

/// `/\b(track|api|wire)\b/` (ASCII word boundaries, as in JS).
fn mentions_lwc_decorator(s: &str) -> bool {
    let b = s.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    ["track", "api", "wire"].iter().any(|w| {
        s.match_indices(w).any(|(i, _)| {
            (i == 0 || !word(b[i - 1])) && b.get(i + w.len()).is_none_or(|c| !word(*c))
        })
    })
}

/// `transformers/template.ts` 72-86: a throw is `normalizeToCompilerError(HTML_TRANSFORMER_ERROR, e,
/// { filename })`; the Error-level diagnostics become `CompilerAggregateError(errors.map(e =>
/// CompilerError.from(e, { filename })))` in recovery mode (no message: ""), else
/// `CompilerError.from(errors[0], { filename })`.
fn transform_template(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, TransformError> {
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
    let r = lwc_template::compile(src, filename, config).map_err(|e| {
        TransformError::Compiler(normalize_to_compiler_error(
            &TransformerErrors::HTML_TRANSFORMER_ERROR,
            CompileError::new(e.message, filename),
            Some(filename),
        ))
    })?;
    let warnings = template_diagnostics(
        r.warnings,
        filename,
        options.experimental_error_recovery_mode,
    )?;
    Ok(CompileResult {
        code: r.code,
        map: None,
        warnings,
        css_scope_tokens: Some(r.css_scope_tokens),
    })
}

/// template.ts:76-90 on the template compiler's diagnostics: the errors are exactly
/// :76 `result.warnings.filter((warning) => warning.level === DiagnosticLevel.Error)`; any -> throw,
/// recovery mode `new CompilerAggregateError(errors.map((e) => CompilerError.from(e, { filename })))` (:78-82),
/// else `CompilerError.from(errors[0], { filename })` (:84-85). The result's warnings are :90
/// `.filter((warning) => warning.level === DiagnosticLevel.Warning)`. `Fatal` and `Log` match neither
/// ("currently unused", :87-88): 9.5.3 drops them silently.
fn template_diagnostics(
    ds: Vec<lwc_shared::CompilerDiagnostic>,
    filename: &str,
    recovery: bool,
) -> Result<Vec<lwc_shared::CompilerDiagnostic>, TransformError> {
    let (errors, rest): (Vec<_>, Vec<_>) = ds
        .into_iter()
        .partition(|w| w.level == DiagnosticLevel::Error);
    if let Some(first) = errors.first() {
        return Err(if recovery {
            TransformError::Aggregate(CompilerAggregateError {
                errors: errors
                    .iter()
                    .map(|d| CompilerError::from_diagnostic(d, Some(filename)))
                    .collect(),
                message: String::new(),
            })
        } else {
            TransformError::Compiler(CompilerError::from_diagnostic(first, Some(filename)))
        });
    }
    Ok(rest
        .into_iter()
        .filter(|w| w.level == DiagnosticLevel::Warning)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiler(e: TransformError) -> CompilerError {
        match e {
            TransformError::Compiler(e) => e,
            e => panic!("not a CompilerError: {e:?}"),
        }
    }

    /// Shipped 9.5.3 `templateTransform`: recovery mode -> `CompilerAggregateError` with message "" and one
    /// member per Error diagnostic; normal mode -> the first diagnostic alone.
    #[test]
    fn template_recovery_mode_aggregates_like_shipped() {
        let src = "<template><div lwc:a=\"1\" lwc:b=\"2\"></div><template></template></template>";
        let o = CompileOptions {
            name: "foo".into(),
            namespace: "x".into(),
            experimental_error_recovery_mode: true,
            ..Default::default()
        };
        let TransformError::Aggregate(a) = dispatch(src, "foo.html", &o).unwrap_err() else {
            panic!("aggregate")
        };
        assert_eq!(a.message, "");
        assert_eq!(
            a.errors.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![Some(1127), Some(1077)]
        );
        assert!(
            a.errors
                .iter()
                .all(|e| e.filename.as_deref() == Some("foo.html"))
        );
        let o = CompileOptions {
            experimental_error_recovery_mode: false,
            ..o
        };
        let e = compiler(dispatch(src, "foo.html", &o).unwrap_err());
        assert_eq!(
            e.message,
            "LWC1127: Invalid directive \"lwc:a\" on element <div>."
        );
    }

    #[test]
    fn thrown_template_failure_is_lwc1008() {
        // A template-compiler throw (invalid customRendererConfig) is normalized against HTML_TRANSFORMER_ERROR.
        let o = CompileOptions {
            custom_renderer_config: Some(lwc_shared::CustomRendererConfig {
                directives: vec![],
                elements: vec![lwc_shared::CustomRendererElementConfig {
                    tag_name: "x-foo".into(),
                    namespace: None,
                    attributes: None,
                }],
            }),
            ..CompileOptions::default()
        };
        let e = compiler(dispatch("<template></template>", "foo.html", &o).unwrap_err());
        assert_eq!(
            e.message,
            "Error: LWC1008: custom renderer config disallows custom element tag: x-foo"
        );
        assert_eq!(
            (e.code, e.location, e.url.as_str(), e.filename.as_deref()),
            (Some(1008), None, "", Some("foo.html"))
        );
    }

    /// template.ts:76/:90 keep `level === Error` (thrown) and `level === Warning` (returned) only; `Fatal` and
    /// `Log` are dropped ("currently unused", :87-88). No 9.5.3 input reaches a Fatal/Log template diagnostic:
    /// every @lwc/errors info has `level: Error|Warning` and template-compiler never passes
    /// `useStrictErrorOverride`. So the oracle injects them: shipped 9.5.3 `transformSync` run live in node
    /// (`$LWC_ORACLE`, pinned at b9e8007), with `@lwc/template-compiler` wrapped (as
    /// dist/index.cjs requires it) to append `{ code: 9999, level }` diagnostics to `warnings`. Each line of
    /// the oracle's output is compared with `template_diagnostics` on the same injection. Missing node or a
    /// moved oracle is a failure, not a skip.
    #[test]
    fn template_levels_match_shipped() {
        use DiagnosticLevel::*;
        const SETS: &[&[DiagnosticLevel]] = &[
            &[Fatal],
            &[Log],
            &[Warning],
            &[Error],
            &[Fatal, Error],
            &[Fatal, Warning],
            &[Log, Warning, Fatal],
        ];
        const ORACLE: &str = r#"
const Module = require('module');
const root = process.env.LWC_ORACLE; if (!root) { console.error('LWC_ORACLE is unset'); process.exit(2); }
let inject = [];
const load = Module._load;
const wrap = (f) => f && function (...a) {
    const r = f(...a);
    r.warnings = [...r.warnings, ...inject.map((level) => ({ code: 9999, message: 'LWC9999: injected', level }))];
    return r;
};
Module._load = function (req) {
    const m = load.apply(this, arguments);
    return req === '@lwc/template-compiler' ? { ...m, default: wrap(m.default), compile: wrap(m.compile) } : m;
};
const { transformSync } = require(root + '/packages/@lwc/compiler/dist/index.cjs');
const show = (ds) => ds.map((d) => d.code + ':' + d.level).join(',');
for (const recovery of [false, true]) for (const set of JSON.parse(process.argv[1])) {
    inject = set;
    let out;
    try {
        const r = transformSync('<template><div></div></template>', 'x.html', { name: 'x', namespace: 'x', experimentalErrorRecoveryMode: recovery });
        out = 'returned ' + show(r.warnings);
    } catch (e) {
        out = e.errors ? 'aggregate ' + show(e.errors) : 'threw ' + show([e]);
    }
    console.log(set.join('+') + ' recovery=' + recovery + ' ' + out);
}
"#;
        let root = lwc_shared::oracle_root()
            .to_str()
            .expect("LWC_ORACLE must be UTF-8")
            .to_string();
        let head = std::process::Command::new("git")
            .args(["-C", &root, "rev-parse", "HEAD"])
            .output()
            .expect("git is required");
        let head = String::from_utf8(head.stdout).unwrap();
        assert!(
            head.starts_with("b9e8007"),
            "oracle {root} is at {head:?}, want b9e8007"
        );
        let sets = format!(
            "[{}]",
            SETS.iter()
                .map(|s| format!(
                    "[{}]",
                    s.iter()
                        .map(|l| l.js_value().to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        let out = std::process::Command::new("node")
            .args(["-e", ORACLE, &sets])
            .output()
            .expect("node is required");
        assert!(
            out.status.success(),
            "oracle failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let want = String::from_utf8(out.stdout).unwrap();

        let show = |ds: &mut dyn Iterator<Item = (Option<u32>, DiagnosticLevel)>| {
            ds.map(|(c, l)| format!("{}:{}", c.unwrap(), l.js_value()))
                .collect::<Vec<_>>()
                .join(",")
        };
        let mut got = String::new();
        for recovery in [false, true] {
            for set in SETS {
                let ds = set
                    .iter()
                    .map(|&l| {
                        lwc_shared::CompilerDiagnostic::new(
                            Some(9999),
                            "LWC9999: injected",
                            l,
                            None,
                            None,
                        )
                    })
                    .collect();
                let out = match template_diagnostics(ds, "x.html", recovery) {
                    Ok(w) => format!(
                        "returned {}",
                        show(&mut w.iter().map(|d| (d.code, d.level)))
                    ),
                    Err(TransformError::Aggregate(a)) => format!(
                        "aggregate {}",
                        show(&mut a.errors.iter().map(|e| (e.code, e.level)))
                    ),
                    Err(TransformError::Compiler(e)) => {
                        format!("threw {}", show(&mut std::iter::once((e.code, e.level))))
                    }
                    Err(e) => panic!("{set:?} recovery={recovery}: {e:?}"),
                };
                let name = set
                    .iter()
                    .map(|l| l.js_value().to_string())
                    .collect::<Vec<_>>()
                    .join("+");
                got += &format!("{name} recovery={recovery} {out}\n");
            }
        }
        assert_eq!(got, want);
    }

    /// options.ts:206-211 -> `generateCompilerError(INVALID_SOURCEMAP_PROPERTY, { messageArgs })`: the info's code,
    /// level and url, and no filename / location (own `undefined` in 9.5.3) even though a file was named.
    #[test]
    fn options_error_is_generate_compiler_error() {
        let e = lwc_shared::Sourcemap::from_js_str("foo", "foo.js").unwrap_err();
        let e = compiler(options_error(e));
        assert_eq!(
            (e.code, e.level, e.filename, e.location),
            (Some(1021), DiagnosticLevel::Error, None, None)
        );
        assert_eq!(e.url, lwc_shared::errors::url_of(Some(1021)));
    }

    /// The level filter is the template facade's only: a real Warning (LWC1149, `key` off a for:each) still
    /// reaches `CompileResult.warnings`, in both modes. 9.5.3 `transformSync` gives `warnings: [{code: 1149, level: 2}]`.
    #[test]
    fn template_warning_reaches_compile_result() {
        for recovery in [false, true] {
            let o = CompileOptions {
                name: "foo".into(),
                namespace: "x".into(),
                experimental_error_recovery_mode: recovery,
                ..Default::default()
            };
            let r = dispatch("<template><div key={k}></div></template>", "foo.html", &o)
                .expect("compiles");
            let got: Vec<_> = r.warnings.iter().map(|w| (w.code, w.level)).collect();
            assert_eq!(
                got,
                [(Some(1149), DiagnosticLevel::Warning)],
                "recovery={recovery}"
            );
        }
    }

    #[test]
    fn routes_css_and_js_csr() {
        let o = CompileOptions::default();
        // empty css → Ok from style stub
        let css = dispatch("", "a.css", &o).expect("empty css");
        assert_eq!(css.map.as_deref(), Some(""));
        // .js routes to lwc-js: empty module compiles; lwc-js parse errors surface.
        assert_eq!(dispatch("", "a.js", &o).unwrap().code, "");
        assert!(dispatch("let = ;", "a.js", &o).is_err());
        let lws = CompileOptions {
            enable_lightning_web_security_transforms: true,
            ..CompileOptions::default()
        };
        // LWS is implemented in lwc-js (no longer fail-closed). Empty module stays empty, and a global
        // read is actually rewritten (shipped JS: `window === globalThis || window === document ? location : window.location`).
        assert_eq!(dispatch("", "a.js", &lws).unwrap().code, "");
        let g = "export const a = window.location;\n";
        let (off, on) = (
            dispatch(g, "a.js", &o).unwrap().code,
            dispatch(g, "a.js", &lws).unwrap().code,
        );
        assert!(!off.contains("globalThis"), "{off}");
        assert!(
            on.contains(
                "window === globalThis || window === document ? location : window.location"
            ),
            "{on}"
        );
        assert!(dispatch("", "a.txt", &o).is_err());
    }

    #[test]
    fn routes_html_csr_to_template_not_ssr() {
        let o = CompileOptions::default();
        let r = dispatch("<template></template>", "a.html", &o)
            .expect("html CSR should reach template");
        assert!(r.code.contains("registerTemplate"), "{}", r.code);
    }

    /// JS transformSync('', 'a.html'): LWC1072 "Missing root template tag" at line 0, column 0
    /// (fixture parsing-errors/empty-template; location {line:0,column:0,start:0,length:0}).
    #[test]
    fn empty_html_is_1072_at_0_0() {
        for src in ["", " \n "] {
            let e =
                compiler(dispatch(src, "a.html", &CompileOptions::default()).expect_err("1072"));
            assert!(
                e.message.starts_with("LWC1072: Missing root template tag"),
                "{}",
                e.message
            );
            assert_eq!(e.code, Some(1072));
            let l = e.location.expect("location");
            assert_eq!(
                (l.line, l.column, l.start, l.length),
                (0, 0, Some(0), Some(0))
            );
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
            let r = dispatch(src, f, &o).unwrap_or_else(|e| panic!("{f} + target_ssr: {e:?}"));
            assert!(
                r.code.contains("ssr-runtime"),
                "{f} + target_ssr must emit SSR module, got: {}",
                &r.code[..r.code.len().min(120)]
            );
        }
        // css never routes via target_ssr
        let css = dispatch("", "a.css", &o).expect("css ignores target_ssr");
        assert_eq!(css.map.as_deref(), Some(""));
    }

    #[test]
    fn facade_does_not_forward_computed_member_expression() {
        // Shipped transformSync has no experimentalComputedMemberExpression → LWC1038 via the facade.
        let o = CompileOptions {
            name: "foo".into(),
            namespace: "x".into(),
            ..CompileOptions::default()
        };
        let e = compiler(
            dispatch("<template>{val[state.foo]}</template>", "foo.html", &o).unwrap_err(),
        );
        assert_eq!(e.code, Some(1038), "{}", e.message);
    }

    #[test]
    fn forwards_custom_renderer_config() {
        let src = "<template>\n    <div class=\"city\">a</div>\n    <div>b</div>\n</template>";
        let off = CompileOptions {
            name: "foo".into(),
            namespace: "x".into(),
            ..CompileOptions::default()
        };
        let on = CompileOptions {
            custom_renderer_config: Some(lwc_shared::CustomRendererConfig {
                directives: vec![],
                elements: vec![lwc_shared::CustomRendererElementConfig {
                    tag_name: "div".into(),
                    namespace: None,
                    attributes: None,
                }],
            }),
            ..off.clone()
        };
        let n = |o: &CompileOptions| {
            dispatch(src, "foo.html", o)
                .unwrap()
                .code
                .matches("renderer")
                .count()
        };
        assert!(n(&on) > n(&off), "on={} off={}", n(&on), n(&off));
    }

    #[test]
    fn experimental_dynamic_directive_defaults_true_via_facade() {
        let src = "<template><x-foo lwc:dynamic={ctor}></x-foo></template>";
        let o = CompileOptions {
            name: "foo".into(),
            namespace: "x".into(),
            ..CompileOptions::default()
        };
        assert_eq!(o.experimental_dynamic_directive, None);
        let w = |o: &CompileOptions| {
            dispatch(src, "foo.html", o)
                .unwrap()
                .warnings
                .iter()
                .filter_map(|w| w.code)
                .collect::<Vec<_>>()
        };
        assert_eq!(w(&o), vec![1187]);
        assert_eq!(
            w(&CompileOptions {
                experimental_dynamic_directive: Some(true),
                ..o.clone()
            }),
            vec![1187]
        );
    }

    #[test]
    fn instrumentation_counters_reach_template_and_js() {
        use lwc_shared::{CompilerMetric, Instrumentation, InstrumentationHandle};
        use std::sync::{Arc, Mutex};
        #[derive(Default)]
        struct Rec(Mutex<Vec<CompilerMetric>>);
        impl Instrumentation for Rec {
            fn increment_counter(&self, m: CompilerMetric) {
                self.0.lock().unwrap().push(m);
            }
        }
        let rec = Arc::new(Rec::default());
        let o = CompileOptions {
            name: "foo".into(),
            namespace: "x".into(),
            dynamic_import_loader: "@x/loader".into(),
            instrumentation: Some(InstrumentationHandle(rec.clone())),
            ..CompileOptions::default()
        };
        dispatch(
            "<template lwc:render-mode=\"light\"><x-foo lwc:dynamic={ctor}></x-foo></template>",
            "foo.html",
            &o,
        )
        .unwrap();
        dispatch(
            "export default function f() { return import('a'); }\n",
            "foo.js",
            &o,
        )
        .unwrap();
        assert_eq!(
            *rec.0.lock().unwrap(),
            vec![
                CompilerMetric::LwcRenderModeDirective,
                CompilerMetric::LwcDynamicDirective,
                CompilerMetric::DynamicImportTransform
            ]
        );
        // No handle: nothing to call, compile still succeeds.
        dispatch(
            "export default function f() { return import('a'); }\n",
            "foo.js",
            &CompileOptions {
                instrumentation: None,
                ..o
            },
        )
        .unwrap();
    }

    #[test]
    fn js_sourcemap_on_and_inline() {
        let o = CompileOptions::default();
        assert!(dispatch("", "a.js", &o).unwrap().map.is_none());
        let src = "export const a = 1;\n";
        let on = dispatch(
            src,
            "a.js",
            &CompileOptions {
                sourcemap: lwc_shared::Sourcemap::On,
                ..o.clone()
            },
        )
        .unwrap();
        assert!(
            on.map
                .as_deref()
                .is_some_and(|m| m.contains("\"sources\":[\"a.js\"]")),
            "{:?}",
            on.map
        );
        let m = on.map.as_deref().unwrap();
        assert!(
            m.contains("\"version\":3") && !m.contains("\"mappings\":\"\""),
            "map must be v3 with non-empty mappings: {m}"
        );
        let inline = dispatch(
            src,
            "a.js",
            &CompileOptions {
                sourcemap: lwc_shared::Sourcemap::Inline,
                ..o.clone()
            },
        )
        .unwrap();
        assert!(inline.map.is_none());
        assert!(
            inline
                .code
                .contains("\n//# sourceMappingURL=data:application/json;charset=utf-8;base64,"),
            "{}",
            inline.code
        );
    }
}
