// Row `style-api`: compiler.apiVersion / style.apiVersion boundary matrix over the full style-compiler corpus.
// @lwc/style-compiler reads no APIFeature (transform.ts clamps via getAPIVersionFromNumber and passes the result to the
// postcss plugin, which declares but never reads it), so there is no feature boundary; the set is the clamp edges
// (57 below LOWEST=58, 67 above HIGHEST=66), every known version 58–66, and omitted — i.e. it pins version-invariance.
// Fixture-pinned apiVersion is overridden. Same observation as the `style` row. Goldens: goldens/style-api/<fixture>.json
// keyed by cell (v57…v67, omitted), JS oracle only.
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, beforeAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { evalStylesheetModule } from '../src/evalStyle';
import { checkGoldenCell, findFiles, resetGoldenDir, apiLabel, apiOpt, type ApiCell } from '../src/optionRows';
import { oracleRoot } from '../src/oracleRoot';

const ROOT = `${oracleRoot()}/packages/@lwc/style-compiler/src/__tests__/fixtures`;
const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/style-api');
const fixtures = findFiles(ROOT, 'actual.css').map((f) => path.relative(ROOT, path.dirname(f)));
export const STYLE_API_CELLS: ApiCell[] = [57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 'omitted'];
resetGoldenDir(GOLDEN_DIR);

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

describe('style-api', () => {
    it('finds 69 style fixtures', () => { if (fixtures.length !== 69) throw new Error(`got ${fixtures.length}`); });
    for (const rel of fixtures) {
        for (const v of STYLE_API_CELLS) {
            it(`${apiLabel(v)}/${rel}`, () => {
                const dir = path.join(ROOT, rel);
                const cfgPath = path.join(dir, 'config.json');
                const cfg = fs.existsSync(cfgPath) ? JSON.parse(fs.readFileSync(cfgPath, 'utf8')) : {};
                const { scoped, customProperties: _dead, apiVersion: _pinned, ...rest } = cfg;
                void _dead; void _pinned;
                let observed: unknown;
                try {
                    const res = backend.transform(fs.readFileSync(path.join(dir, 'actual.css'), 'utf8'), scoped ? 'actual.scoped.css' : 'actual.css',
                        { name: 'x', namespace: 'x', scopedStyles: !!scoped, ...rest, ...apiOpt(v) } as any);
                    observed = { ok: true, ...evalStylesheetModule(res.code) };
                } catch (e: any) {
                    observed = { ok: false, name: e?.name, line: e?.location?.line ?? e?.line ?? null, column: e?.location?.column ?? e?.column ?? null };
                }
                checkGoldenCell(GOLDEN_DIR, rel, apiLabel(v), observed);
            });
        }
    }
});
