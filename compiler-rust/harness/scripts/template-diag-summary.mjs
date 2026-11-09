#!/usr/bin/env node
// Board summary for the template-diag row(s): validate a template-diag-diff/v2 report and print ONE board line.
// Clean = a case whose result is in the closed enum {STATIC_EQUAL, REWRITE_EQUAL} (D-0219). Anything else, including
// an unknown value, is unclean. v1 reports and the old UNSOUND-ws-stripped-hash label path are rejected.
// Gating: crash>0 (Rust panic) or totals.oracleError>0 (JS oracle harness failure) => exit 1.
// usage: node scripts/template-diag-summary.mjs <report.json> <engine> <row>
// stdout: the row line (only if the report is valid). exit 0 = valid and crash 0; exit 1 = invalid/missing/crash,
// reason on stderr. No false-pass path: anything unexpected in the report is a failure, never a pass.
// This script NEVER writes the report it reads. After validation it writes the stamp as its own artifact next to
// the report: <report minus .json>.soundness.json = {schema:"template-diag-soundness/v1", report:<basename>,
// report_sha256:<sha256 of the report bytes read>, soundness:<derived from meta.codegenCompare.mode>, comparator:"..."}.
// publish-official.sh refuses when that artifact is missing, label-mismatched or names other report bytes.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const [file, engine, row] = process.argv.slice(2);
const fail = (why) => { console.error(why); process.exit(1); };
if (!file || !engine || !row) fail('usage: template-diag-summary.mjs <report.json> <engine> <row>');
if (!fs.existsSync(file)) fail(`missing report ${file}`);
let r, bytes;
try { bytes = fs.readFileSync(file); r = JSON.parse(bytes.toString('utf8')); } catch (e) { fail(`invalid JSON ${file}: ${e.message}`); }

const nat = (v) => Number.isInteger(v) && v >= 0;
if (r?.schema !== 'template-diag-diff/v2') fail(`schema ${JSON.stringify(r?.schema)} != template-diag-diff/v2`);
if (r.meta?.engine !== engine) fail(`report engine ${JSON.stringify(r.meta?.engine)} != ${engine}`);
const t = r.totals ?? {};
for (const k of ['cases', 'clean', 'staticEqual', 'rewriteEqual', 'pass', 'codegenEquivalent', 'diagPass', 'diagMismatch', 'codegenOnly', 'codegenUnclean', 'oracleError']) {
    if (!nat(t[k])) fail(`totals.${k} not a count: ${t[k]}`);
}
if (t.cases === 0) fail('0 cases (empty corpus)');
if (t.diagMismatch !== t.cases - t.diagPass) fail(`totals inconsistent: diagMismatch ${t.diagMismatch} != cases ${t.cases} - diagPass ${t.diagPass}`);
if (!Array.isArray(r.mismatches) || r.mismatches.length !== t.diagMismatch) fail(`mismatches[] length ${r.mismatches?.length} != totals.diagMismatch ${t.diagMismatch}`);
const srcSum = Object.values(r.perSource ?? {}).reduce((a, s) => a + (s?.cases ?? NaN), 0);
if (srcSum !== t.cases) fail(`perSource cases sum ${srcSum} != totals.cases ${t.cases}`);
const BUCKETS = ['oracle-error', 'crash', 'seam-reject', 'missing-diag', 'extra-diag', 'code', 'level', 'message', 'location', 'order', 'codegen-only'];
for (const b of BUCKETS) {
    if (!nat(r.buckets?.[b])) fail(`buckets.${b} not a count: ${r.buckets?.[b]}`);
    if (!nat(r.primary?.[b])) fail(`primary.${b} not a count: ${r.primary?.[b]}`);
}
const primSum = BUCKETS.reduce((a, b) => a + r.primary[b], 0);
if (primSum !== t.cases - t.pass) fail(`primary sum ${primSum} != cases - identical ${t.cases - t.pass}`);
if (r.buckets['oracle-error'] !== t.oracleError) fail(`totals.oracleError ${t.oracleError} != buckets.oracle-error ${r.buckets['oracle-error']}`);
if (r.buckets['codegen-only'] !== t.codegenOnly) fail(`totals.codegenOnly ${t.codegenOnly} != buckets.codegen-only ${r.buckets['codegen-only']}`);

const MODE_SOUNDNESS = {
    'exact-bytes': 'SOUND-exact-only',
    'exact-bytes+semdiff': 'SOUND',
    'exact-bytes+semdiff-exec': 'SOUND',
};
const mode = r.meta?.codegenCompare?.mode;
const SOUNDNESS = MODE_SOUNDNESS[mode];
if (!SOUNDNESS) fail(`meta.codegenCompare.mode ${JSON.stringify(mode)} unknown (cannot derive soundness)`);

