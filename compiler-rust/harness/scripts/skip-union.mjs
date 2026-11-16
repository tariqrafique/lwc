#!/usr/bin/env node
// AB-0008 r4 (G2b-1): one test file split across board rows by `-t` filters (js-options + js-api) must run every
// case somewhere. Reads each row's vitest JSON report and fails when a case is skipped in EVERY row, so a new
// describe (or an `it.skip`) can never sit silently outside all filters.
//   skip-union.mjs <label> <row>=<vitest.json> <row>=<vitest.json> ...   rc 0 ok, 1 skipped-everywhere, 2 unusable
//   skip-union.mjs --self-test
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

class Bad extends Error {}
const bad = (m) => { throw new Bad(m); };
const SK = new Set(['skipped', 'pending', 'todo', 'disabled']);   // = board-baseline.mjs ids()

/** row -> Map(fullName -> 'run' | 'skipped') from one report (exactly one test file). */
function statuses(row, report) {
    if (!Array.isArray(report?.testResults) || report.testResults.length !== 1) bad(`${row}: want exactly 1 test file in the report, got ${report?.testResults?.length}`);
    const file = report.testResults[0];
    if (!Array.isArray(file.assertionResults)) bad(`${row}: no assertionResults`);
    const m = new Map();
    for (const a of file.assertionResults) {
        if (typeof a.fullName !== 'string' || a.fullName === '') bad(`${row}: unusable case id ${JSON.stringify(a.fullName)}`);
        if (m.has(a.fullName)) bad(`${row}: duplicate case id ${JSON.stringify(a.fullName)}`);
        const st = a.status === 'passed' || a.status === 'failed' ? 'run' : SK.has(a.status) ? 'skipped' : bad(`${row}: unknown status '${a.status}' for ${JSON.stringify(a.fullName)}`);
        m.set(a.fullName, st);
    }
    return { file: path.basename(String(file.name ?? '')), m };
}

export function skipUnion(label, rows) {
    if (rows.length < 1) bad('need at least one <row>=<report>');
    const per = rows.map(([row, report]) => ({ row, ...statuses(row, report) }));
    const [first] = per;
    if (first.m.size === 0) bad(`${first.row}: empty case set`);
    for (const p of per.slice(1)) {
        if (p.file !== first.file) bad(`${p.row} ran ${p.file}, ${first.row} ran ${first.file}: not one test file`);
        const missing = [...first.m.keys()].filter((k) => !p.m.has(k));
        const extra = [...p.m.keys()].filter((k) => !first.m.has(k));
        if (missing.length || extra.length) bad(`${p.row} and ${first.row} disagree on the case set (${missing.length} missing, ${extra.length} extra)`);
    }
    const everywhere = [...first.m.keys()].filter((k) => per.every((p) => p.m.get(k) === 'skipped')).sort();
    return { label, cases: first.m.size, rows: per.map((p) => p.row), everywhere };
}

function report(r) {
    if (r.everywhere.length === 0) return `${r.label} skip-union: ${r.cases} cases, 0 skipped in every row (${r.rows.join(' ')})`;
    const shown = r.everywhere.slice(0, 10).map((x) => JSON.stringify(x)).join(', ') + (r.everywhere.length > 10 ? `, ... (+${r.everywhere.length - 10})` : '');
    return `!! SKIP-UNION: ${r.label}: ${r.everywhere.length} of ${r.cases} case(s) skipped in every row (${r.rows.join(' ')}): ${shown}`;
}

function main(argv) {
    if (argv[0] === '--self-test') return selfTest();
    const [label, ...pairs] = argv;
    if (!label || pairs.length === 0) bad('usage: skip-union.mjs <label> <row>=<vitest.json>...');
    const rows = pairs.map((p) => {
        const i = p.indexOf('=');
        if (i <= 0) bad(`bad <row>=<report> argument '${p}'`);
        const row = p.slice(0, i), f = p.slice(i + 1);
        let rep; try { rep = JSON.parse(fs.readFileSync(f, 'utf8')); } catch (e) { bad(`${row}: unreadable report ${f}: ${e.message}`); }
        return [row, rep];
    });
    const r = skipUnion(label, rows);
    console.log(report(r));
    return r.everywhere.length ? 1 : 0;
}

