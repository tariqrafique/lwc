//! D-0293 (amends D-0205): the caller's JS `InstrumentationObject`, called synchronously from the compile
//! on the JS thread that entered `transform` (no ThreadsafeFunction, no replay). Upstream calls
//! `instrumentation?.incrementCounter(metric)` mid-compile, so a callback that throws stops the compile
//! at that point; [`JsCallbacks`] hands the thrown value to the emit site as
//! [`lwc_shared::CounterError::Threw`] (with the `lwcCode` / `code` / `level` upstream's catch reads) and
//! keeps the raw value for a site with no catch (the SSR template parse), which rethrows it unchanged.
//! [`JsWarn`] is the loader's `console.warn`, called by the validator before anything is compiled
//! (options.ts:185-219, D-0212).
use lwc_shared::{
    CaughtError, CompilerMetric, CounterError, DeprecationId, DiagnosticLevel, JsLevel, JsProp,
    ThrownValue,
};
use napi::sys;
use napi::{Env, JsUnknown, NapiRaw};
use std::ffi::CString;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// A JSON caller's `instrumentation` (no loader): JSON has no functions, so every counter call is
/// upstream's `incrementCounter is not a function`.
pub(crate) struct NotCallable;

impl lwc_shared::Instrumentation for NotCallable {
    fn increment_counter(&self, _: CompilerMetric) -> Result<(), CounterError> {
        Err(CounterError::NotCallable)
    }
}

/// A raw JS value of the current `transform` call (valid only on its thread, during the call).
#[derive(Clone, Copy)]
pub(crate) struct RawValue(pub(crate) sys::napi_value);
// SAFETY: only dereferenced through napi on the thread that created it, while the call is live.
unsafe impl Send for RawValue {}

fn check(status: sys::napi_status, what: &str) -> napi::Result<()> {
    if status == sys::Status::napi_ok {
        Ok(())
    } else {
        Err(napi::Error::new(
            napi::Status::from(status),
            format!("lwc-compiler-napi instrumentation: {what} failed"),
        ))
    }
}

/// napi reads on one `transform` call's env. Callers run on the entering thread during the call.
#[derive(Clone, Copy)]
struct Js(sys::napi_env);

impl Js {
    /// The pending exception, cleared (the raw thrown value; `undefined` if none was pending).
    fn take_exception(self) -> sys::napi_value {
        let mut e = std::ptr::null_mut();
        // SAFETY: on the call's thread during the call (see the type).
        unsafe { sys::napi_get_and_clear_last_exception(self.0, &mut e) };
        if e.is_null() {
            // SAFETY: as above.
            unsafe { sys::napi_get_undefined(self.0, &mut e) };
        }
        e
    }

    /// `obj[name]`; `Err` = the getter threw (the raw exception, cleared).
    fn get(self, obj: sys::napi_value, name: &str) -> Result<sys::napi_value, sys::napi_value> {
        let key = CString::new(name).expect("static property name");
        let mut v = std::ptr::null_mut();
        // SAFETY: as above.
        let st = unsafe { sys::napi_get_named_property(self.0, obj, key.as_ptr(), &mut v) };
        if st == sys::Status::napi_ok {
            Ok(v)
        } else {
            Err(self.take_exception())
        }
    }

    /// `name in obj`; `false` if the check threw (a proxy trap).
    fn has(self, obj: sys::napi_value, name: &str) -> bool {
        let key = self.string(name);
        let mut b = false;
        // SAFETY: as above; `obj` is an object.
        let st = unsafe { sys::napi_has_property(self.0, obj, key, &mut b) };
        if st != sys::Status::napi_ok {
            self.take_exception();
            return false;
        }
        b
    }

    /// `f.call(this, arg)`; `Err` = it threw (the raw exception, cleared).
    fn call(
        self,
        this: sys::napi_value,
        f: sys::napi_value,
        arg: sys::napi_value,
    ) -> Result<sys::napi_value, sys::napi_value> {
        let mut out = std::ptr::null_mut();
        // SAFETY: as above; `f` is a function.
        let st = unsafe { sys::napi_call_function(self.0, this, f, 1, &arg, &mut out) };
        if st == sys::Status::napi_ok {
            Ok(out)
        } else {
            Err(self.take_exception())
        }
    }

    fn type_of(self, v: sys::napi_value) -> i32 {
        let mut t = sys::ValueType::napi_undefined;
        // SAFETY: as above.
        unsafe { sys::napi_typeof(self.0, v, &mut t) };
        t
    }

    fn string(self, s: &str) -> sys::napi_value {
        let mut v = std::ptr::null_mut();
        // SAFETY: as above.
        unsafe { sys::napi_create_string_utf8(self.0, s.as_ptr().cast(), s.len(), &mut v) };
        v
    }

