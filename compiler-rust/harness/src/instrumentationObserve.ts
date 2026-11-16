// AB-0008 G2b (D-0009 / D-0205): real JS `instrumentation` callbacks through a backend, observed as the ordered
// call sequence plus the compile outcome. Shared by template-options (`inst/`) and js-options (`inst/`); goldens
// are the JS oracle (transformSync calling the callbacks itself), so the Rust seam (wrap.js strips the callbacks
// before JSON, the native collects events, wrap.js replays them) must reproduce the same sequence and errors.
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