function selfTest() {
    let fail = 0;
    const rep = (cases, file = '/x/test/js-options.behavior.test.ts') => ({ testResults: [{ name: file, assertionResults: cases.map(([fullName, status]) => ({ fullName, status })) }] });
    const t = fs.mkdtempSync(path.join(os.tmpdir(), 'skip-union-test.'));
    const run = (name, want, reports, re) => {
        const args = ['js-options', ...reports.map(([row, r], i) => { const f = path.join(t, `${i}.json`); fs.writeFileSync(f, typeof r === 'string' ? r : JSON.stringify(r)); return `${row}=${f}`; })];
        let rc, out = '';
        const log = console.log; console.log = (m) => { out += `${m}\n`; };
        try { rc = main(args); } catch (e) { if (!(e instanceof Bad)) throw e; rc = 2; out += e.message; } finally { console.log = log; }
        const ok = rc === want && (!re || re.test(out));
        console.log(`${ok ? 'ok  ' : 'FAIL'} ${name} -> rc ${rc}${ok ? '' : ` (want ${want}${re ? `, /${re.source}/` : ''}): ${out.trim()}`}`);
        if (!ok) fail++;
    };
    const api = ['js-options: api a', 'passed'], apiSk = ['js-options: api a', 'skipped'];
    const lws = ['js-options: lws b', 'passed'], lwsSk = ['js-options: lws b', 'skipped'];
    run('split rows cover every case', 0, [['js-options', rep([apiSk, lws])], ['js-api', rep([api, lwsSk])]], /2 cases, 0 skipped in every row/);
    run('failed counts as run', 0, [['js-options', rep([apiSk, ['js-options: lws b', 'failed']])], ['js-api', rep([api, lwsSk])]]);
    run('plant: unrouted case skipped in both rows', 1, [['js-options', rep([apiSk, lws, ['plant unrouted', 'skipped']])], ['js-api', rep([api, lwsSk, ['plant unrouted', 'skipped']])]], /SKIP-UNION: js-options: 1 of 3 .*"plant unrouted"/);
    run('it.skip inside a routed describe', 1, [['js-options', rep([apiSk, lws, ['js-options: lws c', 'skipped']])], ['js-api', rep([api, lwsSk, ['js-options: lws c', 'todo']])]], /"js-options: lws c"/);
    run('single row: every skipped case fails', 1, [['js-options', rep([apiSk, lws])]], /"js-options: api a"/);
    run('rows disagree on the case set', 2, [['js-options', rep([apiSk, lws])], ['js-api', rep([api])]], /disagree/);
    run('different test files', 2, [['js-options', rep([apiSk, lws])], ['js-api', rep([api, lwsSk], '/x/test/other.test.ts')]], /not one test file/);
    run('unknown status', 2, [['js-options', rep([['a', 'weird']])]], /unknown status/);
    run('duplicate id', 2, [['js-options', rep([lws, lws])]], /duplicate/);
    run('empty case set', 2, [['js-options', rep([])]], /empty/);
    run('two test files in one report', 2, [['js-options', { testResults: [rep([lws]).testResults[0], rep([lws]).testResults[0]] }]], /exactly 1/);
    run('unreadable report', 2, [['js-options', '{not json']], /unreadable/);
    fs.rmSync(t, { recursive: true, force: true });
    console.log(`skip-union self-test: ${fail ? 'FAILED' : 'ok'}`);
    return fail ? 1 : 0;
}

if (import.meta.url === `file://${process.argv[1]}`) {
    let rc;
    try { rc = main(process.argv.slice(2)); } catch (e) { if (!(e instanceof Bad)) throw e; console.error(`!! skip-union: ${e.message}`); rc = 2; }
    process.exit(rc);
}
