// @vitest-environment jsdom
// CSR DOM render parity (see CSR_RENDER.md). Each fixture is bundled with rollup through the CompilerBackend
// (`targetSSR: false`, src/ssrRender.ts's @lwc/rollup-plugin port), imported, rendered with @lwc/engine-dom
// `createElement('fixture-test', { is })` + props + `document.body.appendChild`, flushed, and its DOM (shadow roots
// inlined as <template shadowrootmode>) compared byte-exact to goldens recorded from the JS backend.
// Corpora (env CORPUS = ssr | template | all, default all):
//   A ssr-fixtures:      @lwc/ssr-compiler/src/__tests__/fixtures/**/config.json (entry + props), native shadow.
//   B template-fixtures: @lwc/template-compiler/src/__tests__/fixtures/**/actual.html with a minimal component.
// Tests run sequentially: the DOM, window error events and unhandled rejections are attributed to one fixture at a time.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { compareError, observeError, type CompareResult, type ErrorObservation } from '../src/compareError';
import { existsUp, listFixtures, FIXTURES_ROOT } from '../src/ssrRender';
import { UPDATE, readGolden, writeGolden } from '../src/optionRows';
import {
    ENGINE_DOM_ESM, TEMPLATE_FIXTURES_ROOT, bundleSsrFixtureCsr, bundleTemplateFixtureCsr, describeValue,
    listTemplateFixtures, readTemplateFixture, serializeNode, trackShadowRoots,
} from '../src/csrRender';
import { formatHTML } from '/workspace/lwc/scripts/test-utils/format-html';
// Must be the same file as csrRender.ts ENGINE_DOM_ESM (the bundles' `lwc` path) so there is ONE engine instance.
// Static literal: vite's client (jsdom) transform does not resolve a non-literal dynamic import of this path.
import * as engineDom from '/workspace/lwc/packages/@lwc/engine-dom/dist/index.js';
import { oracleRoot, canonOraclePath } from '../src/oracleRoot';

const BACKEND_ID = process.env.LWC_COMPILER_BACKEND ?? 'js';
const CORPUS = process.env.CORPUS ?? 'all';
if (!['ssr', 'template', 'all'].includes(CORPUS)) throw new Error(`bad CORPUS ${CORPUS} (ssr | template | all)`);

const here = path.dirname(fileURLToPath(import.meta.url));
// Scratch bundles live inside harness/ (not node_modules) so vite inlines them and they share this file's engine-dom.
const OUT = path.resolve(here, `../.csr-render-out/${BACKEND_ID}-${CORPUS}`);
fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT, { recursive: true });
const GOLDEN_DIR = path.resolve(here, '../goldens/csr-render');

type Status = 'ok' | 'compile-error' | 'render-error' | 'excluded';
interface Observed { status: Status; warnings?: string[]; html?: string; head?: string; error?: string; errors?: string[]; logs?: string[]; reason?: string; note?: string }
type Golden = Record<string, Observed>;
const loadGolden = (f: string): Golden => (UPDATE ? {} : readGolden(f));   // missing golden file = failure, never {}
// Scratch paths, and engine-dom's global VM counter in dev messages (`[object:vm Ctor (1211)]`), which depends on how
// many components rendered earlier in the run (CORPUS=all vs a single corpus), are not compiler output.
// Rollup prints module ids relative to process.cwd() (`../../lwc/...` from the live harness, `../../../workspace/lwc/...`
// from a /tmp snapshot): resolve them and anchor the oracle at `<lwc>` so goldens don't depend on where the harness runs.
const ORACLE_ROOT = oracleRoot();
const cwdIds = (s: string) => s.replace(/(?:\.\.\/)+[\w@.\-\/]+/g, (m) => {
    const abs = path.resolve(process.cwd(), m);
    return abs.startsWith(ORACLE_ROOT + '/') ? '<lwc>' + abs.slice(ORACLE_ROOT.length) : abs;
});
const norm = (s: string) => canonOraclePath(cwdIds(s.split(OUT).join('<out>')).replace(/\[object:vm (\S+) \(\d+\)\]/g, '[object:vm $1 (#)]'));

let backend: CompilerBackend;
let lwc: any;
let currentErrors: string[] | null = null;
const strays: string[] = [];
let savedRejection: Function[] = [];
const onRejection = (r: unknown) => (currentErrors ?? strays).push(`unhandledRejection: ${describeValue(r)}`);
const onWindowError = (e: ErrorEvent) => { e.preventDefault(); (currentErrors ?? strays).push(`uncaught: ${describeValue(e.error ?? e.message)}`); };

