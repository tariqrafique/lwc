import { createRequire } from 'module'; import fs from 'fs';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const cases = JSON.parse(fs.readFileSync(new URL('./cases.json', import.meta.url)));
const out = {};
for (const [k, src] of Object.entries(cases)) for (const rec of [false, true]) {
  const key = k + (rec ? ' [rec]' : '');
  try { transformSync(src, 'test.js', { namespace: 'lwc', name: 'test', enablePrivateMethods: true, experimentalErrorRecoveryMode: rec }); out[key] = 'NO ERROR'; }
  catch (e) { out[key] = e.errors ? { errors: e.errors.map((x) => ({ code: x.code ?? null, message: x.message, location: x.location ?? null })) }
    : { code: e.code ?? null, location: e.location ?? null, message: e.message.split('\n')[0] }; }
}
console.log(JSON.stringify(out, null, 1));
