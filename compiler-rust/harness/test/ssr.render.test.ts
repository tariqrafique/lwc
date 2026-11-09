// 0d.3 SSR render-HTML parity, per CompilationMode. Port of upstream
// packages/@lwc/ssr-compiler/src/__tests__/fixtures.spec.ts:
//   bundle each fixture (rollup + LWC plugin, targetSSR, ssrMode) → import → serverSideRenderComponent('fixture-test',
//   Component, config.props ?? {}, config.styleDedupe ?? true, mode) → formatHTML → compare to `ssrFiles.expected ??
//   expected.html`; any render error message → `ssrFiles.error ?? error.txt`; a bundle (compile) error → error file
//   gets err.message and the expected file must be empty. Files are compared byte-exact like toMatchFileSnapshot.
// Every module goes through the harness CompilerBackend (`LWC_COMPILER_BACKEND=js|rust`) via src/ssrRender.ts's port
// of @lwc/rollup-plugin. Mode: env `SSR_MODE` = sync (default, = upstream DEFAULT_SSR_MODE) | async | asyncYield.
// serverSideRenderComponent drives the mode: sync → direct call, async → awaited promise, asyncYield → for-await over
// the async generator. Upstream has a single expected.html for all modes; this suite applies it to each mode.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { compareError, observeError, type CompareResult, type ErrorObservation } from '../src/compareError';
import { getBackend, type CompilerBackend } from '../src/backend';
import { bundleFixture, existsUp, listFixtures, FIXTURES_ROOT, SSR_RUNTIME_ESM, SSR_RUNTIME_KIND, type SsrMode } from '../src/ssrRender';
import { UPDATE, readGolden, writeGolden } from '../src/optionRows';
import { formatHTML } from '/workspace/lwc/scripts/test-utils/format-html';

const SSR_MODE = (process.env.SSR_MODE ?? 'sync') as SsrMode;
if (!['sync', 'async', 'asyncYield'].includes(SSR_MODE)) throw new Error(`bad SSR_MODE ${SSR_MODE}`);
const BACKEND_ID = process.env.LWC_COMPILER_BACKEND ?? 'js';

if (!fs.existsSync(SSR_RUNTIME_ESM)) {
    throw new Error(`${SSR_RUNTIME_ESM} missing: build it with \`cd /workspace/lwc && npx nx run @lwc/ssr-runtime:build\``);
}

const here = path.dirname(fileURLToPath(import.meta.url));
// Bundles are written to harness/.ssr-render-out (scratch, safe to delete): inside the harness so the bare `vitest`
// external (some fixtures `import { expect } from 'vitest'`) resolves to the harness's vitest, and NOT under
// node_modules, which vitest would externalize to native Node import, yielding a second @lwc/ssr-runtime instance
// (different SYMBOL__GENERATE_MARKUP) than the one this test renders with. Upstream likewise writes to <fixture>/dist.
const OUT = path.resolve(here, `../.ssr-render-out/${BACKEND_ID}-${SSR_MODE}-${SSR_RUNTIME_KIND}`);
fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT, { recursive: true });

const fixtures = listFixtures();
let backend: CompilerBackend;
let runtime: any;
type Row = { fixture: string; pass: boolean; oracle: 'expected' | 'mode-golden'; stage: 'compile' | 'render' | 'ok'; errorCompare?: CompareResult; err: string; wantErr: string; htmlMatch: boolean; html?: string };
const report: Row[] = [];

