// node cmp.cjs <napi.node> <cases.json>... ; cases {name|id, src, filename, opts?, recover?}
const { createRequire } = require('module'); const fs = require('fs');
const oracle = createRequire('/workspace/lwc/packages/@lwc/compiler/')('@lwc/compiler');
const nat = require(process.argv[2]);
const cwd = process.cwd() + '/';
const strip = (s) => (typeof s === 'string' ? s.split(cwd).join('') : s);
const loc = (l) => l == null ? l : Object.fromEntries(['line','column','index','start','length'].filter(k => l[k] != null).map(k => [k, l[k]]));
const one = (e) => ({ code: e.code ?? null, message: strip(e.message), level: e.level ?? null, url: e.url ?? null, location: loc(e.location) ?? null });
const obs = (e) => Array.isArray(e.errors) ? { message: strip(e.message), errors: e.errors.map(one) } : one(e);
let tot = 0, same = 0; const rows = [];
for (const f of process.argv.slice(3)) for (const c of JSON.parse(fs.readFileSync(f, 'utf8'))) {
  const opts = { namespace: 'x', name: 'foo', ...(c.opts || {}), ...(c.recover ? { experimentalErrorRecoveryMode: true } : {}) };
  let js; try { oracle.transformSync(c.src, c.filename, opts); js = 'ok'; } catch (e) { js = obs(e); }
  let rs; try { const r = nat.transform(c.src, c.filename, opts); rs = r.error ? obs(r.error) : 'ok'; } catch (e) { rs = obs(e); }
  const m = JSON.stringify(js) === JSON.stringify(rs); tot++; if (m) same++;
  rows.push({ file: f.split('/').pop(), name: c.name || c.id, match: m, shipped: js, rust: rs });
}
if (process.env.OUT) fs.writeFileSync(process.env.OUT, JSON.stringify(rows, null, 1));
for (const r of rows) if (!r.match) console.log('DIFF', r.file, r.name, '\n  shipped', JSON.stringify(r.shipped), '\n  rust   ', JSON.stringify(r.rust));
console.log(`${same}/${tot} match`);
