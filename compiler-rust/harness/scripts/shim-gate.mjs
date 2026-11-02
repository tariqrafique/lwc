// Parity-shim gate. Scans crates/**/*.rs.
//  - Tagged counts (non-gating, cutover blockers): SHIM(template-gap:, SHIM( / TODO(parity
//  - UNTAGGED (GATING): a line matching a suspicious word that does not itself carry a SHIM( tag,
//    unless allowlisted in shim-allowlist.tsv (path<TAB>word<TAB>reason; reason required).
// Words: shim, compat, workaround, hack, mask*, rescan/re-scan.
import fs from 'node:fs'; import path from 'node:path';
const here = path.dirname(new URL(import.meta.url).pathname);
const root = process.env.SHIM_ROOT ? path.resolve(process.env.SHIM_ROOT) : path.resolve(here, '../../crates');
const WORD = /\b(shims?|compat|workarounds?|hacks?|hacky|re-?scan\w*|mask\w*)\b|\w*_compat\b|\bcompat_\w*/gi;
const allow = [];
const allowFile = path.resolve(here, '../shim-allowlist.tsv');
if (fs.existsSync(allowFile)) for (const l of fs.readFileSync(allowFile, 'utf8').split('\n')) {
  if (!l.trim() || l.startsWith('#')) continue;
  const [p, w, reason] = l.split('\t');
  if (!reason?.trim()) { console.log(`  ALLOWLIST-INVALID (no reason): ${l}`); process.exitCode = 1; continue; }
  allow.push({ p, w: w.toLowerCase() });
}
const files = []; (function walk(d) { for (const e of fs.readdirSync(d, { withFileTypes: true })) {
  if (e.name === 'target' || e.name.startsWith('.')) continue;
  const f = path.join(d, e.name); e.isDirectory() ? walk(f) : f.endsWith('.rs') && files.push(f); } })(root);
let tg = 0, tagged = 0; const untagged = [];
for (const f of files) {
  const rel = path.relative(root, f);
  fs.readFileSync(f, 'utf8').split('\n').forEach((line, i) => {
    tg += (line.match(/SHIM\(template-gap:/g) || []).length;
    tagged += (line.match(/SHIM\(|TODO\(parity/g) || []).length;
    if (/SHIM\(/.test(line)) return;
    for (const m of line.matchAll(WORD)) {
      const w = m[0].toLowerCase();
      if (allow.some(a => a.p === rel && a.w === w)) continue;
      untagged.push(`${rel}:${i + 1}: [${m[0]}] ${line.trim().slice(0, 140)}`); break;
    }
  });
}
console.log(`shim-check: SHIM(template-gap:=${tg}  SHIM(|TODO(parity=${tagged}  (cutover blockers until 0)  untagged=${untagged.length} (GATING)`);
for (const u of untagged) console.log(`  UNTAGGED ${u}`);
if (untagged.length) process.exitCode = 1;
