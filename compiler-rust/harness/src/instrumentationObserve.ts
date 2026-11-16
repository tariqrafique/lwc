// AB-0008 G2b (D-0009 / D-0205): real JS `instrumentation` callbacks through a backend, observed as the ordered
// call sequence plus the compile outcome. Shared by template-options (`inst/`) and js-options (`inst/`); goldens
// are the JS oracle (transformSync calling the callbacks itself), so the Rust seam (wrap.js passes the object
// beside the JSON options, the native calls it during the compile) must reproduce the same sequence and errors.
import type { CompilerBackend } from './backend';
import { errorObservation, errorMultiset } from './optionRows';

export type Call = [kind: 'log' | 'incrementCounter', arg: unknown];

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

/** Error text without the cwd Babel prefixes to a relative filename (the board's snapshot cwd differs from the golden's). */
const relMessage = (m: unknown) => String(m ?? '').split(`${process.cwd()}/`).join('');

export function observeInstrumented(
    backend: CompilerBackend,
    src: string,
    filename: string,
    opts: Record<string, unknown>,
    shape: (calls: Call[]) => unknown,
) {
    const calls: Call[] = [];
    try {
        const res = backend.transform(src, filename, { ...opts, instrumentation: shape(calls) } as any);
        return { calls, ok: true, warnings: (res.warnings ?? []).map((w: any) => w.code) };
    } catch (e: any) {
        return { calls, ...errorObservation(e), errors: errorMultiset(e), message: relMessage(e?.message) };
    }
}
