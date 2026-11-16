//! `@lwc/errors` `compiler/instrumentation.ts` and `@lwc/compiler` `options.ts` deprecation warnings (J3,
//! D-0205 / D-0212). [`CompilerMetric`] and [`DeprecationId`] are generated from the pinned oracle
//! (`tools/gen-instrumentation/gen.mjs`); the workspace's only spelling of their strings (gate:
//! `tests/instrumentation_gate.rs`).
use crate::CompilerDiagnostic;

// Generated (tools/*/gen.mjs, checked byte-for-byte by its --check test): rustfmt must not touch it.
#[rustfmt::skip]
mod generated;
pub use generated::{CompilerMetric, DeprecationId};

/// `@lwc/errors` `InstrumentationObject`. Rust consumers implement it directly; over napi JS callbacks
/// can't cross the JSON options object, so D-0205 collects events with [`InstrumentationRecorder`] and
/// the loader (`wrap.js`) replays them into the caller's callbacks (AB-0008 G2b).
pub trait Instrumentation: Send + Sync {
    /// JS `log(errorInfo, config)`. Upstream compilers never call it today; kept for API parity.
    fn log(&self, _diagnostic: &CompilerDiagnostic) {}
    /// JS `incrementCounter(metric)`.
    fn increment_counter(&self, metric: CompilerMetric);
    /// `false` when the JS `InstrumentationObject` is present but `incrementCounter` is not a
    /// function (`instrumentation: {}`): upstream's `instrumentation?.incrementCounter(m)` then throws
    /// a TypeError at the first metric. Rust implementations are always callable.
    fn counter_callable(&self) -> bool {
        true
    }
}

/// Upstream `instrumentation?.incrementCounter(metric)` threw `TypeError: ... is not a function`;
/// each emit site maps it to the error upstream surfaces there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterNotCallable;

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
pub struct InstrumentationRecorder {
    events: std::sync::Mutex<Vec<InstrumentationEvent>>,
    /// The JS object had no callable `incrementCounter` (see [`Instrumentation::counter_callable`]).
    counter_missing: bool,
}

impl InstrumentationRecorder {
    /// A recorder for a JS `InstrumentationObject` whose `incrementCounter` is (`true`) or is not a function.
    pub fn for_js(counter_callable: bool) -> Self {
        Self {
            events: std::sync::Mutex::default(),
            counter_missing: !counter_callable,
        }
    }
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
    fn counter_callable(&self) -> bool {
        !self.counter_missing
    }
}

/// Shared handle so options stay `Clone + Debug + Eq` (equality is pointer identity).
#[derive(Clone)]
pub struct InstrumentationHandle(pub std::sync::Arc<dyn Instrumentation>);

impl InstrumentationHandle {
    pub fn new(i: impl Instrumentation + 'static) -> Self {
        Self(std::sync::Arc::new(i))
    }
    /// `instrumentation?.incrementCounter(metric)`: `Err` (nothing recorded) when the JS object has no
    /// callable `incrementCounter`, where upstream throws.
    pub fn increment_counter(&self, metric: CompilerMetric) -> Result<(), CounterNotCallable> {
        if !self.0.counter_callable() {
            return Err(CounterNotCallable);
        }
        self.0.increment_counter(metric);
        Ok(())
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

    #[test]
    fn missing_js_counter_errs_and_records_nothing() {
        let rec = std::sync::Arc::new(InstrumentationRecorder::for_js(false));
        let h = InstrumentationHandle(rec.clone());
        assert_eq!(
            h.increment_counter(CompilerMetric::LwcSpreadDirective),
            Err(CounterNotCallable)
        );
        assert!(rec.take().is_empty());
        let rec = std::sync::Arc::new(InstrumentationRecorder::for_js(true));
        let h = InstrumentationHandle(rec.clone());
        assert_eq!(
            h.increment_counter(CompilerMetric::LwcSpreadDirective),
            Ok(())
        );
        assert_eq!(
            rec.take(),
            vec![InstrumentationEvent::IncrementCounter(
                CompilerMetric::LwcSpreadDirective
            )]
        );
    }
}
