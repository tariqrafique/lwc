import fs from 'node:fs';
import path from 'node:path';
import { describe, it, expect, beforeAll } from 'vitest';
import { UPDATE, readGolden, writeGolden } from '../src/optionRows';
import { compareError } from '../src/compareError';
import { getBackend, type CompilerBackend } from '../src/backend';
import { evalTemplateModule } from '../src/evalTemplate';

const ROOT = '/workspace/lwc/packages/@lwc/template-compiler/src/__tests__/fixtures';
const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/template');

function findFixtures(dir: string): string[] {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
        const p = path.join(dir, e.name);
        return e.isDirectory() ? findFixtures(p) : e.name === 'actual.html' ? [p] : [];
    });
}
const fixtures = findFixtures(ROOT).sort();
// Temporary JS bridges until Rust owns the option. Documented for Migrate; fixtures must succeed through the seam.
// Oracle is pristine upstream b9e8007 (no local patches; see notes/lwc-rust-migrate/oracle-local-patches.diff).
const SEAM_BRIDGES: Record<string, string> = {};
// 'expression/computed': pristine oracle: transformSync does NOT forward experimentalComputedMemberExpression; golden is what shipped transformSync does. Rust honoring it = deliberate extension, needs a decisions.tsv entry + own golden
// Config keys that still cannot be reproduced through CompilerBackend — expect throw; golden records seamGap.
const SEAM_GAPS: Record<string, string> = {
    'expression/computed':
        'throws LWC1038: shipped transformSync does not forward experimentalComputedMemberExpression; upstream template-compiler fixture sets it directly',
};
void SEAM_BRIDGES; // kept for Migrate audit trail
const diag = (ws: any[] = []) => ws.map((w) => ({ code: w.code, level: w.level }));
const byKey = (a: { code: number; level: number }[]) => [...a].sort((x, y) => x.code - y.code || x.level - y.level);

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

it('finds 567 template fixtures', () => expect(fixtures.length).toBe(567));

describe('template behavioral', () => {
    for (const file of fixtures) {
        const dir = path.dirname(file);
        const rel = path.relative(ROOT, dir);
        it(rel, () => {
            const name = path.basename(dir);
            const cfgPath = path.join(dir, 'config.json');
            const cfg = fs.existsSync(cfgPath) ? JSON.parse(fs.readFileSync(cfgPath, 'utf8')) : {};
            // experimentalComputedMemberExpression is a template-compiler Config field, NOT a TransformOptions key:
            // shipped transformSync ignores it (SEAM_GAPS 'expression/computed'); the rust seam rejects unknown keys.
            // Strip it so both backends see exactly the shipped transformSync call. Covered directly by
            // lwc-template::computed_member_expression_flag (option-inventory).
            delete cfg.experimentalComputedMemberExpression;
            const meta = JSON.parse(fs.readFileSync(path.join(dir, 'metadata.json'), 'utf8'));
            const expected = diag(meta.warnings);
            // template-compiler reports errors as level<=1 (Fatal=0, Error=1) diagnostics; transformSync throws on those.
            const gap = SEAM_GAPS[rel];
            const expectError = !!gap || expected.some((d) => d.level <= 1);
            let observed: any;
            try {
                const res = backend.transform(fs.readFileSync(file, 'utf8'), `${name}.html`, { namespace: 'x', name, ...cfg });
                const warnings = diag(res.warnings);
                // transformSync surfaces the same diagnostic codes/levels the fixture recorded.
                expect(byKey(warnings)).toEqual(byKey(expected));
                const ev = evalTemplateModule(res.code);
                expect(ev.isFunction).toBe(true);
                expect(ev.registered).toBe(true);
                observed = { ok: true, ...ev, warnings, cssScopeTokens: res.cssScopeTokens ?? null };
            } catch (e: any) {
                if (!expectError) throw e;
                const code = e.code ?? null;
                const firstErr = expected.find((d) => d.level <= 1);
                if (code != null && firstErr) expect(code).toBe(firstErr.code);
                observed = { ok: false, name: e.name, code, line: e.location?.line ?? null, ...(gap ? { seamGap: gap } : {}), expectedErrorCodes: expected.filter((d) => d.level <= 1).map((d) => d.code) };
            }
            if (expectError) expect(observed.ok).toBe(false);
            const g = path.join(GOLDEN_DIR, rel.replace(/[\\/]/g, '__') + '.json');
            if (UPDATE) {
                writeGolden(g, observed);
            } else {
                const golden = readGolden(g);
                // Shared error comparator (src/compareError.ts): goldens carry no message text, so this asserts the
                // error outcome (no false pass on a non-throw; parser errors by kind + line). Exact fields: toEqual below.
                if (golden.ok === false) {
                    const err = (o: any) => ({ name: o.name, code: o.code, message: '', line: o.line });
                    expect(compareError(err(golden), observed.ok ? null : err(observed))).toMatchObject({ pass: true });
                }
                expect(observed).toEqual(golden);
            }
        });
    }
});
