// Row `style-upstream`: style-stage compile of the live upstream CSS inputs that are not in the
// 69-fixture `style` / `style-api` rows.
//
//   transformers-specs — the six inline cases in @lwc/compiler transformers/__tests__/transform-css.spec.ts
//   unported-inline    — @lwc/style-compiler inline.spec.ts `it('examples')` (`@import "@foo/test"`).
//                        The spec's resolverModule is dead in style-compiler (same drop as the `style` row).
//                        Filename `test.css`: the compiler style stage dispatches on `.css`; the spec passes
//                        `test` to style-compiler, which ignores the extension. The stylesheet text is the spec's.
//   rollup-plugin      — the three fixture CSS files. `*.scoped.css` is compiled with scopedStyles
//                        (rollup sets scoped from `?scoped=true` on that import).
//   playground         — playground/src/modules/x/app/app.css
//
// Observation matches the `style` row (eval of the emitted stylesheet, or errorObservation). Goldens are
// the JS backend only. A mismatch fails the case.
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, expect, beforeAll } from 'vitest';
import { getBackend, type CompilerBackend } from '../src/backend';
import { evalStylesheetModule } from '../src/evalStyle';
import { checkGolden, errorObservation } from '../src/optionRows';
import { oracleRoot } from '../src/oracleRoot';

const ORACLE = oracleRoot();
const GOLDEN_DIR = path.resolve(import.meta.dirname, '../goldens/style-upstream');

type Input = {
    id: string;
    src: string;
    filename: string;
    name: string;
    namespace: string;
    scoped: boolean;
    erm: boolean;
};

const HOST_DIV = `
        :host {
            color: red;
        }

        div {
            background-color: red;
        }
    `;
const INLINE_IMPORT = `
            @import "@foo/test";
        `;

const INLINE: Input[] = [
    { id: 'transformers-specs/invalid-lt', src: '<', filename: 'foo.css', name: 'foo', namespace: 'x', scoped: false, erm: false },
    { id: 'transformers-specs/host-div', src: HOST_DIV, filename: 'foo.css', name: 'foo', namespace: 'x', scoped: false, erm: false },
    { id: 'transformers-specs/var-bg-color', src: 'div { color: var(--bg-color); }', filename: 'foo.css', name: 'foo', namespace: 'x', scoped: false, erm: false },
    { id: 'transformers-specs/erm-lt', src: '<', filename: 'foo.css', name: 'foo', namespace: 'x', scoped: false, erm: true },
    { id: 'transformers-specs/grave-accent', src: '/* Comment with grave accents `#` */', filename: 'foo.css', name: 'foo', namespace: 'x', scoped: false, erm: false },
    { id: 'transformers-specs/backslash', src: '.foo { content: "x\\x"; }', filename: 'foo.css', name: 'foo', namespace: 'x', scoped: false, erm: false },
    { id: 'unported-inline/examples', src: INLINE_IMPORT, filename: 'test.css', name: 'foo', namespace: 'x', scoped: false, erm: false },
];

const FILES: Array<{ id: string; rel: string }> = [
    { id: 'rollup-plugin/scoped-styles/src/x/app/app.scoped.css', rel: 'packages/@lwc/rollup-plugin/src/__tests__/fixtures/scoped-styles/src/x/app/app.scoped.css' },
    { id: 'rollup-plugin/test/test.css', rel: 'packages/@lwc/rollup-plugin/src/__tests__/fixtures/test/test.css' },
    { id: 'rollup-plugin/typescript/typescript.css', rel: 'packages/@lwc/rollup-plugin/src/__tests__/fixtures/typescript/typescript.css' },
    { id: 'playground/src/modules/x/app/app.css', rel: 'playground/src/modules/x/app/app.css' },
];

function fileInput(id: string, rel: string): Input {
    const abs = path.join(ORACLE, rel);
    const filename = path.basename(rel);
    return {
        id,
        src: fs.readFileSync(abs, 'utf8'),
        filename,
        name: 'x',
        namespace: 'x',
        scoped: filename.endsWith('.scoped.css'),
        erm: false,
    };
}

const inputs: Input[] = [...INLINE, ...FILES.map((f) => fileInput(f.id, f.rel))];

let backend: CompilerBackend;
beforeAll(async () => { backend = await getBackend(); });

function observe(input: Input): unknown {
    const opts = {
        name: input.name,
        namespace: input.namespace,
        scopedStyles: input.scoped,
        ...(input.erm ? { experimentalErrorRecoveryMode: true } : {}),
    };
    try {
        const res = backend.transform(input.src, input.filename, opts);
        expect(res.map ?? '').toBe('');
        const ev = evalStylesheetModule(res.code);
        for (const s of ev.sheets) expect(s.scoped).toBe(input.scoped);
        return { ok: true, ...ev };
    } catch (e: any) {
        return { ...errorObservation(e), multipleCssErrors: String(e?.message ?? '').includes('Multiple CSS errors') };
    }
}

describe('style-upstream', () => {
    it('finds 11 upstream style inputs', () => expect(inputs.length).toBe(11));
    for (const input of inputs) {
        it(input.id, () => checkGolden(GOLDEN_DIR, input.id, observe(input)));
    }
});
