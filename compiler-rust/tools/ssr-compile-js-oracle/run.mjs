// compile_js differential oracle: `@lwc/ssr-compiler` (JS, LWC_ROOT dist) vs the Rust napi this tree built, per case,
// over the public-property (cases-props.mjs), @wire (cases-wire.mjs), class-member (cases-members.mjs), generate-markup
// (cases-markup.mjs) and dynamic-import (cases-dynamic.mjs) suites. A case is a source string, or `{ src, dynamicImports }`. The cases that differ must be
// exactly the ones in known-diffs.txt: a new diff fails, and so does a listed case that now matches (the list
// only shrinks). `--print` dumps every case's upstream and ours lines.
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import os from 'node:os';
import { makeCompare } from './compare.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '../..');
const TOOL = path.relative(REPO, HERE);
const LWC = process.env.LWC_ROOT || '/workspace/lwc';
// Exit codes: 0 = every diff is listed and every listed case still differs; 1 = a new or stale diff, or the list or
// case set fails the $MAIN shrink check; 2 = no verdict (no napi or upstream build, SSR_ORACLE_NAPI set, $MAIN or
// known-diffs.txt unreadable, a bad argument, or an extractor could not find an expected node).
const setupFail = (msg) => { console.error(`!! ssr-compile-js-oracle: ${msg}`); process.exit(2); };
process.on('uncaughtException', (e) => setupFail(e?.stack ?? String(e)));
// argv is a closed set, each flag at most once: --root=<tree> --main=<ref> --napi=<path> --print. Anything else (an
// unknown flag, a positional, a duplicate, an empty or missing value, --print=x) is exit 2, so a typo in a gate's
// command line can never fall back to a default. Defaults apply only when a flag is absent.
const USAGE = 'usage: run.mjs [--root=<tree>] [--main=<ref>] [--napi=<path>] [--print]';
const opts = {};
for (const a of process.argv.slice(2)) {
  const eq = a.indexOf('=');
  const [k, v] = eq < 0 ? [a, undefined] : [a.slice(0, eq), a.slice(eq + 1)];
  if (!['--root', '--main', '--napi', '--print'].includes(k)) { /*RULE:arg-unknown*/ setupFail(`unknown argument ${JSON.stringify(a)}; ${USAGE}`); }
  if (k in opts) { /*RULE:arg-dup*/ setupFail(`${k} given twice; ${USAGE}`); }
  if (k === '--root' && !v) { /*RULE:arg-empty-root*/ setupFail(`--root needs a value (--root=<tree>); ${USAGE}`); }
  if (k === '--main' && !v) { /*RULE:arg-empty-main*/ setupFail(`--main needs a value (--main=<ref>); ${USAGE}`); }
  if (k === '--napi' && !v) { /*RULE:arg-empty-napi*/ setupFail(`--napi needs a value (--napi=<path>); ${USAGE}`); }
  if (k === '--print' && v !== undefined) { /*RULE:arg-print-value*/ setupFail(`--print takes no value; ${USAGE}`); }
  opts[k] = v ?? true;
}
// `--root=<tree>` (absent: this script's own tree) is the tree under test, as shim-gate's CRATES_ROOT: its
// known-diffs.txt, cases-*.mjs and napi target are read from there, and $MAIN from `git show` in its repo. Once
// D-0254 and D-0226 are wired, land.sh is meant to run main's copy of this script with --root=<branch worktree>,
// so the rules below are main's, not the branch's. That is the intended invocation, not what land.sh does today.
const ROOT = path.resolve(opts['--root'] ?? REPO);
const BRANCH = path.join(ROOT, TOOL);
const MAIN = opts['--main'] ?? 'main';
// The napi under test is the cdylib that `cargo build -p lwc-compiler-napi --release` (land.sh, install-napi.sh)
// leaves in this tree's target dir (D-0226: the gate runs against the built napi). `--napi=<path>` names another
// build output explicitly. The old SSR_ORACLE_NAPI env override is retired: an inherited env var must never
// pick the binary a gate judges, so its presence alone means no verdict (with or without --napi=).
if (/*RULE:env*/ process.env.SSR_ORACLE_NAPI !== undefined) setupFail('SSR_ORACLE_NAPI is retired; unset it and pass --napi=<path> for a dev binary');
const targetDir = process.env.CARGO_TARGET_DIR ? path.resolve(ROOT, process.env.CARGO_TARGET_DIR) : path.join(ROOT, 'target');
const NAPI = path.resolve(opts['--napi'] ?? path.join(targetDir, 'release/liblwc_compiler_napi.so'));
if (!fs.existsSync(NAPI)) setupFail(`napi not found: ${NAPI} (run \`cargo build -p lwc-compiler-napi --release\` in ${ROOT})`);
const UPSTREAM = path.join(LWC, 'packages/@lwc/ssr-compiler/dist/index.cjs');
if (!fs.existsSync(UPSTREAM)) setupFail(`upstream build not found: ${UPSTREAM} (run yarn build in ${LWC})`);
const require = createRequire(import.meta.url);
const { compileComponentForSSR } = require(UPSTREAM);
// process.dlopen loads the cdylib under its cargo name (require() only treats `.node` files as addons).
const napi = (() => { const m = { exports: {} }; try { process.dlopen(m, NAPI); } catch (e) { setupFail(`cannot load napi ${NAPI}: ${e.message}`); } return m.exports; })();
const NAPI_SHA = createHash('sha256').update(fs.readFileSync(NAPI)).digest('hex').slice(0, 12);
const FILE = '/x/foo/foo.js';

