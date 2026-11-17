#!/usr/bin/env node
// Prints the semantic-diff-ssr board line. Exit 0 when crash, exec-differs, and
// exec-throw-one-side are all 0. Exit 1 when that gate fails (line still printed).
// Exit 2 when the summary or meta is unusable.
import fs from 'node:fs';

const REASONS = [
    'printer-equal',
    'static-observed-differs',
    'exec-static-comments-differ',
    'exec-differs',
    'exec-throw-one-side',
    'exec-uncovered',
];
const MODES = ['sync', 'async', 'asyncYield'];

function arg(name) {
    const i = process.argv.indexOf(name);
    if (i < 0 || !process.argv[i + 1]) {
        console.error(`semantic-diff-ssr-line: missing ${name}`);
        process.exit(2);
    }
    return process.argv[i + 1];
}

const summary = JSON.parse(fs.readFileSync(arg('--summary'), 'utf8'));
const metaLines = fs.readFileSync(arg('--meta'), 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
const exclusions = JSON.parse(fs.readFileSync(arg('--exclusions'), 'utf8'));
if (!['semdiff-summary/v1', 'semdiff-summary/v2'].includes(summary.schema)) {
    console.error(`semantic-diff-ssr-line: summary schema ${summary.schema}`);
    process.exit(2);
}
if (summary.corpus !== 'ssr-compiler') {
    console.error(`semantic-diff-ssr-line: corpus ${summary.corpus}`);
    process.exit(2);
}
if (summary.exec?.engine !== 'ssr-runtime') {
    console.error(`semantic-diff-ssr-line: exec.engine ${summary.exec?.engine}`);
    process.exit(2);
}
if (typeof summary.totals?.crash !== 'number' || !Array.isArray(summary.cases)) {
    console.error('semantic-diff-ssr-line: summary totals.crash or cases missing');
    process.exit(2);
}
const modeById = new Map(metaLines.map((m) => [m.id, m.mode]));
const counts = Object.fromEntries(REASONS.map((r) => [r, 0]));
const byMode = Object.fromEntries(MODES.map((m) => [m, Object.fromEntries(REASONS.map((r) => [r, 0]))]));
const other = {};
for (const c of summary.cases) {
    const reason = c.verdict?.reason;
    const mode = modeById.get(c.id);
    if (!mode) {
        console.error(`semantic-diff-ssr-line: no meta for ${c.id}`);
        process.exit(2);
    }
    if (Object.prototype.hasOwnProperty.call(counts, reason)) {
        counts[reason]++;
        byMode[mode][reason]++;
    } else {
        other[reason] = (other[reason] ?? 0) + 1;
    }
}
const ex = Array.isArray(exclusions.cases) ? exclusions.cases : [];
const exByMode = Object.fromEntries(MODES.map((m) => [m, 0]));
const exByReason = {};
for (const e of ex) {
    exByMode[e.mode] = (exByMode[e.mode] ?? 0) + 1;
    exByReason[e.reason] = (exByReason[e.reason] ?? 0) + 1;
}
const parts = [
    ...REASONS.map((r) => `${r}=${counts[r]}`),
    `crash=${summary.totals.crash}`,
    `exclusions=${ex.length}`,
];
const otherKeys = Object.keys(other).sort();
if (otherKeys.length) parts.push(`other=${otherKeys.map((k) => `${k}:${other[k]}`).join(',')}`);
const exKeys = Object.keys(exByReason).sort();
if (exKeys.length) parts.push(`exclusion-reasons=${exKeys.map((k) => `${k}:${exByReason[k]}`).join(',')}`);
for (const m of MODES) {
    const b = byMode[m];
    parts.push(
        `${m}:pe=${b['printer-equal']},sod=${b['static-observed-differs']},esc=${b['exec-static-comments-differ']},ed=${b['exec-differs']},et=${b['exec-throw-one-side']},eu=${b['exec-uncovered']},ex=${exByMode[m] ?? 0}`,
    );
}
const gateOk = summary.totals.crash === 0 && counts['exec-differs'] === 0 && counts['exec-throw-one-side'] === 0;
parts.push(`gate=${gateOk ? 'ok' : 'FAIL'}`);
process.stdout.write(`semantic-diff-ssr  ${parts.join('  ')}\n`);
if (!gateOk) {
    for (const c of summary.cases) {
        const reason = c.verdict?.reason;
        if (reason === 'exec-differs' || reason === 'exec-throw-one-side') {
            const fd = JSON.stringify(c.verdict?.first_diff ?? null);
            console.error(`${reason}\t${modeById.get(c.id)}\t${c.id}\t${fd.slice(0, 500)}`);
        }
    }
    process.exit(1);
}
