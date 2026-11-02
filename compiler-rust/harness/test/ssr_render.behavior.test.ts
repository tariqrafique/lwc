// 0d.3 SSR HTML render compare, per CompilationMode. Port of upstream ssr-compiler `__tests__/fixtures.spec.ts`:
// rollup + @lwc/rollup-plugin bundles each fixture, `serverSideRenderComponent(..., mode)` renders it through `formatHTML`.
// Oracle:
// - LWC_COMPILER_BACKEND=js (default): must equal on-disk `expected.html` / `error.txt` (upstream oracle; sync only
//   is fully green upstream — async/asyncYield have real runtime differences, see README).
// - LWC_COMPILER_BACKEND=rust: differential. The same fixture is bundled+rendered twice in the same mode, once with
//   JS `transformSync` and once with the Rust addon; rendered HTML / render error must be identical, and a compile
//   error on one side must be a compile error with the same LWCxxxx code on the other.
// Mode: `SSR_MODE` (alias `LWC_SSR_MODE`) = sync (default) | async | asyncYield.
// Backend seam: @lwc/rollup-plugin's CJS dist calls `compiler.transformSync` by property lookup at transform time; we
// hand it a mutable compiler object and pick the backend per plugin instance (transform is synchronous, so a
// module-level "current backend" set around the hook call cannot race).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { describe, it, expect, beforeAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { formatHTML } from '/workspace/lwc/scripts/test-utils/format-html';

const LWC = '/workspace/lwc/packages/@lwc';
const ROOT = `${LWC}/ssr-compiler/src/__tests__/fixtures`;
const SSR_MODE = (process.env.SSR_MODE ?? process.env.LWC_SSR_MODE ?? 'sync') as 'sync' | 'async' | 'asyncYield';
if (!['sync', 'async', 'asyncYield'].includes(SSR_MODE)) throw new Error(`bad SSR_MODE ${SSR_MODE}`);

const req = createRequire(`${LWC}/rollup-plugin/package.json`);
const { rollup } = req('rollup');
// One native ESM runtime instance shared by the bundles (via `paths`) and the render call.
const RUNTIME = `${LWC}/ssr-runtime/dist/index.js`;
const runtime: any = await import(RUNTIME);
try { runtime.setHooks({ sanitizeHtmlContent: (c: unknown) => String(c) }); } catch { /* already set */ }
// Node 20 require(esm) hands out read-only namespaces, so give rollup-plugin's CJS dist a mutable compiler object.
const compiler = { ...req(`${LWC}/compiler/dist/index.cjs`) };
const jsTransformSync = compiler.transformSync;
const Module = req('node:module');
const origLoad = Module._load;
Module._load = function (request: string, parent: any, ...rest: unknown[]) {
    if (request === '@lwc/compiler' && parent?.filename?.startsWith(`${LWC}/rollup-plugin/`)) return compiler;
    return origLoad.call(this, request, parent, ...rest);
};
const lwcRollupPluginMod = req(`${LWC}/rollup-plugin/dist/index.cjs`);
Module._load = origLoad;
const lwcRollupPlugin = lwcRollupPluginMod.default ?? lwcRollupPluginMod;

let backend: CompilerBackend;
let current: CompilerBackend | null = null;
compiler.transformSync = (src: string, filename: string, options: any) => {
    if (!current || current.id === 'js') return jsTransformSync(src, filename, options);
    const { outputConfig: _o, stylesheetConfig: _s, ...rest } = options;
    return { ...current.transform(src, filename, rest), map: null };
};
beforeAll(async () => { backend = await getBackend(); });

function walk(dir: string, pred: (p: string) => boolean): string[] {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
        const p = path.join(dir, e.name);
        return e.isDirectory() ? walk(p, pred) : pred(p) ? [p] : [];
    });
}
const fixtures = walk(ROOT, (p) => path.basename(p) === 'config.json').map(path.dirname).sort();
const OUT = fs.mkdtempSync(path.join(os.tmpdir(), `lwc-ssr-render-${SSR_MODE}-`));

function plugin(dirname: string, cfg: any, be: CompilerBackend | null) {
    const p = lwcRollupPlugin({
        targetSSR: true, ssrMode: SSR_MODE, enableDynamicComponents: true, enableLwcOn: true, experimentalDynamicDirective: true,
        modules: [{ dir: path.resolve(dirname, 'modules') }],
        dynamicImports: { loader: `${ROOT}/../utils/custom-loader.js`, strictSpecifier: false },
        experimentalComplexExpressions: cfg.experimentalComplexExpressions,
    });
    const transform = p.transform;
    p.transform = function (this: unknown, src: string, id: string) {
        current = be;
        try { return transform.call(this, src, id); } finally { current = null; }
    };
    return p;
}

