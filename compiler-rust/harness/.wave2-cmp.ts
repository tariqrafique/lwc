// wave2 scratch (not part of the harness): diff rust observations vs template-api goldens.
// usage: LWC_COMPILER_NAPI_PATH=... LWC_COMPILER_BACKEND=rust tsx .wave2-cmp.ts <cellLabel|all> [fixture-substr]
import fs from 'node:fs'; import path from 'node:path';
import { getBackend } from './src/backend';
import { observeTemplate, TEMPLATE_ROOT } from './src/templateObserve';
import { findFiles } from './src/optionRows';
const G = path.resolve(import.meta.dirname, 'goldens/template-api');
const [cellArg, sub = ''] = process.argv.slice(2);
const CELLS: any[] = [57, 58, 59, 60, 61, 62, 65, 66, 67, 'omitted'];
const lab = (v: any) => v === 'omitted' ? 'omitted' : `v${v}`;
const b = await getBackend();
const fixtures = findFiles(TEMPLATE_ROOT, 'actual.html').map((f) => path.relative(TEMPLATE_ROOT, path.dirname(f))).filter((r) => r.includes(sub));
const canon = (x: any): any => Array.isArray(x) ? x.map(canon) : x && typeof x === 'object' ? Object.fromEntries(Object.keys(x).sort().filter((k) => x[k] !== undefined).map((k) => [k, canon(x[k])])) : x;
const diffKeys = (a: any, g: any) => [...new Set([...Object.keys(a ?? {}), ...Object.keys(g ?? {})])].filter((k) => JSON.stringify(a?.[k]) !== JSON.stringify(g?.[k]));
for (const rel of fixtures) {
  const dir = path.join(TEMPLATE_ROOT, rel); const name = path.basename(dir);
  const cfgPath = path.join(dir, 'config.json');
  const { apiVersion: _p, experimentalComputedMemberExpression: _e, ...rest } = fs.existsSync(cfgPath) ? JSON.parse(fs.readFileSync(cfgPath, 'utf8')) : {};
  const gold = JSON.parse(fs.readFileSync(path.join(G, rel.replaceAll('/', '__') + '.json'), 'utf8'));
  for (const v of CELLS) {
    if (cellArg !== 'all' && lab(v) !== cellArg) continue;
    const o = observeTemplate(b, fs.readFileSync(path.join(dir, 'actual.html'), 'utf8'), `${name}.html`, { namespace: 'x', name, ...rest, ...(v === 'omitted' ? {} : { apiVersion: v }) }, { detail: true });
    const g = gold[lab(v)];
    const o2 = canon(JSON.parse(JSON.stringify(o))); const gc = canon(g);
    if (JSON.stringify(o2) === JSON.stringify(gc)) { console.log(`PASS\t${lab(v)}\t${rel}`); continue; }
    const ks = diffKeys(o2, gc);
    console.log(`FAIL\t${lab(v)}\t${rel}\t${ks.join(',')}`);
    if (process.env.V) for (const k of ks) console.log(`   ${k}: rust=${JSON.stringify(o2[k]).slice(0, 600)}\n   ${k}: gold=${JSON.stringify(g?.[k]).slice(0, 600)}`);
  }
}
