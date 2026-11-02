import fs from 'fs'; import { execFileSync } from 'child_process';
const BIN = '/workspace/lwc-compiler-rust/target/debug/examples/errdump';
const cases = JSON.parse(fs.readFileSync(new URL('./cases.json', import.meta.url)));
const out = {};
for (const [k, src] of Object.entries(cases)) {
  const r = JSON.parse(execFileSync(BIN, ['epm'], { input: src }).toString());
  out[k] = r === 'NO ERROR' ? r : { code: r.code, location: r.location, message: r.message };
}
console.log(JSON.stringify(out, null, 1));
