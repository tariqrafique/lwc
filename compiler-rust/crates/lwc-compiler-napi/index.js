// @lwc/rust-compiler ESM loader. Loads the native binary and wraps it with wrap.js; no compile logic
// (AB-D0005). Option decisions belong to Rust. wrap.js passes the D-0205 instrumentation object beside
// the options (the native calls it synchronously) and prints D-0212 deprecation messages. No ThreadsafeFunction.
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { wrapBinding } from './wrap.js';

const SUPPORTED = ['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64-msvc', 'win32-arm64-msvc'];
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
        `@lwc/rust-compiler-${t}`,
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
        `@lwc/rust-compiler: no native binary for ${t}. Tried: ${tried.join('; ')}. ` +
            `Supported: ${SUPPORTED.join(', ')}. Reinstall without --omit=optional.`,
        { cause: last },
    );
}

// S1: wrap the native export so instrumentation callbacks and deprecations behave the way upstream's do.
export const { transform } = wrapBinding(load());
