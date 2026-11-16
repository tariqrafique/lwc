// lwc-compiler-rs ESM loader. Loads the native binary and re-exports it; no compile logic
// (AB-D0005). Option decisions belong to Rust. The wrapper only (1) replays D-0205
// instrumentation events the native collected into the caller's callbacks, and (2) prints
// D-0212 deprecation messages Rust already decided to emit. No ThreadsafeFunction.
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const SUPPORTED = ['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64-msvc'];
const requireNative = createRequire(import.meta.url);

function triple() {
    const base = `${process.platform}-${process.arch}`;
    if (process.platform !== 'linux') return process.platform === 'win32' ? `${base}-msvc` : base;
    const glibc = process.report?.getReport?.()?.header?.glibcVersionRuntime;
    return `${base}-${glibc ? 'gnu' : 'musl'}`;
}

function load() {
    const t = triple();
    const candidates = [
        fileURLToPath(new URL(`./lwc-compiler.${t}.node`, import.meta.url)),
        `lwc-compiler-rs-${t}`,
    ];
    const tried = [];
    let last;
    for (const c of candidates) {
        try {
            return requireNative(c);
        } catch (e) {
            last = e;
            tried.push(`${c}: ${e?.code ?? ''} ${String(e?.message ?? e).split('\n')[0]}`.trim());
        }
    }
    throw new Error(
        `lwc-compiler-rs: no native binary for ${t}. Tried: ${tried.join('; ')}. ` +
            `Supported: ${SUPPORTED.join(', ')}. Reinstall without --omit=optional.`,
        { cause: last },
    );
}

const binding = load();

function replayAndStrip(result, instrumentation) {
    if (!result || typeof result !== 'object') return result;
    const events = result.instrumentationEvents;
    if (events && instrumentation && typeof instrumentation === 'object') {
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

// S1: wrap the native export so events and deprecations surface the way upstream does.
export function transform(src, filename, options) {
    const instrumentation =
        options !== null && typeof options === 'object' ? options.instrumentation : undefined;
    return replayAndStrip(binding.transform(src, filename, options), instrumentation);
}
