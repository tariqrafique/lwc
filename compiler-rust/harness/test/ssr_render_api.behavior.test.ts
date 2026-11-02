// Row `ssr_render_api` (underscores: board.sh routes `ssr-render-*` rows to ssr.render.test.ts with that suffix as the
// mode): apiVersion boundary matrix over the full ssr-compiler fixture corpus, observed as rendered HTML (same bundle +
// serverSideRenderComponent + formatHTML path as the `ssr-render` row, SSR_MODE=sync).
// APIFeatures on the SSR path: @lwc/ssr-compiler reads ENABLE_COMPLEX_TEMPLATE_EXPRESSIONS@66 (compile-template/
// expression.ts, needs experimentalComplexExpressions); it calls @lwc/template-compiler parse(), whose parser reads
// TREAT_ALL_PARSE5_ERRORS_AS_ERRORS@59 and ENABLE_COMPLEX_TEMPLATE_EXPRESSIONS@66 (template codegen features are not on
// this path). @lwc/ssr-runtime reads none. Boundary set: {58,59,65,66} + clamp edges 57/67 + omitted.
// apiVersion goes through the rollup-plugin port like upstream (plugin clamps with getAPIVersionFromNumber; omitted →
// HIGHEST). No ssr fixture config pins apiVersion. Goldens: goldens/ssr_render_api/<fixture>.json keyed by cell, each
// {stage, error, html}; JS oracle only. Fixtures upstream marks `.skip` are skipped (as in ssr-render).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it, beforeAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { bundleFixture, existsUp, listFixtures, FIXTURES_ROOT, SSR_RUNTIME_ESM, SSR_RUNTIME_KIND } from '../src/ssrRender';
import { checkGoldenCell, resetGoldenDir, apiLabel, BACKEND_ID, type ApiCell } from '../src/optionRows';
import { formatHTML } from '/workspace/lwc/scripts/test-utils/format-html';

const here = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.resolve(here, `../.ssr-render-out/api-${BACKEND_ID}-${SSR_RUNTIME_KIND}`);
fs.rmSync(OUT, { recursive: true, force: true });
const GOLDEN_DIR = path.resolve(here, `../goldens/ssr_render_api${SSR_RUNTIME_KIND === 'dist' ? '' : `.${SSR_RUNTIME_KIND}`}`);
resetGoldenDir(GOLDEN_DIR);
const CELLS: ApiCell[] = [57, 58, 59, 65, 66, 67, 'omitted'];
const fixtures = listFixtures();

let backend: CompilerBackend;
let runtime: any;
beforeAll(async () => {
    backend = await getBackend();
    runtime = await import(SSR_RUNTIME_ESM);
    try { runtime.setHooks({ sanitizeHtmlContent: (c: unknown) => String(c) }); } catch { /* already set */ }
});

describe.concurrent(`ssr_render_api [${BACKEND_ID}/sync/${SSR_RUNTIME_KIND}]`, () => {
    it('finds 387 ssr fixtures', () => { if (fixtures.length !== 387) throw new Error(`got ${fixtures.length}`); });
    for (const dirname of fixtures) {
        const rel = path.relative(FIXTURES_ROOT, dirname);
        const opts = existsUp(dirname, '.skip') ? { skip: true } : {};
        for (const v of CELLS) {
            it(`${apiLabel(v)}/${rel}`, opts, async () => {
                const config = JSON.parse(fs.readFileSync(path.join(dirname, 'config.json'), 'utf8'));
                let observed: { stage: string; error: string; html: string };
                let code: string | undefined;
                try {
                    code = await bundleFixture(backend, { entry: config.entry, dirname, experimentalComplexExpressions: config.experimentalComplexExpressions, ssrMode: 'sync', ...(v === 'omitted' ? {} : { apiVersion: v }) });
                } catch (err: any) {
                    observed = { stage: 'compile', error: String(err?.message ?? err), html: '' };
                }
                if (code !== undefined) {
                    const outFile = path.join(OUT, apiLabel(v), rel, 'compiled-ssr.mjs');
                    fs.mkdirSync(path.dirname(outFile), { recursive: true });
                    fs.writeFileSync(outFile, code);
                    try {
                        const Component = (await import(/* @vite-ignore */ outFile)).default;
                        const html = formatHTML(await runtime.serverSideRenderComponent('fixture-test', Component, config?.props ?? {}, config?.styleDedupe ?? true, 'sync'));
                        observed = { stage: 'ok', error: '', html };
                    } catch (err: any) {
                        observed = { stage: 'render', error: String(err?.message ?? err), html: '' };
                    }
                }
                checkGoldenCell(GOLDEN_DIR, rel, apiLabel(v), observed!);
            });
        }
    }
});

// TREAT_ALL_PARSE5_ERRORS_AS_ERRORS@59 on the SSR path: no ssr fixture contains a parse5 error, so the rendered-HTML
// matrix above never differs at 58→59. Supplement (compile-only, tagged: rendered HTML is infeasible because the SSR
// template compile fails at every version; what changes at 59 is how it fails) with a template whose only problem is
// a parse5 'closing-of-element-with-open-child-elements' / 'open-elements-left-after-eof' (warn <59, error ≥59).
const PARSE5_CASES: Record<string, string> = {
    'unclosed-div': `<template><div></template>`,
    'valid-control': `<template><div>ok</div></template>`,
};
describe('ssr_render_api: parse5 compile-only', () => {
    for (const [name, src] of Object.entries(PARSE5_CASES)) {
        for (const v of CELLS) {
            it(`${apiLabel(v)}/parse5/${name}`, () => {
                let observed: unknown;
                try {
                    const r = backend.transform(src, path.resolve('component.html'), { namespace: 'x', name: 'component', targetSSR: true, ...(v === 'omitted' ? {} : { apiVersion: v }) } as any);
                    observed = { stage: 'ok', warnings: (r.warnings ?? []).map((w: any) => w.code) };
                } catch (e: any) {
                    observed = { stage: 'compile', error: String(e?.message ?? e).split('\n')[0] };
                }
                checkGoldenCell(GOLDEN_DIR, `_compile-only/parse5/${name}`, apiLabel(v), observed);
            });
        }
    }
});
