// Row `ssr_dynamic_imports` (underscores: board.sh routes `ssr-*` rows to ssr.behavior.test.ts): port of
// @lwc/ssr-compiler src/__tests__/dynamic-imports.spec.ts through transformSync (targetSSR) via the CompilerBackend.
//   strict {false,true,omitted} × loader {unset,'myLoader'} × specifier {literal "x/foo", identifier woohoo}
//   + dynamicImports omitted × specifier, + numeric literal (strict) + three imports hoisting the loader once.
// Observed: ok / error codes, static import specifiers (es-module-lexer), dynamic import count, loader import count.
// Goldens: goldens/ssr_dynamic_imports, JS oracle only.
import path from 'node:path';
import { createRequire } from 'node:module';
import { describe, it, beforeAll, expect } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { checkGolden, errorObservation } from '../src/optionRows';
import { oracleRoot } from '../src/oracleRoot';

const lexer = createRequire(`${oracleRoot()}/package.json`)('es-module-lexer');
const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/ssr_dynamic_imports');
const FILENAME = path.resolve('component.js');
const src = (body: string) => `
                import { LightningElement } from 'lwc';
                export default class extends LightningElement {}
                export default async function rando () {
${body}
                }
            `;

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); await lexer.init; });

function observe(source: string, dynamicImports: Record<string, unknown> | undefined) {
    try {
        const { code } = backend.transform(source, FILENAME, { namespace: 'x', name: 'component', targetSSR: true, ...(dynamicImports ? { dynamicImports } : {}) } as any);
        const [imports] = lexer.parse(code);
        // deduped: how many import statements name one module is printer layout, not behavior
        const statics = [...new Set<string>(imports.filter((i: any) => i.d === -1).map((i: any) => i.n))].sort();
        const dynamics = imports.filter((i: any) => i.d > -1).map((i: any) => i.n ?? null);
        return {
            ok: true, staticImports: statics, dynamicImports: dynamics,
            loaderImports: imports.filter((i: any) => i.n === 'myLoader').length,
            mentions: ['x/foo', 'x/bar', 'x/baz'].filter((s) => code.includes(s)),
        };
    } catch (e: any) {
        return errorObservation(e);
    }
}

describe('ssr dynamic imports', () => {
    for (const strict of [false, true, 'omitted'] as const) {
        for (const loader of [undefined, 'myLoader']) {
            for (const isStrict of [false, true]) {
                const key = `strict-${strict}/loader-${loader ?? 'unset'}/${isStrict ? 'literal' : 'identifier'}`;
                it(key, () => {
                    const di: Record<string, unknown> = { ...(loader === undefined ? {} : { loader }), ...(strict === 'omitted' ? {} : { strictSpecifier: strict }) };
                    const o = observe(src(`                    await import(${isStrict ? '"x/foo"' : 'woohoo'});`), di);
                    // upstream spec assertions (strict defaults to true when omitted)
                    const effStrict = strict !== false;
                    if (effStrict && !isStrict) expect(o.ok ? [] : (o as any).codes).toContain(1121);
                    else if (o.ok) expect((o as any).loaderImports > 0).toBe(!!loader);
                    checkGolden(GOLDEN_DIR, key, o);
                });
            }
        }
    }
    for (const isStrict of [false, true]) {
        it(`dynamicImports-omitted/${isStrict ? 'literal' : 'identifier'}`, () =>
            checkGolden(GOLDEN_DIR, `dynamicImports-omitted/${isStrict ? 'literal' : 'identifier'}`, observe(src(`                    await import(${isStrict ? '"x/foo"' : 'woohoo'});`), undefined)));
    }
    it('numeric-literal-strict', () => {
        const o = observe(src('                    await import(123);'), { loader: 'myLoader', strictSpecifier: true });
        expect(o.ok ? [] : (o as any).codes).toContain(1121);
        checkGolden(GOLDEN_DIR, 'numeric-literal-strict', o);
    });
    it('hoisted-once', () => {
        const o = observe(src(`                    await import('x/foo');\n                    await import('x/bar');\n                    await import('x/baz');`), { loader: 'myLoader', strictSpecifier: true });
        expect((o as any).loaderImports).toBe(1);
        expect((o as any).mentions).toEqual(['x/foo', 'x/bar', 'x/baz']);
        checkGolden(GOLDEN_DIR, 'hoisted-once', o);
    });
});
