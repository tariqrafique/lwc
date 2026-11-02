import fs from 'node:fs';
import path from 'node:path';
import { describe, it, expect, beforeAll } from 'vitest';
import { compareError, observeError } from '../src/compareError';
import { getBackend, type CompilerBackend } from '../src/backend';
import { evalSsrModule } from '../src/evalSsr';

const ROOT = '/workspace/lwc/packages/@lwc/ssr-compiler/src/__tests__/fixtures';
// One board row per JS-shipped CompilationMode (@lwc/shared ssr.ts). sync keeps the original golden dir.
const SSR_MODE = (process.env.SSR_MODE ?? 'sync') as 'sync' | 'async' | 'asyncYield';
if (!['sync', 'async', 'asyncYield'].includes(SSR_MODE)) throw new Error(`bad SSR_MODE ${SSR_MODE}`);
const GOLDEN_DIR = path.resolve(import.meta.dirname, SSR_MODE === 'sync' ? '../goldens/ssr' : `../goldens/ssr-${SSR_MODE}`);
const UPDATE = !!process.env.UPDATE_GOLDENS;
if (UPDATE && process.env.LWC_COMPILER_BACKEND === 'rust') throw new Error('Refusing UPDATE_GOLDENS with rust backend: goldens are the JS oracle');

function walk(dir: string, pred: (p: string) => boolean): string[] {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
        const p = path.join(dir, e.name);
        return e.isDirectory() ? walk(p, pred) : pred(p) ? [p] : [];
    });
}
const fixtures = walk(ROOT, (p) => path.basename(p) === 'config.json').map(path.dirname).sort();
// Per-file compile mirrors the upstream rollup options (fixtures.spec.ts) minus bundling.
const BASE = {
    targetSSR: true, ssrMode: SSR_MODE, enableDynamicComponents: true, enableLwcOn: true, experimentalDynamicDirective: true,
    dynamicImports: { loader: '/virtual/custom-loader.js', strictSpecifier: false },
};
// Options the seam cannot reproduce; none known yet.
const SEAM_GAPS: Record<string, string> = {};

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

it('finds 387 ssr fixtures', () => expect(fixtures.length).toBe(387));

const errCode = (e: any): number | null => {
    if (typeof e.code === 'number') return e.code;
    const m = /LWC(\d+)/.exec(String(e.message));
    return m ? Number(m[1]) : null;
};

describe(`ssr behavioral [${SSR_MODE}]`, () => {
    for (const dir of fixtures) {
        const rel = path.relative(ROOT, dir);
        it(rel, () => {
            const cfg = JSON.parse(fs.readFileSync(path.join(dir, 'config.json'), 'utf8'));
            const errFile = path.join(dir, cfg.ssrFiles?.error ?? 'error.txt');
            const errText = fs.existsSync(errFile) ? fs.readFileSync(errFile, 'utf8').trim() : '';
            // Compile-time errors are recorded as "LWCxxxx: ..."; other error.txt contents are render-time (0d.3).
            const m = /^LWC(\d+):/.exec(errText);
            const expectedCode = m ? Number(m[1]) : null;
            const modDir = path.join(dir, 'modules');
            const files = fs.existsSync(modDir) ? walk(modDir, (p) => /\.(js|html|css)$/.test(p)).sort() : [];
            const results: Record<string, unknown> = {};
            const thrownCodes: number[] = [];
            let uncodedCompileError = false;
            for (const f of files) {
                const r = path.relative(modDir, f);
                const [namespace, name] = r.split(path.sep);
                try {
                    const res = backend.transform(fs.readFileSync(f, 'utf8'), path.basename(f), {
                        ...BASE, namespace, name, ...(cfg.experimentalComplexExpressions ? { experimentalComplexExpressions: true } : {}),
                    });
                    const warnings = (res.warnings ?? []).map((w) => ({ code: w.code, level: w.level }));
                    if (f.endsWith('.css')) { results[r] = { ok: true, warnings }; continue; }
                    const ev = evalSsrModule(res.code);
                    // helper modules (adapters, mixins) may have no default export; templates must.
                    if (f.endsWith('.html')) {
                        expect(ev.defaultExport!.templateRegistered).toBe(true);
                        expect(ev.defaultExport!.kind).toMatch(/function/);
                    }
                    results[r] = { ok: true, ...ev, warnings };
                } catch (e: any) {
                    // Uncoded compile errors (e.g. inner-outer-html): fixture error text must match the thrown message
                    // (shared comparator: substring for LWC/plain text; uncoded Babel parser errors by kind/file/line/col).
                    const uncoded = expectedCode == null && !!errText && compareError(errText, observeError(e), { exact: 'includes' }).pass;
                    if (expectedCode == null && !uncoded) throw e;
                    if (uncoded) uncodedCompileError = true;
                    const code = errCode(e);
                    if (code != null) thrownCodes.push(code);
                    results[r] = { ok: false, code };
                }
            }
            if (expectedCode != null) expect(thrownCodes).toContain(expectedCode);
            const observed = { expectedCompileError: expectedCode, uncodedCompileError, renderErrorDeferred: !m && !!errText && !uncodedCompileError, modules: results };
            const g = path.join(GOLDEN_DIR, rel.replace(/[\\/]/g, '__') + '.json');
            if (UPDATE || !fs.existsSync(g)) {
                fs.mkdirSync(GOLDEN_DIR, { recursive: true });
                fs.writeFileSync(g, JSON.stringify(observed, null, 2) + '\n');
            } else {
                expect(observed).toEqual(JSON.parse(fs.readFileSync(g, 'utf8')));
            }
        });
    }
});
