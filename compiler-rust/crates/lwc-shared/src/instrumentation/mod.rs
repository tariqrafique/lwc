//! `@lwc/errors` `compiler/instrumentation.ts` and `@lwc/compiler` `options.ts` deprecation warnings (J3,
//! D-0205 / D-0212). [`CompilerMetric`] and [`DeprecationId`] are generated from the pinned oracle
//! (`tools/gen-instrumentation/gen.mjs`); the workspace's only spelling of their strings (gate:
//! `tests/instrumentation_gate.rs`).
use crate::CompilerDiagnostic;

// Generated (tools/*/gen.mjs, checked byte-for-byte by its --check test): rustfmt must not touch it.
#[rustfmt::skip]
mod generated;
pub use generated::{CompilerMetric, DeprecationId};

/// `@lwc/errors` `InstrumentationObject`. Rust consumers implement it directly. Over napi the binding
/// implements it by calling the caller's JS callbacks synchronously, on the JS thread, during the
/// compile (AB-0008 G2b r4; no ThreadsafeFunction), so a callback that throws stops the compile where
/// upstream's does.
pub trait Instrumentation: Send + Sync {
    /// JS `log(errorInfo, config)`. Upstream compilers never call it today; kept for API parity.
    fn log(&self, _diagnostic: &CompilerDiagnostic) {}
    /// JS `incrementCounter(metric)`.
    fn increment_counter(&self, metric: CompilerMetric);
    /// Upstream's `instrumentation?.incrementCounter(metric)`, which can throw: the JS object has no
    /// callable `incrementCounter`, or the callback threw. Rust implementations never fail.
    fn try_increment_counter(&self, metric: CompilerMetric) -> Result<(), CounterError> {
        self.increment_counter(metric);
        Ok(())
    }
}

/// Upstream `instrumentation?.incrementCounter(metric)` threw. Each emit site maps it to the error
/// upstream surfaces there ([`CounterError::template_catch`], [`CounterError::babel_catch`]).
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
    /// An object or function: `String(e.name)` and `String(e.message)` (`"undefined"` when absent).
    Object { name: String, message: String },
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
    /// Normalized: the template's only diagnostic is LWC1052 with this message argument.
    Generic(String),
    /// `normalizeToDiagnostic` itself throws (it reads `e.lwcCode` and `'level' in e`), so this
    /// `TypeError` escapes the template compiler; @lwc/compiler then wraps it as LWC1008.
    Escapes { name: &'static str, message: String },
}

impl CounterError {
    /// The template-compiler `compile` catch. `not_a_function` is the emit site's TypeError text.
    pub fn template_catch(&self, not_a_function: &str) -> TemplateCatch {
        match self {
            CounterError::NotCallable => TemplateCatch::Generic(not_a_function.to_string()),
            CounterError::Threw(ThrownValue::Object { message, .. }) => {
                TemplateCatch::Generic(message.clone())
            }
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

    /// The error leaving Babel when a plugin visitor's counter call throws: @babel/core's transform
    /// catch runs `e.message = `${filename}: ${e.message}`` (strict mode), then @lwc/compiler normalizes
    /// it as LWC1007. `not_a_function` is the emit site's TypeError text.
    pub fn babel_catch(&self, not_a_function: &str, filename: &str) -> crate::CompileError {
        match self {
            CounterError::NotCallable => {
                crate::CompileError::babel("TypeError", not_a_function, filename, None, None)
            }
            CounterError::Threw(ThrownValue::Object { name, message }) => {
                crate::CompileError::babel(name, message.as_str(), filename, None, None)
            }
            // Reading `e.message` on undefined / null throws from inside Babel's catch.
            CounterError::Threw(ThrownValue::Nullish(n)) => crate::CompileError::babel_unprefixed(
                "TypeError",
                format!("Cannot read properties of {n} (reading 'message')"),
                filename,
            ),
            // Assigning `e.message` on a primitive throws in strict mode.
            CounterError::Threw(ThrownValue::Primitive { type_of, display }) => {
                crate::CompileError::babel_unprefixed(
                    "TypeError",
                    format!("Cannot create property 'message' on {type_of} '{display}'"),
                    filename,
                )
            }
        }
    }
}

/// One `InstrumentationObject` call made during a compile, in call order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstrumentationEvent {
    /// `log(errorInfo, config)`.
    Log(CompilerDiagnostic),
    /// `incrementCounter(metric)`.
    IncrementCounter(CompilerMetric),
}

/// An [`Instrumentation`] that records every call as an [`InstrumentationEvent`] (Rust callers and tests).
#[derive(Debug, Default)]
pub struct InstrumentationRecorder {
    events: std::sync::Mutex<Vec<InstrumentationEvent>>,
}

impl InstrumentationRecorder {
    /// The events recorded so far, in call order; leaves the recorder empty.
    pub fn take(&self) -> Vec<InstrumentationEvent> {
        std::mem::take(&mut *self.events.lock().unwrap_or_else(|e| e.into_inner()))
    }
    fn push(&self, e: InstrumentationEvent) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(e)
    }
}

