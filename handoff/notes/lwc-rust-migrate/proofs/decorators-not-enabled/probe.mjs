import { createRequire } from 'module';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const napi = createRequire(import.meta.url)('/workspace/lwc-compiler-rust/harness/lwc-compiler-napi.node');
export const V = {
  'anon-api': 'export default class { @api x; }\n',
  'anon-foo': 'export default class { @foo x; }\n',
  'anon-track': 'export default class { @track x; }\n',
  'anon-wire': 'export default class { @wire(a) x; }\n',
  'anon-api-method': 'export default class { @api m() {} }\n',
  'named-api': 'export default class Foo { @api x; }\n',
  'named-foo': 'export default class Foo { @foo x; }\n',
  'named-ext-api': 'export default class Foo extends Bar { @api x; }\n',
  'nondefault-api': 'class Foo { @api x; }\nexport default Foo;\n',
  'expr-api': 'const Foo = class { @api x; };\nexport default Foo;\n',
  'expr-foo': 'export const Foo = class { @foo x; };\n',
  'anon-api-imported': 'import { api } from "lwc";\nexport default class { @api x; }\n',
  'anon-foo-imported-lwc': 'import { LightningElement } from "lwc";\nexport default class { @foo x; }\n',
  'named-ext-le-foo': 'import { LightningElement } from "lwc";\nexport default class Foo extends LightningElement { @foo x; }\n',
  'anon-trackword-foo': 'export default class { @foo track; }\n',
  'anon-api-call': 'export default class { @api() x; }\n',
  'class-decorator': '@api\nexport default class {}\n',
};
const strip = (s) => s && s.replaceAll(process.cwd() + '/', '<cwd>/');
const out = {};
for (const rec of [false, true]) for (const [id, src] of Object.entries(V)) {
  const o = { name: 'foo', namespace: 'x', experimentalErrorRecoveryMode: rec };
  let s, r;
  try { transformSync(src, 'foo.js', o); s = 'ok'; } catch (e) {
    s = { name: e.name, keys: Object.keys(e), code: e.code, url: e.url, level: e.level, location: e.location, message: strip(e.message), errors: e.errors?.map(x => ({ code: x.code, message: strip(x.message), location: x.location })) };
  }
  const re = napi.transform(src, 'foo.js', o).error;
  r = re ? { code: re.code, url: re.url, location: re.location, message: strip(re.message), errors: re.errors?.map(x => ({ code: x.code, message: x.message })) } : 'ok';
  out[(rec ? 'rec:' : '') + id] = { src, shipped: s, rust: r };
}
import fs from 'fs';
fs.writeFileSync(new URL('./probe.json', import.meta.url), JSON.stringify(out, null, 1));
for (const [k, v] of Object.entries(out)) console.log(k, '\n S', JSON.stringify(v.shipped).slice(0, 400), '\n R', JSON.stringify(v.rust).slice(0, 250));
