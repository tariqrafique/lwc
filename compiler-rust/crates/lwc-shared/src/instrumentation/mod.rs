//! `@lwc/errors` `compiler/instrumentation.ts` and `@lwc/compiler` `options.ts` deprecation warnings (J3,
//! D-0205 / D-0212). [`CompilerMetric`] and [`DeprecationId`] are generated from the pinned oracle
//! (`tools/gen-instrumentation/gen.mjs`); the workspace's only spelling of their strings (gate:
//! `tests/instrumentation_gate.rs`).
use crate::CompilerDiagnostic;
use crate::errors::{CaughtError, JsProp, ParserDiagnostics, convert_error_to_diagnostic};

// Generated (tools/*/gen.mjs, checked byte-for-byte by its --check test): rustfmt must not touch it.
#[rustfmt::skip]
mod generated;
pub use generated::{CompilerMetric, DeprecationId};

/// `@lwc/errors` `InstrumentationObject`, as the compilers use it: they only call
/// `instrumentation?.incrementCounter(metric)` (never `log`). Over napi the binding implements it by
/// calling the caller's JS callback synchronously, on the JS thread, during the compile (D-0293), so a
/// callback that throws stops the compile where upstream's does.
pub trait Instrumentation: Send + Sync {
    /// `instrumentation?.incrementCounter(metric)`. `Err` where the JS call throws: the object has no
    /// callable `incrementCounter`, or the callback threw. A Rust implementation returns `Ok`.
    fn increment_counter(&self, metric: CompilerMetric) -> Result<(), CounterError>;
}

/// Upstream `instrumentation?.incrementCounter(metric)` threw. Each emit site maps it to what upstream
/// surfaces there ([`CounterError::template_catch`], [`CounterError::babel_catch`],
/// [`CounterError::rethrow`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterError {
    /// `TypeError: ... is not a function` (`instrumentation: {}`, `incrementCounter: 1`, a primitive).
    NotCallable,
    /// The callback (or an `incrementCounter` getter) threw this value.
    Threw(ThrownValue),
}

/// A JS value thrown by an instrumentation callback, reduced to what upstream's catch sites read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThrownValue {
    /// An object or function: the properties `@lwc/errors` and Babel's catch read off it.
    /// Boxed: it is the rare path, and it keeps every `Result<_, CounterError>` small.
    Object(Box<CaughtError>),
    /// `undefined` or `null` (the JS spelling).
    Nullish(&'static str),
    /// A string, number, boolean, bigint or symbol: its `typeof` and `String(value)`.
    Primitive {
        type_of: &'static str,
        display: String,
    },
}

/// How upstream's template-compiler `compile` catch (index.ts:76-79,
/// `normalizeToDiagnostic(GENERIC_PARSING_ERROR, e)`) treats a counter throw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateCatch {
    /// Normalized: `compile` pushes this diagnostic (prefixed `Unexpected compilation error: `).
    Diagnostic(CompilerDiagnostic),
    /// `normalizeToDiagnostic` itself throws (it reads `e.lwcCode` and `'level' in e`), so this
    /// `TypeError` escapes the template compiler; @lwc/compiler then wraps it as LWC1008.
    Escapes { name: &'static str, message: String },
}

/// A throw that leaves `transformSync` untouched: a counter throw under targetSSR (ssr-compiler
/// compile-template/index.ts:83-95 calls template-compiler `parse` (index.ts:33-38), which has no catch,
/// and no facade normalizes it), and every throw out of `validateTransformOptions` that is not an
/// `invariant` (@lwc/compiler transformer.ts:82 runs it outside any try).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rethrow {
    /// A `TypeError` the JS engine itself raised, with this message: calling a non-callable
    /// `incrementCounter`, or reading a property of an explicit `null` option (`outputConfig: null`).
    TypeError(&'static str),
    /// A caller callback (`incrementCounter`, or the loader's `console.warn`) threw this value; the
    /// binding rethrows the original JS value itself.
    Value(ThrownValue),
}

/// The `code` @babel/core's transform catch assigns when the thrown error has a falsy one.
pub const BABEL_TRANSFORM_ERROR: &str = "BABEL_TRANSFORM_ERROR";

