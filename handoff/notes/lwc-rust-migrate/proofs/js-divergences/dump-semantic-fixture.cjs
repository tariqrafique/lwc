// Writes lwc-js tests/fixtures/semantic-errors-shipped.json from the live 9.5.3 oracle (/workspace/lwc @ b9e8007).
// Usage: LWC_JS_CRATE=<worktree>/crates/lwc-js node dump-semantic-fixture.cjs
// Every input of semantic-cases.json runs as foo.js in normal mode (id) and in experimentalErrorRecoveryMode (id-rec).
// Accepted divergences are recorded, never skipped: the case keeps its shipped value and carries
// `accepted_divergence: "<D-id>"`; semantic_errors.rs asserts it still differs from shipped (AB-L0063 (2)).
const fs = require('fs');
const path = require('path');
const { createRequire } = require('module');
const crate = process.env.LWC_JS_CRATE;
if (!crate) throw new Error('LWC_JS_CRATE is not set: point it at the lwc-js crate of the worktree to write (…/crates/lwc-js)');
const fixtures = path.join(crate, 'tests', 'fixtures');
if (!fs.existsSync(path.join(crate, 'Cargo.toml')) || !fs.existsSync(fixtures)) throw new Error(`LWC_JS_CRATE=${crate} is not an lwc-js crate (no Cargo.toml / tests/fixtures)`);
const oracle = createRequire('/workspace/lwc/packages/@lwc/compiler/')('@lwc/compiler');
// Outcome ACCEPTED_DIVERGENCES only: 9.5.3 recovery compiles, we reject (D-0245). The 14 inputs both sides
// reject (D-0259) are a class/location *shape* difference, not an outcome difference — not listed here.
const D0245 = 'D-0245';
const ACCEPTED = {
  ...Object.fromEntries([
    'with', 'delete-ident', 'legacy-octal', 'var-eval', 'dup-label', 'dup-proto', 'break-undef-label',
    'rw-let-1', 'rw-static', 'rw-yield-param', 'rw-let-shorthand', 'rw-let-line2', 'rw-let-template',
    'dup-params', 'dup-export', 'dup-default-export', 'export-undeclared', 'priv-undeclared',
    'new-target-top', 'super-top', 'return-top', 'continue-undef', 'break-outside',
    'assign-eval', 'assign-arguments', 'octal-escape', 'fn-in-if', 'labeled-fn', 'await-top-id',
    'yield-id', 'strict-impl', 'getter-param', 'setter-noparam', 'class-ctor-dup',
    'super-call-no-extends', 'use-strict-nonsimple',
    'abstract-decl', 'abstract-default-named',
  ].map((id) => [`${id}-rec`, D0245])),
};
const cwd = process.cwd() + '/';
const strip = (s) => (typeof s === 'string' ? s.split(cwd).join('') : s);
const loc = (l) => l == null ? null : Object.fromEntries(['line', 'column', 'index', 'start', 'length'].filter((k) => l[k] != null).map((k) => [k, l[k]]));
const out = [];
for (const c of JSON.parse(fs.readFileSync(path.join(__dirname, 'semantic-cases.json'), 'utf8'))) {
  for (const recover of [false, true]) {
    const id = recover ? `${c.id}-rec` : c.id;
    const opts = { namespace: 'x', name: 'foo', ...(recover ? { experimentalErrorRecoveryMode: true } : {}) };
    let shipped;
    try { oracle.transformSync(c.src, 'foo.js', opts); shipped = 'ok'; } catch (e) {
      if (Array.isArray(e.errors)) throw new Error(`${id}: aggregate; extend the fixture shape`);
      shipped = { code: e.code ?? null, message: strip(e.message), filename: e.filename ?? null, location: loc(e.location) };
    }
    out.push({ id, src: c.src, filename: 'foo.js', recover, shipped, ...(ACCEPTED[id] ? { accepted_divergence: ACCEPTED[id] } : {}) });
  }
}
for (const id of Object.keys(ACCEPTED)) if (!out.some((c) => c.id === id)) throw new Error(`ACCEPTED orphan ${id}`);
fs.writeFileSync(path.join(fixtures, 'semantic-errors-shipped.json'), JSON.stringify(out, null, 1) + '\n');
console.log(out.length, 'cases');