// ── Mode goldens (goldens/ssr-render/<mode>.json) ────────────────────────────────────────────────────────────────
// Upstream only ever runs fixtures.spec.ts in DEFAULT_SSR_MODE ('sync'); expected.html is not valid in every mode.
// For fixtures where the *JS oracle* provably cannot produce expected.html in a mode, for a documented upstream reason,
// the JS backend's actual rendered output is recorded (UPDATE_GOLDENS=1, JS backend only) and is the oracle for that
// fixture in that mode. Every other fixture is compared to expected.html / error.txt byte-exact. A divergence without a
// known reason fails recording (= harness bug). Reasons:
const MODE_REASONS: Record<string, (rel: string, html: string) => string | undefined> = {
    sync: () => undefined,
    // @lwc/ssr-runtime setStaticInternals() only builds an asyncYield generator for 'asyncYield'; 'async' falls through to
    // makeGenerateMarkupSync, which does `markup += renderTemplate(...)` on the compiler's `async function __lwcTmpl`,
    // so the shadow content renders as "[object Promise]" and template-time errors become unhandled rejections.
    async: (_rel, html) => (html.includes('[object Promise]') ? 'upstream-async-runtime: async templates stringified as [object Promise] by makeGenerateMarkupSync' : undefined),
    // asyncYield awaits between yields, so microtasks queued in constructor/connectedCallback run before the template
    // reads the field; expected.html encodes sync ordering.
    asyncYield: (rel) => ({
        'dynamic-imports/basic': 'upstream-asyncYield-microtask: import().then() in constructor resolves before template reads `importee`',
        rehydration: 'upstream-asyncYield-microtask: Promise.then() in connectedCallback sets `reactive=1` before template renders',
    } as Record<string, string>)[rel],
};
const GOLDEN = path.resolve(here, `../goldens/ssr-render/${SSR_MODE}${SSR_RUNTIME_KIND === 'dist' ? '' : `.${SSR_RUNTIME_KIND}`}.json`);
type ModeGolden = Record<string, { reason: string; outputs: Record<string, string> }>;
const modeGolden: ModeGolden = UPDATE ? {} : readGolden(GOLDEN);   // missing golden file = failure, never {} (which would silently drop overrides)
const recorded: ModeGolden = {};

// In async mode, template errors inside the stringified promises surface as unhandled rejections (see above). They are
// collected (not dropped) and asserted in a final test to be errors some fixture expects, instead of crashing the run.
const strayRejections: string[] = [];
let savedRejectionListeners: Function[] = [];
const onStray = (reason: any) => { strayRejections.push(String(reason?.message ?? reason)); };

beforeAll(async () => {
    backend = await getBackend();
    runtime = await import(SSR_RUNTIME_ESM);
    // Upstream vi.mock('@lwc/ssr-runtime') installs this hook once.
    try { runtime.setHooks({ sanitizeHtmlContent: (c: unknown) => String(c) }); } catch { /* already overridden */ }
    if (SSR_MODE === 'async') {
        savedRejectionListeners = process.listeners('unhandledRejection');
        process.removeAllListeners('unhandledRejection');
        process.on('unhandledRejection', onStray);
    }
});
afterAll(() => {
    if (SSR_MODE === 'async') {
        process.off('unhandledRejection', onStray);
        for (const l of savedRejectionListeners) process.on('unhandledRejection', l as any);
    }
    if (UPDATE) {
        const sorted = Object.fromEntries(Object.entries(recorded).sort(([a], [b]) => a.localeCompare(b)));
        writeGolden(GOLDEN, sorted, 1);
    }
    const file = path.join(OUT, 'report.json');
    fs.writeFileSync(file, JSON.stringify({ backend: BACKEND_ID, mode: SSR_MODE, runtime: SSR_RUNTIME_KIND, napi: process.env.LWC_COMPILER_NAPI_PATH ?? null, strayRejections, rows: report.sort((a, b) => a.fixture.localeCompare(b.fixture)) }, null, 1));
});

// Known oracle divergences, each with its reason. Upstream's vitest aliases @lwc/ssr-runtime to its TS src; the built
// dist (rollup) renames the SSR stub `unwrap` to `unwrap$1` (it collides with observable-membrane's `unwrap`), and these
// fixtures render the runtime's exported function `.name`s. They pass with SSR_RUNTIME=src.
const DIST_RUNTIME_SKIPS: Record<string, string> = SSR_RUNTIME_KIND !== 'dist' ? {} : Object.fromEntries(
    ['barrel-lwc-exports/export-all', 'barrel-lwc-exports/export-all-alias', 'barrel-lwc-exports/export-named']
        .map((f) => [f, 'dist bundle renames ssr-runtime `unwrap` → `unwrap$1`; fixture prints export fn names (run SSR_RUNTIME=src)']),
);
const read = (p: string) => fs.readFileSync(p, 'utf8');
const allExpectedErrors = new Set<string>();

