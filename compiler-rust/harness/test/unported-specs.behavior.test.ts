// Row `unported-specs` (gating): compiler-observable cases from the unit specs
// that were not fixture corpora. Each case goes through the backend seam
// (transform) and is compared to the JS golden. Specs that only call a
// JS-internal helper (normalizeConfig, parse(), estemplate, the private-method
// Babel pipeline stages, validateTransformOptions latches) are named in
// harness/coverage-proposals.tsv and are not cases here.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { checkGolden, errorObservation, goldenName, readGolden } from '../src/optionRows';
import { canonOraclePath } from '../src/oracleRoot';
import path from 'node:path';

const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/unported-specs');
const DIFFS = path.resolve(import.meta.dirname, '../unported-specs.diffs');
const RATCHET = path.resolve(import.meta.dirname, '../ratchets/unported-specs.txt');
const diffsThisRun = new Set<string>();
let backend: CompilerBackend;
let semdiff = '';
beforeAll(async () => {
    backend = await getBackend();
    if (backend.id !== 'js') {
        const dirs = [process.env.CARGO_TARGET_DIR, path.resolve(import.meta.dirname, '../../target')].filter(Boolean) as string[];
        for (const dir of dirs) {
            const p = path.join(dir, 'release/semdiff');
            if (fs.existsSync(p)) { semdiff = p; break; }
        }
        if (!semdiff) throw new Error('semdiff binary missing');
    }
});

function equivalent(oracleCode: string, rustCode: string): boolean {
    const dir = fs.mkdtempSync(path.join('/tmp', 'unported-sd-'));
    const a = path.join(dir, 'oracle.js');
    const b = path.join(dir, 'rust.js');
    fs.writeFileSync(a, oracleCode);
    fs.writeFileSync(b, rustCode);
    const r = spawnSync(semdiff, ['pair', a, b], { encoding: 'utf8' });
    fs.rmSync(dir, { recursive: true, force: true });
    if (r.status !== 0) throw new Error(`semdiff pair rc=${r.status}: ${(r.stderr || '').slice(0, 300)}`);
    return JSON.parse(r.stdout).class === 'EQUIVALENT-BY-REWRITE';
}

function run(src: string, filename: string, opts: Record<string, unknown>): unknown {
    try {
        const res = backend.transform(src, filename, { name: 'foo', namespace: 'x', ...opts });
        return {
            ok: true,
            code: res.code,
            warnings: (res.warnings ?? []).map((w) => ({ code: w.code, message: w.message })),
        };
    } catch (e: any) {
        const err = errorObservation(e);
        return { ok: false, name: err.name, codes: err.codes, message: canonOraclePath(String(e?.message ?? '')) };
    }
}

const IMG = '<template><img src="http://example.com/img.png" crossorigin="anonymous"></template>';
const CLASS = `import { LightningElement } from 'lwc';
export default class extends LightningElement {}
`;

