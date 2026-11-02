// Dumps shipped 9.5.3's thrown recovery-mode aggregate: every own property (keys, types, values) of the
// aggregate and of each errors[] entry, plus prototype chain names. Writes shipped-shape.json.
import fs from 'fs'; import { createRequire } from 'module';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const cases = JSON.parse(fs.readFileSync(new URL('../recovery-errors/cases.json', import.meta.url)));
const pick = process.argv.slice(2);
const desc = (o) => { if (o == null || typeof o !== 'object') return o; const out = {};
  for (const k of Object.getOwnPropertyNames(o)) { if (k === 'stack') { out.stack = '<stack>'; continue; }
    const d = Object.getOwnPropertyDescriptor(o, k); const v = d.value;
    out[k] = k === 'errors' ? v.map(desc) : (v && typeof v === 'object' ? JSON.parse(JSON.stringify(v)) : v);
    if (!d.enumerable) out[k + '@nonEnumerable'] = true; }
  const chain = []; for (let p = Object.getPrototypeOf(o); p && p !== Object.prototype; p = Object.getPrototypeOf(p)) chain.push(p.constructor.name);
  out['@proto'] = chain; out['@name'] = o.name; out['@code'] = o.code; return out; };
const res = {};
for (const k of pick.length ? pick : Object.keys(cases)) { const c = cases[k];
  try { transformSync(c.src, 'foo.js', { name: 'foo', namespace: 'x', experimentalErrorRecoveryMode: true, ...c.opts }); res[k] = 'NO ERROR'; }
  catch (e) { res[k] = desc(e); } }
fs.writeFileSync(new URL('./shipped-shape.json', import.meta.url), JSON.stringify(res, null, 1));
