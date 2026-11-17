// @vitest-environment jsdom
// Row `lbc` (gating): whole-component compile + SSR render + CSR render for
// corpus:lbc and corpus:lbc-recipes. Each included bundle is compiled through
// the current backend (JS oracle when recording goldens, Rust napi on the
// board) and compared byte-exact to the JS golden. No whitespace is removed
// before the compare. A bundle whose JS golden and this backend both fail to
// compile SSR and CSR is excluded as compile-error-both: skipped, named, and
// counted. Anything else stays in the ratcheted pass/fail set.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { serializeNode, trackShadowRoots, ENGINE_DOM_ESM } from '../src/csrRender';
import { bundleLbcComponent, discoverLbcBundles, type LbcBundle } from '../src/lbcBundle';
import { UPDATE, readGolden, writeGolden, goldenName } from '../src/optionRows';
import { styleCorpusRoot } from '../src/oracleRoot';
import { SSR_RUNTIME_ESM } from '../src/ssrRender';
// Same dist files the bundles pin. ENGINE_DOM_ESM and SSR_RUNTIME_ESM are
// oracleRoot() paths (harness/scripts/oracle-root.mjs / LWC_ORACLE).
const engineDom = await import(ENGINE_DOM_ESM);
const ssrRuntime = await import(SSR_RUNTIME_ESM);

const BACKEND_ID = process.env.LWC_COMPILER_BACKEND ?? 'js';
const here = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.resolve(here, `../.lbc-out/${BACKEND_ID}`);
const GOLDEN_DIR = path.resolve(here, '../goldens/lbc');
fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT, { recursive: true });

const CORPUS = styleCorpusRoot();
const bundles = discoverLbcBundles(CORPUS);

type Side = { status: 'ok'; html: string } | { status: 'compile-error'; message: string } | { status: 'render-error'; message: string };
type Observed = { ssr: Side; csr: Side; asyncErrors?: string[] };

const exclusions: { id: string; why: string }[] = [];
const stray: string[] = [];
let captureStray = false;
// Components that call Math.random (button onclick examples) must render the
// same bytes on every run and both backends. The sequence is per test.
let randomState = 0x12345678;
function seededRandom(): number {
    randomState = (randomState + 0x6d2b79f5) | 0;
    let t = Math.imul(randomState ^ (randomState >>> 15), 1 | randomState);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

function norm(s: string): string {
    return s
        .split(CORPUS).join('<corpus>')
        .split(OUT).join('<out>')
        .replace(/\[object:vm (\S+) \(\d+\)\]/g, '[object:vm $1 (#)]');
}

function messageOf(e: unknown): string {
    if (e instanceof Error) return e.message || e.name;
    return String(e);
}

let backend: CompilerBackend;
let lwc: typeof engineDom;

// Callback throws this corpus records on the observation, plus the carousel
// auto-scroll timer (default scrollDuration is 5s) that sets tabIndex on a
// pagination item after the host has been cleared. Anything else is re-emitted
// so Vitest fails the process.
const KNOWN_CALLBACK = new Set<string>([
    '"getBoundingClientRect" is not supported in this environment',
    '(intermediate value).connectedCallback is not a function',
    '(intermediate value).disconnectedCallback is not a function',
    "Cannot destructure property 'horizontal' of 'this._alignment' as it is undefined.",
    "Cannot read properties of null (reading 'popover')",
    "Cannot read properties of undefined (reading 'LABEL_INLINE')",
    "Cannot read properties of undefined (reading 'STANDARD')",
    "Cannot read properties of undefined (reading 'editableCustomType')",
    "Cannot read properties of undefined (reading 'formatAddressAllFields')",
    "Cannot read properties of undefined (reading 'getType')",
    "Cannot set properties of undefined (setting 'tabIndex')",
    'Class constructor sldsCommon cannot be invoked without \'new\'',
    'Class constructor sldsUtilsGrid cannot be invoked without \'new\'',
    'Class constructor sldsUtilsMargin cannot be invoked without \'new\'',
    'Class constructor sldsUtilsTruncation cannot be invoked without \'new\'',
    'Use of <lightning-modal> as an element is not supported. See developer documentation for extending the LightningModal class.',
    'Use of <lightning-overlay> as an element is not supported. See developer documentation for extending the LightningOverlay class.',
    '_missingExportShim is not a constructor',
    '_missingExportShim is not a function',
]);

function isKnownCallback(e: unknown): boolean {
    return KNOWN_CALLBACK.has(norm(messageOf(e)));
}

const note = (e: unknown) => { if (captureStray) stray.push(norm(messageOf(e))); };

// Vitest treats an error as handled while this file also listens. Re-emit
// unexpected throws with only Vitest's listener so the run exits nonzero.
function failUnexpected(e: unknown): void {
    const err = e instanceof Error ? e : new Error(messageOf(e));
    process.off('uncaughtException', onUncaught);
    process.emit('uncaughtException', err);
    process.on('uncaughtException', onUncaught);
}

function onUncaught(err: unknown): void {
    // Late timers (carousel auto-scroll) are not part of the observation.
    if (isKnownCallback(err)) return;
    failUnexpected(err);
}

function onRejection(reason: unknown): void {
    if (isKnownCallback(reason)) { note(reason); return; }
    failUnexpected(reason);
}

beforeAll(async () => {
    backend = await getBackend();
    trackShadowRoots();
    lwc = engineDom;
    if (typeof lwc.createElement !== 'function') throw new Error(`${ENGINE_DOM_ESM}: no createElement`);
    if (typeof ssrRuntime.serverSideRenderComponent !== 'function') throw new Error(`${SSR_RUNTIME_ESM}: no serverSideRenderComponent`);
    try { ssrRuntime.setHooks?.({ sanitizeHtmlContent: (c: unknown) => String(c) }); } catch { /* already set */ }
    // jsdom reports a custom-element callback throw as window "error". Cancelling
    // a known one keeps it on the observation. Anything else fails the run.
    window.addEventListener('error', (e) => {
        const err = e.error ?? e.message;
        if (!isKnownCallback(err)) { failUnexpected(err); return; }
        e.preventDefault();
        note(err);
    });
    window.addEventListener('unhandledrejection', (e) => {
        if (!isKnownCallback(e.reason)) { failUnexpected(e.reason); return; }
        e.preventDefault();
        note(e.reason);
    });
    process.on('uncaughtException', onUncaught);
    process.on('unhandledRejection', onRejection);
});

afterAll(() => {
    const discovered = bundles.length;
    const excluded = exclusions.length;
    const included = discovered - excluded;
    const lines = [
        `lbc: discovered=${discovered} included=${included} excluded=${excluded} compile-error-both=${exclusions.length}`,
    ];
    for (const e of exclusions) lines.push(`lbc exclude ${e.id} ${e.why}`);
    const text = lines.join('\n') + '\n';
    const dest = process.env.LBC_SUMMARY;
    if (dest) {
        fs.mkdirSync(path.dirname(dest), { recursive: true });
        fs.writeFileSync(dest, text);
    }
});

async function renderSsr(code: string, id: string): Promise<Side> {
    const file = path.join(OUT, id, 'ssr.mjs');
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, code);
    try {
        const Component = (await import(/* @vite-ignore */ file)).default;
        if (Component == null) return { status: 'render-error', message: 'ssr default export missing' };
        const html = await ssrRuntime.serverSideRenderComponent('lwc-lbc', Component, {}, true, 'sync');
        return { status: 'ok', html: norm(String(html ?? '')) };
    } catch (e) {
        return { status: 'render-error', message: norm(messageOf(e)) };
    }
}

