// semdiff: deterministic N-case sample of a semdiff manifest (SD-2a smoke). Order = sha256(case id) ascending, so the
// sample depends only on the case ids (not manifest order, time, or randomness). Paths are made absolute so the sample
// can live elsewhere.
//   node tools/semdiff/scripts/sample.mjs <manifest.json> <n> <out.json>
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const [src, nArg, out] = process.argv.slice(2);
const n = Number(nArg);
if (!src || !out || !Number.isInteger(n) || n < 1) {
    console.error('usage: sample.mjs <manifest.json> <n> <out.json>');
    process.exit(2);
}
const m = JSON.parse(readFileSync(src, 'utf8'));
const base = path.dirname(path.resolve(src));
const abs = (p) => (p === undefined ? p : path.resolve(base, p));
const h = (s) => createHash('sha256').update(s).digest('hex');
const cases = m.cases
    .map((c) => ({ c, k: h(c.id) }))
    .sort((a, b) => (a.k < b.k ? -1 : a.k > b.k ? 1 : 0))
    .slice(0, n)
    .map(({ c }) => ({ ...c, oracle: abs(c.oracle), rust: abs(c.rust) }));
writeFileSync(out, JSON.stringify({ ...m, cases }, null, 1) + '\n');
console.log(`sampled ${cases.length}/${m.cases.length} by sha256(id) -> ${out}`);