const extra = (c) => (c.dynamicImports ? { dynamicImports: c.dynamicImports } : {});
// transformSync's normalizeOptions (compiler options.ts DEFAULT_DYNAMIC_IMPORT_CONFIG): the napi side gets it from
// the Rust options' defaults; compileComponentForSSR alone does not apply it.
const DYNAMIC_IMPORT_DEFAULTS = { loader: '', strictSpecifier: true };
const upstream = (c) => compileComponentForSSR(c.src, FILE, { name: 'foo', namespace: 'x', dynamicImports: { ...DYNAMIC_IMPORT_DEFAULTS, ...c.dynamicImports } }).code;
const ours = (c) => {
  const r = napi.transform(c.src, FILE, { name: 'foo', namespace: 'x', targetSSR: true, ...extra(c) });
  if (r.error) {
    const e = new Error(r.error.message ?? JSON.stringify(r.error));
    e.location = r.error.location ?? (r.error.line != null ? { line: r.error.line, column: r.error.column } : undefined);
    throw e;
  }
  return r.code;
};

// Token-stream compare and per-suite extract live in compare.mjs (so the self-test can call them without
// loading a napi). String literals compare by cooked value (D-0196, D-0257); the expected node of an
// extractor must be present unless the case lists it in expectAbsent, or the run is no-verdict (exit 2).
const acorn = require(path.join(LWC, 'node_modules/acorn'));
const { extractors } = makeCompare(acorn);

const line = (run, c, extract) => {
  try {
    return { kind: 'ok', value: extract(run(c), c) };
  } catch (e) {
    if (e.extractMissing) return { kind: 'missing', value: e.message };
    const l = e.location;
    return { kind: 'ok', value: `ERR ${e.message.split('\n')[0]} @${l ? `${l.line}:${l.column}${l.length != null ? `+${l.start},${l.length}` : ''}` : '-'}` };
  }
};

// known-diffs.txt only shrinks, checked against $MAIN (`--main=<ref>`, default `main`), D-0253:
//   raise: every branch known-diff id must already be a known-diff id on $MAIN (no new entry, no swap);
//   drop:  every case id on $MAIN must still exist on the branch (deleting a case is not a way to shrink).
// $MAIN's list and cases are read with `git show` in ROOT's repo, never from a working tree. The branch's and
// $MAIN's case ids both come from loadSuites + caseIds, so the two sides cannot be derived differently.
const parseList = (text) => text.split('\n').map((l) => l.replace(/#.*/, '').trim()).filter(Boolean);
const listIds = (text) => parseList(text).map((l) => l.split(/\s+/)[0]);
const CASE_FILE = /^cases-([\w-]+)\.mjs$/;
// files: [[basename, source]] of one tree's cases-*.mjs -> { suite: cases }. Each is imported from a private temp
// copy (they are self-contained), so the branch and $MAIN load the same way.
const loadSuites = async (files) => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'ssr-oracle-cases-'));
  try {
    const suites = {};
    for (const [f, text] of files) {
      const suite = f.match(CASE_FILE)?.[1];
      if (!suite) continue;
      fs.writeFileSync(path.join(tmp, f), text);
      suites[suite] = (await import(path.join(tmp, f))).cases;
    }
    if (Object.keys(suites).length === 0) throw new Error('no cases-*.mjs');
    return suites;
  } finally { fs.rmSync(tmp, { recursive: true, force: true }); }
};
const caseIds = (suites) => new Set(Object.entries(suites).flatMap(([suite, cases]) => Object.keys(cases).map((n) => `${suite}/${n}`)));

