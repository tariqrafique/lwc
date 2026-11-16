// Row `template-options`: compiler options on the template (and CSS) path not varied by the `template` row.
//   edd/<v>   experimentalDynamicDirective {omitted,false,true} × every template fixture using lwc:dynamic (fixture cfg
//             value removed) + upstream transform-html.spec lwc:dynamic snippet
//   inst/<shape>/<recovery>/<snippet>  instrumentation (AB-0008 G2b): real callbacks and non-callable shapes
//             (src/instrumentationObserve.ts) × metric snippets (lwc:spread, lwc:render-mode, lwc:dynamic) × erm
//   inst/real/fixture/<rel>  real callbacks × every template fixture using lwc:spread / lwc:render-mode / lwc:dynamic
//   inst/throw-<value>/<recovery>/<snippet>  incrementCounter throws <value> (AB-0008 G2b r4) × 4 metric snippets × erm
//   inst/throw-coded-<value>/<recovery>/<snippet>  a thrown object with lwcCode / code / level (AB-0008 r5 D3),
//             observed with levels and messages
//   inst/ssr/<shape>/<snippet>, inst/ssr-throw-<value>/<snippet>  targetSSR: the parse has no catch, so the
//             thrown value reaches the caller unchanged (AB-0008 r5 D2) × the parser metric snippets
//   warn/<case>  console.warn (outputConfig.minify deprecation) vs counter call order (AB-0008 r5 D1)
//   warn/warn-throws-<value>/<snippet>  console.warn throws: validation stops, nothing compiles, the value is
//             rethrown as is (carry-forward #10 a)
//   null/<case>  explicit null vs omitted/undefined for stylesheetConfig, outputConfig, outputConfig.minify
//             (carry-forward #10 c); file order matters for the once-per-process stylesheetConfig warn
//   ssr-stamp/<mode>/<case>  targetSSR × ssrMode {sync,async,asyncYield} × templates: the LWC version stamp in the
//             compiled template (compile-template/index.ts:141-150 puts it last in `__lwcTmpl`'s body; engine-core
//             checkVersionMismatch reads it from the function's toString()) (PLAN item 18 F7)
//   parse-order/<erm>/<case>  malformed HTML mixing tokenizer, tree-construction and preprocessor parse errors: the thrown
//             error's message + location at the default apiVersion, and the full ordered errors[] under
//             experimentalErrorRecoveryMode (parse5 reports in occurrence order; PLAN item 18 F11, customHeaderCpe)
//   erm/<v>   experimentalErrorRecoveryMode {true,false,omitted} × upstream transform-html.spec experimentalErrorRecoveryMode
//             cases (valid, 3 invalid directives, custom element) + transform-css.spec `<` case
// Goldens: goldens/template-options, JS oracle only (UPDATE_GOLDENS=1, JS backend).
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, beforeAll, expect } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { checkGolden, findFiles, errorObservation, errorMultiset } from '../src/optionRows';
import { observeAggregate } from '../src/compareError';
import { observeTemplate, TEMPLATE_ROOT } from '../src/templateObserve';
import {
    CODED_THROWING_SHAPES, INSTRUMENTATION_SHAPES, THROWING_SHAPES, THROWING_WARNS, observeInstrumented, observeRethrow,
    observeWarnOrder, observeWarnThrow,
} from '../src/instrumentationObserve';

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