beforeAll(async () => {
    backend = await getBackend();
    trackShadowRoots();
    lwc = engineDom;
    if (lwc.createElement === undefined) throw new Error(`${ENGINE_DOM_ESM}: no createElement`);
    savedRejection = process.listeners('unhandledRejection');
    process.removeAllListeners('unhandledRejection');
    process.on('unhandledRejection', onRejection);
    window.addEventListener('error', onWindowError);
});
afterAll(() => {
    window.removeEventListener('error', onWindowError);
    process.off('unhandledRejection', onRejection);
    for (const l of savedRejection) process.on('unhandledRejection', l as any);
});

const tick = () => new Promise((r) => setTimeout(r, 0));

/** Import the bundle, mount it, flush, serialize, unmount. Every error/console line is captured, never thrown. */
async function render(code: string, rel: string, props: Record<string, unknown>): Promise<Observed> {
    const file = path.join(OUT, rel, 'compiled-csr.mjs');
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, code);
    const errors: string[] = (currentErrors = []);
    const logs: string[] = [];
    const methods = ['error', 'warn', 'info', 'log'] as const;
    const saved = methods.map((m) => console[m]);
    methods.forEach((m) => { console[m] = (...a: unknown[]) => { logs.push(`${m}: ${a.map(describeValue).join(' ')}`); }; });
    const headBefore = new Set(Array.from(document.head.childNodes));
    let html = '';
    let head = '';
    try {
        let elm: HTMLElement | undefined;
        try {
            const Component = (await import(/* @vite-ignore */ file)).default;
            elm = lwc.createElement('fixture-test', { is: Component });
            for (const [k, v] of Object.entries(props)) (elm as any)[k] = v;
            document.body.appendChild(elm!);
        } catch (e) { errors.push(`render: ${describeValue(e)}`); }
        for (let i = 0; i < 3; i++) await tick();
        html = Array.from(document.body.childNodes, serializeNode).join('');
        head = Array.from(document.head.childNodes).filter((n) => !headBefore.has(n)).map(serializeNode).join('');
        try { if (elm?.parentNode) elm.parentNode.removeChild(elm); } catch (e) { errors.push(`disconnect: ${describeValue(e)}`); }
        await tick();
    } finally {
        methods.forEach((m, i) => { console[m] = saved[i]; });
        currentErrors = null;
        document.body.textContent = '';
    }
    return {
        status: errors.length ? 'render-error' : 'ok',
        html: norm(html),
        ...(head ? { head: norm(head) } : {}),
        ...(errors.length ? { errors: errors.map(norm) } : {}),
        ...(logs.length ? { logs: logs.map(norm) } : {}),
    };
}

interface Corpus { key: 'ssr' | 'template'; file: string; expectCount: number; cases: { rel: string; exclude?: string; run: () => Promise<Observed> }[] }

function ssrCorpus(): Corpus {
    const dirs = listFixtures();
    return {
        key: 'ssr', file: 'ssr-fixtures.json', expectCount: 387,
        cases: dirs.map((dirname) => {
            const rel = path.relative(FIXTURES_ROOT, dirname);
            // Upstream `.skip` markers (testFixtureDir existsUp) are honored; `.only` has no meaning in a parity board.
            const exclude = existsUp(dirname, '.skip') ? 'upstream .skip marker (ssr-compiler testFixtureDir)' : undefined;
            return {
                rel, exclude,
                run: async () => {
                    const config = JSON.parse(fs.readFileSync(path.join(dirname, 'config.json'), 'utf8'));
                    let code: string;
                    try {
                        code = await bundleSsrFixtureCsr(backend, { entry: config.entry, dirname, experimentalComplexExpressions: config.experimentalComplexExpressions });
                    } catch (e: any) {
                        compileErrors.set(rel, { ...observeError(e), message: norm(String(e?.message ?? e)) });
                        return { status: 'compile-error' as const, error: norm(String(e?.message ?? e)) };
                    }
                    const obs = await render(code, rel, config?.props ?? {});
                    // Diagnostic only (report.json, not pass/fail): does the CSR DOM equal upstream's SSR expected.html?
                    // Upstream formatHTML throws on some unbalanced markup; that counts as no match.
                    try {
                        ssrMatch[rel] = formatHTML(obs.html ?? '') === fs.readFileSync(path.join(dirname, config?.ssrFiles?.expected ?? 'expected.html'), 'utf8');
                    } catch {
                        ssrMatch[rel] = false;
                    }
                    return obs;
                },
            };
        }),
    };
}
const ssrMatch: Record<string, boolean> = {};
// Structured compile error per fixture (name/filename/line/column) for src/compareError.ts's parser-error policy.
const compileErrors = new Map<string, ErrorObservation>();

