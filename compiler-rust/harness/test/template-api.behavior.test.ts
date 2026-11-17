// Row `template-api`: template.apiVersion / compiler.apiVersion boundary matrix over the full template-compiler corpus
// (567 fixtures). APIFeatures @lwc/template-compiler reads (shared/src/api-version.ts minApiVersion):
//   LOWERCASE_SCOPE_TOKENS@59 (codegen helpers/static-element), TREAT_ALL_PARSE5_ERRORS_AS_ERRORS@59 (parser/html),
//   USE_FRAGMENTS_FOR_LIGHT_DOM_SLOTS@60 (codegen helpers), TEMPLATE_CLASS_NAME_OBJECT_BINDING@62 (codegen),
//   ENABLE_COMPLEX_TEMPLATE_EXPRESSIONS@66 (parser/expression*, needs experimentalComplexExpressions).
// Boundary set: each min version and the one before it {58,59,60,61,62,65,66}, the clamp edges 57 (<LOWEST=58) and
// 67 (>HIGHEST=66), and omitted. Fixture-pinned apiVersion (13 configs) is overridden, so it is not the coverage.
// Observation = `template` row shape + version-gated signals (apiHelpers, static fragments). Not asserted against the
// fixture's metadata.json (that is recorded for the fixture's own version); goldens enforce every cell.
// Goldens: goldens/template-api/<fixture>.json keyed by cell, JS oracle only.
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, beforeAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { checkGoldenCell, findFiles, resetGoldenDir, apiLabel, apiOpt, type ApiCell } from '../src/optionRows';
import { observeTemplate, TEMPLATE_ROOT } from '../src/templateObserve';

const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/template-api');
const fixtures = findFiles(TEMPLATE_ROOT, 'actual.html').map((f) => path.relative(TEMPLATE_ROOT, path.dirname(f)));
const CELLS: ApiCell[] = [57, 58, 59, 60, 61, 62, 65, 66, 67, 'omitted'];
resetGoldenDir(GOLDEN_DIR);

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

describe('template-api', () => {
    it('finds 567 template fixtures', () => { if (fixtures.length !== 567) throw new Error(`got ${fixtures.length}`); });
    for (const rel of fixtures) {
        const dir = path.join(TEMPLATE_ROOT, rel);
        const name = path.basename(dir);
        for (const v of CELLS) {
            it(`${apiLabel(v)}/${rel}`, () => {
                const cfgPath = path.join(dir, 'config.json');
                const cfg = fs.existsSync(cfgPath) ? JSON.parse(fs.readFileSync(cfgPath, 'utf8')) : {};
                // Same as the `template` row: experimentalComputedMemberExpression is not a TransformOptions key.
                const { apiVersion: _pinned, experimentalComputedMemberExpression: _ecme, ...rest } = cfg;
                void _pinned; void _ecme;
                const observed = observeTemplate(backend, fs.readFileSync(path.join(dir, 'actual.html'), 'utf8'), `${name}.html`,
                    { namespace: 'x', name, ...rest, ...apiOpt(v) }, { detail: true });
                checkGoldenCell(GOLDEN_DIR, rel, apiLabel(v), observed);
            });
        }
    }
});
