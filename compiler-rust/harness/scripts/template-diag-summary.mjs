#!/usr/bin/env node
// Board summary for the template-diag row(s): validate a template-diag-diff/v1 report and print ONE board line.
// Gating inside a non-gating row: crash>0 (Rust panic) or totals.oracleError>0 (JS oracle harness failure) => exit 1.
// usage: node scripts/template-diag-summary.mjs <report.json> <engine> <row>
// stdout: the row line (only if the report is valid). exit 0 = valid and crash 0; exit 1 = invalid/missing/crash,
// reason on stderr. No false-pass path: anything unexpected in the report is a failure, never a pass.
import fs from 'node:fs';

const [file, engine, row] = process.argv.slice(2);
const fail = (why) => { console.error(why); process.exit(1); };
if (!file || !engine || !row) fail('usage: template-diag-summary.mjs <report.json> <engine> <row>');
if (!fs.existsSync(file)) fail(`missing report ${file}`);
let r;
try { r = JSON.parse(fs.readFileSync(file, 'utf8')); } catch (e) { fail(`invalid JSON ${file}: ${e.message}`); }

const nat = (v) => Number.isInteger(v) && v >= 0;
if (r?.schema !== 'template-diag-diff/v1') fail(`schema ${JSON.stringify(r?.schema)} != template-diag-diff/v1`);
if (r.meta?.engine !== engine) fail(`report engine ${JSON.stringify(r.meta?.engine)} != ${engine}`);
const t = r.totals ?? {};
for (const k of ['cases', 'pass', 'diagPass', 'diagMismatch', 'codegenOnly', 'oracleError']) if (!nat(t[k])) fail(`totals.${k} not a count: ${t[k]}`);
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
// primary = each case counted once under its first bucket; diag buckets + crash + codegen-only must cover every non-identical case.
const primSum = BUCKETS.reduce((a, b) => a + r.primary[b], 0);
if (primSum !== t.cases - t.pass) fail(`primary sum ${primSum} != cases - identical ${t.cases - t.pass}`);
if (r.buckets['oracle-error'] !== t.oracleError) fail(`totals.oracleError ${t.oracleError} != buckets.oracle-error ${r.buckets['oracle-error']}`);
if (r.buckets['codegen-only'] !== t.codegenOnly) fail(`totals.codegenOnly ${t.codegenOnly} != buckets.codegen-only ${r.buckets['codegen-only']}`);

// Mismatch breakdown by primary bucket (sums to the mismatch count together with crash).
const parts = ['code', 'message', 'level', 'location', 'order', 'missing-diag', 'extra-diag'];
if (r.primary['seam-reject'] || engine !== 'crate') parts.push('seam-reject');
const crash = r.buckets.crash, oerr = t.oracleError;
const line = `cases ${t.cases}  clean ${t.diagPass}  mismatches ${t.diagMismatch} (${parts.map((b) => `${b} ${r.primary[b]}`).join(', ')})`
    + `  crash ${crash}  oracle-error ${oerr}  codegen-only ${t.codegenOnly}  ${crash || oerr ? '!! CRASH/ORACLE-ERROR (gating)' : '(diagnostic, non-gating)'}`;
console.log(`${row.padEnd(22)} ${line}`);
if (oerr > 0) fail(`${oerr} oracle-error case(s): JS oracle harness failed, not a diagnostic (see clusters oracle-error:* in ${file})`);
if (crash > 0) fail(`${crash} crash/panic case(s) (see clusters with key crash:* in ${file})`);
