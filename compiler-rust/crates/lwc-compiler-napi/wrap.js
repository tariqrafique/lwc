// lwc-compiler-rs binding wrapper (AB-D0005: no compile logic, no option decisions). Used by the ESM
// loader (index.js) and imported directly by the harness Rust backend, so the board runs the shipped
// seam (AB-0008 G2b).
// It only passes two callables beside the JSON options (functions cannot cross JSON), which the native
// calls synchronously on this thread (no ThreadsafeFunction, no replay):
// (1) the D-0293 instrumentation object, moved out of the options, called during the compile;
// (2) `warn`, upstream's `console.warn`, called with each D-0212 deprecation message Rust decides on
//     while it validates the options, so before anything is compiled (options.ts:185-219).

function warn(message) {
    console.warn(message);
}

/** Wrap a loaded native binding: `{ transform(src, filename, options, instrumentation, warn) }`. */
export function wrapBinding(binding) {
    function transform(src, filename, options) {
        const instrumentation =
            options !== null && typeof options === 'object' ? options.instrumentation : undefined;
        if (instrumentation === undefined || instrumentation === null) {
            return binding.transform(src, filename, options, undefined, warn);
        }
        // The key stays present (Rust decides what presence means); its value becomes JSON-safe.
        const wire = { ...options, instrumentation: {} };
        return binding.transform(src, filename, wire, instrumentation, warn);
    }
    return { transform };
}
