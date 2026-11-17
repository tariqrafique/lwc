import { createRequire } from 'module';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const napi = createRequire(import.meta.url)('/workspace/lwc-compiler-rust/harness/lwc-compiler-napi.node');
const LE = 'import { LightningElement, api, track, wire } from "lwc";\n';
export const V = {
  'nested-inner-anon': LE + 'export default class Foo extends LightningElement { @api y; m() { return class { @api x; }; } }\n',
  'nested-outer-anon': LE + 'export default class { @api y; m() { return class extends LightningElement { @api x; }; } }\n',
  'two-classes': LE + 'class A { @api x; }\nexport default class B extends LightningElement { @api y; }\n',
  'two-classes-rev': LE + 'export default class B extends LightningElement { @api y; }\nclass A { @foo x; }\n',
  'ext-le-classdeco': LE + '@api\nexport default class Foo extends LightningElement {}\n',
  'ext-le-classdeco-foo': LE + '@foo\nexport default class Foo extends LightningElement { @api x; }\n',
  'ext-le-only-foo-and-field': LE + 'export default class Foo extends LightningElement { @foo x; y = 1; }\n',
  'ext-le-foo-and-api': LE + 'export default class Foo extends LightningElement { @foo x; @api y; }\n',
  'dynimport-then-anon': 'import("a");\nexport default class { @api x; }\n',
  'anon-api-multiline': 'export default class {\n  @api x;\n}\n',
  'anon-empty-body-classdeco': '@foo\nclass A {}\nexport default A;\n',
  'anon-static-api': LE + 'export default class { @api static x; }\n',
  'anon-getter': LE + 'export default class { @api get x() { return 1; } }\n',
  'old-api-anon-api': ['export default class { @api x; }\n', { apiVersion: 58 }],
  'old-api-anon-api-imp': [LE + 'export default class { @api x; }\n', { apiVersion: 58 }],
  'old-api-anon-foo': [LE + 'export default class { @foo x; }\n', { apiVersion: 58 }],
  'ts-anon-api': ['export default class { @api x: number; }\n', {}, 'foo.ts'],
  'unicode-before': 'const s = "😀é";\nexport default class { @api x; }\n',
};
const strip = (s) => s && s.replaceAll(process.cwd() + '/', '<cwd>/');
const out = {};
for (const rec of [false, true]) for (const [id, v] of Object.entries(V)) {
  const [src, extra = {}, fn = 'foo.js'] = Array.isArray(v) ? v : [v];
  const o = { name: 'foo', namespace: 'x', experimentalErrorRecoveryMode: rec, ...extra };
  let s, r;
  try { transformSync(src, fn, o); s = 'ok'; } catch (e) {
    s = { code: e.code, url: e.url, location: e.location, message: strip(e.message), errors: e.errors?.map(x => ({ code: x.code, message: strip(x.message).slice(0,80) })) };
  }
  let re; try { re = napi.transform(src, fn, o).error; } catch (e) { re = { message: 'THROW ' + e.message }; }
  r = re ? { code: re.code, url: re.url, location: re.location, message: strip(re.message), errors: re.errors?.map(x => ({ code: x.code, message: x.message.slice(0,80) })) } : 'ok';
  out[(rec ? 'rec:' : '') + id] = { src, shipped: s, rust: r };
}
import fs from 'fs';
fs.writeFileSync(new URL('./probe2.json', import.meta.url), JSON.stringify(out, null, 1));
for (const [k, v] of Object.entries(out)) console.log(k, '\n S', JSON.stringify(v.shipped).slice(0, 700), '\n R', JSON.stringify(v.rust).slice(0, 200));
