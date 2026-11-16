// AB-0008 G2b (D-0009 / D-0205): real JS `instrumentation` callbacks through a backend, observed as the ordered
// call sequence plus the compile outcome. Shared by template-options (`inst/`) and js-options (`inst/`); goldens
// are the JS oracle (transformSync calling the callbacks itself), so the Rust seam (wrap.js passes the object
// beside the JSON options, the native calls it during the compile) must reproduce the same sequence and errors.
import type { CompilerBackend } from './backend';
import { errorObservation, errorMultiset } from './optionRows';

export type Call = [kind: 'log' | 'incrementCounter' | 'warn', arg: unknown];

/** `instrumentation` values: real callbacks, and shapes whose `incrementCounter` is not a function. */
export const INSTRUMENTATION_SHAPES: Record<string, (calls: Call[]) => unknown> = {
    real: (calls) => ({
        log: (d: any) => calls.push(['log', d?.code ?? null]),
        incrementCounter: (m: unknown) => calls.push(['incrementCounter', m]),
    }),
    'counter-only': (calls) => ({ incrementCounter: (m: unknown) => calls.push(['incrementCounter', m]) }),
    empty: () => ({}),
    'log-only': (calls) => ({ log: (d: any) => calls.push(['log', d?.code ?? null]) }),
    'counter-not-a-function': () => ({ incrementCounter: 1 }),
    primitive: () => 1,
    null: () => null,
};

/**
 * `incrementCounter` callbacks that throw (AB-0008 G2b r4): the oracle stops the compile at the throw and
 * surfaces a coded error built from the thrown value (LWC1052 / LWC1008 for templates, LWC1007 for JS), so the
 * native must call them during the compile. One per value class upstream's catch sites read differently.
 */
const thrower = (value: () => unknown, onCall = 1) => (calls: Call[]) => ({
    log: (d: any) => calls.push(['log', d?.code ?? null]),
    incrementCounter: (m: unknown) => {
        calls.push(['incrementCounter', m]);
        if (calls.filter(([k]) => k === 'incrementCounter').length === onCall) throw value();
    },
});
export const THROWING_SHAPES: Record<string, (calls: Call[]) => unknown> = {
    error: thrower(() => new Error('boom')),
    'type-error': thrower(() => new TypeError('tboom')),
    'named-object': thrower(() => ({ name: 'Custom', message: 'm' })),
    'empty-object': thrower(() => ({})),
    function: thrower(() => function thrownFn() {}),
    string: thrower(() => 'strboom'),
    number: thrower(() => 1.5),
    boolean: thrower(() => true),
    bigint: thrower(() => 10n),
    symbol: thrower(() => Symbol('s')),
    null: thrower(() => null),
    undefined: thrower(() => undefined),
    'error-on-second-call': thrower(() => new Error('second'), 2),
};

/**
 * AB-0008 r5 D3: thrown objects carrying @lwc/errors' own keys. getCodeFromError (utils.ts:88-95) takes a
 * numeric `lwcCode`, then a numeric `code`; convertErrorToDiagnostic (errors.ts:200-215) keeps the raw
 * message when `lwcCode` is truthy and reads `'level' in error`. Observed with `detail` (levels, messages).
 */
export const CODED_THROWING_SHAPES: Record<string, (calls: Call[]) => unknown> = {
    'lwc-code': thrower(() => ({ lwcCode: 1234, message: 'm' })),
    code: thrower(() => ({ code: 1234, message: 'm' })),
    'level-0': thrower(() => ({ level: 0, message: 'm' })),
    'level-2': thrower(() => ({ level: 2, message: 'm' })),
    'code-level-0': thrower(() => ({ code: 1111, level: 0, message: 'm' })),
    'error-lwc-code': thrower(() => Object.assign(new Error('e'), { lwcCode: 1100 })),
    'lwc-code-string': thrower(() => ({ lwcCode: 'x', message: 'm' })),
    'code-string': thrower(() => ({ code: 'X', message: 'm' })),
};

