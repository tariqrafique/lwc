// Writes an lwc-js oracle fixture from cmp.cjs OUT dumps: {id, src, filename, opts, shipped}.
// Usage: OUT=/tmp/o-<f>.json QUIET=1 node cmp.cjs <f>.json (each f); node dump-fixture.cjs <out.json> <keep-regex> <exclude-regex|-> f1 f2 ...
const fs = require('fs');
const [outPath, keep, exclude, ...files] = process.argv.slice(2);
const out = [], seen = new Set();
for (const f of files) {
  const cases = JSON.parse(fs.readFileSync(`${__dirname}/${f}.json`)).map((c) => ({ opts: {}, ...c }));
  const arr = JSON.parse(fs.readFileSync(`/tmp/o-${f}.json`));
  for (const c of cases) {
    if (seen.has(c.name) || !new RegExp(keep).test(c.name) || (exclude !== '-' && new RegExp(exclude).test(c.name))) continue;
    seen.add(c.name);
    const s = arr.find((x) => x.name === c.name).shipped;
    out.push({ id: c.name, src: c.src, filename: c.filename, opts: c.opts,
      shipped: typeof s === 'string' ? s : s.errors ? { aggregate: s.errors.map((e) => ({ code: e.code, message: e.message })) } : { code: s.code, message: s.message } });
  }
}
fs.writeFileSync(outPath, JSON.stringify(out, null, 1) + '\n');
console.log(out.length, 'cases ->', outPath);