const LIST = path.join(BRANCH, 'known-diffs.txt');
let listText;
try { listText = fs.readFileSync(LIST, 'utf8'); } catch (e) { /*RULE:branch-list*/ setupFail(`cannot read ${LIST}: ${e.message}`); }
const entries = parseList(listText);
const known = new Map(entries.map((l) => [l.split(/\s+/)[0], l]));
let suites;
try {
  suites = await loadSuites(fs.readdirSync(BRANCH).filter((f) => CASE_FILE.test(f)).map((f) => [f, fs.readFileSync(path.join(BRANCH, f), 'utf8')]));
} catch (e) { setupFail(`cannot read the cases under ${BRANCH}: ${e.message}`); }
const noExtractor = Object.keys(suites).filter((s) => !extractors[s]);
if (noExtractor.length) setupFail(`no extractor for suite ${noExtractor.join(', ')} (cases-<suite>.mjs under ${BRANCH})`);
let gitDir = BRANCH;
const git = (...args) => execFileSync('git', ['-C', gitDir, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
let mainKnown, mainCases;
try {
  const top = git('rev-parse', '--show-toplevel').trim();
  gitDir = top;
  const rel = path.relative(top, BRANCH).split(path.sep).join('/');
  git('rev-parse', '--verify', '--quiet', `${MAIN}^{commit}`);
  mainKnown = new Set(listIds(git('show', `${MAIN}:${rel}/known-diffs.txt`)));
  const files = git('ls-tree', '--name-only', MAIN, '--', `${rel}/`).split('\n').map((f) => path.posix.basename(f)).filter((f) => CASE_FILE.test(f));
  mainCases = caseIds(await loadSuites(files.map((f) => [f, git('show', `${MAIN}:${rel}/${f}`)])));
} catch (e) { /*RULE:main-ref*/ setupFail(`cannot read the oracle on ${MAIN}: ${String(e.stderr || e.message).trim().split('\n')[0]}`); }
const branchCases = caseIds(suites);
const listProblems = [];
if (known.size !== entries.length) listProblems.push(`known-diffs.txt lists a case twice (${entries.length} lines, ${known.size} cases)`);
const raised = [...known.keys()].filter((id) => !mainKnown.has(id));
if (/*RULE:raise*/ raised.length) listProblems.push(`not a known diff on ${MAIN} (the list only shrinks): ${raised.join(' ')}`);
const dropped = [...mainCases].filter((id) => !branchCases.has(id));
if (/*RULE:drop*/ dropped.length) listProblems.push(`case on ${MAIN} missing from the branch (deleting a case does not shrink the list): ${dropped.join(' ')}`);
const print = opts['--print'] === true;
const diffs = [];
const extractProblems = [];
let total = 0;
for (const [suite, cases] of Object.entries(suites)) {
  const extract = extractors[suite];
  for (const [name, src] of Object.entries(cases)) {
    total++;
    const id = `${suite}/${name}`;
    const c = typeof src === 'string' ? { src } : src;
    const [up, us] = [line(upstream, c, extract), line(ours, c, extract)];
    if (print) console.log(`${id}\n  up:   ${up.value}\n  ours: ${us.value}`);
    if (up.kind === 'missing' || us.kind === 'missing') {
      extractProblems.push(`${id}: up=${up.kind === 'missing' ? up.value : 'ok'} ours=${us.kind === 'missing' ? us.value : 'ok'}`);
      continue;
    }
    if (up.value !== us.value) diffs.push([id, up.value, us.value]);
  }
}
let failed = listProblems.length > 0;
for (const p of listProblems) console.log(`KNOWN-DIFFS ${p}`);
for (const p of extractProblems) console.log(`EXTRACT ${p}`);
for (const [id, up, us] of diffs) {
  if (known.has(id)) continue;
  failed = true;
  console.log(`NEW DIFF ${id}\n  up:   ${up}\n  ours: ${us}`);
}
for (const id of known.keys()) {
  if (diffs.some(([d]) => d === id)) continue;
  failed = true;
  console.log(`STALE ${id}: matches upstream (or no longer exists); delete it from known-diffs.txt`);
}
console.log(`${total} cases, ${diffs.length} differ, ${known.size} known (main=${MAIN} root=${ROOT})  napi sha256:${NAPI_SHA} ${NAPI}`);
process.exit(extractProblems.length ? 2 : failed ? 1 : 0);
