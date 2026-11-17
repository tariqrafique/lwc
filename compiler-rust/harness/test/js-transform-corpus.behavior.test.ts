// Row `js-transform-corpus` (gating): compile each component's class module on
// the current backend. The JS backend records the transform output as the
// golden. The Rust backend compares that golden with `semdiff pair` (semantic
// equivalence, not byte identity). A non-equivalent id must be listed in
// harness/js-transform-corpus.diffs. The list cannot gain an id: a new diff
// fails the test. An id that is now equivalent fails too, so the list only
// shrinks. harness/ratchets/js-transform-corpus.txt is that count; ratchet-ceiling
// refuses a raise.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { discoverBundles, type LbcBundle } from '../src/lbcBundle';
import { UPDATE, readGolden, writeGolden, goldenName, errorObservation } from '../src/optionRows';
import { canonOraclePath } from '../src/oracleRoot';
import { jsTransformRoots } from '../src/upstreamRoots';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '../..');
const GOLDEN_DIR = path.resolve(here, '../goldens/js-transform-corpus');
const DIFFS = path.resolve(here, '../js-transform-corpus.diffs');
const RATCHET = path.resolve(here, '../ratchets/js-transform-corpus.txt');
const bundles = discoverBundles(jsTransformRoots());

type Ok = { ok: true; code: string };
type Err = { ok: false; name: string; codes: number[]; message: string };
type Obs = Ok | Err;

const diffsThisRun = new Set<string>();
let backend: CompilerBackend;
let semdiff: string | null = null;

function semdiffBin(): string {
    const dirs: string[] = [];
    if (process.env.CARGO_TARGET_DIR) dirs.push(process.env.CARGO_TARGET_DIR);
    dirs.push(path.join(repoRoot, 'target'));
    for (const dir of dirs) {
        const p = path.join(dir, 'release/semdiff');
        if (fs.existsSync(p)) return p;
    }
    throw new Error('semdiff binary missing (release/semdiff). The board builds it before this row.');
}

function observe(bundle: LbcBundle): Obs {
    const src = fs.readFileSync(bundle.js, 'utf8');
    const filename = path.basename(bundle.js);
    try {
        const res = backend.transform(src, filename, { name: bundle.name, namespace: bundle.namespace });
        return { ok: true, code: res.code };
    } catch (e: any) {
        const err = errorObservation(e);
        const message = canonOraclePath(String(e?.message ?? '')).split(process.cwd()).join('<cwd>');
        return { ok: false, name: err.name, codes: err.codes, message };
    }
}

function equivalent(oracleCode: string, rustCode: string): boolean {
    const dir = fs.mkdtempSync(path.join('/tmp', 'js-tx-'));
    const a = path.join(dir, 'oracle.js');
    const b = path.join(dir, 'rust.js');
    fs.writeFileSync(a, oracleCode);
    fs.writeFileSync(b, rustCode);
    const r = spawnSync(semdiff!, [ 'pair', a, b ], { encoding: 'utf8' });
    fs.rmSync(dir, { recursive: true, force: true });
    if (r.status !== 0) throw new Error(`semdiff pair rc=${r.status}: ${(r.stderr || r.stdout || '').slice(0, 400)}`);
    const verdict = JSON.parse(r.stdout);
    return verdict.class === 'EQUIVALENT-BY-REWRITE';
}

function readDiffs(): string[] {
    if (!fs.existsSync(DIFFS)) return [];
    return fs.readFileSync(DIFFS, 'utf8').split('\n').map((s) => s.trim()).filter(Boolean);
}

describe('js-transform-corpus', () => {
    beforeAll(async () => {
        backend = await getBackend();
        if (backend.id !== 'js') semdiff = semdiffBin();
    });

    it('discovers every component class', () => {
        const count = (c: string) => bundles.filter((b) => b.corpus === c).length;
        expect({
            lbc: count('lbc'),
            'lbc-recipes': count('lbc-recipes'),
            'integration-wtr': count('integration-wtr'),
            'integration-wdio': count('integration-wdio'),
            'integration-types': count('integration-types'),
            'rollup-plugin': count('rollup-plugin'),
            'perf-benchmarks-components': count('perf-benchmarks-components'),
            'engine-server': count('engine-server'),
            playground: count('playground'),
        }).toEqual({
            lbc: 396,
            'lbc-recipes': 126,
            'integration-wtr': 1269,
            'integration-wdio': 119,
            'integration-types': 4,
            'rollup-plugin': 22,
            'perf-benchmarks-components': 15,
            'engine-server': 4,
            playground: 2,
        });
        expect(bundles.length).toBeGreaterThan(0);
    });

    for (const bundle of bundles) {
        it(bundle.id, () => {
            const observed = observe(bundle);
            if (backend.id === 'js') {
                const g = path.join(GOLDEN_DIR, goldenName(bundle.id));
                if (UPDATE) writeGolden(g, observed, 0);
                else expect(observed).toEqual(readGolden(g));
                return;
            }
            const golden = readGolden(path.join(GOLDEN_DIR, goldenName(bundle.id))) as Obs;
            if (golden.ok && observed.ok) {
                if (!equivalent(golden.code, observed.code)) diffsThisRun.add(bundle.id);
                return;
            }
            if (!golden.ok && !observed.ok) {
                if (JSON.stringify(golden.codes) !== JSON.stringify(observed.codes)) diffsThisRun.add(bundle.id);
                return;
            }
            throw new Error(`${bundle.id}: one backend compiled and the other threw`);
        });
    }
});

afterAll(() => {
    if (!backend || backend.id === 'js') return;
    const got = [...diffsThisRun].sort();
    if (process.env.JS_TX_WRITE_DIFFS === '1') {
        fs.mkdirSync(path.dirname(RATCHET), { recursive: true });
        fs.writeFileSync(DIFFS, got.length ? got.join('\n') + '\n' : '');
        fs.writeFileSync(RATCHET, `${got.length}\n`);
        return;
    }
    const allowed = readDiffs();
    const want = [...allowed].sort();
    const added = got.filter((id) => !want.includes(id));
    const removed = want.filter((id) => !got.includes(id));
    expect({ added, removed, count: got.length }).toEqual({ added: [], removed: [], count: want.length });
    const ratchet = fs.readFileSync(RATCHET, 'utf8').trim();
    expect(ratchet).toBe(String(want.length));
});
