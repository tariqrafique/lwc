// Usage: node cmp.cjs cases.json [napi.node]  -> prints shipped vs rust observation per case, and MATCH/DIFF.
// case: {name, src, filename, opts}. Observation: ok | {name,message,code,location,errors[]{code,message,location}}; cwd -> <cwd>/.
const { createRequire } = require('module');
const fs = require('fs');
const oracle = createRequire('/workspace/oracle953/')('@lwc/compiler');
const nat = require(process.argv[3] || '/workspace/wt/js/harness/lwc-compiler-napi.node');
const cwd = process.cwd() + '/';
const strip = (s) => (typeof s === 'string' ? s.split(cwd).join('') : s);
const loc = (l) => (l ? { line: l.line, column: l.column, ...(l.index != null ? { index: l.index } : {}), ...(l.start != null ? { start: l.start, length: l.length } : {}) } : null);
const obs = (e) => ({ name: e.name, message: strip(e.message), code: e.code ?? null, location: loc(e.location),
  ...(Array.isArray(e.errors) ? { errors: e.errors.map((x) => ({ code: x.code ?? null, message: strip(x.message), location: loc(x.location) })) } : {}) });
const cases = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const out = [];
for (const c of cases) {
  const opts = { namespace: 'x', name: 'foo', ...(c.opts || {}) };
  let js, rs;
  try { oracle.transformSync(c.src, c.filename, opts); js = 'ok'; } catch (e) { js = obs(e); }
  const r = nat.transform(c.src, c.filename, opts);
  rs = r.error ? obs({ ...r.error, name: r.error.name ?? (r.error.errors ? 'AggregateError' : 'Error') }) : 'ok';
  const same = JSON.stringify(js) === JSON.stringify(rs);
  out.push({ name: c.name, match: same, shipped: js, rust: rs });
  console.log(same ? 'MATCH' : 'DIFF ', c.name);
  if (!same && !process.env.QUIET) console.log('  shipped:', JSON.stringify(js), '\n  rust:   ', JSON.stringify(rs));
}
if (process.env.OUT) fs.writeFileSync(process.env.OUT, JSON.stringify(out, null, 1));
