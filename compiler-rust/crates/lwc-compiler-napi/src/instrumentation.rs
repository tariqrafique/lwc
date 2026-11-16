//! D-0205 / AB-0008 G2b r4: the caller's JS `InstrumentationObject`, called synchronously from the
//! compile on the JS thread that entered `transform` (no ThreadsafeFunction, no replay). Upstream calls
//! `instrumentation?.incrementCounter(metric)` mid-compile, so a callback that throws stops the compile
//! at that point; [`JsCallbacks`] hands the thrown value to the emit site as
//! [`lwc_shared::CounterError::Threw`], which maps it to the error upstream's catch produces there.
use lwc_shared::{CompilerDiagnostic, CompilerMetric, CounterError, ThrownValue};
use napi::sys;
use napi::{Env, JsUnknown, NapiRaw};
use std::ffi::CString;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// A JSON caller's `instrumentation` (no loader): JSON has no functions, so every counter call is
/// upstream's `incrementCounter is not a function`.
pub(crate) struct NotCallable;

impl lwc_shared::Instrumentation for NotCallable {
    fn increment_counter(&self, _: CompilerMetric) {}
    fn try_increment_counter(&self, _: CompilerMetric) -> Result<(), CounterError> {
        Err(CounterError::NotCallable)
    }
}

/// The caller's object for one `transform` call. Holds raw handles of that call's scope: they are used
/// only on the entering thread and only until [`JsCallbacks::end`] (both checked on every call).
pub(crate) struct JsCallbacks {
    env: sys::napi_env,
    /// `instrumentation` coerced to an object (property reads on a primitive go through its wrapper,
    /// as JS member access does).
    obj: sys::napi_value,
    thread: std::thread::ThreadId,
    live: AtomicBool,
    /// A `log` callback threw: rethrown unchanged when the call ends. Upstream compilers never call
    /// `log` (kept for API parity), so no compile path reaches this today.
    log_threw: Mutex<Option<RawValue>>,
}

#[derive(Clone, Copy)]
struct RawValue(sys::napi_value);
// SAFETY: only dereferenced through napi on the thread that created it, while the call is live.
unsafe impl Send for RawValue {}

