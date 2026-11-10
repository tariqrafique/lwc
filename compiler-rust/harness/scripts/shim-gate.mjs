// Parity-shim gate.
//  - UNTAGGED (GATING, crates/**/*.rs only): a line matching a suspicious word that does not itself
//    carry a SHIM( tag, unless allowlisted in shim-allowlist.tsv (path<TAB>word<TAB>reason).
//  - Tagged counts (non-gating, cutover blockers, crates only): SHIM(template-gap:, SHIM( / TODO(parity
//  - SHIM registry (GATING, repo-wide via SHIM_FILE_LIST or git ls-files): every SHIM(<tag>) must match a row in
//    shim-registry.tsv (path repo-relative<TAB>tag<TAB>owner<TAB>expiry<TAB>reason); every row must
//    still occur (STALE otherwise). TODO(parity stays count-only. Expiry is NOT machine-evaluated —
//    enforced at the named stage's review (S<n> / AB-... / D-NNNN).
//  Env: CRATES_ROOT, SHIM_ALLOWLIST, SHIM_REGISTRY (branch inputs); SHIM_OWNERS + SHIM_REPO_ROOT
//    (OWNERS and repo root: land.sh passes MAIN's OWNERS and the branch tree). Gate constants
//    (exclusion list, expiry regex) live in this file — land runs MAIN's copy.
//  OWNERS is required on MAIN (no lowercase fallback): missing/empty/malformed => rc 2.
//  Skip only '#' comment lines and '=token' alias lines; every other non-empty line needs ≥2 columns.
// Words: shim, compat, workaround, hack, mask*, rescan/re-scan.
//  - SSR compile_js scanner RATCHET (GATING, AB-0002 / AB-D0001 cond 5): OCCURRENCES of the AB regex
//    `.lines()|.find(|.contains(|.starts_with(|.strip_prefix(` in crates/lwc-ssr/src/compile_js/**/*.rs,
//    excluding test code (a `#[cfg(test)]` directly followed, blank lines allowed, by `mod <ident> {`
//    excludes that block to its matching `}`; any other `#[cfg(test)]` excludes nothing; files named
//    tests.rs or under a tests/ dir), must EQUAL the one-integer baseline in
//    $CRATES_ROOT/../harness/ratchets/ssr-compile-js-scanners.txt (the branch's tree, also when main's copy
//    of this script runs). Above: fail. Below: fail until the baseline is lowered in the same commit.
//    Fails closed on any error.
import fs from 'node:fs'; import path from 'node:path';
import { execFileSync } from 'node:child_process';
const here = path.dirname(new URL(import.meta.url).pathname);

function internal(msg) {
  console.log(`  REGISTRY-ERROR (fail closed): ${msg}`);
  process.exit(2);
}

try { run(); } catch (e) { internal(e && (e.stack || e.message) || String(e)); }