describe('unported-specs', () => {
    const cases: Array<{ id: string; src: string; filename: string; opts?: Record<string, unknown> }> = [
        { id: 'template-index/api58-extra-close', src: '<template></template></template>', filename: 'x.html', opts: { apiVersion: 58 } },
        { id: 'template-index/api59-extra-close', src: '<template></template></template>', filename: 'x.html', opts: { apiVersion: 59 } },
        { id: 'template-index/static-on', src: IMG, filename: 'x.html', opts: { enableStaticContentOptimization: true } },
        { id: 'template-index/static-off', src: IMG, filename: 'x.html', opts: { enableStaticContentOptimization: false } },
        { id: 'collect-params/object', src: '<template><button onclick="{({ a, b }) => a + b}"></button></template>', filename: 'x.html', opts: { experimentalComplexExpressions: true } },
        { id: 'collect-params/array-hole', src: '<template><button onclick="{([, y]) => y}"></button></template>', filename: 'x.html', opts: { experimentalComplexExpressions: true } },
        { id: 'collect-params/rest', src: '<template><button onclick="{(...rest) => rest.length}"></button></template>', filename: 'x.html', opts: { experimentalComplexExpressions: true } },
        { id: 'collect-params/default-rejected', src: '<template><button onclick="{(x = 1) => x}"></button></template>', filename: 'x.html', opts: { experimentalComplexExpressions: true } },
        { id: 'ssr-compilation/implicit-template-import', src: CLASS, filename: 'component.js', opts: { targetSSR: true } },
        { id: 'ssr-compilation/ts-template-import', src: CLASS, filename: 'component.ts', opts: { targetSSR: true } },
        { id: 'ssr-compilation/version-comment', src: CLASS, filename: 'component.js', opts: { targetSSR: true } },
        {
            id: 'ssr-compilation/wire-and-track',
            filename: 'test.js',
            opts: { targetSSR: true },
            src: `import { track, wire, LightningElement } from "lwc";
import { getFoo } from "data-service";
export default class Test extends LightningElement {
  @track
  @wire(getFoo, { key1: "$prop1", key2: ["fixed", "array"] })
  wiredWithTrack;
}
`,
        },
        {
            id: 'ssr-compilation/wire-and-api',
            filename: 'test.js',
            opts: { targetSSR: true },
            src: `import { api, wire, LightningElement } from "lwc";
import { getFoo } from "data-service";
export default class Test extends LightningElement {
  @api
  @wire(getFoo, { key1: "$prop1", key2: ["fixed"] })
  wiredWithApi() {}
}
`,
        },
        {
            id: 'ssr-compilation/wire-computed',
            filename: 'test.js',
            opts: { targetSSR: true },
            src: `import { wire, LightningElement } from "lwc";
import { getFoo } from "data-service";
const symbol = Symbol.for("key");
export default class Test extends LightningElement {
  @wire(getFoo, { [[symbol]]: "$prop1", key2: ["fixed", "array"] })
  wiredFoo;
}
`,
        },
        { id: 'ssr-template/version-comment', src: '<template></template>', filename: 'component.html', opts: { targetSSR: true } },
        {
            id: 'ssr-template/iterator-nested',
            filename: 'component.html',
            opts: { targetSSR: true },
            src: `<template>
            <template iterator:it={items.nested.data}>
                <li key={it.value}>{it.value}</li>
            </template>
        </template>`,
        },
        {
            id: 'api-decorator/track',
            filename: 'test.js',
            opts: { targetSSR: true },
            src: `import { api, track, LightningElement } from "lwc";
export default class Test extends LightningElement {
  @track
  @api
  apiWithTrack = "foo";
}
`,
        },
        {
            id: 'api-decorator/bool-default-true',
            filename: 'test.js',
            opts: { targetSSR: true },
            src: `import { api, LightningElement } from "lwc";
export default class Test extends LightningElement {
  @api publicProp = true;
}
`,
        },
        {
            id: 'api-decorator/data-prefix',
            filename: 'test.js',
            opts: { targetSSR: true },
            src: `import { api, LightningElement } from "lwc";
export default class Test extends LightningElement {
  @api dataFooBar;
}
`,
        },
        {
            id: 'api-decorator/on-prefix',
            filename: 'test.js',
            opts: { targetSSR: true },
            src: `import { api, LightningElement } from "lwc";
export default class Test extends LightningElement {
  @api onChangeHandler;
}
`,
        },
        {
            id: 'api-decorator/part',
            filename: 'test.js',
            opts: { targetSSR: true },
            src: `import { api, LightningElement } from "lwc";
export default class Test extends LightningElement {
  @api part;
}
`,
        },
        { id: 'transform/returns-code', src: `console.log('Hello')`, filename: 'foo.js' },
        {
            id: 'warnings/empty-namespace',
            filename: 'foo.js',
            opts: { name: '', namespace: '' },
            src: `import { LightningElement } from 'lwc';
export default class extends LightningElement {};
`,
        },
    ];

    it('ports the compiler-observable unit specs', () => {
        expect(cases.length).toBe(23);
    });

    for (const c of cases) {
        it(c.id, () => {
            const observed = run(c.src, c.filename, c.opts ?? {}) as { ok: boolean; code?: string; warnings?: unknown };
            if (process.env.LWC_COMPILER_BACKEND !== 'rust') {
                checkGolden(GOLDEN_DIR, c.id, observed);
                return;
            }
            const want = readGolden(path.join(GOLDEN_DIR, goldenName(c.id))) as { ok: boolean; code?: string; warnings?: unknown };
            expect(observed.ok).toBe(want.ok);
            expect(observed.warnings ?? []).toEqual(want.warnings ?? []);
            if (!observed.ok || !want.ok) {
                expect(observed).toEqual(want);
                return;
            }
            if (!equivalent(want.code ?? '', observed.code ?? '')) diffsThisRun.add(c.id);
        });
    }

    afterAll(() => {
        if (!backend || backend.id === 'js') return;
        const got = [...diffsThisRun].sort();
        if (process.env.UNPORTED_WRITE_DIFFS === '1') {
            fs.mkdirSync(path.dirname(RATCHET), { recursive: true });
            fs.writeFileSync(DIFFS, got.length ? got.join('\n') + '\n' : '');
            fs.writeFileSync(RATCHET, `${got.length}\n`);
            return;
        }
        const want = fs.readFileSync(DIFFS, 'utf8').split('\n').map((s) => s.trim()).filter(Boolean).sort();
        const added = got.filter((id) => !want.includes(id));
        const removed = want.filter((id) => !got.includes(id));
        expect({ added, removed }).toEqual({ added: [], removed: [] });
        expect(fs.readFileSync(RATCHET, 'utf8').trim()).toBe(String(want.length));
    });
});
