// lwc-compiler-rs binding wrapper (AB-D0005: no compile logic, no option decisions). Used by the ESM
// loader (index.js); harness/src/napiWrap.js is a byte-identical copy for the harness Rust backend (the
// harness runs from a harness-only copy in land self-tests), so the board runs the shipped seam (AB-0008
// G2b). tests/loader_guard.rs `harness_copy_is_byte_identical` keeps the two equal.
// It only (1) strips the D-0205 instrumentation callbacks before the serde boundary (functions cannot
// cross JSON) and tells the native which callbacks are functions, (2) prints D-0212 deprecation messages
// Rust already decided to emit, and (3) replays the instrumentation events the native collected into the
// caller's callbacks. No ThreadsafeFunction.

function replayAndStrip(result, instrumentation) {
    if (!result || typeof result !== 'object') return result;
    // validateOptions warns before the compile runs, so deprecations print before any event replays.
    const deprecations = result.deprecations;
    if (Array.isArray(deprecations)) {
        for (const message of deprecations) {
            console.warn(message);
        }
    }
    if (Object.prototype.hasOwnProperty.call(result, 'deprecations')) {
        delete result.deprecations;
    }
    const events = result.instrumentationEvents;
    if (events && instrumentation != null) {
        for (const ev of events) {
            if (ev && ev.kind === 'incrementCounter' && typeof instrumentation.incrementCounter === 'function') {
                instrumentation.incrementCounter(ev.metric);
            } else if (ev && ev.kind === 'log' && typeof instrumentation.log === 'function') {
                instrumentation.log(ev.diagnostic);
            }
        }
    }
    if (Object.prototype.hasOwnProperty.call(result, 'instrumentationEvents')) {
        delete result.instrumentationEvents;
    }
    return result;
}

/** Wrap a loaded native binding: `{ transform(src, filename, options) }`. */
export function wrapBinding(binding) {
    function transform(src, filename, options) {
        const instrumentation =
            options !== null && typeof options === 'object' ? options.instrumentation : undefined;
        if (instrumentation === undefined || instrumentation === null) {
            return replayAndStrip(binding.transform(src, filename, options), instrumentation);
        }
        // The key stays present (Rust decides what presence means); its value becomes JSON-safe.
        const wire = { ...options, instrumentation: {} };
        const callbacks = {
            log: typeof instrumentation.log === 'function',
            incrementCounter: typeof instrumentation.incrementCounter === 'function',
        };
        return replayAndStrip(binding.transform(src, filename, wire, callbacks), instrumentation);
    }
    return { transform };
}
