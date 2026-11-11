import fs from 'fs'; import { execFileSync } from 'child_process';
const BIN = '/workspace/lwc-compiler-rust/target/debug/examples/errdump';
const cases = JSON.parse(fs.readFileSync(new URL('./cases.json', import.meta.url)));
const out = {};
for (const [k, src] of Object.entries(cases)) for (const rec of [false, true]) {
  const key = k + (rec ? ' [rec]' : '');
  const r = JSON.parse(execFileSync(BIN, ['epm', ...(rec ? ['rec'] : []), 'filename=test.js', 'name=test', 'namespace=lwc'], { input: src }).toString());
  out[key] = r === 'NO ERROR' ? r : rec && r.message.startsWith('Multiple errors') ? { errors: r.diagnostics.map((d) => ({ code: d.code, message: d.message, location: d.location })) }
    : { code: r.code, location: r.location, message: r.message.split('\n')[0] };
}
console.log(JSON.stringify(out, null, 1));
