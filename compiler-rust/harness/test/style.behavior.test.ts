import fs from 'node:fs';
import path from 'node:path';
import { describe, it, expect, beforeAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { evalStylesheetModule } from '../src/evalStyle';

const ROOT = '/workspace/lwc/packages/@lwc/style-compiler/src/__tests__/fixtures';
const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/style');
const UPDATE = !!process.env.UPDATE_GOLDENS;

function findFixtures(dir: string): string[] {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
        const p = path.join(dir, e.name);
        return e.isDirectory() ? findFixtures(p) : e.name === 'actual.css' ? [p] : [];
    });
}
const fixtures = findFixtures(ROOT).sort();

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

it('finds 69 style fixtures', () => expect(fixtures.length).toBe(69));

describe('style behavioral', () => {
    for (const file of fixtures) {
        const dir = path.dirname(file);
        const rel = path.relative(ROOT, dir);
        it(rel, () => {
            const cfgPath = path.join(dir, 'config.json');
            const cfg = fs.existsSync(cfgPath) ? JSON.parse(fs.readFileSync(cfgPath, 'utf8')) : {};
            // customProperties.resolverModule: dead in style-compiler (declared, never read; 0/8 outputs differ) -> dropped, not remapped.
            const { scoped, customProperties: _deadResolverModule, ...rest } = cfg;
            const filename = scoped ? 'actual.scoped.css' : 'actual.css';
            const errFile = path.join(dir, 'error.json');
            const expectError = fs.existsSync(errFile) && fs.readFileSync(errFile, 'utf8').trim() !== '';
            let observed: any;
            try {
                const res = backend.transform(fs.readFileSync(file, 'utf8'), filename, {
                    name: 'x', namespace: 'x', scopedStyles: !!scoped,
                    ...rest,
                });
                expect(res.map ?? '').toBe(''); // style map stays "" in Phase 0
                const ev = evalStylesheetModule(res.code);
                for (const s of ev.sheets) {
                    expect(s.scoped).toBe(!!scoped);
                    const c = s.css['x|host=false|dir=false'];
                    if (c.includes('[x-host]') || c.includes('[x]')) expect(s.css['-|host=true|dir=false']).not.toMatch(/\[x(-host)?\]/);
                }
                observed = { ok: true, ...ev };
            } catch (e: any) {
                if (!expectError) throw e;
                const expected = JSON.parse(fs.readFileSync(errFile, 'utf8'));
                const exp0 = Array.isArray(expected) ? expected[0] : expected;
                expect(e).toBeInstanceOf(Error);
                // @lwc/compiler wraps into CompilerError; loose: line matches when both have one.
                const line = e.location?.line ?? e.line;
                if (exp0.line != null && line != null) expect(line).toBe(exp0.line);
                observed = { ok: false, name: e.name, line: line ?? null, column: e.location?.column ?? e.column ?? null, expectedName: exp0.name };
            }
            if (expectError) expect(observed.ok).toBe(false);
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
