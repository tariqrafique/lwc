//! `@lwc/errors` `compiler/instrumentation.ts` and `@lwc/compiler` `options.ts` deprecation warnings (J3,
//! D-0205 / D-0212). [`CompilerMetric`] and [`DeprecationId`] are generated from the pinned oracle
//! (`tools/gen-instrumentation/gen.mjs`); the workspace's only spelling of their strings (gate:
//! `tests/instrumentation_gate.rs`).
use crate::CompilerDiagnostic;

// Generated (tools/*/gen.mjs, checked byte-for-byte by its --check test): rustfmt must not touch it.
#[rustfmt::skip]
mod generated;
pub use generated::{CompilerMetric, DeprecationId};

/// `@lwc/errors` `InstrumentationObject` for Rust consumers (not exposed over napi: JS callbacks
/// can't cross the JSON options object; D-0205 collects events with [`InstrumentationRecorder`] instead).
pub trait Instrumentation: Send + Sync {
    /// JS `log(errorInfo, config)`. Upstream compilers never call it today; kept for API parity.
    fn log(&self, _diagnostic: &CompilerDiagnostic) {}
    /// JS `incrementCounter(metric)`.
    fn increment_counter(&self, metric: CompilerMetric);
}

/// One `InstrumentationObject` call made during a compile, in call order (D-0205: Rust collects them, the
/// JS loader replays them into the caller's callbacks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstrumentationEvent {
    /// `log(errorInfo, config)`.
    Log(CompilerDiagnostic),
    /// `incrementCounter(metric)`.
    IncrementCounter(CompilerMetric),
}

/// An [`Instrumentation`] that records every call as an [`InstrumentationEvent`].
#[derive(Debug, Default)]
pub struct InstrumentationRecorder(std::sync::Mutex<Vec<InstrumentationEvent>>);

impl InstrumentationRecorder {
    /// The events recorded so far, in call order; leaves the recorder empty.
    pub fn take(&self) -> Vec<InstrumentationEvent> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
    fn push(&self, e: InstrumentationEvent) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(e)
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
    pub fn increment_counter(&self, metric: CompilerMetric) {
        self.0.increment_counter(metric)
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
        h.increment_counter(CompilerMetric::LwcRenderModeDirective);
        h.log(&d);
        h.increment_counter(CompilerMetric::LwcDynamicDirective);
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
}