    fn undefined(self) -> sys::napi_value {
        let mut v = std::ptr::null_mut();
        // SAFETY: as above.
        unsafe { sys::napi_get_undefined(self.0, &mut v) };
        v
    }

    fn global(self, name: &str) -> Result<sys::napi_value, sys::napi_value> {
        let mut global = std::ptr::null_mut();
        // SAFETY: as above.
        unsafe { sys::napi_get_global(self.0, &mut global) };
        self.get(global, name)
    }

    /// JS `!!v`.
    fn truthy(self, v: sys::napi_value) -> bool {
        let mut b = std::ptr::null_mut();
        let mut out = false;
        // SAFETY: as above; ToBoolean never throws.
        unsafe {
            sys::napi_coerce_to_bool(self.0, v, &mut b);
            sys::napi_get_value_bool(self.0, b, &mut out);
        }
        out
    }

    /// A JS number's value.
    fn number(self, v: sys::napi_value) -> f64 {
        let mut n = f64::NAN;
        // SAFETY: as above; `v` is a number.
        unsafe { sys::napi_get_value_double(self.0, v, &mut n) };
        n
    }

    /// JS `String(v)` (a symbol prints as `Symbol(desc)`); a throwing `toString` gives `"undefined"`.
    fn js_string(self, v: sys::napi_value) -> String {
        let s = match self.global("String").and_then(|f| self.call(f, f, v)) {
            Ok(s) => s,
            Err(_) => return "undefined".into(),
        };
        let mut len = 0usize;
        // SAFETY: `s` is a JS string (String() always returns one).
        unsafe { sys::napi_get_value_string_utf8(self.0, s, std::ptr::null_mut(), 0, &mut len) };
        let mut buf = vec![0u8; len + 1];
        let mut written = 0usize;
        // SAFETY: `buf` holds len + 1 bytes.
        unsafe {
            sys::napi_get_value_string_utf8(
                self.0,
                s,
                buf.as_mut_ptr().cast(),
                len + 1,
                &mut written,
            )
        };
        buf.truncate(written);
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// `e[name]` as `getCodeFromError` / `convertErrorToDiagnostic` test it (a throwing getter: falsy).
    fn code_prop(self, e: sys::napi_value, name: &str) -> JsProp {
        use sys::ValueType as T;
        let Ok(v) = self.get(e, name) else {
            return JsProp::Falsy;
        };
        if !self.truthy(v) {
            return JsProp::Falsy;
        }
        match self.type_of(v) {
            T::napi_number => {
                let n = self.number(v);
                if n.fract() == 0.0 && n > 0.0 && n <= f64::from(u32::MAX) {
                    // Exact: an integer in u32 range.
                    JsProp::Number(n as u32)
                } else {
                    JsProp::OtherTruthy
                }
            }
            T::napi_string => JsProp::Text(self.js_string(v)),
            _ => JsProp::OtherTruthy,
        }
    }

    /// `'level' in e ? e.level : <absent>`.
    fn level_prop(self, e: sys::napi_value) -> JsLevel {
        if !self.has(e, "level") {
            return JsLevel::Absent;
        }
        let Ok(v) = self.get(e, "level") else {
            return JsLevel::Other;
        };
        if self.type_of(v) != sys::ValueType::napi_number {
            return JsLevel::Other;
        }
        match self.number(v) {
            0.0 => JsLevel::Level(DiagnosticLevel::Fatal),
            1.0 => JsLevel::Level(DiagnosticLevel::Error),
            2.0 => JsLevel::Level(DiagnosticLevel::Warning),
            3.0 => JsLevel::Level(DiagnosticLevel::Log),
            _ => JsLevel::Other,
        }
    }

    /// A thrown value reduced to what upstream's catch sites read.
    fn thrown(self, e: sys::napi_value) -> ThrownValue {
        use sys::ValueType as T;
        match self.type_of(e) {
            T::napi_undefined => ThrownValue::Nullish("undefined"),
            T::napi_null => ThrownValue::Nullish("null"),
            T::napi_object | T::napi_function | T::napi_external => {
                let text = |name| {
                    self.get(e, name)
                        .map(|v| self.js_string(v))
                        .unwrap_or_else(|_| "undefined".into())
                };
                ThrownValue::Object(Box::new(CaughtError {
                    lwc_code: self.code_prop(e, "lwcCode"),
                    code: self.code_prop(e, "code"),
                    name: text("name"),
                    message: text("message"),
                    level: self.level_prop(e),
                    filename: None,
                    location: None,
                }))
            }
            t => ThrownValue::Primitive {
                type_of: match t {
                    T::napi_string => "string",
                    T::napi_number => "number",
                    T::napi_boolean => "boolean",
                    T::napi_symbol => "symbol",
                    _ => "bigint",
                },
                display: self.js_string(e),
            },
        }
    }
}

/// The caller's object for one `transform` call. Holds raw handles of that call's scope: they are used
/// only on the entering thread and only until [`JsCallbacks::end`] (both checked on every call).
pub(crate) struct JsCallbacks {
    js: Js,
    /// `instrumentation` coerced to an object (property reads on a primitive go through its wrapper,
    /// as JS member access does).
    obj: sys::napi_value,
    thread: std::thread::ThreadId,
    live: AtomicBool,
    /// The raw value the last callback threw, for [`lwc_shared::Rethrow::Value`].
    threw: Mutex<Option<RawValue>>,
}

// SAFETY: every method that touches `js` / `obj` first checks it runs on `thread` while `live`.
unsafe impl Send for JsCallbacks {}
// SAFETY: as above; the handles are never used concurrently (one JS thread).
unsafe impl Sync for JsCallbacks {}

impl JsCallbacks {
    pub(crate) fn new(env: Env, value: JsUnknown) -> napi::Result<std::sync::Arc<Self>> {
        let env = env.raw();
        let mut obj = std::ptr::null_mut();
        // SAFETY: `env` and `value` come from the current napi callback.
        check(
            unsafe { sys::napi_coerce_to_object(env, value.raw(), &mut obj) },
            "coerce instrumentation to object",
        )?;
        Ok(std::sync::Arc::new(Self {
            js: Js(env),
            obj,
            thread: std::thread::current().id(),
            live: AtomicBool::new(true),
            threw: Mutex::new(None),
        }))
    }

