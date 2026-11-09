import fs from 'node:fs';
import path from 'node:path';
import { describe, it, expect, beforeAll } from 'vitest';
import { UPDATE, readGolden, writeGolden } from '../src/optionRows';
import { compareError, compareAggregate, observeAggregate } from '../src/compareError';
import { getBackend, type CompilerBackend } from '../src/backend';
import { evalJsModule } from '../src/evalJs';

const ROOT = '/workspace/lwc/packages/@lwc/babel-plugin-component/src/__tests__/fixtures';
const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/js');

function findFixtures(dir: string): string[] {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
        const p = path.join(dir, e.name);
        return e.isDirectory() ? findFixtures(p) : e.name === 'actual.js' ? [p] : [];
    });
}
const fixtures = findFixtures(ROOT).sort();
// Temporary JS bridges until Rust owns the option. Migrate-approved.
// Oracle is pristine upstream b9e8007 (no local patches; see notes/lwc-rust-migrate/oracle-local-patches.diff).
const SEAM_BRIDGES: Record<string, string> = {};
// 'dynamic-imports/passthough-with-no-config': pristine oracle: omitted dynamicImports -> strictSpecifier TRUE (shipped DEFAULT_DYNAMIC_IMPORT_CONFIG); no bridge
// Remaining unreproducible fixtures through the seam (expect throw / special golden).
const SEAM_GAPS: Record<string, string> = {
    'dynamic-imports/passthough-with-no-config':
        'throws LWC1121: shipped transformSync defaults dynamicImports.strictSpecifier=true; upstream babel-plugin fixture runs with the plugin default (off)',
};
void SEAM_BRIDGES;
const lwcCodes = (s: string) => [...s.matchAll(/LWC(\d{4})/g)].map((m) => Number(m[1]));

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

it('finds 173 js fixtures', () => expect(fixtures.length).toBe(173));

describe('js behavioral', () => {
    for (const file of fixtures) {
        const dir = path.dirname(file);
        const rel = path.relative(ROOT, dir);
        it(rel, () => {
            const read = (f: string) => (fs.existsSync(path.join(dir, f)) ? fs.readFileSync(path.join(dir, f), 'utf8').trim() : '');
            const cfg = read('config.json') ? JSON.parse(read('config.json')) : {};
            const { parserOpts, ...rest } = cfg;
            // Same base opts as upstream index.spec (namespace lwc, name test, filename test.js);
            // upstream always runs the private-method transforms, so enablePrivateMethods is on; parserOpts.errorRecovery maps to transformSync's experimentalErrorRecoveryMode.
            const opts = { namespace: 'lwc', name: 'test', enablePrivateMethods: true, ...rest, ...(parserOpts?.errorRecovery ? { experimentalErrorRecoveryMode: true } : {}) };
            const errJson = read('error.json');
            const fixtureErr = errJson ? JSON.parse(errJson) : null;
            const recovered = read('lwcErrors.json') ? JSON.parse(read('lwcErrors.json')) : null;
            const recoveredCodes = Array.isArray(recovered) ? recovered.flatMap((e: any) => lwcCodes(e.message ?? '')) : [];
            const gap = SEAM_GAPS[rel];
            // error recovery: babel-plugin returns lwcErrors metadata; transformSync throws CompilerAggregateError instead.
            const expectError = gap ? gap.startsWith('throws') : !!fixtureErr || recoveredCodes.length > 0;
            const expectedCodes = gap ? lwcCodes(gap) : fixtureErr ? lwcCodes(fixtureErr.message ?? '') : recoveredCodes;
            let observed: any;
            try {
                const res = backend.transform(fs.readFileSync(file, 'utf8'), 'test.js', opts);
                const warnings = (res.warnings ?? []).map((w) => ({ code: w.code, level: w.level }));
                const ev = evalJsModule(res.code);
                observed = { ok: true, ...ev, warnings };
            } catch (e: any) {
                if (!expectError) throw e;
                const msgs = [e.message ?? '', ...(e.errors ?? []).map((x: any) => x.message ?? '')].join('\n');
                const codes = [...new Set([...(e.code != null ? [e.code] : []), ...(e.errors ?? []).map((x: any) => x.code).filter((c: any) => c != null), ...lwcCodes(msgs)])].sort();
                // every fixture-expected LWC code must be surfaced by the seam error
                for (const c of expectedCodes) expect(codes).toContain(c);
                const aggregate = observeAggregate(e);
                observed = { ok: false, name: e.name, codes, line: e.location?.line ?? null, expectedCodes, ...(aggregate ? { aggregate } : {}), ...(gap ? { seamGap: gap } : {}) };
            }
            expect(observed.ok).toBe(!expectError);
            const g = path.join(GOLDEN_DIR, rel.replace(/[\\/]/g, '__') + '.json');
            if (UPDATE) {
                writeGolden(g, observed);
            } else {
                const golden = readGolden(g);
                // Shared error comparator (src/compareError.ts): goldens carry no message text, so this asserts the
                // error outcome (no false pass on a non-throw; parser errors by kind + line). Exact fields: toEqual below.
                if (golden.ok === false) {
                    const err = (o: any) => ({ name: o.name, code: o.codes?.[0] ?? null, message: '', line: o.line });
                    expect(compareError(err(golden), observed.ok ? null : err(observed))).toMatchObject({ pass: true });
                    if (golden.aggregate) expect(compareAggregate(golden.aggregate, observed.aggregate)).toEqual({ pass: true, mode: 'aggregate' });
                }
                expect(observed).toEqual(golden);
            }
        });
    }
});
