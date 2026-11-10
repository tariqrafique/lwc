//! Template-expression errors shared by the legacy and complex validators.
//!
//! The complex path and the legacy path both parse with oxc (`expression_complex`). This module
//! keeps the `{…}` predicate, the reserved-word set, and the wrapped diagnostic shapes. The hand
//! lexer (`lex`, `Tok`, `Parsed`, `acorn_text`) is gone (AB-D0004 S6).

use lwc_shared::api_version::{ApiFeature, is_api_feature_enabled};
use lwc_shared::errors::ParserDiagnostics;
use lwc_shared::{LwcErrorInfo, generate_error_message};

/// TS `isExpression`: `{...}` with length > 2.
pub(crate) fn is_expression(s: &str) -> bool {
    s.len() > 2 && s.starts_with('{') && s.ends_with('}')
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExprError {
    pub code: u32,
    pub message: String,
}

impl ExprError {
    /// An `invariant` CompilerError through `withErrorWrapping`: `Invalid expression {source} - LWCnnnn: …`.
    pub(crate) fn invariant(source: &str, info: &LwcErrorInfo, args: &[&str]) -> Self {
        ExprError {
            code: info.code,
            message: format!(
                "Invalid expression {source} - {}",
                generate_error_message(info, Some(args))
            ),
        }
    }

    /// An acorn SyntaxError through `withErrorWrapping`: `LWC1083: … Invalid expression {source} - {msg} (line:col)`.
    pub(crate) fn syntax(source: &str, msg: &str, line: usize, col: usize) -> Self {
        let info = &ParserDiagnostics::TEMPLATE_EXPRESSION_PARSING_ERROR;
        ExprError {
            code: info.code,
            message: generate_error_message(
                info,
                Some(&[&format!(
                    "Invalid expression {source} - {msg} ({line}:{col})"
                )]),
            ),
        }
    }

    /// Error-level diagnostic at `l`; `message` already carries the upstream prefix/wrapping.
    pub(crate) fn into_diagnostic(
        self,
        l: lwc_shared::SourceLocation,
    ) -> lwc_shared::CompilerDiagnostic {
        lwc_shared::CompilerDiagnostic::new(
            Some(self.code),
            self.message,
            lwc_shared::DiagnosticLevel::Error,
            None,
            Some(l.into()),
        )
    }
}

/// TS `REVERSED_KEYWORDS` (parser/utils/javascript.ts): reserved words plus the strict-mode-only ones.
const RESERVED: &[&str] = &[
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "static",
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
];

/// TS `isReservedES6Keyword` (parseIdentifier → 1211).
pub(crate) fn is_reserved(w: &str) -> bool {
    RESERVED.contains(&w)
}

/// Validation options (TS ParserCtx config + call-site flags).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Opts {
    /// `experimentalComplexExpressions`.
    pub complex: bool,
    /// `experimentalComputedMemberExpression`.
    pub computed_ok: bool,
    pub api_version: u32,
    /// TS `unquotedAttributeExpression` (attribute value written as `attr={…}`).
    pub unquoted: bool,
}

impl Opts {
    /// TS `isComplexTemplateExpressionEnabled`.
    pub(crate) fn cte(&self) -> bool {
        self.complex
            && is_api_feature_enabled(
                ApiFeature::EnableComplexTemplateExpressions,
                self.api_version,
            )
    }
}

#[cfg(test)]
pub(crate) fn check(src: &str, complex: bool) -> Option<ExprError> {
    check_with(src, complex, false)
}

/// Text-node semantics at the resolved default API version (`getAPIVersionFromNumber(undefined)`; tests).
#[cfg(test)]
pub(crate) fn check_with(src: &str, complex: bool, computed_ok: bool) -> Option<ExprError> {
    check_opts(
        src,
        &Opts {
            complex,
            computed_ok,
            api_version: crate::config::Config::default()
                .normalize()
                .expect("default config")
                .api_version,
            unquoted: false,
        },
    )
}