// AB-0008 G2b: the template metrics (parser/index.ts:898 render-mode, :1106 dynamic; codegen/index.ts:605 spread).
const INST_SNIPPETS: Record<string, string> = {
    spread: '<template><div lwc:spread={p}></div><x-a lwc:spread={q}></x-a></template>',
    'render-mode': '<template lwc:render-mode="light"><div></div></template>',
    dynamic: SPEC_DYNAMIC,
    mixed: '<template lwc:render-mode="light"><x-a lwc:dynamic={c}></x-a><div lwc:spread={p}></div><x-b lwc:dynamic={d}></x-b></template>',
    'parse-error-then-spread': '<template><div lwc:spread={p}></div><div lwc:invalid-directive="x"></div></template>',
    plain: '<template><div>hi</div></template>',
};
const INST_FIXTURES = findFiles(TEMPLATE_ROOT, 'actual.html').filter((f) => /lwc:(spread|render-mode|dynamic)\b/.test(fs.readFileSync(f, 'utf8'))).map((f) => path.relative(TEMPLATE_ROOT, path.dirname(f)));
describe('template-options: inst', () => {
    for (const [shape, make] of Object.entries(INSTRUMENTATION_SHAPES)) {
        for (const [label, erm] of [['erm-false', false], ['erm-true', true]] as const) {
            for (const [name, src] of Object.entries(INST_SNIPPETS)) {
                const key = `inst/${shape}/${label}/${name}`;
                it(key, () => checkGolden(GOLDEN_DIR, key, observeInstrumented(backend, src, 'foo.html',
                    { namespace: 'x', name: 'foo', experimentalDynamicDirective: true, experimentalErrorRecoveryMode: erm }, make)));
            }
        }
    }
    // G2b r4: a throwing callback ends the compile with the oracle's coded error (spread.html repro: codegen).
    for (const [shape, make] of Object.entries(THROWING_SHAPES)) {
        for (const [label, erm] of [['erm-false', false], ['erm-true', true]] as const) {
            for (const name of ['spread', 'render-mode', 'dynamic', 'mixed']) {
                const key = `inst/throw-${shape}/${label}/${name}`;
                it(key, () => checkGolden(GOLDEN_DIR, key, observeInstrumented(backend, INST_SNIPPETS[name], 'spread.html',
                    { namespace: 'x', name: 'spread', experimentalDynamicDirective: true, experimentalErrorRecoveryMode: erm }, make)));
            }
        }
    }
    // r5 D3: coded thrown objects through the template-compiler catch (getCodeFromError, convertErrorToDiagnostic).
    for (const [shape, make] of Object.entries(CODED_THROWING_SHAPES)) {
        for (const [label, erm] of [['erm-false', false], ['erm-true', true]] as const) {
            for (const name of ['spread', 'render-mode']) {
                const key = `inst/throw-coded-${shape}/${label}/${name}`;
                it(key, () => checkGolden(GOLDEN_DIR, key, observeInstrumented(backend, INST_SNIPPETS[name], 'spread.html',
                    { namespace: 'x', name: 'spread', experimentalDynamicDirective: true, experimentalErrorRecoveryMode: erm }, make, { detail: true })));
            }
        }
    }
    // r5 D2: targetSSR (only the parser metrics run; SSR has its own codegen).
    const ssrOpts = { namespace: 'x', name: 'spread', targetSSR: true, experimentalDynamicDirective: true };
    for (const name of ['render-mode', 'dynamic']) {
        for (const [shape, make] of Object.entries(INSTRUMENTATION_SHAPES)) {
            const key = `inst/ssr/${shape}/${name}`;
            it(key, () => checkGolden(GOLDEN_DIR, key, observeRethrow(backend, INST_SNIPPETS[name], 'spread.html', ssrOpts, make)));
        }
        for (const [shape, make] of Object.entries({ ...THROWING_SHAPES, ...CODED_THROWING_SHAPES })) {
            const key = `inst/ssr-throw-${shape}/${name}`;
            it(key, () => checkGolden(GOLDEN_DIR, key, observeRethrow(backend, INST_SNIPPETS[name], 'spread.html', ssrOpts, make)));
        }
    }
    it('finds 53 metric fixtures', () => expect(INST_FIXTURES.length).toBe(53));
    for (const rel of INST_FIXTURES) {
        const name = path.basename(rel);
        it(`inst/real/fixture/${rel}`, () => checkGolden(GOLDEN_DIR, `inst/real/fixture/${rel}`,
            observeInstrumented(backend, fs.readFileSync(path.join(TEMPLATE_ROOT, rel, 'actual.html'), 'utf8'), `${name}.html`,
                { namespace: 'x', name, ...cfgOf(rel) }, INSTRUMENTATION_SHAPES.real)));
    }
});