impl CounterError {
    /// The template-compiler `compile` catch. `not_a_function` is the emit site's TypeError text.
    pub fn template_catch(&self, not_a_function: &str) -> TemplateCatch {
        let normalized = |e: &CaughtError| {
            let mut d =
                convert_error_to_diagnostic(e, &ParserDiagnostics::GENERIC_PARSING_ERROR, None);
            d.message = format!("Unexpected compilation error: {}", d.message);
            TemplateCatch::Diagnostic(d)
        };
        match self {
            CounterError::NotCallable => {
                normalized(&CaughtError::plain("TypeError", not_a_function))
            }
            CounterError::Threw(ThrownValue::Object(e)) => normalized(e),
            // getCodeFromError reads `error.lwcCode` first.
            CounterError::Threw(ThrownValue::Nullish(n)) => TemplateCatch::Escapes {
                name: "TypeError",
                message: format!("Cannot read properties of {n} (reading 'lwcCode')"),
            },
            // convertErrorToDiagnostic: `'level' in error` on a primitive.
            CounterError::Threw(ThrownValue::Primitive { display, .. }) => TemplateCatch::Escapes {
                name: "TypeError",
                message: format!("Cannot use 'in' operator to search for 'level' in {display}"),
            },
        }
    }

    /// The value leaving Babel when a plugin visitor's counter call throws: @babel/core's transform catch
    /// runs `e.message = `${filename}: ${e.message}`` then `if (!e.code) e.code = "BABEL_TRANSFORM_ERROR"`
    /// (strict mode). @lwc/compiler then normalizes it (javascript.ts:121). `not_a_function` is the emit
    /// site's TypeError text.
    pub fn babel_catch(&self, not_a_function: &str, filename: &str) -> CaughtError {
        let caught = |e: &CaughtError| {
            let mut e = e.clone();
            e.message = format!("{filename}: {}", e.message);
            if e.code == JsProp::Falsy {
                e.code = JsProp::Text(BABEL_TRANSFORM_ERROR.into());
            }
            e
        };
        match self {
            CounterError::NotCallable => caught(&CaughtError::plain("TypeError", not_a_function)),
            CounterError::Threw(ThrownValue::Object(e)) => caught(e),
            // Reading `e.message` on undefined / null throws from inside Babel's catch.
            CounterError::Threw(ThrownValue::Nullish(n)) => CaughtError::plain(
                "TypeError",
                &format!("Cannot read properties of {n} (reading 'message')"),
            ),
            // Assigning `e.message` on a primitive throws in strict mode.
            CounterError::Threw(ThrownValue::Primitive { type_of, display }) => CaughtError::plain(
                "TypeError",
                &format!("Cannot create property 'message' on {type_of} '{display}'"),
            ),
        }
    }

    /// No catch between the emit site and the caller (the SSR template parse).
    pub fn rethrow(self, not_a_function: &'static str) -> Rethrow {
        match self {
            CounterError::NotCallable => Rethrow::TypeError(not_a_function),
            CounterError::Threw(v) => Rethrow::Value(v),
        }
    }
}

/// Shared handle so options stay `Clone + Debug + Eq` (equality is pointer identity).
#[derive(Clone)]
pub struct InstrumentationHandle(pub std::sync::Arc<dyn Instrumentation>);

impl InstrumentationHandle {
    pub fn new(i: impl Instrumentation + 'static) -> Self {
        Self(std::sync::Arc::new(i))
    }
    /// `instrumentation?.incrementCounter(metric)`: `Err` where upstream's call throws.
    pub fn increment_counter(&self, metric: CompilerMetric) -> Result<(), CounterError> {
        self.0.increment_counter(metric)
    }
}

impl std::fmt::Debug for InstrumentationHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InstrumentationHandle(..)")
    }
}
impl PartialEq for InstrumentationHandle {
    fn eq(&self, o: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &o.0)
    }
}
impl Eq for InstrumentationHandle {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DiagnosticLevel;
    use crate::errors::JsLevel;

    #[test]
    fn js_value_round_trips() {
        for &m in CompilerMetric::ALL {
            assert_eq!(CompilerMetric::from_js(m.js_value()), Some(m));
        }
        assert_eq!(CompilerMetric::from_js("nope"), None);
    }

