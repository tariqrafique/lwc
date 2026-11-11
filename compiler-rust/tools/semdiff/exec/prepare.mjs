// semdiff SD-2a, per case: compile the generated component and the child stubs with the ORACLE @lwc/compiler (so both
// sides get identical non-template code), and write `map.json` for hooks.mjs. Run once per case, before the side runs.
//   node --import ./register.mjs prepare.mjs <case-dir>      (reads <case-dir>/input.json)
// On a generator limitation (the synthesized component does not compile) map.json carries `gap`, which the verdict
// turns into UNCLASSIFIED exec-uncovered (design §3). Any other failure exits non-zero = harness failure.
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { ENGINE_DOM_ESM } from '../../../harness/src/csrRender.ts';
import { LWC } from '../../../harness/src/ssrRender.ts';

const dir = process.argv[2];
const input = JSON.parse(fs.readFileSync(path.join(dir, 'input.json'), 'utf8'));
const { transformSync } = createRequire(import.meta.url)(`${LWC}/compiler/dist/index.cjs`);

const IDENT = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
function componentSource() {
    const lines = ["import { LightningElement, api } from 'lwc';", "import { method } from 'semdiff:rt';", 'export default class extends LightningElement {'];
    if (input.light) lines.push("    static renderMode = 'light';");
    for (const r of input.roots) {
        if (!IDENT.test(r.name)) throw Object.assign(new Error(`root ${JSON.stringify(r.name)} is not an identifier`), { gap: true });
        if (r.kind === 'fn') lines.push(`    ${r.name}(...args) { return method(this, ${JSON.stringify(r.name)}, args); }`);
        else lines.push(`    @api ${r.name};`);
    }
    lines.push('}', '');
    return lines.join('\n');
}

function compile(src, file, namespace, name) {
    return transformSync(src, file, { namespace, name }).code;
}

function write(rel, code) {
    const p = path.join(dir, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, code);
    return rel;
}

function stub(spec, i) {
    const [ns, name] = spec.split('/');
    const base = `stub-${i}/${name}`;
    write(`${base}.js`, compile('import { LightningElement } from "lwc";\nexport default class extends LightningElement {}\n', `${name}.js`, ns, name));
    write(`${base}.html.js`, compile('<template></template>\n', `${name}.html`, ns, name));
    return `${base}.js`;
}

const map = { engine: ENGINE_DOM_ESM, component: 'sd.js', stubs: {}, dyn: null, gap: null };
let src = '';
try {
    src = componentSource();
    write('sd.src.js', src);
    write('sd.js', compile(src, 'sd.js', 'x', 'sd'));
} catch (e) {
    // Only the synthesized component can fail here (identifier roots, names the compiler rejects for @api).
    map.gap = `component-compile: ${String(e.message).split('\n')[0]}`;
}
if (!map.gap) {
    input.stubs.forEach((spec, i) => {
        if (!/^[a-z][\w-]*\/[\w-]+$/.test(spec)) throw new Error(`bad stub specifier ${spec}`);
        map.stubs[spec] = stub(spec, i);
    });
    if (input.dyn_ctor) map.dyn = stub('x/sddyn', 'dyn');
}
fs.writeFileSync(path.join(dir, 'map.json'), JSON.stringify(map, null, 1) + '\n');