// AB-0008 r5 D1: the outputConfig.minify deprecation warns on every call (no latch), at validation.
describe('template-options: warn', () => {
    const minify = { namespace: 'x', name: 'spread', experimentalDynamicDirective: true, outputConfig: { minify: true } };
    for (const name of ['spread', 'render-mode', 'mixed']) {
        it(`warn/minify/${name}`, () => checkGolden(GOLDEN_DIR, `warn/minify/${name}`,
            observeWarnOrder(backend, INST_SNIPPETS[name], 'spread.html', minify, INSTRUMENTATION_SHAPES.real)));
        it(`warn/minify-throw/${name}`, () => checkGolden(GOLDEN_DIR, `warn/minify-throw/${name}`,
            observeWarnOrder(backend, INST_SNIPPETS[name], 'spread.html', minify, THROWING_SHAPES.error)));
    }
    for (const name of ['render-mode', 'dynamic']) {
        it(`warn/minify-ssr/${name}`, () => checkGolden(GOLDEN_DIR, `warn/minify-ssr/${name}`,
            observeWarnOrder(backend, INST_SNIPPETS[name], 'spread.html', { ...minify, targetSSR: true }, INSTRUMENTATION_SHAPES.real)));
    }
    it('warn/minify-invalid-sourcemap/render-mode', () => checkGolden(GOLDEN_DIR, 'warn/minify-invalid-sourcemap/render-mode',
        observeWarnOrder(backend, INST_SNIPPETS['render-mode'], 'spread.html',
            { ...minify, outputConfig: { minify: true, sourcemap: 'bogus' } }, INSTRUMENTATION_SHAPES.real)));
    // Carry-forward #10 (a): a throwing console.warn aborts at validation (no counter call, raw value rethrown).
    for (const [value, make] of Object.entries(THROWING_WARNS)) {
        for (const name of ['spread', 'render-mode', 'mixed']) {
            it(`warn/warn-throws-${value}/${name}`, () => checkGolden(GOLDEN_DIR, `warn/warn-throws-${value}/${name}`,
                observeWarnThrow(backend, INST_SNIPPETS[name], 'spread.html', minify, make)));
        }
        it(`warn/warn-throws-${value}/ssr-render-mode`, () => checkGolden(GOLDEN_DIR, `warn/warn-throws-${value}/ssr-render-mode`,
            observeWarnThrow(backend, INST_SNIPPETS['render-mode'], 'spread.html', { ...minify, targetSSR: true }, make)));
    }
});

// Carry-forward #10 (c): upstream checks `!isUndefined` (options.ts:189, :198, :215), so an explicit null is passed:
// stylesheetConfig null warns (once per process, so `undefined` goes first and the repeat shows the latch),
// outputConfig null throws reading `sourcemap`, outputConfig.minify null warns. Omitted and undefined do nothing.
describe('template-options: null', () => {
    const base = { namespace: 'x', name: 'spread', experimentalDynamicDirective: true };
    const cases: Array<[string, Record<string, unknown>]> = [
        ['undefined', { stylesheetConfig: undefined, outputConfig: { minify: undefined, sourcemap: undefined } }],
        ['output-config-undefined', { outputConfig: undefined }],
        ['stylesheet-config', { stylesheetConfig: null }],
        ['stylesheet-config-latched', { stylesheetConfig: null }],
        ['output-config', { outputConfig: null }],
        ['output-config-ssr', { outputConfig: null, targetSSR: true }],
        ['minify', { outputConfig: { minify: null } }],
        ['minify-invalid-sourcemap', { outputConfig: { minify: null, sourcemap: 'bogus' } }],
    ];
    for (const [label, opts] of cases) {
        it(`null/${label}/render-mode`, () => checkGolden(GOLDEN_DIR, `null/${label}/render-mode`,
            observeWarnOrder(backend, INST_SNIPPETS['render-mode'], 'spread.html', { ...base, ...opts }, INSTRUMENTATION_SHAPES.real)));
    }
});