/** Error text without the cwd Babel prefixes to a relative filename (the board's snapshot cwd differs from the golden's). */
const relMessage = (m: unknown) => String(m ?? '').split(`${process.cwd()}/`).join('');

/**
 * The call sequence and outcome. `detail` (coded shapes) adds what codes alone hide: each warning's level and
 * message, the thrown error's level, and the length of the code (a dropped Fatal diagnostic leaves it empty).
 */
export function observeInstrumented(
    backend: CompilerBackend,
    src: string,
    filename: string,
    opts: Record<string, unknown>,
    shape: (calls: Call[]) => unknown,
    { detail = false, calls = [] as Call[] } = {},
) {
    try {
        const res = backend.transform(src, filename, { ...opts, instrumentation: shape(calls) } as any);
        if (!detail) return { calls, ok: true, warnings: (res.warnings ?? []).map((w: any) => w.code) };
        return {
            calls,
            ok: true,
            codeLength: res.code.length,
            warnings: (res.warnings ?? []).map((w: any) => ({ code: w.code, level: w.level, message: w.message })),
        };
    } catch (e: any) {
        const thrown = { calls, ...errorObservation(e), errors: errorMultiset(e), message: relMessage(e?.message) };
        return detail ? { ...thrown, level: e?.level ?? null } : thrown;
    }
}

/**
 * AB-0008 r5 D1: upstream's `console.warn` (options.ts:185-219) runs while the options are validated, so a
 * deprecation warning precedes every counter call. `console.warn` calls join the sequence as `warn`.
 */
export function observeWarnOrder(
    backend: CompilerBackend,
    src: string,
    filename: string,
    opts: Record<string, unknown>,
    shape: (calls: Call[]) => unknown,
) {
    const calls: Call[] = [];
    const warn = console.warn;
    console.warn = (...args: unknown[]) => { calls.push(['warn', args.map(String).join(' ')]); };
    try {
        return observeInstrumented(backend, src, filename, opts, shape, { calls });
    } finally {
        console.warn = warn;
    }
}

/** A thrown value as the caller sees it, without assuming it is an Error. */
function thrownShape(e: any) {
    if (e === null || (typeof e !== 'object' && typeof e !== 'function')) return { type: e === null ? 'null' : typeof e, value: typeof e === 'symbol' ? e.toString() : String(e) };
    return { type: typeof e, name: e.name ?? null, message: e.message === undefined ? null : relMessage(e.message), lwcCode: e.lwcCode ?? null, code: e.code ?? null, level: e.level ?? null };
}

/**
 * AB-0008 r5 D2: under targetSSR, ssr-compiler compile-template/index.ts:83-95 calls template-compiler
 * `parse` (index.ts:33-38), which has no catch, and @lwc/compiler does not normalize SSR errors: the caller
 * gets the callback's own thrown value (`identical`), or the call's own TypeError for a non-callable counter.
 */
export function observeRethrow(
    backend: CompilerBackend,
    src: string,
    filename: string,
    opts: Record<string, unknown>,
    shape: (calls: Call[]) => unknown,
) {
    const calls: Call[] = [];
    const inst = shape(calls) as any;
    let threw = false;
    let thrown: unknown;
    const wrapped = inst !== null && typeof inst === 'object' && typeof inst.incrementCounter === 'function'
        ? { ...inst, incrementCounter: (m: unknown) => { try { return inst.incrementCounter(m); } catch (e) { threw = true; thrown = e; throw e; } } }
        : inst;
    try {
        const res = backend.transform(src, filename, { ...opts, instrumentation: wrapped } as any);
        return { calls, ok: true, warnings: (res.warnings ?? []).map((w: any) => w.code) };
    } catch (e: any) {
        return { calls, ok: false, callbackThrew: threw, identical: threw && e === thrown, ...thrownShape(e) };
    }
}