function run() {
const root = process.env.CRATES_ROOT ? path.resolve(process.env.CRATES_ROOT) : path.resolve(here, '../../crates');
if (!root || !fs.existsSync(root) || !fs.statSync(root).isDirectory()) {
  internal(`bad or missing CRATES_ROOT: ${root}`);
}
const WORD = /\b(shims?|compat|workarounds?|hacks?|hacky|re-?scan\w*|mask\w*)\b|\w*_compat\b|\bcompat_\w*/gi;
const allow = [];
const allowFile = process.env.SHIM_ALLOWLIST ? path.resolve(process.env.SHIM_ALLOWLIST) : path.resolve(here, '../shim-allowlist.tsv');
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

// --- SHIM registry (GATING, repo-wide) ---
// Expiry is NOT machine-evaluated; enforced at the named stage's review (see harness/README.md).
// OWNERS and the exclusion list come from MAIN (SHIM_OWNERS / this file); branch cannot add a lane.
if (process.env.SHIM_REGISTRY === '') internal('empty SHIM_REGISTRY');
const registryFile = process.env.SHIM_REGISTRY !== undefined
  ? path.resolve(process.env.SHIM_REGISTRY)
  : path.resolve(here, '../shim-registry.tsv');
const repoRoot = process.env.SHIM_REPO_ROOT ? path.resolve(process.env.SHIM_REPO_ROOT)
  : process.env.BOARD_REPO_ROOT ? path.resolve(process.env.BOARD_REPO_ROOT)
  : path.resolve(root, '..');
if (!fs.existsSync(repoRoot) || !fs.statSync(repoRoot).isDirectory()) {
  internal(`bad or missing SHIM_REPO_ROOT: ${repoRoot}`);
}
// MAIN's OWNERS path (land.sh: $MAIN/OWNERS). Required — no fallback. Never read the branch's OWNERS.
const ownersFile = process.env.SHIM_OWNERS !== undefined
  ? String(process.env.SHIM_OWNERS)
  : path.join(repoRoot, 'OWNERS');
if (process.env.SHIM_OWNERS === '') internal('empty SHIM_OWNERS');
function loadOwnerLanes(f) {
  // Skip only '#' comments and '=token' alias lines (explicit). Every other non-empty line
  // must have ≥2 tab columns; missing/empty OWNERS or a malformed line => rc 2.
  if (!f || !fs.existsSync(f)) internal(`missing OWNERS: ${f || '<unset>'}`);
  let text; try { text = fs.readFileSync(f, 'utf8'); } catch (e) { internal(`OWNERS unreadable: ${e.message}`); }
  if (!text.trim()) internal(`empty OWNERS: ${f}`);
  const set = new Set();
  for (const [i, l] of text.split('\n').entries()) {
    if (!l.trim()) continue;
    if (l.startsWith('#')) continue;                         // comment
    if (l.startsWith('=')) {                                 // =token alias line
      const cols = l.split('\t');
      if (cols.length < 2) internal(`OWNERS malformed alias (line ${i + 1}): ${l}`);
      continue;
    }
    const bare = l.replace(/[ \t]#.*$/, '');
    const cols = bare.split('\t');
    if (cols.length < 2) internal(`OWNERS malformed (line ${i + 1}, need ≥2 columns): ${l}`);
    for (const o of cols[1].split(',')) {
      const t = o.trim();
      if (t && t !== '*') set.add(t);                        // lanes exclude shared '*'
    }
  }
  if (!set.size) internal(`OWNERS has no owner lanes: ${f}`);
  return set;
}
const ownerLanes = loadOwnerLanes(ownersFile);
const EXPIRY_OK = /^(S[0-9]+|AB-[A-Z]?[0-9]{4}|D-[0-9]{4})$/;
if (!fs.existsSync(registryFile)) internal(`missing registry: ${registryFile}`);
let regStat; try { regStat = fs.statSync(registryFile); } catch (e) { internal(`registry unreadable: ${e.message}`); }
if (regStat.isDirectory()) internal(`registry is directory: ${registryFile}`);
let regText; try { regText = fs.readFileSync(registryFile, 'utf8'); } catch (e) { internal(`registry unreadable: ${e.message}`); }

const rows = [];
let malformed = 0;
if (regText.includes('\r')) {
  console.log(`  REGISTRY-INVALID: CRLF (\\r) rejected`);
  malformed++; process.exitCode = 1;
} else {
  regText.split('\n').forEach((l, i) => {
    if (!l.trim() || l.startsWith('#')) return;
    const cols = l.split('\t');
    if (cols.length !== 5) {
      console.log(`  REGISTRY-INVALID (line ${i + 1}, need exactly 5 columns): ${l}`);
      malformed++; process.exitCode = 1; return;
    }
    const [rp, tag, owner, expiry, reason] = cols;
    const blank = ![rp, tag, owner, expiry, reason].every(x => String(x).trim() !== '');
    const ownerOk = ownerLanes.has(owner);
    const expiryOk = EXPIRY_OK.test(expiry);
    if (blank || !ownerOk || !expiryOk) {
      console.log(`  REGISTRY-INVALID (line ${i + 1}): ${l}`);
      malformed++; process.exitCode = 1; return;
    }
    rows.push({ path: rp, tag, owner, expiry, reason });
  });
}

// Closed exclusion list (MAIN's gate): docs/tests/source that mention SHIM( literally. Not a glob.
const SHIM_SCAN_EXCLUDE = [
  'harness/scripts/shim-gate.mjs',
  'scripts/test-shim-registry.sh',
  'harness/README.md',
  'harness/shim-registry.tsv',
  'harness/decisions.snapshot.tsv',
];
for (const ex of SHIM_SCAN_EXCLUDE) {
  const abs = path.join(repoRoot, ex);
  if (!fs.existsSync(abs)) internal(`exclusion missing under repo: ${ex}`);
}
const excludeSet = new Set(SHIM_SCAN_EXCLUDE);

let listed;
if (process.env.SHIM_FILE_LIST) {
  // Fail closed: official-board writes git-ls-tree of BOARD_COMMIT into this file (NUL-separated).
  try {
    listed = fs.readFileSync(process.env.SHIM_FILE_LIST).toString('utf8').split('\0').filter(Boolean);
  } catch (e) {
    internal(`SHIM_FILE_LIST unreadable (${process.env.SHIM_FILE_LIST}): ${e.message}`);
  }
} else {
  try {
    listed = execFileSync('git', ['-C', repoRoot, 'ls-files', '-z'], { encoding: 'buffer' })
      .toString('utf8').split('\0').filter(Boolean);
  } catch (e) {
    internal(`git ls-files failed under ${repoRoot}: ${e.message}`);
  }
}
const SHIM_TAG = /SHIM\(([^)]*)\)/g;
const occ = []; // {path, tag, line} repo-relative
for (const rel of listed) {
  if (excludeSet.has(rel)) continue;
  const abs = path.join(repoRoot, rel);
  let text;
  try { text = fs.readFileSync(abs, 'utf8'); } catch (e) { internal(`unreadable ${rel}: ${e.message}`); }
  text.split('\n').forEach((line, i) => {
    SHIM_TAG.lastIndex = 0;
    let m; while ((m = SHIM_TAG.exec(line)) !== null) occ.push({ path: rel, tag: m[1], line: i + 1 });
  });
}
const untracked = [];
for (const o of occ) {
  if (!rows.some(r => r.path === o.path && r.tag === o.tag)) untracked.push(o);
}
const stale = [];
for (const r of rows) {
  if (!occ.some(o => o.path === r.path && o.tag === r.tag)) stale.push(r);
}
console.log(`shim-registry: tags=${occ.length} rows=${rows.length} untracked=${untracked.length} stale=${stale.length} (GATING)`);
for (const u of untracked) console.log(`  UNTRACKED ${u.path}:${u.line} ${u.tag}`);
for (const s of stale) console.log(`  STALE ${s.path} ${s.tag}`);
if (untracked.length || stale.length || malformed) process.exitCode = 1;

const SCANNER = /\.lines\(\)|\.find\(|\.contains\(|\.starts_with\(|\.strip_prefix\(/g;
const TEST_MOD = /#\[cfg\(test\)\][ \t]*(?:\r?\n[ \t]*)*mod[ \t]+[A-Za-z_][A-Za-z0-9_]*[ \t]*\{/g;
// Offset just past the `}` matching the `{` at `open`, skipping comments, strings and char literals.
function matchingBrace(s, open, file) {
  let depth = 0;
  for (let i = open; i < s.length; i++) {
    const c = s[i];
    if (c === '/' && s[i + 1] === '/') { i = s.indexOf('\n', i); if (i < 0) break; continue; }
    if (c === '/' && s[i + 1] === '*') { i = s.indexOf('*/', i + 2) + 1; if (i <= 0) break; continue; }
    const raw = /^r(#*)"/.exec(s.slice(i, i + 260));
    if (raw && !/[A-Za-z0-9_]/.test(s[i - 1] ?? '')) { const end = s.indexOf('"' + raw[1], i + raw[0].length); if (end < 0) break; i = end + raw[1].length; continue; }
    if (c === '"') { for (i++; i < s.length && s[i] !== '"'; i++) if (s[i] === '\\') i++; continue; }
    if (c === "'") { const ch = /^'(?:\\.[^']*|[^\\'])'/.exec(s.slice(i, i + 16)); if (ch) { i += ch[0].length - 1; } continue; }
    if (c === '{') depth++;
    else if (c === '}' && --depth === 0) return i + 1;
  }
  throw new Error(`${file}: unbalanced braces in #[cfg(test)] mod at offset ${open}`);
}
function scannerRatchet() {
  const dir = path.join(root, 'lwc-ssr/src/compile_js');
  const baseFile = path.resolve(root, '../harness/ratchets/ssr-compile-js-scanners.txt');
  const raw = fs.readFileSync(baseFile, 'utf8');
  if (!/^\d+\n?$/.test(raw)) throw new Error(`${baseFile}: must hold exactly one integer line, got ${JSON.stringify(raw)}`);
  const baseline = Number(raw.trim());
  const rs = []; (function walk(d) { for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const f = path.join(d, e.name);
    if (e.isDirectory()) { if (e.name !== 'tests') walk(f); }
    else if (e.name.endsWith('.rs') && e.name !== 'tests.rs') rs.push(f); } })(dir);
  if (!rs.length) throw new Error(`${dir}: no .rs files`);
  const sites = []; let n = 0;
  for (const f of rs.sort()) {
    const src = fs.readFileSync(f, 'utf8');
    const excluded = []; // [startOffset, endOffset) of #[cfg(test)] mod blocks
    for (const m of src.matchAll(TEST_MOD)) excluded.push([m.index, matchingBrace(src, m.index + m[0].length - 1, f)]);
    let off = 0;
    src.split('\n').forEach((l, i) => {
      const start = off; off += l.length + 1;
      if (excluded.some(([a, b]) => start >= a && start < b)) return;
      const k = (l.match(SCANNER) || []).length;
      if (k) { n += k; sites.push(`${path.relative(root, f)}:${i + 1}: x${k} ${l.trim().slice(0, 140)}`); }
    });
  }
  console.log(`ssr-compile-js-scanners: count=${n} baseline=${baseline} (GATING, must be equal)`);
  if (n === baseline) return;
  if (n > baseline) console.log(`  RATCHET-FAIL: ${n - baseline} scanner site(s) above baseline ${baseline}; port to the oxc AST instead`);
  else console.log(`  RATCHET-FAIL: count ${n} is below baseline ${baseline}; lower harness/ratchets/ssr-compile-js-scanners.txt to ${n} in the same commit`);
  for (const s of sites) console.log(`  SITE ${s}`);
  process.exitCode = 1;
}
try { scannerRatchet(); } catch (e) { console.log(`  RATCHET-ERROR (fail closed): ${e.message}`); process.exitCode = 1; }
} // run()
