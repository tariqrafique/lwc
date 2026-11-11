// Shipped 9.5.3 decorator-validation / "Decorators are not enabled" outcomes -> lwc-js test fixture.
import { createRequire } from 'module';
import fs from 'fs';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const { V } = await import('./variants.mjs');
const strip = (s) => s && s.replaceAll(process.cwd() + '/', '');
const cases = [];
for (const rec of [false, true]) for (const [id, v] of Object.entries(V)) {
  const [src, opts = {}, filename = 'foo.js'] = Array.isArray(v) ? v : [v];
  let shipped;
  try { transformSync(src, filename, { name: 'foo', namespace: 'x', experimentalErrorRecoveryMode: rec, ...opts }); shipped = 'ok'; }
  catch (e) { shipped = { code: e.code ?? null, message: strip(e.message), hasLocation: e.location !== undefined, errors: e.errors?.map((x) => x.code) ?? null }; }
  cases.push({ id: (rec ? 'rec:' : '') + id, src, opts, filename, recover: rec, shipped });
}
fs.writeFileSync('/workspace/lwc-compiler-rust/crates/lwc-js/tests/fixtures/decorators-not-enabled-shipped.json', JSON.stringify(cases, null, 1) + '\n');
console.log(cases.length, 'cases');