// SAFETY: every method that touches `env` / `obj` first checks it runs on `thread` while `live`.
unsafe impl Send for JsCallbacks {}
// SAFETY: as above; the handles are never used concurrently (one JS thread).
unsafe impl Sync for JsCallbacks {}

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
            env,
            obj,
            thread: std::thread::current().id(),
            live: AtomicBool::new(true),
            log_threw: Mutex::new(None),
        }))
    }

    /// End of the `transform` call: the handles go stale. Rethrows a `log` throw unchanged.
    pub(crate) fn end(&self) -> napi::Result<()> {
        self.live.store(false, Ordering::SeqCst);
        let pending = self
            .log_threw
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(RawValue(v)) = pending {
            // SAFETY: still inside the `transform` callback on its thread.
            check(unsafe { sys::napi_throw(self.env, v) }, "rethrow")?;
            return Err(napi::Error::new(
                napi::Status::PendingException,
                "instrumentation log threw",
            ));
        }
        Ok(())
    }

    fn usable(&self) -> bool {
        self.live.load(Ordering::SeqCst) && std::thread::current().id() == self.thread
    }

    /// The pending exception, cleared (the raw thrown value; `undefined` if none was pending).
    fn take_exception(&self) -> sys::napi_value {
        let mut e = std::ptr::null_mut();
        // SAFETY: usable() was checked by the caller.
        unsafe { sys::napi_get_and_clear_last_exception(self.env, &mut e) };
        if e.is_null() {
            // SAFETY: as above.
            unsafe { sys::napi_get_undefined(self.env, &mut e) };
        }
        e
    }

    /// `obj[name]`; `Err` = the getter threw (the raw exception, cleared).
    fn get(&self, obj: sys::napi_value, name: &str) -> Result<sys::napi_value, sys::napi_value> {
        let key = CString::new(name).expect("static property name");
        let mut v = std::ptr::null_mut();
        // SAFETY: usable() was checked by the caller.
        let st = unsafe { sys::napi_get_named_property(self.env, obj, key.as_ptr(), &mut v) };
        if st == sys::Status::napi_ok {
            Ok(v)
        } else {
            Err(self.take_exception())
        }
    }

    /// `f.call(this, arg)`; `Err` = it threw (the raw exception, cleared).
    fn call(
        &self,
        this: sys::napi_value,
        f: sys::napi_value,
        arg: sys::napi_value,
    ) -> Result<sys::napi_value, sys::napi_value> {
        let mut out = std::ptr::null_mut();
        // SAFETY: usable() was checked by the caller; `f` is a function.
        let st = unsafe { sys::napi_call_function(self.env, this, f, 1, &arg, &mut out) };
        if st == sys::Status::napi_ok {
            Ok(out)
        } else {
            Err(self.take_exception())
        }
    }

    fn type_of(&self, v: sys::napi_value) -> i32 {
        let mut t = sys::ValueType::napi_undefined;
        // SAFETY: usable() was checked by the caller.
        unsafe { sys::napi_typeof(self.env, v, &mut t) };
        t
    }

    /// `obj[name](arg)` with `this = obj`, read now as upstream's optional call does. `Ok(false)`: the
    /// property is not a function.
    fn call_method(&self, name: &str, arg: sys::napi_value) -> Result<bool, sys::napi_value> {
        let f = self.get(self.obj, name)?;
        if self.type_of(f) != sys::ValueType::napi_function {
            return Ok(false);
        }
        self.call(self.obj, f, arg).map(|_| true)
    }

    fn string(&self, s: &str) -> sys::napi_value {
        let mut v = std::ptr::null_mut();
        // SAFETY: usable() was checked by the caller.
        unsafe { sys::napi_create_string_utf8(self.env, s.as_ptr().cast(), s.len(), &mut v) };
        v
    }

    fn global(&self, name: &str) -> Result<sys::napi_value, sys::napi_value> {
        let mut global = std::ptr::null_mut();
        // SAFETY: usable() was checked by the caller.
        unsafe { sys::napi_get_global(self.env, &mut global) };
        self.get(global, name)
    }

    /// A thrown value reduced to what upstream's catch sites read.
    fn thrown(&self, e: sys::napi_value) -> ThrownValue {
        use sys::ValueType as T;
        match self.type_of(e) {
            T::napi_undefined => ThrownValue::Nullish("undefined"),
            T::napi_null => ThrownValue::Nullish("null"),
            T::napi_object | T::napi_function | T::napi_external => {
                let prop = |name| {
                    self.get(e, name)
                        .map(|v| self.js_string(v))
                        .unwrap_or_else(|_| "undefined".into())
                };
                ThrownValue::Object {
                    name: prop("name"),
                    message: prop("message"),
                }
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

    /// JS `String(v)` (a symbol prints as `Symbol(desc)`); a throwing `toString` gives `"undefined"`.
    fn js_string(&self, v: sys::napi_value) -> String {
        let s = match self.global("String").and_then(|f| self.call(f, f, v)) {
            Ok(s) => s,
            Err(_) => return "undefined".into(),
        };
        let mut len = 0usize;
        // SAFETY: `s` is a JS string (String() always returns one).
        unsafe { sys::napi_get_value_string_utf8(self.env, s, std::ptr::null_mut(), 0, &mut len) };
        let mut buf = vec![0u8; len + 1];
        let mut written = 0usize;
        // SAFETY: `buf` holds len + 1 bytes.
        unsafe {
            sys::napi_get_value_string_utf8(
                self.env,
                s,
                buf.as_mut_ptr().cast(),
                len + 1,
                &mut written,
            )
        };
        buf.truncate(written);
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// `JSON.parse(json)` (the diagnostic object handed to `log`).
    fn parse_json(&self, json: &str) -> Result<sys::napi_value, sys::napi_value> {
        let j = self.global("JSON")?;
        let parse = self.get(j, "parse")?;
        self.call(j, parse, self.string(json))
    }
}

impl lwc_shared::Instrumentation for JsCallbacks {
    fn increment_counter(&self, metric: CompilerMetric) {
        let _ = self.try_increment_counter(metric);
    }

    /// `instrumentation?.incrementCounter(metric)`, called now; a throw ends the compile at this metric.
    fn try_increment_counter(&self, metric: CompilerMetric) -> Result<(), CounterError> {
        if !self.usable() {
            return Err(CounterError::Threw(ThrownValue::Object {
                name: "Error".into(),
                message: "lwc-compiler-napi: instrumentation used outside its transform call"
                    .into(),
            }));
        }
        let arg = self.string(metric.js_value());
        match self.call_method("incrementCounter", arg) {
            Ok(true) => Ok(()),
            Ok(false) => Err(CounterError::NotCallable),
            Err(v) => Err(CounterError::Threw(self.thrown(v))),
        }
    }

    /// `log(diagnostic)` when it is a function, with the warning shape `{ code, level, message, filename }`.
    /// A throw is kept and rethrown unchanged when the call ends ([`JsCallbacks::end`]).
    fn log(&self, d: &CompilerDiagnostic) {
        if !self.usable() {
            return;
        }
        let json = serde_json::json!({
            "code": d.code,
            "level": d.level.js_value(),
            "message": d.message,
            "filename": d.filename,
        })
        .to_string();
        let r = self
            .parse_json(&json)
            .and_then(|arg| self.call_method("log", arg));
        if let Err(v) = r {
            let mut slot = self.log_threw.lock().unwrap_or_else(|e| e.into_inner());
            slot.get_or_insert(RawValue(v));
        }
    }
}
