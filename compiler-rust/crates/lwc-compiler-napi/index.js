// lwc-compiler-rs ESM loader. Loads the native binary and re-exports it; no compile logic, no option
// inspection, no environment overrides (AB-D0005). Order: sibling .node, then the per-triple package.
// tests/loader_guard.rs enforces: no environment access, no option-key literals.
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

// S1: the native exports as-is (S3 renames to transformSync + async transform).
export const transform = binding.transform;