    /// End of the `transform` call: the handles go stale. Returns the raw value the last callback threw.
    pub(crate) fn end(&self) -> Option<RawValue> {
        self.live.store(false, Ordering::SeqCst);
        self.threw.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    fn usable(&self) -> bool {
        self.live.load(Ordering::SeqCst) && std::thread::current().id() == self.thread
    }
}

impl lwc_shared::Instrumentation for JsCallbacks {
    /// `instrumentation?.incrementCounter(metric)`, called now (`this` = the object, read as upstream's
    /// optional call does); a throw ends the compile at this metric.
    fn increment_counter(&self, metric: CompilerMetric) -> Result<(), CounterError> {
        if !self.usable() {
            return Err(CounterError::Threw(ThrownValue::Object(Box::new(
                CaughtError::plain(
                    "Error",
                    "lwc-compiler-napi: instrumentation used outside its transform call",
                ),
            ))));
        }
        let js = self.js;
        let threw = |v| {
            *self.threw.lock().unwrap_or_else(|e| e.into_inner()) = Some(RawValue(v));
            CounterError::Threw(js.thrown(v))
        };
        let f = js.get(self.obj, "incrementCounter").map_err(threw)?;
        if js.type_of(f) != sys::ValueType::napi_function {
            return Err(CounterError::NotCallable);
        }
        js.call(self.obj, f, js.string(metric.js_value()))
            .map(|_| ())
            .map_err(threw)
    }
}

/// The loader's `warn(message)` (wrap.js: upstream's `console.warn`), called by the validator. The first
/// throw stops further warns and is rethrown before anything is compiled, as upstream's would leave
/// `validateTransformOptions`.
pub(crate) struct JsWarn {
    js: Js,
    f: Option<sys::napi_value>,
    threw: Option<RawValue>,
}

impl JsWarn {
    /// `warn`: the loader's function; `None` (a direct binding caller) prints nothing.
    pub(crate) fn new(env: Env, warn: Option<JsUnknown>) -> Self {
        let js = Js(env.raw());
        // SAFETY: `warn` comes from the current napi callback.
        let f = warn
            .map(|w| unsafe { w.raw() })
            .filter(|&f| js.type_of(f) == sys::ValueType::napi_function);
        Self { js, f, threw: None }
    }

    pub(crate) fn warn(&mut self, d: DeprecationId) {
        let Some(f) = self.f else { return };
        if self.threw.is_some() {
            return;
        }
        let js = self.js;
        if let Err(v) = js.call(js.undefined(), f, js.string(d.message())) {
            self.threw = Some(RawValue(v));
        }
    }

    /// The value a warn threw, if any.
    pub(crate) fn threw(&self) -> Option<RawValue> {
        self.threw
    }
}

/// Throw `v` (a raw value of this call) as the binding's exception.
pub(crate) fn throw_raw(env: Env, v: RawValue) -> napi::Error {
    // SAFETY: `v` belongs to the current `transform` call on this thread.
    if let Err(e) = check(unsafe { sys::napi_throw(env.raw(), v.0) }, "rethrow") {
        return e;
    }
    napi::Error::new(napi::Status::PendingException, "rethrown JS value")
}