impl Instrumentation for InstrumentationRecorder {
    fn log(&self, d: &CompilerDiagnostic) {
        self.push(InstrumentationEvent::Log(d.clone()))
    }
    fn increment_counter(&self, m: CompilerMetric) {
        self.push(InstrumentationEvent::IncrementCounter(m))
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
        self.0.try_increment_counter(metric)
    }
    pub fn log(&self, d: &CompilerDiagnostic) {
        self.0.log(d)
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

    #[test]
    fn js_value_round_trips() {
        for &m in CompilerMetric::ALL {
            assert_eq!(CompilerMetric::from_js(m.js_value()), Some(m));
        }
        assert_eq!(CompilerMetric::from_js("nope"), None);
    }

    #[test]
    fn recorder_keeps_call_order_through_the_handle() {
        let rec = std::sync::Arc::new(InstrumentationRecorder::default());
        let h = InstrumentationHandle(rec.clone());
        let d = CompilerDiagnostic::new(Some(1187), "m", DiagnosticLevel::Warning, None, None);
        h.increment_counter(CompilerMetric::LwcRenderModeDirective)
            .unwrap();
        h.log(&d);
        h.increment_counter(CompilerMetric::LwcDynamicDirective)
            .unwrap();
        assert_eq!(
            rec.take(),
            vec![
                InstrumentationEvent::IncrementCounter(CompilerMetric::LwcRenderModeDirective),
                InstrumentationEvent::Log(d),
                InstrumentationEvent::IncrementCounter(CompilerMetric::LwcDynamicDirective),
            ]
        );
        assert!(rec.take().is_empty());
    }

    /// Oracle (scratch/harness/ab-0008-r4-oracle2.log): what each catch site makes of a thrown value.
    #[test]
    fn counter_throw_maps_to_upstream_catch_sites() {
        let obj = CounterError::Threw(ThrownValue::Object {
            name: "Error".into(),
            message: "boom".into(),
        });
        let s = |v: &str| v.to_string();
        let prim = |t, d: &str| {
            CounterError::Threw(ThrownValue::Primitive {
                type_of: t,
                display: d.into(),
            })
        };
        assert_eq!(obj.template_catch("nf"), TemplateCatch::Generic(s("boom")));
        assert_eq!(
            CounterError::NotCallable.template_catch("nf"),
            TemplateCatch::Generic(s("nf"))
        );
        assert_eq!(
            CounterError::Threw(ThrownValue::Nullish("null")).template_catch("nf"),
            TemplateCatch::Escapes {
                name: "TypeError",
                message: s("Cannot read properties of null (reading 'lwcCode')")
            }
        );
        assert_eq!(
            prim("string", "strboom").template_catch("nf"),
            TemplateCatch::Escapes {
                name: "TypeError",
                message: s("Cannot use 'in' operator to search for 'level' in strboom")
            }
        );
        let e = obj.babel_catch("nf", "a.js");
        assert_eq!(
            (e.name.as_deref(), e.message.as_str()),
            (Some("Error"), "a.js: boom")
        );
        let e = prim("bigint", "10").babel_catch("nf", "a.js");
        assert_eq!(
            (e.name.as_deref(), e.message.as_str()),
            (
                Some("TypeError"),
                "Cannot create property 'message' on bigint '10'"
            )
        );
        let e = CounterError::Threw(ThrownValue::Nullish("undefined")).babel_catch("nf", "a.js");
        assert_eq!(
            e.message,
            "Cannot read properties of undefined (reading 'message')"
        );
    }
}
