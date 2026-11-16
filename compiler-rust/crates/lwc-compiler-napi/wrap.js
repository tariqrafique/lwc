// lwc-compiler-rs binding wrapper (AB-D0005: no compile logic, no option decisions). Used by the ESM
// loader (index.js) and imported directly by the harness Rust backend, so the board runs the shipped
// seam (AB-0008 G2b).
// It only (1) moves the D-0205 instrumentation object out of the options before the serde boundary
// (functions cannot cross JSON) and passes it beside them, so the native calls its callbacks
// synchronously during the compile (no ThreadsafeFunction, no replay), and (2) prints D-0212
// deprecation messages Rust already decided to emit.

function printDeprecations(result) {
    if (!result || typeof result !== 'object') return result;
    const deprecations = result.deprecations;
    if (Array.isArray(deprecations)) {
        for (const message of deprecations) {
            console.warn(message);
        }
    }
    if (Object.prototype.hasOwnProperty.call(result, 'deprecations')) {
        delete result.deprecations;
    }
    return result;
}

/** Wrap a loaded native binding: `{ transform(src, filename, options, instrumentation?) }`. */
export function wrapBinding(binding) {
    function transform(src, filename, options) {
        const instrumentation =
            options !== null && typeof options === 'object' ? options.instrumentation : undefined;
        if (instrumentation === undefined || instrumentation === null) {
            return printDeprecations(binding.transform(src, filename, options));
        }
        // The key stays present (Rust decides what presence means); its value becomes JSON-safe.
        const wire = { ...options, instrumentation: {} };
        return printDeprecations(binding.transform(src, filename, wire, instrumentation));
    }
    return { transform };
}
