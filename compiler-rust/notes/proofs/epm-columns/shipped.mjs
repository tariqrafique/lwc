import { createRequire } from 'module';
const require = createRequire('/workspace/oracle953/');
const { transformSync } = require('@lwc/compiler');
import fs from 'fs';
const cases = JSON.parse(fs.readFileSync(new URL('./cases.json', import.meta.url)));
const out = {};
for (const [k, src] of Object.entries(cases)) {
  try { transformSync(src, '/abs/dir/foo.js', { name: 'foo', namespace: 'x', enablePrivateMethods: true }); out[k] = 'NO ERROR'; }
  catch (e) { out[k] = { code: e.code, location: e.location, message: e.message }; }
}
console.log(JSON.stringify(out, null, 1));
