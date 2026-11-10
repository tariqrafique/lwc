// Differential oracle for lwc-template::expr::bind (AB-D0004 S5; was lwc-ssr bind_expr.rs).
// Builds a corpus of template expressions (SSR fixtures + template-compiler fixtures + adversarial set), runs the JS
// `bindExpression(expr, isLocal, 'instance', /*experimentalComplexExpressions*/ true)` from the built
// @lwc/template-compiler dist on each, and records WHICH identifiers got `instance.` (by UTF-16 offset + name).
// Output: crates/lwc-template/tests/fixtures/bind_corpus.json, consumed by lwc-template `js_bind_corpus`.
// Usage: node harness/scripts/gen-bind-corpus.mjs   (then: cargo test -p lwc-template js_bind_corpus)
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { oracleRoot } from './oracle-root.mjs';

const LWC = oracleRoot();
const require = createRequire(path.join(LWC, 'package.json'));
const { parseExpressionAt } = require('acorn');
const { walk } = require('estree-walker');
const { bindExpression } = require(path.join(LWC, 'packages/@lwc/template-compiler/dist/index.cjs'));
const here = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.resolve(here, '../../crates/lwc-template/tests/fixtures/bind_corpus.json');

// Pull `{...}` expressions (brace-balanced, string/template aware) plus the loop locals declared in the same file.
function extract(html) {
    const exprs = [];
    for (let i = 0; i < html.length; i++) {
        if (html[i] !== '{') continue;
        let depth = 0, j = i, q = null;
        for (; j < html.length; j++) {
            const c = html[j];
            if (q) { if (c === '\\') j++; else if (c === q) q = null; continue; }
            if (c === '"' || c === "'" || c === '`') q = c;
            else if (c === '{') depth++;
            else if (c === '}' && --depth === 0) break;
        }
        if (j >= html.length) break;
        exprs.push(html.slice(i + 1, j));
        i = j;
    }
    const locals = new Set();
    for (const m of html.matchAll(/for:(?:item|index)\s*=\s*["']([\w$]+)["']/g)) locals.add(m[1]);
    for (const m of html.matchAll(/iterator:([\w$]+)/g)) locals.add(m[1]);
    for (const m of html.matchAll(/lwc:slot-data\s*=\s*["']([\w$]+)["']/g)) locals.add(m[1]);
    return { exprs, locals: [...locals].sort() };
}

function* walkFiles(dir) {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
        const p = path.join(dir, e.name);
        if (e.isDirectory()) yield* walkFiles(p);
        else if (e.name.endsWith('.html')) yield p;
    }
}

const cases = new Map(); // key src\0scope -> {src, scope, origin}
function add(src, scope, origin) {
    const key = src + '\0' + scope.join(',');
    if (!cases.has(key)) cases.set(key, { src, scope, origin });
}
for (const [label, root] of [
    ['ssr', `${LWC}/packages/@lwc/ssr-compiler/src/__tests__/fixtures`],
    ['tc', `${LWC}/packages/@lwc/template-compiler/src/__tests__/fixtures`],
]) {
    for (const f of walkFiles(root)) {
        if (/expected|\/dist\//.test(f)) continue;
        const { exprs, locals } = extract(fs.readFileSync(f, 'utf8'));
        for (const e of exprs) {
            if (!e.trim()) continue;
            const origin = `${label}:${path.relative(root, f)}`;
            add(e, [], origin);
            if (locals.length) add(e, locals, origin);
        }
    }
}

// Adversarial set: every construct where a naive "skip any name bound anywhere" binder diverges from JS scoping.
const ADV = [
    ['{a}', []], ['{a, b}', []], ['{a, b: c, [d]: e}', []], ['{[d.e]: f}', []], ['{...a, b}', []], ['({a})', ['a']],
    ['x => x', []], ['x => x + y', []], ['(x, y) => x + y + z', []], ['(x => x)(x)', []], ['[x => x, x]', []],
    ['x => y => x + y', []], ['x => (x => x)(x) + x', []], ['(x => x) && x', []], ['a.map(x => x.b) + x', []],
    ['x => (y => x)(y)', []], ['({a, b: [c, ...d]}) => a + b + c + d + e', []], ['([a, , b]) => a + b + c', []],
    ['(...r) => r.length + n', []], ['({a: {b}}) => a + b', []], ['item => item.id === selectedId', ['item']],
    ['row => row + item', ['item']], ['item => item', ['item']],
    ['a.b.c.d', []], ['a[b][c.d]', []], ['a[b.c[d]]', []], ['a?.b?.[c]?.(d)', []], ['a?.[b]', []], ['a.b(c, ...d)', []],
    ['`${a} and ${b.c} ${`${d}`}`', []], ['tag`x${a}`', []], ['this', []], ['this.a + b', []],
    ['undefined', []], ['a === undefined', []], ['void 0 ?? undefined', []], ['null ?? a', []],
    ['Math.max(a, b)', []], ['JSON.stringify(a)', []], ['window.location', []], ['NaN + Infinity', []],
    ['[...a, ...b.c]', []], ['f(...a)', []], ['new Foo(a)', []], ['typeof a === "string"', []], ['!a || -b', []],
    ['a ? b : c', []], ['(a, b)', []], ['a = b', []], ['a += 1', []], ['a++', []], ['a in b', []], ['a instanceof B', []],
    ['item.value + index', ['item', 'index']], ['it.value.x', ['it']], ['item[key]', ['item']], ['x', ['x']], ['x', []],
    ['{ key: item, value }', ['item']], ['a.b.c(x => x.d[e])', []], ['(a) => (b) => (c) => a + b + c + d', []],
    ['(a) => [(a) => a, a, b]', []], ['async', []], ['of', []], ['let', []], ['get', []], ['{get, set}', []],
    ['{ "a-b": c }', []], ['{ 1: c }', []], ['a ?? (b || c)', []], ['(a ?? b) || c', []], ['a.undefined', []],
    ['{undefined}', []], ['x => undefined', []], ['{a: a}', []], ['{a: b => b + a}', []], ['[a, [b, [c]]]', []],
];
for (const [src, scope] of ADV) add(src, scope, 'adversarial');

const out = [];
let parseErr = 0, bindErr = 0;
for (const { src, scope, origin } of cases.values()) {
    let ast;
    try {
        ast = parseExpressionAt(src, 0, { ecmaVersion: 2022, allowAwaitOutsideFunction: false, onComment: () => { throw new Error('comment'); } });
        if (src.slice(ast.end).trim() !== '') throw new Error('trailing');
    } catch { parseErr++; continue; }
    // Template expressions may only contain arrow functions; function/class expressions are compile errors upstream
    // (template-compiler fixtures expression-complex/invalid/*), so bind never sees them. Keep them visible, unchecked.
    let rejected = null;
    walk(ast, { enter(n) { if (/^(Function|Class)(Expression|Declaration)$/.test(n.type)) rejected = n.type; } });
    if (rejected) { out.push({ src, scope, origin, excluded: `${rejected} is rejected upstream` }); continue; }
    const local = new Set(scope);
    let bound;
    try {
        bound = bindExpression(ast, (n) => local.has(n.name), 'instance', true);
    } catch (e) { bindErr++; out.push({ src, scope, origin, error: String(e.message ?? e).slice(0, 120) }); continue; }
    // Synthesized `instance.X` member nodes have no `start`; their `property` is the original identifier node.
    const prefixed = [];
    const seen = new Set();
    walk(bound, {
        enter(node) {
            if (node.type === 'MemberExpression' && node.object.type === 'Identifier' && node.object.name === 'instance' &&
                node.object.start === undefined && node.property.start !== undefined && !seen.has(node.property)) {
                seen.add(node.property);
                prefixed.push([node.property.start, node.property.name]);
            }
        },
    });
    prefixed.sort((a, b) => a[0] - b[0]);
    out.push({ src, scope, origin, prefixed });
}
out.sort((a, b) => (a.origin + a.src + a.scope).localeCompare(b.origin + b.src + b.scope));
fs.writeFileSync(OUT, JSON.stringify(out, null, 0).replace(/\},\{/g, '},\n{') + '\n');
console.log(`${out.length} cases (${out.filter((c) => c.prefixed).length} compared, ${out.filter((c) => c.excluded).length} excluded, ${bindErr} JS bind errors, ${parseErr} unparseable skipped) -> ${OUT}`);
