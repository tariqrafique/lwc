// Dump compiled SSR output for every module file under the ssr-compiler fixtures.
// usage: node scripts/dump-ssr.mjs <rust|js> <sync|async|asyncYield> <outdir> [napiPath]
import fs from 'node:fs'; import path from 'node:path'; import { createRequire } from 'node:module';
const [which, mode, out, napi] = process.argv.slice(2);
const ROOT = '/workspace/lwc/packages/@lwc/ssr-compiler/src/__tests__/fixtures';
const req = createRequire(import.meta.url);
const rust = which === 'rust' ? req(napi ?? path.resolve('lwc-compiler-napi.node')) : null;
const js = which === 'js' ? req('/workspace/lwc/packages/@lwc/compiler/dist/index.cjs') : null;
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).flatMap((e) => { const p = path.join(d, e.name); return e.isDirectory() ? walk(p) : [p]; });
let n = 0, errs = 0;
for (const f of walk(ROOT).filter((p) => /\/modules\/.*\.(html|js|ts)$/.test(p)).sort()) {
  const m = /\/modules\/([^/]+)\/([^/]+)\//.exec(f); if (!m) continue;
  const opts = { namespace: m[1], name: m[2], targetSSR: true, ssrMode: mode, enableDynamicComponents: true, enableLwcOn: true, experimentalDynamicDirective: true };
  let code;
  try {
    if (rust) { const r = rust.transform(fs.readFileSync(f, 'utf8'), f, opts); code = r.error ? `ERROR ${r.error.message}` : r.code; }
    else code = js.transformSync(fs.readFileSync(f, 'utf8'), f, opts).code;
  } catch (e) { code = `ERROR ${e.message}`; }
  if (code.startsWith('ERROR')) errs++;
  const o = path.join(out, path.relative(ROOT, f)); fs.mkdirSync(path.dirname(o), { recursive: true }); fs.writeFileSync(o, code); n++;
}
console.log(`${which} ${mode}: ${n} files, ${errs} errors -> ${out}`);
