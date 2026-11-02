// js-eval: compile a JS fixture, append its driver, run the result as a real ES module in a fresh node process,
// and compare stdout + exit code with the JS-oracle golden. Covers runtime semantics an in-process eval can't
// (e.g. private fields through a Proxy: loose lowering vs native #x). Fixture dir: fixtures/js-eval/<name>/
// with input.js, driver.mjs, config.json { compile: [opts...], filename }. One case per compile options object.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { describe, it, expect, beforeAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';

const ROOT = path.resolve(import.meta.dirname, '../fixtures/js-eval');
const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/js-eval');
const UPDATE = !!process.env.UPDATE_GOLDENS;
if (UPDATE && process.env.LWC_COMPILER_BACKEND === 'rust') throw new Error('Refusing UPDATE_GOLDENS with rust backend: goldens are the JS oracle');

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

describe('js-eval behavioral', () => {
    for (const name of fs.readdirSync(ROOT).sort()) {
        const dir = path.join(ROOT, name);
        const cfg = JSON.parse(fs.readFileSync(path.join(dir, 'config.json'), 'utf8'));
        cfg.compile.forEach((opts: any, i: number) => {
            it(`${name}#${i}`, () => {
                const src = fs.readFileSync(path.join(dir, 'input.js'), 'utf8');
                const driver = fs.readFileSync(path.join(dir, cfg.run ?? 'driver.mjs'), 'utf8');
                const { code } = backend.transform(src, cfg.filename ?? 'foo.js', opts);
                const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'lwc-js-eval-'));
                const file = path.join(tmp, 'm.mjs');
                fs.writeFileSync(file, code + '\n' + driver);
                const r = spawnSync(process.execPath, [file], { encoding: 'utf8', timeout: 20000 });
                fs.rmSync(tmp, { recursive: true, force: true });
                const observed = { status: r.status, stdout: r.stdout, stderrFirstLine: r.status === 0 ? '' : (r.stderr.split('\n').find((l) => /Error/.test(l)) ?? '') };
                const g = path.join(GOLDEN_DIR, `${name}#${i}.json`);
                if (UPDATE || !fs.existsSync(g)) {
                    if (!UPDATE) throw new Error(`missing golden ${g} (record with UPDATE_GOLDENS=1 on the JS backend)`);
                    fs.mkdirSync(GOLDEN_DIR, { recursive: true });
                    fs.writeFileSync(g, JSON.stringify(observed, null, 2) + '\n');
                } else {
                    expect(observed).toEqual(JSON.parse(fs.readFileSync(g, 'utf8')));
                }
            });
        });
    }
});