// PLAN item 18 F7: the stamp count, and the lines that close `__lwcTmpl` (stamp, then `}`), trimmed so the printers'
// indentation does not matter.
describe('template-options: ssr-stamp', () => {
    const templates: Record<string, string> = {
        empty: '<template></template>',
        text: '<template><div>hi {name}</div></template>',
        light: '<template lwc:render-mode="light"><p>x</p></template>',
        nested: '<template><template if:true={a}><span>a</span></template><x-child></x-child></template>',
    };
    for (const mode of ['sync', 'async', 'asyncYield']) {
        for (const [name, src] of Object.entries(templates)) {
            it(`ssr-stamp/${mode}/${name}`, () => {
                let observed: unknown;
                try {
                    const code = backend.transform(src, 'stamp.html', { namespace: 'x', name: 'stamp', targetSSR: true, ssrMode: mode } as any).code;
                    const lines = code.split('\n').map((l) => l.trim());
                    const end = lines.findIndex((l) => l.startsWith('__lwcTmpl.'));
                    observed = { ok: true, stamps: code.split('/*LWC compiler v').length - 1, close: end < 2 ? null : lines.slice(end - 2, end) };
                } catch (e: any) { observed = errorObservation(e); }
                checkGolden(GOLDEN_DIR, `ssr-stamp/${mode}/${name}`, observed);
            });
        }
    }
});

// PLAN item 18 F11: parse5 reports preprocessor, tokenizer and tree-construction errors in occurrence order, and the
// template compiler throws the first Error-level one (template.ts:76-86), so the order decides the thrown error.
// Cases: an unclosed start tag before its end tag (tokenizer then tree), a control character inside an attribute
// name (preprocessor then tokenizer), and LightningFlowComponents customHeaderCpe.html trimmed to its first element.
describe('template-options: parse-order', () => {
    const cases: Record<string, string> = {
        'unclosed-start-tag': '<template><div a={b}\n</div></template>',
        'control-char-in-attr-name': '<template><div x\u0001"y></div></template>',
        'custom-header-cpe': [
            '<template>',
            `    <lightning-radio-group class='displayModeClass' name="select_displayMode" label={inputValues.displayMode.label}`,
            '        options={displayChoicesAsOptions} value={inputValues.displayMode.value} type="radio"',
            '        onchange={handleValueChange}',
            '    </lightning-radio-group>',
            '',
            '    <lightning-input type="number" label={inputValues.orderNumber.label} name="select_orderNumber"',
            '        value={inputValues.orderNumber.value} onchange={handleValueChange}></lightning-input>',
            '</template>',
            '',
        ].join('\n'),
    };
    const observe = (src: string, opts: Record<string, unknown>) => {
        try {
            backend.transform(src, 'customHeaderCpe.html', opts as any);
            return { ok: true };
        } catch (e: any) {
            const l = e?.location;
            return {
                ok: false,
                name: e?.name ?? typeof e,
                code: e?.code ?? null,
                message: String(e?.message ?? e),
                location: l && typeof l === 'object' ? { line: l.line ?? null, column: l.column ?? null, start: l.start ?? null, length: l.length ?? null } : null,
                aggregate: observeAggregate(e) ?? null,
            };
        }
    };
    const ERM_CELLS: Array<[string, Record<string, unknown>]> = [['default', {}], ['erm-true', { experimentalErrorRecoveryMode: true }]];
    for (const [label, extra] of ERM_CELLS) {
        for (const [name, src] of Object.entries(cases)) {
            const key = `parse-order/${label}/${name}`;
            it(key, () => checkGolden(GOLDEN_DIR, key, observe(src, { namespace: 'x', name: 'customHeaderCpe', ...extra })));
        }
    }
});
