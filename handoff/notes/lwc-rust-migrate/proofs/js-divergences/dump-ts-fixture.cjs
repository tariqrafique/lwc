// Writes lwc-js tests/fixtures/ts-parse-shipped.json from the 9.5.3 oracle (via cmp.cjs OUT dumps).
// Usage: for f in t2 tsx tsx2 tsx3; do OUT=/tmp/o-$f.json QUIET=1 node cmp.cjs $f.json; done; node dump-ts-fixture.cjs
const fs = require('fs');
const out = [], seen = new Set();
for (const f of ['t2', 'tsx', 'tsx2', 'tsx3']) {
  const cases = JSON.parse(fs.readFileSync(`${__dirname}/${f}.json`)).map((c) => ({ opts: {}, ...c }));
  const res = JSON.parse(fs.readFileSync(`/tmp/o-${f}.json`));
  const arr = Array.isArray(res) ? res : Object.values(res);
  for (const c of cases) {
    if (seen.has(c.name)) continue; seen.add(c.name);
    const r = arr.find((x) => x.name === c.name);
    // Recovery-mode acceptance of Babel-recoverable syntax is the logged divergence (decisions.tsv); not pinned here.
    if (r.shipped === 'ok' && c.opts.experimentalErrorRecoveryMode) continue;
    const s = r.shipped;
    out.push({ id: c.name, src: c.src, filename: c.filename, recover: !!c.opts.experimentalErrorRecoveryMode,
      shipped: s === 'ok' ? 'ok' : { code: s.code, message: s.message, location: s.location } });
  }
}
fs.writeFileSync('/workspace/wt/js/crates/lwc-js/tests/fixtures/ts-parse-shipped.json', JSON.stringify(out, null, 1) + '\n');
console.log(out.length, 'cases');
