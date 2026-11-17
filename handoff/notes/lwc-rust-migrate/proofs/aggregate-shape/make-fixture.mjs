// Pins shipped 9.5.3's recovery AggregateError for 3 fixtures into the napi seam-test fixture.
// Entry filenames are cwd-resolved by Babel; the cwd prefix is replaced with "<cwd>/".
import fs from 'fs'; import { createRequire } from 'module';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const cases = JSON.parse(fs.readFileSync(new URL('../recovery-errors/cases.json', import.meta.url)));
const KEYS = ['api/reserved-property-name-class', 'extra/import-before-and-between-classes', 'extra/decorator-used-as-value'];
const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
const out = [];
for (const k of KEYS) {
  const { src, opts } = cases[k];
  try { transformSync(src, 'foo.js', { name: 'foo', namespace: 'x', experimentalErrorRecoveryMode: true, ...opts }); throw new Error('no error'); }
  catch (e) {
    out.push({ fixture: k, src, name: e.name, message: e.message, hasOwnCode: own(e, 'code'), code: e.code ?? null,
      hasOwnFilename: own(e, 'filename'), ownKeys: Object.getOwnPropertyNames(e).filter((x) => x !== 'stack'),
      errors: e.errors.map((x) => ({ ownKeys: Object.getOwnPropertyNames(x).filter((y) => y !== 'stack'), name: x.name, message: x.message, code: x.code,
        filename: x.filename.replace(process.cwd() + '/', '<cwd>/'), location: x.location, level: x.level, url: x.url })) });
  }
}
fs.writeFileSync('/workspace/lwc-compiler-rust/crates/lwc-compiler-napi/tests/fixtures/aggregate-shipped.json', JSON.stringify(out, null, 1) + '\n');
console.log(out.map((o) => `${o.fixture}: ${o.errors.length} errors, ownKeys=${o.ownKeys}, entryKeys=${o.errors[0].ownKeys}`).join('\n'));