async function renderCsr(code: string, id: string): Promise<Side> {
    const file = path.join(OUT, id, 'csr.mjs');
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, code);
    try {
        const Component = (await import(/* @vite-ignore */ file)).default;
        if (Component == null) return { status: 'render-error', message: 'csr default export missing' };
        const elm = lwc.createElement('lwc-lbc', { is: Component });
        document.body.appendChild(elm);
        for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r, 0));
        const html = Array.from(document.body.childNodes, serializeNode).join('');
        return { status: 'ok', html: norm(html) };
    } catch (e) {
        return { status: 'render-error', message: norm(messageOf(e)) };
    } finally {
        document.body.textContent = '';
    }
}

async function stage(bundle: LbcBundle, targetSSR: boolean, id: string): Promise<Side> {
    let code: string;
    try {
        code = await bundleLbcComponent(backend, bundle, {
            targetSSR,
            lwcPath: ENGINE_DOM_ESM,
            ssrRuntimePath: SSR_RUNTIME_ESM,
        });
    } catch (e) {
        return { status: 'compile-error', message: norm(messageOf(e)) };
    }
    return targetSSR ? renderSsr(code, id) : renderCsr(code, id);
}

function bothCompileError(o: Observed): boolean {
    return o.ssr.status === 'compile-error' && o.csr.status === 'compile-error';
}

function uniqueStray(): string[] {
    const out: string[] = [];
    const seen = new Set<string>();
    for (const s of stray) {
        if (seen.has(s)) continue;
        seen.add(s);
        out.push(s);
    }
    stray.length = 0;
    return out;
}

describe('lbc whole-component', () => {
    it('discovers lbc and lbc-recipes bundles', () => {
        // Pinned lightning-base-components@1.28.19-alpha and base-components-recipes@9c5f95d.
        // A bundle's default export reaches LightningElement directly or through a
        // superclass defined in the same corpus (LightningShadowBaseClass and similar).
        // The matrix text says 258 lbc bundles; this pin has 257 that extend
        // LightningElement directly and 396 once those subclasses are included.
        expect(bundles.filter((b) => b.corpus === 'lbc').length).toBe(396);
        expect(bundles.filter((b) => b.corpus === 'lbc-recipes').length).toBe(126);
    });

    for (const bundle of bundles) {
        it(bundle.id, async (ctx) => {
            stray.length = 0;
            captureStray = true;
            randomState = 0x12345678;
            const random = Math.random;
            Math.random = seededRandom;
            let observed: Observed;
            try {
                observed = {
                    ssr: await stage(bundle, true, bundle.id),
                    csr: await stage(bundle, false, bundle.id),
                };
                for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
                const asyncErrors = uniqueStray();
                if (asyncErrors.length) observed.asyncErrors = asyncErrors;
            } finally {
                captureStray = false;
                Math.random = random;
            }
            const g = path.join(GOLDEN_DIR, goldenName(bundle.id));
            if (UPDATE) {
                writeGolden(g, observed, 1);
                return;
            }
            const golden = readGolden(g) as Observed;
            if (bothCompileError(golden) && bothCompileError(observed)) {
                exclusions.push({ id: bundle.id, why: 'compile-error-both' });
                ctx.skip();
                return;
            }
            expect(observed).toEqual(golden);
        });
    }
});
