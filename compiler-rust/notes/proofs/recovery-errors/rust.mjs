import fs from 'fs'; import { execFileSync } from 'child_process';
const BIN = '/workspace/lwc-compiler-rust/target/debug/examples/errdump';
const cases = JSON.parse(fs.readFileSync(new URL('./cases.json', import.meta.url)));
const out = {};
for (const [k, { src, opts }] of Object.entries(cases)) {
  if (Object.keys(opts).some((o) => o !== 'dynamicImports') || (opts.dynamicImports && (opts.dynamicImports.loader != null || opts.dynamicImports.strictSpecifier !== true))) throw new Error('unsupported opts ' + k);
  const r = JSON.parse(execFileSync(BIN, ['epm', 'rec', 'filename=test.js', 'name=test', 'namespace=lwc'], { input: src }).toString());
  out[k] = r === 'NO ERROR' ? r : { message: r.message, code: r.code, errors: r.diagnostics.map((d) => ({ code: d.code, message: d.message, location: d.location })) };
}
console.log(JSON.stringify(out, null, 1));
