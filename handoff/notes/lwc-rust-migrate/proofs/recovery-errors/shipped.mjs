import { createRequire } from 'module'; import fs from 'fs';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const cases = JSON.parse(fs.readFileSync(new URL('./cases.json', import.meta.url)));
const out = {};
for (const [k, { src, opts }] of Object.entries(cases)) {
  try { transformSync(src, 'test.js', { namespace: 'lwc', name: 'test', enablePrivateMethods: true, experimentalErrorRecoveryMode: true, ...opts }); out[k] = 'NO ERROR'; }
  catch (e) { out[k] = { name: e.name, ctor: e.constructor?.name, message: e.message, code: e.code ?? null,
    errors: e.errors ? e.errors.map((x) => ({ code: x.code ?? null, message: x.message, location: x.location ?? null })) : null }; }
}
console.log(JSON.stringify(out, null, 1));
