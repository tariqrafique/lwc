// Own keys + values of shipped 9.5.3's thrown single (non-aggregate) error for each case.
import fs from 'fs'; import { createRequire } from 'module'; import { CASES } from './cases.mjs';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const out = [];
for (const c of CASES) {
  try { transformSync(c.src, c.filename, { name: 'foo', namespace: 'x', ...(c.opts ?? {}) }); out.push({ ...c, error: null }); }
  catch (e) {
    const ownKeys = Object.getOwnPropertyNames(e).filter((k) => k !== 'stack');
    out.push({ ...c, error: { ownKeys, name: e.name, proto: Object.getPrototypeOf(e).constructor.name, message: e.message.replace(process.cwd() + '/', '<cwd>/'),
      code: e.code, filename: e.filename, location: e.location, level: e.level, url: e.url } });
  }
}
fs.writeFileSync(new URL('./shipped.json', import.meta.url), JSON.stringify(out, null, 1) + '\n');
fs.writeFileSync('/workspace/lwc-compiler-rust/crates/lwc-compiler-napi/tests/fixtures/single-error-shipped.json', JSON.stringify(out, null, 1) + '\n');
for (const o of out) console.log(o.id, o.error && JSON.stringify({ k: o.error.ownKeys.join(','), code: o.error.code, level: o.error.level, url: o.error.url, loc: o.error.location, proto: o.error.proto }));
