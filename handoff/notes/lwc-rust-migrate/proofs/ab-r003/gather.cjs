// Seam corpus for AB-R003 (no-behavior-change proof): every upstream fixture source x {normal, recovery} x {csr, ssr},
// plus the Rust crates' shipped-oracle case files.
const fs = require('fs'), path = require('path');
const L = '/workspace/lwc/packages/@lwc', W = '/workspace/wt/js';
const walk = (d, f, out = []) => { for (const e of fs.readdirSync(d, { withFileTypes: true })) { const p = path.join(d, e.name); e.isDirectory() ? walk(p, f, out) : f(p) && out.push(p); } return out; };
const cases = [];
const add = (src, filename, opts = {}) => {
  for (const rec of [false, true]) for (const ssr of [false, true]) cases.push({ src, filename, opts: { name: 'foo', namespace: 'x', ...opts, experimentalErrorRecoveryMode: rec, targetSSR: ssr } });
};
const cfg = (d) => { try { return JSON.parse(fs.readFileSync(path.join(d, 'config.json'), 'utf8')); } catch { return {}; } };
for (const p of walk(L + '/babel-plugin-component/src/__tests__/fixtures', (p) => /actual\.(js|ts)$/.test(p))) add(fs.readFileSync(p, 'utf8'), 'foo.js');
for (const p of walk(L + '/template-compiler/src/__tests__/fixtures', (p) => p.endsWith('actual.html'))) add(fs.readFileSync(p, 'utf8'), 'foo.html');
for (const p of walk(L + '/style-compiler/src/__tests__/fixtures', (p) => p.endsWith('.css'))) add(fs.readFileSync(p, 'utf8'), 'foo.css');
const files = [W + '/crates/lwc-compiler-napi/tests/fixtures/single-error-shipped.json', W + '/crates/lwc-compiler-napi/tests/fixtures/aggregate-shipped.json',
  ...fs.readdirSync(W + '/crates/lwc-js/tests/fixtures').filter((f) => f.endsWith('.json')).map((f) => W + '/crates/lwc-js/tests/fixtures/' + f),
  ...fs.readdirSync('../js-divergences').filter((f) => f.endsWith('.json')).map((f) => '../js-divergences/' + f)];
for (const f of files) { const j = JSON.parse(fs.readFileSync(f, 'utf8')); for (const c of (Array.isArray(j) ? j : j.cases || [])) if (typeof c.src === 'string') add(c.src, c.filename || 'foo.js', c.opts || {}); }
// option-validation errors
for (const sm of ['foo', 1, {}]) add('export const a = 1;', 'a.js', { outputConfig: { sourcemap: sm } });
add('x', 'foo.txt'); add('', 'foo.html'); add('', 'foo.js'); add('', 'foo.css'); add('a{', 'foo.css'); add('<template><div', 'foo.html');
fs.writeFileSync('/tmp/seam-corpus.json', JSON.stringify(cases));
console.log(cases.length);