function templateCorpus(): Corpus {
    return {
        key: 'template', file: 'template-fixtures.json', expectCount: 567,
        cases: listTemplateFixtures().map((dir) => {
            const fx = readTemplateFixture(dir);
            const exclude = existsUp(dir, '.skip') ? 'upstream .skip marker (template-compiler testFixtureDir)' : undefined;
            return {
                rel: fx.rel, exclude,
                run: async () => {
                    const note = fx.droppedKeys.length
                        ? `config keys ${fx.droppedKeys.join(',')} dropped: not @lwc/compiler TransformOptions (transformSync ignores them; rust seam rejects unknown keys)`
                        : undefined;
                    // Template-compiler warnings are fixture data here (upstream template fixtures expect many), so they
                    // are recorded instead of failing the bundle as the ssr-compiler onwarn does.
                    const warnings: string[] = [];
                    const extra = () => ({ ...(warnings.length ? { warnings: warnings.map(norm) } : {}), ...(note ? { note } : {}) });
                    let code: string;
                    try {
                        code = await bundleTemplateFixtureCsr(backend, fx, (m) => warnings.push(m));
                    } catch (e: any) {
                        compileErrors.set(fx.rel, { ...observeError(e), message: norm(String(e?.message ?? e)) });
                        return { status: 'compile-error' as const, error: norm(String(e?.message ?? e)), ...extra() };
                    }
                    return { ...(await render(code, fx.rel, {})), ...extra() };
                },
            };
        }),
    };
}

const corpora = [...(CORPUS !== 'template' ? [ssrCorpus()] : []), ...(CORPUS !== 'ssr' ? [templateCorpus()] : [])];
interface Row { fixture: string; pass: boolean; status: Status; goldenStatus?: Status; errorCompare?: CompareResult; ssrExpectedMatch?: boolean; got?: Observed; want?: Observed }
const reportRows: Record<string, Row[]> = {};

for (const corpus of corpora) {
    const goldenFile = path.join(GOLDEN_DIR, corpus.file);
    const golden = loadGolden(goldenFile);
    const recorded: Golden = {};
    const rows: (typeof reportRows)[string] = (reportRows[corpus.key] = []);
    describe(`csr render ${corpus.key} [${BACKEND_ID}]`, () => {
        it(`finds ${corpus.expectCount} ${corpus.key} fixtures`, () => expect(corpus.cases.length).toBe(corpus.expectCount));
        if (!UPDATE) {
            it('golden covers exactly the corpus', () => expect(Object.keys(golden).sort()).toEqual(corpus.cases.map((c) => c.rel).sort()));
        }
        for (const c of corpus.cases) {
            it(c.rel, async () => {
                const got: Observed = c.exclude ? { status: 'excluded', reason: c.exclude } : await c.run();
                if (UPDATE) recorded[c.rel] = got;
                const want = UPDATE ? got : golden[c.rel];
                // Compile errors go through the shared comparator: uncoded Babel parser errors compare kind + filename +
                // line/column (wording and code frame not ported); everything else stays the exact JSON compare below.
                const errCmp = want?.status === 'compile-error' && got.status === 'compile-error'
                    ? compareError(want.error ?? '', compileErrors.get(c.rel) ?? got.error ?? '') : null;
                const sansError = (o: Observed) => JSON.stringify({ ...o, error: undefined });
                const parserPass = errCmp?.mode === 'parser' && errCmp.pass && sansError(got) === sansError(want!);
                const identical = JSON.stringify(got) === JSON.stringify(want);
                const pass = !!want && (identical || parserPass);
                rows.push({
                    fixture: c.rel, pass, status: got.status, goldenStatus: want?.status, ...(errCmp ? { errorCompare: errCmp } : {}),
                    ...(c.rel in ssrMatch ? { ssrExpectedMatch: ssrMatch[c.rel] } : {}),
                    // Full observation for mismatches (failure clustering) and for non-ok JS outcomes (review).
                    ...(!pass || got.status !== 'ok' ? { got } : {}),
                    ...(!pass && want ? { want } : {}),
                });
                expect(want, `no golden for ${c.rel} (record with UPDATE_GOLDENS=1 on the js backend)`).toBeDefined();
                if (!identical && errCmp?.mode === 'parser') {
                    expect(errCmp, `parser-error compare for ${c.rel}`).toMatchObject({ pass: true });
                    expect({ ...got, error: undefined }).toEqual({ ...want, error: undefined });
                } else expect(got).toEqual(want);
            });
        }
        afterAll(() => {
            if (UPDATE) {
                const sorted = Object.fromEntries(Object.entries(recorded).sort(([a], [b]) => a.localeCompare(b)));
                writeGolden(goldenFile, sorted, 1);
            }
            const sorted = rows.sort((a, b) => a.fixture.localeCompare(b.fixture));
            fs.writeFileSync(path.join(OUT, `report-${corpus.key}.json`), JSON.stringify({ backend: BACKEND_ID, napi: process.env.LWC_COMPILER_NAPI_PATH ?? null, strays, rows: sorted }, null, 1));
        });
    });
}

describe('csr render: stray errors', () => {
    it('no error escaped fixture attribution', () => expect(strays).toEqual([]));
});
