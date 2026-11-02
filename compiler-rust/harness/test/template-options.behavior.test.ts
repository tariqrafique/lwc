// Row `template-options`: compiler options on the template (and CSS) path not varied by the `template` row.
//   edd/<v>   experimentalDynamicDirective {omitted,false,true} × every template fixture using lwc:dynamic (fixture cfg
//             value removed) + upstream transform-html.spec lwc:dynamic snippet
//   erm/<v>   experimentalErrorRecoveryMode {true,false,omitted} × upstream transform-html.spec experimentalErrorRecoveryMode
//             cases (valid, 3 invalid directives, custom element) + transform-css.spec `<` case
// Goldens: goldens/template-options, JS oracle only (UPDATE_GOLDENS=1, JS backend).
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, beforeAll, expect } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { checkGolden, findFiles, errorObservation, errorMultiset } from '../src/optionRows';
import { observeTemplate, TEMPLATE_ROOT } from '../src/templateObserve';

const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/template-options');
const DYNAMIC = findFiles(TEMPLATE_ROOT, 'actual.html').filter((f) => /lwc:dynamic\b/.test(fs.readFileSync(f, 'utf8'))).map((f) => path.relative(TEMPLATE_ROOT, path.dirname(f)));
const cfgOf = (rel: string) => { const p = path.join(TEMPLATE_ROOT, rel, 'config.json'); return fs.existsSync(p) ? JSON.parse(fs.readFileSync(p, 'utf8')) : {}; };

const SPEC_DYNAMIC = `
                <template>
                    <x-dynamic lwc:dynamic={ctor}></x-dynamic>
                    <x-dynamic-two lwc:dynamic={ctor2}></x-dynamic-two>
                </template>
            `;
const SPEC_ERM: Record<string, string> = {
    valid: `
                <template>
                    <div>Hello</div>
                </template>
            `,
    'invalid-directives': `
                <template>
                    <div lwc:invalid-directive="test"></div>
                    <span lwc:another-invalid="test"></span>
                    <p lwc:yet-another-invalid="test"></p>
                </template>
            `,
    'custom-element': `
                <template>
                    <custom-element lwc:invalid="test"></custom-element>
                </template>
            `,
};

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

const EDD: Array<[string, boolean | undefined]> = [['omitted', undefined], ['false', false], ['true', true]];
describe('template-options: edd', () => {
    it('finds 17 lwc:dynamic fixtures', () => expect(DYNAMIC.length).toBe(17));
    for (const [label, v] of EDD) {
        const set = (o: Record<string, unknown>) => { const { experimentalDynamicDirective: _d, experimentalComputedMemberExpression: _c, ...r } = o; void _d; void _c; return v === undefined ? r : { ...r, experimentalDynamicDirective: v }; };
        it(`edd/${label}/spec/dynamic`, () => checkGolden(GOLDEN_DIR, `edd/${label}/spec/dynamic`, observeTemplate(backend, SPEC_DYNAMIC, 'foo.html', set({ namespace: 'x', name: 'foo' }))));
        for (const rel of DYNAMIC) {
            const name = path.basename(rel);
            it(`edd/${label}/${rel}`, () => checkGolden(GOLDEN_DIR, `edd/${label}/${rel}`,
                observeTemplate(backend, fs.readFileSync(path.join(TEMPLATE_ROOT, rel, 'actual.html'), 'utf8'), `${name}.html`, set({ namespace: 'x', name, ...cfgOf(rel) }))));
        }
    }
});

const ERM: Array<[string, boolean | undefined]> = [['true', true], ['false', false], ['omitted', undefined]];
describe('template-options: erm', () => {
    for (const [label, v] of ERM) {
        const opts = { namespace: 'x', name: 'foo', ...(v === undefined ? {} : { experimentalErrorRecoveryMode: v }) };
        for (const [name, src] of Object.entries(SPEC_ERM)) {
            it(`erm/${label}/html/${name}`, () => checkGolden(GOLDEN_DIR, `erm/${label}/html/${name}`, observeTemplate(backend, src, 'foo.html', opts)));
        }
        it(`erm/${label}/css/lt`, () => {
            let observed: unknown;
            try {
                const res = backend.transform('<', 'foo.css', opts as any);
                observed = { ok: true, hasCode: typeof res.code === 'string' && res.code.length > 0 };
            } catch (e: any) {
                observed = { ...errorObservation(e), errors: errorMultiset(e), multipleCssErrors: String(e?.message ?? '').includes('Multiple CSS errors') };
            }
            checkGolden(GOLDEN_DIR, `erm/${label}/css/lt`, observed);
        });
    }
});