/// `src` is one `{…}` expression through TS `parseExpression` (expression.ts:141) on the legacy
/// (ecma 2020) oxc path. Complex (CTE) quoted and text expressions go to `expression_complex::validate`.
pub(crate) fn check_opts(src: &str, o: &Opts) -> Option<ExprError> {
    crate::parser::expression_complex::validate::legacy(src, o)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn o(complex: bool) -> Opts {
        Opts {
            complex,
            computed_ok: false,
            api_version: 66,
            unquoted: false,
        }
    }
    fn msg(s: &str, complex: bool) -> Option<String> {
        check_opts(s, &o(complex)).map(|e| format!("{} {}", e.code, e.message))
    }
    fn code(s: &str, cte: bool) -> Option<u32> {
        check(s, cte).map(|e| e.code)
    }

    #[test]
    fn legacy_upstream_fixtures() {
        assert_eq!(
            msg("{ }", false).unwrap(),
            "1083 LWC1083: Error parsing template expression: Invalid expression { } - Unexpected token (1:2)"
        );
        assert_eq!(
            msg("{ (foo }", false).unwrap(),
            "1083 LWC1083: Error parsing template expression: Invalid expression { (foo } - Unexpected token (1:7)"
        );
        assert_eq!(
            msg("{ ((foo.bar) }", false).unwrap(),
            "1083 LWC1083: Error parsing template expression: Invalid expression { ((foo.bar) } - Unexpected token (1:13)"
        );
        assert_eq!(
            msg("{ foo) }", false).unwrap(),
            "1083 Invalid expression { foo) } - LWC1083: Error parsing template expression: Unexpected end of expression"
        );
        assert_eq!(
            msg("{class}", false).unwrap(),
            "1083 LWC1083: Error parsing template expression: Invalid expression {class} - Unexpected token (1:6)"
        );
        assert_eq!(
            msg("{foo;title}", false).unwrap(),
            "1074 Invalid expression {foo;title} - LWC1074: Multiple expressions found"
        );
        assert_eq!(
            msg("{   foo;      }", false).unwrap(),
            "1074 Invalid expression {   foo;      } - LWC1074: Multiple expressions found"
        );
        assert_eq!(
            msg("{this.title}", false).unwrap(),
            "1060 Invalid expression {this.title} - LWC1060: Template expression doesn't allow ThisExpression"
        );
        assert_eq!(
            msg("{ Math.random() }", false).unwrap(),
            "1060 Invalid expression { Math.random() } - LWC1060: Template expression doesn't allow CallExpression"
        );
        assert_eq!(
            msg("{classNames[0]}", false).unwrap(),
            "1038 Invalid expression {classNames[0]} - LWC1038: Template expression doesn't allow computed property access"
        );
        for ok in [
            "{ (foo) }",
            "{ ((foo)) }",
            "{ ((foo.bar)) }",
            "{ ((foo).bar) }",
            "{  foo    }",
            "{a.b.c}",
        ] {
            assert_eq!(msg(ok, false), None, "{ok}");
        }
        assert_eq!(code("{Component as default}", false), Some(1083));
    }

    #[test]
    fn api_version_and_unquoted() {
        let old = Opts {
            complex: true,
            computed_ok: false,
            api_version: 59,
            unquoted: false,
        };
        assert_eq!(
            check_opts("{foo()}", &old).unwrap().message,
            "Invalid expression {foo()} - LWC1210: Template expression doesn't allow CallExpression. The current component API version (59) is insufficient and must be increased to at least 66 for this type of expression."
        );
        assert_eq!(check_opts("{val[state.foo]}", &old).unwrap().code, 1209);
        let unq = Opts {
            unquoted: true,
            ..o(true)
        };
        assert_eq!(
            check_opts("{bar()}", &unq).unwrap().message,
            "Invalid expression {bar()} - LWC1208: Template expression doesn't allow CallExpression unless the expression is surrounded by quotes: \"{bar()}\""
        );
        assert_eq!(check_opts("{list[0].name}", &unq).unwrap().code, 1207);
        assert_eq!(
            check_opts(
                "{val[state.foo]}",
                &Opts {
                    computed_ok: true,
                    ..o(false)
                }
            ),
            None
        );
    }

    /// Flag on, API below 66: parseExpression stays a script, so these are 1083
    /// (oracle expression-complex/invalid/{await,super,import-meta} at apiVersion 57–65).
    #[test]
    fn complex_flag_below_api_66_stays_a_script_parse_error() {
        let o = Opts { complex: true, computed_ok: false, api_version: 59, unquoted: false };
        let msg = |s: &str| check_opts(s, &o).unwrap().message;
        assert!(msg("{await foo()}").contains("Unexpected end of expression"), "{}", msg("{await foo()}"));
        assert!(msg("{super('duper')}").contains("'super' keyword outside a method"), "{}", msg("{super('duper')}"));
        assert!(msg("{() => doThing(import.meta.env.SSR)}").contains("Cannot use 'import.meta' outside a module"), "{}", msg("{() => doThing(import.meta.env.SSR)}"));
    }
}