describe.concurrent(`ssr render [${BACKEND_ID}/${SSR_MODE}/${SSR_RUNTIME_KIND}]`, () => {
    it('finds 387 ssr fixtures', () => expect(fixtures.length).toBe(387));
    for (const dirname of fixtures) {
        const rel = path.relative(FIXTURES_ROOT, dirname);
        // Upstream skips: `.skip` marker files (testFixtureDir existsUp): scope-token, wire/errors/throws-when-colliding-prop-then-method.
        const opts = existsUp(dirname, '.only') ? { only: true } : existsUp(dirname, '.skip') || rel in DIST_RUNTIME_SKIPS ? { skip: true } : {};
        it(rel, opts, async () => {
            const config = JSON.parse(read(path.join(dirname, 'config.json')));
            const errorFile = config?.ssrFiles?.error ?? 'error.txt';
            const expectedFile = config?.ssrFiles?.expected ?? 'expected.html';
            const expected = { [errorFile]: read(path.join(dirname, errorFile)), [expectedFile]: read(path.join(dirname, expectedFile)) };
            if (expected[errorFile]) allExpectedErrors.add(expected[errorFile]);
            let got: Record<string, string>;
            let stage: Row['stage'] = 'ok';
            let code: string | undefined;
            let compileError: ErrorObservation | undefined;
            try {
                code = await bundleFixture(backend, { entry: config.entry, dirname, experimentalComplexExpressions: config.experimentalComplexExpressions, ssrMode: SSR_MODE });
            } catch (err: any) {
                stage = 'compile';
                compileError = observeError(err);
                got = { [errorFile]: err.message, [expectedFile]: '' };
            }
            if (code !== undefined) {
                const outFile = path.join(OUT, rel, 'compiled-ssr.mjs');
                fs.mkdirSync(path.dirname(outFile), { recursive: true });
                fs.writeFileSync(outFile, code);
                let result: string | undefined;
                let error: string | undefined;
                try {
                    const Component = (await import(/* @vite-ignore */ outFile)).default;
                    result = formatHTML(await runtime.serverSideRenderComponent('fixture-test', Component, config?.props ?? {}, config?.styleDedupe ?? true, SSR_MODE));
                } catch (err: any) {
                    stage = 'render';
                    error = err?.message;
                }
                got = { [errorFile]: error ?? '', [expectedFile]: result ?? '' };
            }
            // Error text via the shared comparator: an uncoded Babel parser error in the oracle compares kind + filename +
            // line/column against a compile-stage actual; LWC-coded and render-time errors stay byte-exact.
            let errorCompare: CompareResult | undefined;
            const sameError = (want: string) => {
                if (got![errorFile] === want) return true;
                if (!want) return false;
                errorCompare = compareError(want, stage === 'compile' ? compileError : stage === 'render' ? got![errorFile] : undefined);
                return errorCompare.mode === 'parser' && stage === 'compile' && errorCompare.pass;
            };
            const same = (a: Record<string, string>) => sameError(a[errorFile]) && got![expectedFile] === a[expectedFile];
            let want = expected;
            let oracle: Row['oracle'] = 'expected';
            if (UPDATE && !same(expected)) {
                const reason = MODE_REASONS[SSR_MODE](rel, got![expectedFile]);
                if (!reason) throw new Error(`JS oracle diverges from ${expectedFile}/${errorFile} in ${SSR_MODE} with no documented reason (harness bug?)`);
                recorded[rel] = { reason, outputs: got! };
                want = got!;
                oracle = 'mode-golden';
            } else if (modeGolden[rel]) {
                want = modeGolden[rel].outputs;
                oracle = 'mode-golden';
            }
            const pass = same(want);
            report.push({ fixture: rel, pass, oracle, stage, ...(errorCompare ? { errorCompare } : {}), err: got![errorFile], wantErr: want[errorFile], htmlMatch: got![expectedFile] === expected[expectedFile], ...(pass ? {} : { html: got![expectedFile] }) });
            if (pass && got![errorFile] !== want[errorFile]) expect(errorCompare).toMatchObject({ mode: 'parser', pass: true });
            else expect(got!).toEqual(want);
        });
    }
});

// Runs after the concurrent block (sequential describe). Give stray async-mode rejections a tick to land first.
describe('ssr render: stray rejections', () => {
    it('every unhandled rejection is an error some fixture expects', async () => {
        await new Promise((r) => setTimeout(r, 50));
        expect(strayRejections.filter((m) => !allExpectedErrors.has(m))).toEqual([]);
    });
});
