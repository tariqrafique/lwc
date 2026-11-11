// semdiff SD-2a loader hooks (design: notes/arch-board/proposals/semdiff-SD-2a.md §1).
// Resolution only. The side's template module is served byte-for-byte at ONE fixed virtual URL on both sides, so no
// error text or URL can differ by side. `lwc` → the observer. The component and child stubs were compiled by the oracle
// @lwc/compiler (prepare.mjs) into the case dir; `*.css` imports of those modules resolve to `export default undefined`
// (H-1 fixtures have no stylesheet files; both sides see the same value).
// Env (fixed at spawn, visible to this loader thread): SD_EXEC_CASE = case dir, SD_EXEC_TEMPLATE = side module path.
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export const TEMPLATE_URL = 'file:///__semdiff__/x/sd/sd.html.js';
const OBSERVER = new URL('./observer.mjs', import.meta.url).href;
const RT = new URL('./rt.mjs', import.meta.url).href;
const EMPTY_CSS = 'data:text/javascript,export%20default%20undefined%3B';

const CASE = process.env.SD_EXEC_CASE ? pathToFileURL(path.resolve(process.env.SD_EXEC_CASE)).href + '/' : null;
let map;
function m() {
    if (map === undefined) map = CASE ? JSON.parse(fs.readFileSync(new URL('map.json', CASE), 'utf8')) : null;
    return map;
}
const inCase = (u) => CASE !== null && typeof u === 'string' && u.startsWith(CASE);

// Harness TypeScript sources (csrRender.ts) import siblings extensionless; Node's type stripping needs the `.ts`.
const tsSibling = (spec, parent) => spec.startsWith('.') && parent?.endsWith('.ts') && !/\.[cm]?[jt]s$/.test(spec);

export async function resolve(spec, ctx, next) {
    const parent = ctx.parentURL;
    if (spec === TEMPLATE_URL) return { url: TEMPLATE_URL, shortCircuit: true };
    if (spec === 'lwc') return { url: OBSERVER, shortCircuit: true };
    if (spec === 'semdiff:rt') return { url: RT, shortCircuit: true };
    if (spec === 'semdiff:engine') return { url: pathToFileURL(m().engine).href, shortCircuit: true };
    if (tsSibling(spec, parent)) return next(spec + '.ts', ctx);
    if (parent === TEMPLATE_URL || inCase(parent)) {
        if (/\.css(\?.*)?$/.test(spec)) return { url: EMPTY_CSS, shortCircuit: true };
        const mm = m();
        if (parent === new URL(mm.component, CASE).href && spec === './sd.html') return { url: TEMPLATE_URL, shortCircuit: true };
        if (parent === TEMPLATE_URL && spec in mm.stubs) return { url: new URL(mm.stubs[spec], CASE).href, shortCircuit: true };
        if (inCase(parent) && spec.startsWith('./') && spec.endsWith('.html')) {
            return { url: new URL(spec.slice(2) + '.js', parent).href, shortCircuit: true };
        }
        if (parent === TEMPLATE_URL) throw new Error(`semdiff-exec: unresolvable import ${JSON.stringify(spec)} from the template module`);
    }
    return next(spec, ctx);
}

export async function load(url, ctx, next) {
    if (url === TEMPLATE_URL) {
        return { format: 'module', source: fs.readFileSync(process.env.SD_EXEC_TEMPLATE, 'utf8'), shortCircuit: true };
    }
    return next(url, ctx);
}
