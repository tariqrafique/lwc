// Row `js-options`: compiler (transformSync) option dimensions over the JS corpus and upstream spec snippets.
// Ports the option-driven cases of @lwc/compiler src/__tests__/transform-javascript.spec.ts and the babel-plugin
// fixtures with the option varied (audit #1–#6, option-coverage-audit.md). Goldens: goldens/js-options, JS oracle only.
//   api/<v>       apiVersion boundary set (JS path reads DISABLE_OBJECT_REST_SPREAD_TRANSFORMATION@60,
//                 SKIP_UNNECESSARY_REGISTER_DECORATORS@60; registerComponent carries the clamped value) × full corpus
//                 + spec snippets. Fixture-pinned apiVersion is overridden.
//   lws/<b>       enableLightningWebSecurityTransforms {true,false} × spec LWS snippet + async fixtures
//   explicit      isExplicitImport true × full corpus
//   sourcemap/<v> outputConfig.sourcemap {omitted,false,true,'inline','invalid',{},123} × spec snippet + 2 fixtures
//   epm/<v>       enablePrivateMethods {omitted,false,true} × the #method fixtures + spec snippet
//   dyn/<v>       dynamicImports {omitted, strictSpecifier omitted, strictSpecifier false} × dynamic-imports fixtures
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, beforeAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { evalJsModule } from '../src/evalJs';
import { checkGolden, errorObservation, findFiles, apiLabel, apiOpt, type ApiCell } from '../src/optionRows';
import { oracleRoot } from '../src/oracleRoot';

const ROOT = `${oracleRoot()}/packages/@lwc/babel-plugin-component/src/__tests__/fixtures`;
const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/js-options');
const fixtures = findFiles(ROOT, 'actual.js').map((f) => path.relative(ROOT, path.dirname(f)));
const readFx = (rel: string, f: string) => { const p = path.join(ROOT, rel, f); return fs.existsSync(p) ? fs.readFileSync(p, 'utf8') : ''; };
const cfgOf = (rel: string) => { const s = readFx(rel, 'config.json').trim(); return s ? JSON.parse(s) : {}; };
/** Same base opts as the `js` row (upstream index.spec: namespace lwc, name test, private methods on). */
function baseOpts(rel: string): Record<string, unknown> {
    const { parserOpts, ...rest } = cfgOf(rel);
    return { namespace: 'lwc', name: 'test', enablePrivateMethods: true, ...rest, ...(parserOpts?.errorRecovery ? { experimentalErrorRecoveryMode: true } : {}) };
}

// Upstream transform-javascript.spec.ts sources.
const SPEC = {
    restSpread: `export const test = { ...a, b: 1 }`,
    decorators: `import { LightningElement, api, track, wire } from 'lwc';\nimport { getFoo } from 'data-service';\nexport default class Test extends LightningElement {\n    @api foo;\n    @track bar = 1;\n    @wire(getFoo, { id: '$foo' }) wired;\n}\n`,
    lws: `
        export const test = window.location.href;
        export async function foo() {
            await bar();
        }
        async function* baz() {
            yield 1;
            yield 2;
        }
        (async function() {
            for await (const num of baz()) {
                break;
            }
        })();
    `,
    component: `import { LightningElement } from 'lwc';\nexport default class Foo extends LightningElement {\n    handle() { return { ...this.a }; }\n}\n`,
    privateMethod: `import { LightningElement } from 'lwc';\nexport default class Foo extends LightningElement {\n    #privateMethod() { return 1; }\n    connectedCallback() { this.#privateMethod(); }\n}\n`,
};