// Closed clean enum: STATIC_EQUAL (byte-identical) + REWRITE_EQUAL (semdiff EQUIVALENT-BY-REWRITE).
if (t.staticEqual !== t.pass) fail(`totals.staticEqual ${t.staticEqual} != pass ${t.pass}`);
if (t.rewriteEqual !== t.codegenEquivalent) fail(`totals.rewriteEqual ${t.rewriteEqual} != codegenEquivalent ${t.codegenEquivalent}`);
if (t.clean !== t.staticEqual + t.rewriteEqual) fail(`totals.clean ${t.clean} != staticEqual ${t.staticEqual} + rewriteEqual ${t.rewriteEqual}`);
if (t.rewriteEqual + t.codegenUnclean !== t.codegenOnly) fail(`rewriteEqual ${t.rewriteEqual} + codegenUnclean ${t.codegenUnclean} != codegenOnly ${t.codegenOnly}`);
if (mode === 'exact-bytes' && t.rewriteEqual !== 0) fail(`rewriteEqual ${t.rewriteEqual} with semdiff off`);
if (!Array.isArray(r.codegenUnclean) || r.codegenUnclean.length !== t.codegenUnclean) fail(`codegenUnclean[] length ${r.codegenUnclean?.length} != totals.codegenUnclean ${t.codegenUnclean}`);
if ((r.resultCounts?.STATIC_EQUAL ?? 0) !== t.staticEqual) fail(`resultCounts.STATIC_EQUAL ${r.resultCounts?.STATIC_EQUAL} != totals.staticEqual ${t.staticEqual}`);
if ((r.resultCounts?.REWRITE_EQUAL ?? 0) !== t.rewriteEqual) fail(`resultCounts.REWRITE_EQUAL ${r.resultCounts?.REWRITE_EQUAL} != totals.rewriteEqual ${t.rewriteEqual}`);
for (const c of r.codegenUnclean) {
    if (c.result === 'STATIC_EQUAL' || c.result === 'REWRITE_EQUAL') fail(`codegenUnclean id ${c.id} has clean result ${c.result}`);
    if (c.result == null) fail(`codegenUnclean id ${c.id} missing result`);
}
if (typeof r.soundness === 'string' && r.soundness !== SOUNDNESS) fail(`report soundness ${JSON.stringify(r.soundness)} != derived ${SOUNDNESS}: comparator/mode changed, update template-diag-summary.mjs`);
if (typeof r.soundness === 'string' && r.soundness.startsWith('UNSOUND')) fail(`report still carries UNSOUND label ${JSON.stringify(r.soundness)}`);

if (!file.endsWith('.json')) fail(`report ${file} must end in .json (the stamp goes to <report minus .json>.soundness.json)`);
const stampFile = `${file.slice(0, -'.json'.length)}.soundness.json`;
const stamp = {
    schema: 'template-diag-soundness/v1',
    report: path.basename(file),
    report_sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
    soundness: SOUNDNESS,
    comparator: `template-diag-diff.mjs: closed enum {STATIC_EQUAL, REWRITE_EQUAL}; mode ${mode}`,
};
try { fs.writeFileSync(`${stampFile}.tmp`, JSON.stringify(stamp, null, 2) + '\n'); fs.renameSync(`${stampFile}.tmp`, stampFile); }
catch (e) { fail(`cannot write soundness stamp ${stampFile}: ${e.message}`); }

const parts = ['code', 'message', 'level', 'location', 'order', 'missing-diag', 'extra-diag'];
if (r.primary['seam-reject'] || engine !== 'crate') parts.push('seam-reject');
const crash = r.buckets.crash, oerr = t.oracleError;
const line = `cases ${t.cases}  clean ${t.clean}  mismatches ${t.diagMismatch} (${parts.map((b) => `${b} ${r.primary[b]}`).join(', ')})`
    + `  codegen-unclean ${t.codegenUnclean} (STATIC_EQUAL ${t.staticEqual}, REWRITE_EQUAL ${t.rewriteEqual}, ${mode})`
    + `  crash ${crash}  oracle-error ${oerr}  ${crash || oerr ? '!! CRASH/ORACLE-ERROR (gating)' : '(diagnostic, non-gating)'}  ${SOUNDNESS}`;
console.log(`${row.padEnd(22)} ${line}`);
if (oerr > 0) fail(`${oerr} oracle-error case(s): JS oracle harness failed, not a diagnostic (see clusters oracle-error:* in ${file})`);
if (crash > 0) fail(`${crash} crash/panic case(s) (see clusters with key crash:* in ${file})`);