    fn object(message: &str) -> CaughtError {
        CaughtError::plain("Error", message)
    }

    /// Oracle (scratch/harness/ab-0008-r4-oracle2.log, ab-0008-r5-oracle.log): what each catch site makes
    /// of a thrown value.
    #[test]
    fn counter_throw_maps_to_upstream_catch_sites() {
        let obj = CounterError::Threw(ThrownValue::Object(Box::new(object("boom"))));
        let prim = |t, d: &str| {
            CounterError::Threw(ThrownValue::Primitive {
                type_of: t,
                display: d.into(),
            })
        };
        let diag = |c: &CounterError| match c.template_catch("nf") {
            TemplateCatch::Diagnostic(d) => (d.code, d.message, d.level),
            o => panic!("{o:?}"),
        };
        assert_eq!(
            diag(&obj),
            (
                Some(1052),
                "Unexpected compilation error: LWC1052: Error parsing attribute: boom".into(),
                DiagnosticLevel::Error
            )
        );
        assert_eq!(
            diag(&CounterError::NotCallable).1,
            "Unexpected compilation error: LWC1052: Error parsing attribute: nf"
        );
        assert_eq!(
            CounterError::Threw(ThrownValue::Nullish("null")).template_catch("nf"),
            TemplateCatch::Escapes {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'lwcCode')".into()
            }
        );
        assert_eq!(
            prim("string", "strboom").template_catch("nf"),
            TemplateCatch::Escapes {
                name: "TypeError",
                message: "Cannot use 'in' operator to search for 'level' in strboom".into()
            }
        );
        let e = obj.babel_catch("nf", "a.js");
        assert_eq!(
            (e.name.as_str(), e.message.as_str(), &e.code),
            (
                "Error",
                "a.js: boom",
                &JsProp::Text(BABEL_TRANSFORM_ERROR.into())
            )
        );
        let e = prim("bigint", "10").babel_catch("nf", "a.js");
        assert_eq!(
            (e.name.as_str(), e.message.as_str(), &e.code),
            (
                "TypeError",
                "Cannot create property 'message' on bigint '10'",
                &JsProp::Falsy
            )
        );
        let e = CounterError::Threw(ThrownValue::Nullish("undefined")).babel_catch("nf", "a.js");
        assert_eq!(
            e.message,
            "Cannot read properties of undefined (reading 'message')"
        );
        assert_eq!(
            CounterError::NotCallable.rethrow("nf"),
            Rethrow::TypeError("nf")
        );
    }

    /// The template catch on coded thrown objects (oracle: scratch/harness/ab-0008-r5-oracle.log, CSR-rm rows).
    #[test]
    fn template_catch_reads_lwc_code_code_and_level() {
        let diag = |e: CaughtError| match CounterError::Threw(ThrownValue::Object(Box::new(e)))
            .template_catch("nf")
        {
            TemplateCatch::Diagnostic(d) => (d.code, d.message, d.level),
            o => panic!("{o:?}"),
        };
        let lwc = CaughtError {
            lwc_code: JsProp::Number(1234),
            ..object("m")
        };
        assert_eq!(
            diag(lwc),
            (
                Some(1234),
                "Unexpected compilation error: m".into(),
                DiagnosticLevel::Error
            )
        );
        let code = CaughtError {
            code: JsProp::Number(1234),
            ..object("m")
        };
        assert_eq!(
            diag(code),
            (
                Some(1234),
                "Unexpected compilation error: LWC1052: Error parsing attribute: m".into(),
                DiagnosticLevel::Error
            )
        );
        let level0 = CaughtError {
            level: JsLevel::Level(DiagnosticLevel::Fatal),
            ..object("m")
        };
        assert_eq!(diag(level0).2, DiagnosticLevel::Fatal);
        // A truthy non-number lwcCode keeps the raw message but not the code.
        let lwc_str = CaughtError {
            lwc_code: JsProp::Text("x".into()),
            ..object("m")
        };
        assert_eq!(
            diag(lwc_str),
            (
                Some(1052),
                "Unexpected compilation error: m".into(),
                DiagnosticLevel::Error
            )
        );
    }
}
