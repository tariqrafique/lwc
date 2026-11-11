// Loads shipped and Rust isExplicitImport outputs as ES modules against stub `lwc` / adapter modules and
// asserts the observable module surface is identical. Usage: node eval.mjs <shipped.js> <rust.js>
import fs from 'fs'; import path from 'path'; import assert from 'assert/strict';
import { calls, LightningElement } from 'lwc';
import { getFoo } from 'x/fooAdapter';
const here = path.dirname(new URL(import.meta.url).pathname);
const [sFile, rFile] = process.argv.slice(2);
async function load(tag, file) {
  const dst = path.join(here, `mod-${tag}.mjs`);
  fs.writeFileSync(dst, fs.readFileSync(file, 'utf8'));
  const before = calls.length;
  const ns = await import(dst);
  return { ns, reg: calls.slice(before) };
}
function desc(o) {
  return Object.getOwnPropertyNames(o).sort().map((k) => {
    const d = Object.getOwnPropertyDescriptor(o, k);
    return [k, 'value' in d ? typeof d.value : `${!!d.get}/${!!d.set}`, d.enumerable, d.writable ?? null, d.configurable];
  });
}
function surface({ ns, reg }) {
  const C = ns.default;
  const inst = typeof C === 'function' ? new C() : null;
  return {
    exportNames: Object.keys(ns).sort(),
    namedValues: Object.fromEntries(Object.entries(ns).filter(([k]) => k !== 'default' && k !== 'swap').map(([k, v]) => [k, typeof v === 'function' ? `fn(${v.length}):${v(3)}` : v])),
    defaultType: typeof C, name: C?.name, length: C?.length, toStringHead: String(C).split('{')[0].trim(),
    protoChainIsLE: Object.getPrototypeOf(C) === LightningElement, instanceofLE: inst instanceof LightningElement, instanceofC: inst instanceof C,
    staticShape: desc(C), protoShape: desc(C.prototype), instanceShape: desc(inst), instanceJSON: JSON.stringify(inst),
    registerCalls: reg.length, registeredIsDefault: reg.every((r) => r.Ctor === C),
    // binding liveness: a module-internal reassignment of the class binding must show through `default`.
    afterSwap: typeof ns.swap === 'function' ? (ns.swap(), ns.default?.name) : null,
    meta: reg.map((r) => JSON.stringify(r.meta, (k, v) => (v === getFoo ? '<getFoo>' : typeof v === 'function' ? `fn:${JSON.stringify(v({ label: 'L' }))}` : v))),
  };
}
const s = surface(await load('shipped', sFile)), r = surface(await load('rust', rFile));
console.log(JSON.stringify({ shipped: s, rust: r }, null, 1));
assert.deepEqual(r, s);
console.log('OBSERVABLY IDENTICAL');
