// Writes lwc-js tests/fixtures/ts-parse-shipped.json from the live 9.5.3 oracle (/workspace/lwc @ b9e8007).
// Usage: for f in t2 tsx tsx2 tsx3; do OUT=/tmp/o-$f.json node cmp-lwc.cjs <napi.node> $f.json; done
//        LWC_JS_CRATE=<worktree>/crates/lwc-js node dump-ts-fixture.cjs
// Accepted divergences are recorded, never skipped: the case keeps its shipped value and carries
// `accepted_divergence: "<D-id>"`; ts_parse.rs asserts it still differs and pins our output (see D0245.md).
const fs = require('fs');
const path = require('path');
const crate = process.env.LWC_JS_CRATE;
if (!crate) throw new Error('LWC_JS_CRATE is not set: point it at the lwc-js crate of the worktree to write (…/crates/lwc-js)');
const fixtures = path.join(crate, 'tests', 'fixtures');
if (!fs.existsSync(path.join(crate, 'Cargo.toml')) || !fs.existsSync(fixtures)) throw new Error(`LWC_JS_CRATE=${crate} is not an lwc-js crate (no Cargo.toml / tests/fixtures)`);
const ACCEPTED = { 'interface.ts-rec': 'D-0245' };
const out = [], seen = new Set();
for (const f of ['t2', 'tsx', 'tsx2', 'tsx3']) {
  const cases = JSON.parse(fs.readFileSync(`${__dirname}/${f}.json`)).map((c) => ({ opts: {}, ...c }));
  const arr = JSON.parse(fs.readFileSync(`/tmp/o-${f}.json`));
  for (const c of cases) {
    if (seen.has(c.name)) continue; seen.add(c.name);
    const r = arr.find((x) => x.name === c.name);
    const recover = !!c.opts.experimentalErrorRecoveryMode;
    // Recovery-mode acceptance of Babel-recoverable syntax must be an explicit accepted divergence (D-0245).
    if (r.shipped === 'ok' && recover && !ACCEPTED[c.name]) throw new Error(`unlisted recovery-mode divergence: ${c.name}`);
    const s = r.shipped;
    out.push({ id: c.name, src: c.src, filename: c.filename, recover,
      shipped: s === 'ok' ? 'ok' : { code: s.code, message: s.message, location: s.location },
      ...(ACCEPTED[c.name] ? { accepted_divergence: ACCEPTED[c.name] } : {}) });
  }
}
fs.writeFileSync(path.join(fixtures, 'ts-parse-shipped.json'), JSON.stringify(out, null, 1) + '\n');
console.log(out.length, 'cases');