async function compileFixture(dirname: string, cfg: any, be: CompilerBackend | null, outFile: string) {
    const input = 'virtual/fixture/test.js';
    const bundle = await rollup({
        input,
        external: ['lwc', '@lwc/ssr-runtime', 'vitest'],
        plugins: [
            { name: 'virtual', resolveId: (id: string) => (id === input ? id : null), load: (id: string) => (id === input ? `export { default } from "${cfg.entry}";` : null) },
            plugin(dirname, cfg, be),
        ],
        onwarn({ message, code }: any) { // UNUSED_EXTERNAL_IMPORT: Rust emits a superset of runtime imports; harmless for rendering (shape board owns imports).
            if (code !== 'CIRCULAR_DEPENDENCY' && code !== 'UNUSED_EXTERNAL_IMPORT') throw new Error(message); },
    });
    const { output } = await bundle.generate({ format: 'esm', exports: 'named', paths: { lwc: RUNTIME, '@lwc/ssr-runtime': RUNTIME } });
    fs.mkdirSync(path.dirname(outFile), { recursive: true });
    fs.writeFileSync(outFile, output[0].code);
}

interface Rendered { compileErr?: string; err: string; html: string }
async function render(dir: string, rel: string, cfg: any, be: CompilerBackend | null): Promise<Rendered> {
    const outFile = path.join(OUT, rel, `compiled-ssr.${be?.id ?? 'js'}.mjs`);
    try {
        await compileFixture(dir, cfg, be, outFile);
    } catch (e: any) {
        return { compileErr: String(e.message), err: String(e.message), html: '' };
    }
    const mod = await import(outFile);
    try {
        return { err: '', html: formatHTML(await runtime.serverSideRenderComponent('fixture-test', mod.default, cfg.props ?? {}, cfg.styleDedupe ?? true, SSR_MODE)) };
    } catch (e: any) {
        return { err: String(e.message), html: '' };
    }
}

// Harness-environment gaps, identical for every backend/mode (JS oracle fails them too).
const ENV_GAPS: Record<string, string> = Object.fromEntries(
    ['barrel-lwc-exports/export-all', 'barrel-lwc-exports/export-all-alias', 'barrel-lwc-exports/export-named'].map((k) => [
        k, 'ssr-runtime dist names its function `unwrap$1` (rollup build rename); upstream renders against src',
    ])
);
const read = (p: string) => (fs.existsSync(p) ? fs.readFileSync(p, 'utf8') : '');
const lwcCode = (s?: string) => (s ? /LWC\d+/.exec(s)?.[0] : undefined);

describe.concurrent(`ssr render [${SSR_MODE}]`, () => {
    it('finds 387 ssr fixtures', () => expect(fixtures.length).toBe(387));
    for (const dir of fixtures) {
        const rel = path.relative(ROOT, dir);
        const skip = fs.existsSync(path.join(dir, '.skip')) || rel in ENV_GAPS;
        it.skipIf(skip)(rel, async () => {
            const cfg = JSON.parse(fs.readFileSync(path.join(dir, 'config.json'), 'utf8'));
            let want: Rendered;
            if (backend.id === 'js') {
                const wantErr = read(path.join(dir, cfg.ssrFiles?.error ?? 'error.txt'));
                const wantHtml = read(path.join(dir, cfg.ssrFiles?.expected ?? 'expected.html'));
                want = { err: wantErr, html: wantHtml };
            } else {
                want = await render(dir, rel, cfg, null);
            }
            const got = await render(dir, rel, cfg, backend);
            if (got.compileErr != null || want.compileErr != null) {
                // Compile errors: Rust messages are not byte-ported; compare the LWCxxxx code (or text when uncoded).
                const w = want.compileErr ?? want.err;
                expect({ compileErr: got.compileErr != null, code: lwcCode(got.err) ?? got.err })
                    .toEqual({ compileErr: backend.id === 'js' ? true : want.compileErr != null, code: lwcCode(w) ?? w });
                return;
            }
            expect({ err: got.err, html: got.html }).toEqual({ err: want.err, html: want.html });
        });
    }
});