/** Spec-level text signals (the upstream spec asserts these substrings). */
function signals(code: string) {
    return {
        objectSpreadPreserved: /\.\.\.(a|this\.a)\b/.test(code),
        asyncToGenerator: /\b_asyncToGenerator\(/.test(code),
        wrapAsyncGenerator: /\b_wrapAsyncGenerator\(/.test(code),
        asyncIterator: /\b_asyncIterator\(/.test(code),
        locationRewrite: code.includes('(window === globalThis || window === document ? location : window.location).href'),
        nativeAsync: /\basync\s+(function\b|\*)|\bfor\s+await\b/.test(code),
        registerDecoratorsCalls: (code.match(/\b_registerDecorators\(|\bregisterDecorators\(/g) ?? []).length,
        registerComponentCalls: (code.match(/\b_registerComponent\(|\bregisterComponent\(/g) ?? []).length,
    };
}

function mapDesc(map: unknown): unknown {
    if (map === null) return 'null';
    if (map === undefined) return 'undefined';
    if (map === '') return "''";
    const m = typeof map === 'string' ? JSON.parse(map) : map;
    return { version: m.version, hasMappings: typeof m.mappings === 'string' && m.mappings.length > 0, sourcesCount: Array.isArray(m.sources) ? m.sources.length : null };
}
function inlineMap(code: string): unknown {
    const m = code.match(/\/\/# sourceMappingURL=data:application\/json;(?:charset=utf-8;)?base64,([A-Za-z0-9+/=]+)/);
    if (!m) return code.includes('//# sourceMappingURL=data:application/json;') ? { present: true, decodable: false } : null;
    const j = JSON.parse(Buffer.from(m[1], 'base64').toString('utf8'));
    return { present: true, decodable: true, version: j.version, hasMappings: typeof j.mappings === 'string' && j.mappings.length > 0 };
}

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

function observe(src: string, filename: string, opts: Record<string, unknown>, extra: { map?: boolean } = {}) {
    try {
        const res = backend.transform(src, filename, opts as any);
        const warnings = (res.warnings ?? []).map((w: any) => ({ code: w.code, level: w.level }));
        let ev: unknown;
        try { ev = evalJsModule(res.code, { detail: true }); } catch (e: any) { ev = { evalError: e?.name ?? 'Error' }; }
        return { ok: true, eval: ev, warnings, signals: signals(res.code), ...(extra.map ? { map: mapDesc((res as any).map), inlineMap: inlineMap(res.code) } : {}) };
    } catch (e: any) {
        return { ...errorObservation(e), ...(extra.map ? { message: String(e?.message ?? '').split('\n')[0] } : {}) };
    }
}

const API_BOUNDARY: ApiCell[] = [57, 58, 59, 60, 66, 67, 'omitted'];
const withoutApi = (o: Record<string, unknown>) => { const { apiVersion: _drop, ...r } = o; void _drop; return r; };

describe('js-options: api', () => {
    for (const v of API_BOUNDARY) {
        for (const [name, src] of Object.entries(SPEC)) {
            if (name === 'lws') continue;
            it(`api/${apiLabel(v)}/spec/${name}`, () => {
                checkGolden(GOLDEN_DIR, `api/${apiLabel(v)}/spec/${name}`, observe(src, 'foo.js', { namespace: 'x', name: 'foo', enablePrivateMethods: true, ...apiOpt(v) }));
            });
        }
        for (const rel of fixtures) {
            it(`api/${apiLabel(v)}/${rel}`, () => {
                checkGolden(GOLDEN_DIR, `api/${apiLabel(v)}/${rel}`, observe(readFx(rel, 'actual.js'), 'test.js', { ...withoutApi(baseOpts(rel)), ...apiOpt(v) }));
            });
        }
    }
});

const ASYNC_FIXTURES = fixtures.filter((rel) => /\basync\b|\bawait\b/.test(readFx(rel, 'actual.js')));
describe('js-options: lws', () => {
    it('has async fixtures', () => { if (ASYNC_FIXTURES.length === 0) throw new Error('no async fixtures'); });
    for (const b of [true, false]) {
        it(`lws/${b}/spec/lws`, () => checkGolden(GOLDEN_DIR, `lws/${b}/spec/lws`, observe(SPEC.lws, 'foo.js', { namespace: 'x', name: 'foo', enableLightningWebSecurityTransforms: b })));
        for (const rel of ASYNC_FIXTURES) {
            it(`lws/${b}/${rel}`, () => checkGolden(GOLDEN_DIR, `lws/${b}/${rel}`, observe(readFx(rel, 'actual.js'), 'test.js', { ...baseOpts(rel), enableLightningWebSecurityTransforms: b })));
        }
    }
});

describe('js-options: explicit', () => {
    it('explicit/spec/decorators', () => checkGolden(GOLDEN_DIR, 'explicit/spec/decorators', observe(SPEC.decorators, 'foo.js', { namespace: 'x', name: 'foo', isExplicitImport: true })));
    for (const rel of fixtures) {
        it(`explicit/${rel}`, () => checkGolden(GOLDEN_DIR, `explicit/${rel}`, observe(readFx(rel, 'actual.js'), 'test.js', { ...baseOpts(rel), isExplicitImport: true })));
    }
});

const SOURCEMAPS: Array<[string, unknown]> = [['omitted', undefined], ['false', false], ['true', true], ['inline', 'inline'], ['invalid', 'invalid'], ['object', {}], ['number', 123]];
const SOURCEMAP_FIXTURES = ['component/camel-to-kebab-component-name', 'private-methods/private-method-with-params'].filter((r) => fixtures.includes(r));
describe('js-options: sourcemap', () => {
    it('has sourcemap fixtures', () => { if (SOURCEMAP_FIXTURES.length !== 2) throw new Error(`sourcemap fixtures missing: ${SOURCEMAP_FIXTURES}`); });
    for (const [label, sm] of SOURCEMAPS) {
        const oc = sm === undefined ? {} : { outputConfig: { sourcemap: sm } };
        it(`sourcemap/${label}/spec/component`, () => checkGolden(GOLDEN_DIR, `sourcemap/${label}/spec/component`, observe(SPEC.component, 'foo.js', { namespace: 'x', name: 'foo', ...oc }, { map: true })));
        for (const rel of SOURCEMAP_FIXTURES) {
            it(`sourcemap/${label}/${rel}`, () => checkGolden(GOLDEN_DIR, `sourcemap/${label}/${rel}`, observe(readFx(rel, 'actual.js'), 'test.js', { ...baseOpts(rel), ...oc }, { map: true })));
        }
    }
});

const PRIVATE_FIXTURES = fixtures.filter((rel) => /#\w+\s*\(/.test(readFx(rel, 'actual.js')));
describe('js-options: epm', () => {
    it('finds 23 #method fixtures', () => { if (PRIVATE_FIXTURES.length !== 23) throw new Error(`expected 23, got ${PRIVATE_FIXTURES.length}`); });
    for (const [label, val] of [['omitted', undefined], ['false', false], ['true', true]] as const) {
        const set = (o: Record<string, unknown>) => { const { enablePrivateMethods: _d, ...r } = o; void _d; return val === undefined ? r : { ...r, enablePrivateMethods: val }; };
        it(`epm/${label}/spec/privateMethod`, () => checkGolden(GOLDEN_DIR, `epm/${label}/spec/privateMethod`, observe(SPEC.privateMethod, 'foo.js', set({ namespace: 'x', name: 'foo' }))));
        for (const rel of PRIVATE_FIXTURES) {
            it(`epm/${label}/${rel}`, () => checkGolden(GOLDEN_DIR, `epm/${label}/${rel}`, observe(readFx(rel, 'actual.js'), 'test.js', set(baseOpts(rel)))));
        }
    }
});

const DYN_FIXTURES = fixtures.filter((rel) => rel.startsWith('dynamic-imports/'));
describe('js-options: dyn', () => {
    it('finds 5 dynamic-imports fixtures', () => { if (DYN_FIXTURES.length !== 5) throw new Error(`expected 5, got ${DYN_FIXTURES.length}`); });
    const variants: Array<[string, (o: any) => any]> = [
        ['dynamicImports-omitted', ({ dynamicImports: _d, ...r }) => r],
        ['strict-omitted', ({ dynamicImports, ...r }) => { const { strictSpecifier: _s, ...di } = dynamicImports ?? {}; return { ...r, dynamicImports: di }; }],
        ['strict-false', ({ dynamicImports, ...r }) => ({ ...r, dynamicImports: { ...(dynamicImports ?? {}), strictSpecifier: false } })],
    ];
    for (const [label, f] of variants) {
        for (const rel of DYN_FIXTURES) {
            it(`dyn/${label}/${rel}`, () => checkGolden(GOLDEN_DIR, `dyn/${label}/${rel}`, observe(readFx(rel, 'actual.js'), 'test.js', f(baseOpts(rel)))));
        }
    }
});
