#!/usr/bin/env node
// AB-0006 per-case board baseline (harness/board-baseline.tsv, board-baseline/v2). ONE parser + ONE comparator for
// board.sh (equality gate, BOARD_RATCHET_UPDATE) and land.sh (scripts/baseline-ceiling.sh: no row may get worse than the
// pre-land commit's; land.sh's board check: judge-all of the branch blob against a measured board run).
// Every number in the file gates (AB-0006 rework 2): there is no diagnostic/non-gating row here.
//
// Format (tabs; nothing else allowed; LF line ends):
//   line 1  board-baseline/v2
//   line 2  #row<TAB>kind<TAB>value
//   then per row, rows contiguous, each row's lines in this order:
//     <row>  counts   <passed> <failed> <skipped>          vitest rows
//     <row>  counts   <crash> <oracle-error>               template-diag* rows (clean/mismatch counts come from the
//                                                          whitespace-stripped comparator and are NOT recorded; D-0219)
//     <row>  passed   <case id>    one line per passing case, sorted (code-unit order), count == <passed>
//     <row>  failed   <case id>    one line per failing case, sorted, count == <failed>
//     <row>  skipped  <case id>    one line per skipped case, sorted, count == <skipped>
//   The passed+failed+skipped ids are the row's case UNIVERSE (D-0255): a removed id or a newly skipped id is a
//   CASE-SET CHANGE (land.sh: needs a 'Case-Set: <D-id>' trailer naming a decision that lists 'case-set:<row>').
//   A case id is the vitest fullName (describe titles + test title, space-joined) of a test in the row's one test file,
//   restricted to the row's -t filter (tests outside the filter belong to another row and are not recorded).
//   template-diag* rows carry counts only (board.sh also fails any run with crash > 0 or oracle-error > 0).
//
// Subcommands (exit 0 ok / 1 gate verdict / 2 cannot run: malformed input, usage; never a silent pass;
//   ceiling's WORSE verdict is exit 3 so a node failure (exit 1) can never read as a raise-acceptable verdict):
//   ids <row> <vitest.json> <P> <F> <S> [<testNamePattern>]   -> canonical row block on stdout (cross-checked vs summary)
//   check <baseline> <rows...>                                 -> validate (+ rows exactly <rows> in order)
//   judge <baseline> <obs-dir> <rows-run...>                   -> equality gate per run row (a missing observation fails)
//   judge-all <baseline> <obs-dir>                             -> judge every row of <baseline> (land.sh's board check)
//   update <baseline> <obs-dir> <out> <rows...>                -> write <out> from obs iff no row is WORSE
//   judge-all <baseline> <obs-dir> <measured-out>             -> ... and on equality write the measured run as a baseline
//   ceiling <cand-baseline> <main-baseline> [<case-set-out>]   -> land gate: 3 iff a row is worse than / missing vs main's;
//                                                                 <case-set-out>: rows with a case-set change, one per line
//   --self-test
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';

const MAGIC = 'board-baseline/v2';
const KINDS = ['passed', 'failed', 'skipped'];
const COLS = '#row\tkind\tvalue';
const isTd = (row) => row === 'template-diag' || row.startsWith('template-diag-');
const cmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
class Bad extends Error {}
const bad = (m) => { throw new Bad(m); };
const NAT = /^(0|[1-9][0-9]*)$/;
const ROWNAME = /^[A-Za-z0-9_-]+$/;

// parse(text) -> Map(row -> {counts:[a,b,c], failed:[], skipped:[]}) in file order. Throws Bad on any defect.
export function parse(text, where = 'baseline') {
    if (!text.endsWith('\n')) bad(`${where}: must end with a newline`);
    const lines = text.slice(0, -1).split('\n');
    if (lines[0] !== MAGIC) bad(`${where}: line 1 must be '${MAGIC}'`);
    if (lines[1] !== COLS) bad(`${where}: line 2 must be the column header '${COLS.replace(/\t/g, '\\t')}'`);
    const rows = new Map(); let cur = null, stage = 0, last = null;
    for (let i = 2; i < lines.length; i++) {
        const n = i + 1, f = lines[i].split('\t');
        if (f.length !== 3) bad(`${where}: line ${n}: want 3 tab-separated fields, got ${f.length}`);
        const [row, kind, value] = f;
        if (!ROWNAME.test(row)) bad(`${where}: line ${n}: bad row name '${row}'`);
        if (kind === 'counts') {
            if (rows.has(row)) bad(`${where}: line ${n}: row '${row}' appears twice (rows must be contiguous, one counts line)`);
            const c = value.split(' '), want = isTd(row) ? 2 : 3;
            if (c.length !== want || !c.every((x) => NAT.test(x))) bad(`${where}: line ${n}: counts of '${row}' must be ${want} naturals (${isTd(row) ? 'crash oracle-error; a 3-number clean/crash/oracle-error line is refused (D-0219: clean/mismatch stay unratcheted until the comparator is sound)' : 'passed failed skipped'}), got '${value}'`);
            cur = { counts: c.map(Number), passed: [], failed: [], skipped: [] }; rows.set(row, cur); last = row; stage = 0; continue;
        }
        if (!cur || last !== row) bad(`${where}: line ${n}: '${kind}' line for '${row}' not directly under its counts line`);
        const st = KINDS.indexOf(kind) + 1;
        if (st === 0) bad(`${where}: line ${n}: unknown kind '${kind}'`);
        if (isTd(row)) bad(`${where}: line ${n}: template-diag rows carry counts only`);
        if (st < stage) bad(`${where}: line ${n}: '${kind}' out of order (order: counts, passed, failed, skipped)`);
        stage = st;
        if (value === '' || /[\r]/.test(value)) bad(`${where}: line ${n}: empty or CR in case id`);
        const arr = cur[kind];
        if (arr.length && cmp(arr[arr.length - 1], value) >= 0) bad(`${where}: line ${n}: ${kind} ids of '${row}' not strictly sorted / duplicate: '${value}'`);
        arr.push(value);
    }
    for (const [row, r] of rows) if (!isTd(row)) {
        if (r.passed.length !== r.counts[0]) bad(`${where}: row '${row}': ${r.passed.length} passed ids != passed count ${r.counts[0]}`);
        const u = new Set(r.passed); for (const id of [...r.failed, ...r.skipped]) { if (u.has(id)) bad(`${where}: row '${row}': case id in two states: ${JSON.stringify(id)}`); u.add(id); }
        if (r.failed.length !== r.counts[1]) bad(`${where}: row '${row}': ${r.failed.length} failed ids != failed count ${r.counts[1]}`);
        if (r.skipped.length !== r.counts[2]) bad(`${where}: row '${row}': ${r.skipped.length} skipped ids != skipped count ${r.counts[2]}`);
    }
    return rows;
}
export function render(rows) {
    const out = [MAGIC, COLS];
    for (const [row, r] of rows) {
        out.push(`${row}\tcounts\t${r.counts.join(' ')}`);
        for (const id of r.passed ?? []) out.push(`${row}\tpassed\t${id}`);
        for (const id of r.failed) out.push(`${row}\tfailed\t${id}`);
        for (const id of r.skipped) out.push(`${row}\tskipped\t${id}`);
    }
    return out.join('\n') + '\n';
}
const rowEq = (a, b) => a.counts.join(' ') === b.counts.join(' ') && a.passed.join('\n') === b.passed.join('\n') && a.failed.join('\n') === b.failed.join('\n') && a.skipped.join('\n') === b.skipped.join('\n');
const minus = (a, b) => { const s = new Set(b); return a.filter((x) => !s.has(x)); };
const short = (ids, n = 5) => ids.slice(0, n).map((x) => JSON.stringify(x)).join(', ') + (ids.length > n ? `, ... (+${ids.length - n})` : '');
// worse(ref, cand): reasons the candidate is WORSE than the reference (empty = not worse).
// vitest rows: a new failing/skipped id (a swap counts), fewer passed, a row total that dropped, or ids that left the
// failing/skipped sets without passed rising by at least as many (passed_cand - passed_ref >= #ids no longer failing or
// skipped: a deleted failing/skipped case is not a fix). template-diag*: crash or oracle-error rose.
const total = (r) => r.counts[0] + r.counts[1] + r.counts[2];
const universe = (r) => [...r.passed, ...r.failed, ...r.skipped];
// caseSet(ref, cand): D-0255 case-set changes = ids gone from the universe + ids newly skipped (from passed or failed).
export function caseSet(row, ref, cand) {
    if (isTd(row)) return { removed: [], skipped: [] };
    return { removed: minus(universe(ref), universe(cand)), skipped: minus(cand.skipped, ref.skipped) };
}
export function worse(row, ref, cand) {
    const w = [];
    if (isTd(row)) {
        const [k0, o0] = ref.counts, [k1, o1] = cand.counts;
        if (k1 > k0) w.push(`crash ${k0} -> ${k1}`); if (o1 > o0) w.push(`oracle-error ${o0} -> ${o1}`);
        return w;
    }
    const nf = minus(cand.failed, ref.failed), ns = minus(cand.skipped, ref.skipped);
    if (nf.length) w.push(`newly failing ${nf.length}: ${short(nf)}`);
    if (ns.length) w.push(`newly skipped ${ns.length}: ${short(ns)}`);
    if (cand.counts[0] < ref.counts[0]) w.push(`passed ${ref.counts[0]} -> ${cand.counts[0]}`);
    const gone = [...minus(ref.failed, cand.failed), ...minus(ref.skipped, cand.skipped)], rose = cand.counts[0] - ref.counts[0];
    if (gone.length && rose < gone.length) w.push(`${gone.length} id(s) no longer failing/skipped but passed rose by ${rose} (< ${gone.length}): ${short(gone)}`);
    if (total(cand) < total(ref)) w.push(`row total ${total(ref)} -> ${total(cand)}`);
    const rm = caseSet(row, ref, cand).removed;
    if (rm.length) w.push(`removed from the case universe ${rm.length}: ${short(rm)}`);
    return w;
}
// describe(ref, cand): every difference (for the equality gate's message).
function describe(row, ref, cand) {
    const d = [];
    if (ref.counts.join(' ') !== cand.counts.join(' ')) d.push(`counts ${ref.counts.join('/')} -> ${cand.counts.join('/')}`);
    if (!isTd(row)) {
        const nf = minus(cand.failed, ref.failed), fx = minus(ref.failed, cand.failed), ns = minus(cand.skipped, ref.skipped), us = minus(ref.skipped, cand.skipped);
        const rm = minus(universe(ref), universe(cand)), ad = minus(universe(cand), universe(ref));
        if (rm.length) d.push(`removed ${rm.length}: ${short(rm)}`);
        if (ad.length) d.push(`added ${ad.length}: ${short(ad)}`);
        if (nf.length) d.push(`newly failing ${nf.length}: ${short(nf)}`);
        if (fx.length) d.push(`no longer failing ${fx.length}: ${short(fx)}`);
        if (ns.length) d.push(`newly skipped ${ns.length}: ${short(ns)}`);
        if (us.length) d.push(`no longer skipped ${us.length}: ${short(us)}`);
    }
    return d.join('; ');
}
// ids: one vitest JSON report (one test file) -> canonical row block.
export function ids(row, report, P, F, S, pattern) {
    if (!ROWNAME.test(row) || isTd(row)) bad(`ids: bad vitest row name '${row}'`);
    if (!Array.isArray(report?.testResults) || report.testResults.length !== 1) bad(`ids: want exactly 1 test file in the report, got ${report?.testResults?.length}`);
    const file = report.testResults[0];
    if (!Array.isArray(file.assertionResults)) bad('ids: no assertionResults');
    if (file.status === 'failed' && !file.assertionResults.some((a) => a.status === 'failed')) bad(`ids: test file failed with no failing test (load error): ${String(file.message ?? '').slice(0, 200)}`);
    const SK = new Set(['skipped', 'pending', 'todo', 'disabled']);
    const all = { passed: 0, failed: 0, skipped: 0 }, kept = [];
    const re = pattern ? new RegExp(pattern) : null;
    for (const a of file.assertionResults) {
        const st = a.status === 'passed' ? 'passed' : a.status === 'failed' ? 'failed' : SK.has(a.status) ? 'skipped' : bad(`ids: unknown status '${a.status}' for ${JSON.stringify(a.fullName)}`);
        all[st]++;
        if (typeof a.fullName !== 'string' || a.fullName === '' || /[\t\n\r]/.test(a.fullName)) bad(`ids: unusable case id ${JSON.stringify(a.fullName)}`);
        if (re && !re.test(a.fullName)) { if (st !== 'skipped') bad(`ids: ${JSON.stringify(a.fullName)} is outside -t '${pattern}' but ${st}: filter semantics changed`); continue; }
        kept.push([a.fullName, st]);
    }
    if (all.passed !== +P || all.failed !== +F || all.skipped !== +S) bad(`ids: JSON counts ${all.passed}/${all.failed}/${all.skipped} != Tests summary ${P}/${F}/${S}`);
    const seen = new Set();
    for (const [id] of kept) { if (seen.has(id)) bad(`ids: duplicate case id ${JSON.stringify(id)} in row '${row}' (rename the test)`); seen.add(id); }
    const r = { counts: [0, 0, 0], passed: [], failed: [], skipped: [] };
    for (const [id, st] of kept) { r[st].push(id); r.counts[KINDS.indexOf(st)]++; }
    r.passed.sort(cmp); r.failed.sort(cmp); r.skipped.sort(cmp);
    return render(new Map([[row, r]])).split('\n').slice(2).join('\n');   // ends with '\n'
}
const read = (f, where) => { try { return fs.readFileSync(f, 'utf8'); } catch (e) { bad(`${where}: cannot read ${f}: ${e.code ?? e.message}`); } };

function sha256File(p) {
    return crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
}
// judge-all: every observed <row>.tsv must have a matching `sha256sum` line in obs-dir/SHA256SUMS (A).
function verifyObsHashes(obsdir, rows) {
    const man = path.join(obsdir, 'SHA256SUMS');
    if (!fs.existsSync(man)) bad(`judge-all: missing ${man}`);
    const want = new Map();
    for (const line of fs.readFileSync(man, 'utf8').split('\n')) {
        if (!line.trim()) continue;
        const m = /^([0-9a-f]{64})  (.+)$/.exec(line);
        if (!m) bad(`judge-all: malformed SHA256SUMS line: ${line}`);
        if (m[2] === 'SHA256SUMS' || m[2].includes('/') || m[2] === '..' || m[2] === '.') {
            bad(`judge-all: bad SHA256SUMS name ${m[2]}`);
        }
        want.set(m[2], m[1]);
    }
    for (const [name, hex] of want) {
        const fp = path.join(obsdir, name);
        if (!fs.existsSync(fp)) bad(`judge-all: SHA256SUMS names missing file ${name}`);
        const got = sha256File(fp);
        if (got !== hex) bad(`judge-all: sha256 mismatch for ${name}`);
    }
    for (const row of rows) {
        const name = `${row}.tsv`;
        if (fs.existsSync(path.join(obsdir, name)) && !want.has(name)) {
            bad(`judge-all: no SHA256SUMS entry for ${name}`);
        }
    }
}

function loadObs(dir, row) {
    const f = path.join(dir, `${row}.tsv`);
    if (!fs.existsSync(f)) return null;
    const m = parse(`${MAGIC}\n${COLS}\n${read(f, 'obs')}`, `obs ${row}`);
    if (m.size !== 1 || !m.has(row)) bad(`obs ${f}: must hold exactly row '${row}'`);
    return m.get(row);
}
function checkRows(m, rows, where) {
    const got = [...m.keys()];
    if (got.join(' ') !== rows.join(' ')) bad(`${where}: rows must be exactly ROWS in order; got: ${got.join(' ')}`);
}
function main(argv) {
    const [cmd, ...a] = argv;
    if (cmd === 'ids') {
        if (a.length < 5 || a.length > 6) bad('usage: ids <row> <vitest.json> <P> <F> <S> [pattern]');
        let rep; try { rep = JSON.parse(read(a[1], 'ids')); } catch (e) { if (e instanceof Bad) throw e; bad(`ids: invalid JSON ${a[1]}: ${e.message}`); }
        process.stdout.write(ids(a[0], rep, a[2], a[3], a[4], a[5])); return 0;
    }
    if (cmd === 'check') {
        if (a.length < 1) bad('usage: check <baseline> [rows...]');
        const m = parse(read(a[0], 'check'), a[0]); if (a.length > 1) checkRows(m, a.slice(1), a[0]);
        console.log(`board-baseline: ${a[0]} ok (${m.size} rows)`); return 0;
    }
    if (cmd === 'judge' || cmd === 'judge-all') {
        if (cmd === 'judge' ? a.length < 3 : a.length < 2 || a.length > 3) bad('usage: judge <baseline> <obs-dir> <rows-run...> | judge-all <baseline> <obs-dir> [<measured-out>]');
        const bl = parse(read(a[0], cmd), a[0]); let fail = 0;
        const rows = cmd === 'judge' ? a.slice(2) : [...bl.keys()];
        if (cmd === 'judge-all') verifyObsHashes(a[1], rows);
        for (const row of rows) {
            const o = loadObs(a[1], row); if (!o) { console.log(`!! RATCHET: ${row}: no observation (the row did not produce a usable result)`); fail = 1; continue; }
            const b = bl.get(row); if (!b) { console.log(`!! RATCHET: ${row} has no baseline row`); fail = 1; continue; }
            if (rowEq(b, o)) continue;
            const w = worse(row, b, o);
            console.log(`!! RATCHET ${w.length ? 'REGRESSION' : 'IMPROVED (reseed: BOARD_RATCHET_UPDATE=1 full run, commit harness/board-baseline.tsv)'}: ${row}: ${describe(row, b, o)}`);
            fail = 1;
        }
        if (cmd === 'judge-all') {
            console.log(`board-baseline judge-all: ${rows.length} row(s) ${fail ? 'DIFFER from the measured run' : 'equal to the measured run'}`);
            if (!fail && a[2]) fs.writeFileSync(a[2], render(new Map(rows.map((row) => [row, loadObs(a[1], row)]))));
        }
        return fail;
    }
    if (cmd === 'update') {
        if (a.length < 4) bad('usage: update <baseline> <obs-dir> <out> <rows...>');
        const bl = parse(read(a[0], 'update'), a[0]), rows = a.slice(3), out = new Map(); let fail = 0;
        for (const row of rows) {
            const o = loadObs(a[1], row); if (!o) { console.log(`!! RATCHET-UPDATE: ${row}: no observation (row failed)`); fail = 1; continue; }
            const b = bl.get(row);
            if (b) { const w = worse(row, b, o); if (w.length) { console.log(`!! RATCHET-UPDATE refused: ${row} got WORSE: ${w.join('; ')} (accept only by hand edit + Baseline-Raise review)`); fail = 1; } }
            if (!b || !rowEq(b, o)) console.log(`RATCHET-UPDATE: ${row}: ${b ? describe(row, b, o) : 'new row'}`);
            out.set(row, o);
        }
        if (fail) return 1;
        fs.writeFileSync(a[2], render(out)); return 0;
    }
    if (cmd === 'ceiling') {
        // Both files are git blobs extracted by scripts/baseline-ceiling.sh (pre-land commit vs branch commit); presence,
        // file mode and NEW are decided there. Every main row must exist on the branch and be no worse (no DIAG exemption).
        if (a.length !== 2 && a.length !== 3) bad('usage: ceiling <cand-baseline> <main-baseline> [<case-set-out>]');
        const br = parse(read(a[0], 'ceiling'), a[0]), mb = parse(read(a[1], 'ceiling'), a[1]); let fail = 0; const cs = [];
        for (const [row, m] of mb) {
            const b = br.get(row);
            if (!b) {
                console.log(`  BASELINE-CEILING: ${row} exists on main but is missing on the branch`); fail = 1;
                const empty = { counts: [0, 0, 0], passed: [], failed: [], skipped: [] };
                const c = caseSet(row, m, empty);
                cs.push({ row, removed: c.removed, skipped: c.skipped, missing: true });
                continue;
            }
            if (rowEq(m, b)) continue;
            const c = caseSet(row, m, b);
            if (c.removed.length || c.skipped.length) { cs.push({ row, removed: c.removed, skipped: c.skipped, missing: false }); console.log(`  CASE-SET CHANGE: ${row}: removed ${c.removed.length}${c.removed.length ? ` (${short(c.removed)})` : ''}, newly skipped ${c.skipped.length}${c.skipped.length ? ` (${short(c.skipped)})` : ''} (needs 'Case-Set: <D-id>' listing those ids as case-set:${row}:id|id)`); }
            const w = worse(row, m, b);
            if (w.length) { console.log(`  BASELINE-CEILING: ${row} WORSE than main: ${w.join('; ')}`); fail = 1; }
            else console.log(`  baseline ${row} improved: ${describe(row, m, b)}`);
        }
        for (const row of br.keys()) if (!mb.has(row)) console.log(`  baseline ${row} NEW on branch (no ceiling on main)`);
        console.log(`baseline-ceiling: ${mb.size} main row(s) checked, ${fail ? 'REFUSED (a worse row needs its own reviewed id: Baseline-Raise: <AB-id>)' : 'ok'}`);
        if (a[2]) {
            const lines = [];
            for (const it of cs) {
                for (const id of it.removed) lines.push(`${it.row}\tremoved\t${id}`);
                for (const id of it.skipped) lines.push(`${it.row}\tskipped\t${id}`);
                if (it.missing && !it.removed.length && !it.skipped.length) lines.push(`${it.row}\tmissing\t`);
            }
            fs.writeFileSync(a[2], lines.length ? lines.join('\n') + '\n' : '');
        }
        return fail ? 3 : 0;
    }
    if (cmd === '--self-test') return selfTest();
    bad('usage: board-baseline.mjs ids|check|judge|update|ceiling|--self-test ...');
}
function selfTest() {
    const t = fs.mkdtempSync(path.join(os.tmpdir(), 'board-baseline-test.')); let fail = 0;
    const B = (body) => `${MAGIC}\n${COLS}\n${body}`;
    // R(row, passed[], failed[], skipped[]) -> row block (ids sorted by the caller)
    const R = (row, P = [], F = [], S = []) => [`${row}\tcounts\t${P.length} ${F.length} ${S.length}`, ...P.map((x) => `${row}\tpassed\t${x}`), ...F.map((x) => `${row}\tfailed\t${x}`), ...S.map((x) => `${row}\tskipped\t${x}`)].join('\n') + '\n';
    const TD = (k = 0, o = 0) => `template-diag\tcounts\t${k} ${o}\n`;
    const A0 = R('a', ['A p1', 'A p2'], ['A f1'], ['A s1']), D0 = R('d', ['D p']);
    const base = B(A0 + TD() + D0);
    const run = (name, want, args, files = {}, check) => {
        for (const [k, v] of Object.entries(files)) { fs.mkdirSync(path.dirname(path.join(t, k)), { recursive: true }); fs.writeFileSync(path.join(t, k), v); }
        let rc; const log = console.log; console.log = () => {};
        try { rc = main(args.map((x) => x.replace(/^@/, `${t}/`))); } catch (e) { rc = e instanceof Bad ? 2 : 99; } finally { console.log = log; }
        const extra = check ? check() : '';
        if (rc === want && !extra) console.log(`  ok   ${name} (rc=${rc})`); else { console.log(`  FAIL ${name}: rc=${rc} want ${want} ${extra}`); fail = 1; }
        for (const k of [...Object.keys(files), 'out', 'cs']) fs.rmSync(path.join(t, k), { recursive: true, force: true });
    };
    const fileIs = (k, want) => () => { let got; try { got = fs.readFileSync(path.join(t, k), 'utf8'); } catch { got = '<missing>'; } return got === want ? '' : `${k}=${JSON.stringify(got)} want ${JSON.stringify(want)}`; };
    run('parse ok', 0, ['check', '@m', 'a', 'template-diag', 'd'], { m: base });
    run('rows != ROWS', 2, ['check', '@m', 'a', 'd'], { m: base });
    run('v1 line refused', 2, ['check', '@m'], { m: 'board-baseline/v1\n#row\tpassed|clean\tfailed|crash\tskipped|oracle-error\njs-eval\t2\t1\t0\n' });
    run('count/id mismatch (failed)', 2, ['check', '@m'], { m: B('a\tcounts\t0 2 0\na\tfailed\tx\n') });
    run('count/id mismatch (passed ids missing)', 2, ['check', '@m'], { m: B('a\tcounts\t2 0 0\n') });
    run('id in two states', 2, ['check', '@m'], { m: B(R('a', ['x'], ['x'])) });
    run('unsorted ids', 2, ['check', '@m'], { m: B('a\tcounts\t0 2 0\na\tfailed\ty\na\tfailed\tx\n') });
    run('duplicate id', 2, ['check', '@m'], { m: B('a\tcounts\t0 2 0\na\tfailed\tx\na\tfailed\tx\n') });
    run('failed before passed', 2, ['check', '@m'], { m: B('a\tcounts\t1 1 0\na\tfailed\tf\na\tpassed\tp\n') });
    run('skipped before failed', 2, ['check', '@m'], { m: B('a\tcounts\t0 1 1\na\tskipped\ts\na\tfailed\tf\n') });
    run('split row', 2, ['check', '@m'], { m: B(R('a', ['x']) + R('b', ['y']) + R('a', ['z'])) });
    run('td ids refused', 2, ['check', '@m'], { m: B('template-diag\tcounts\t1 0\ntemplate-diag\tfailed\tx\n') });
    run('td clean count refused (3 numbers)', 2, ['check', '@m'], { m: B('template-diag\tcounts\t63115 0 0\n') });
    run('vitest row with 2 numbers refused', 2, ['check', '@m'], { m: B('a\tcounts\t1 0\n') });
    run('no final newline', 2, ['check', '@m'], { m: base.slice(0, -1) });
    const obsA = (s) => ({ 'o/a.tsv': s });
    const obsAll = (a) => ({ 'o/a.tsv': a, 'o/template-diag.tsv': TD(), 'o/d.tsv': D0 });
    const withHashes = (files) => {
        const lines = [];
        const out = { ...files };
        for (const [k, v] of Object.entries(files)) {
            if (k.startsWith('o/') && k.endsWith('.tsv')) {
                const hex = crypto.createHash('sha256').update(v).digest('hex');
                lines.push(`${hex}  ${path.basename(k)}`);
            }
        }
        lines.sort();
        out['o/SHA256SUMS'] = lines.length ? lines.join('\n') + '\n' : '';
        return out;
    };
    run('judge equal', 0, ['judge', '@m', '@o', 'a'], { m: base, ...obsA(A0) });
    run('judge new failure', 1, ['judge', '@m', '@o', 'a'], { m: base, ...obsA(R('a', ['A p1'], ['A f1', 'A p2'], ['A s1'])) });
    run('judge improvement w/o reseed', 1, ['judge', '@m', '@o', 'a'], { m: base, ...obsA(R('a', ['A f1', 'A p1', 'A p2'], [], ['A s1'])) });
    run('judge swap (same counts)', 1, ['judge', '@m', '@o', 'a'], { m: base, ...obsA(R('a', ['A f1', 'A p2'], ['A p1'], ['A s1'])) });
    run('judge rename (same counts)', 1, ['judge', '@m', '@o', 'a'], { m: base, ...obsA(R('a', ['A p1', 'A p3'], ['A f1'], ['A s1'])) });
    run('judge drift on a former DIAG row gates', 1, ['judge', '@m', '@o', 'd'], { m: base, 'o/d.tsv': R('d', [], ['D p']) });
    run('judge missing observation', 1, ['judge', '@m', '@o', 'a'], { m: base });
    run('judge-all equal (+ measured written)', 0, ['judge-all', '@m', '@o', '@out'], withHashes({ m: base, ...obsAll(A0) }), fileIs('out', base));
    run('judge-all td crash', 1, ['judge-all', '@m', '@o', '@out'], withHashes({ m: base, ...obsAll(A0), 'o/template-diag.tsv': TD(9, 0) }), fileIs('out', '<missing>'));
    run('judge-all row not observed', 1, ['judge-all', '@m', '@o'], withHashes({ m: base, ...obsA(A0) }));
    run('judge-all missing SHA256SUMS', 2, ['judge-all', '@m', '@o'], { m: base, ...obsAll(A0) });
    run('judge-all sha256 mismatch', 2, ['judge-all', '@m', '@o'], { m: base, ...obsAll(A0), 'o/SHA256SUMS': '0' * 64 + '  a.tsv\n' });
    run('update improvement writes', 0, ['update', '@m', '@o', '@out', 'a'], { m: base, ...obsA(R('a', ['A f1', 'A p1', 'A p2'], [], ['A s1'])) });
    run('update worse refused', 1, ['update', '@m', '@o', '@out', 'a'], { m: base, ...obsA(R('a', ['A f1', 'A p2'], ['A p1'], ['A s1'])) });
    run('update removed failing id refused', 1, ['update', '@m', '@o', '@out', 'a'], { m: base, ...obsA(R('a', ['A p1', 'A p2'], [], ['A s1'])) });
    const br = (s) => ({ m: base, b: B(s) });
    const rest = TD() + D0;
    run('ceiling equal', 0, ['ceiling', '@b', '@m', '@cs'], br(A0 + rest), fileIs('cs', ''));
    run('ceiling improved (fixed id passes)', 0, ['ceiling', '@b', '@m', '@cs'], br(R('a', ['A f1', 'A p1', 'A p2'], [], ['A s1']) + rest), fileIs('cs', ''));
    run('ceiling added case', 0, ['ceiling', '@b', '@m', '@cs'], br(R('a', ['A p1', 'A p2', 'A p3'], ['A f1'], ['A s1']) + rest), fileIs('cs', ''));
    run('ceiling new failing id', 3, ['ceiling', '@b', '@m', '@cs'], br(R('a', ['A p1'], ['A f1', 'A p2'], ['A s1']) + rest), fileIs('cs', ''));
    run('ceiling swap', 3, ['ceiling', '@b', '@m'], br(R('a', ['A f1', 'A p2'], ['A p1'], ['A s1']) + rest));
    run('ceiling passing case removed (case-set)', 3, ['ceiling', '@b', '@m', '@cs'], br(R('a', ['A p1'], ['A f1'], ['A s1']) + rest), fileIs('cs', 'a\tremoved\tA p2\n'));
    run('ceiling rename (case-set)', 3, ['ceiling', '@b', '@m', '@cs'], br(R('a', ['A p1', 'A p3'], ['A f1'], ['A s1']) + rest), fileIs('cs', 'a\tremoved\tA p2\n'));
    run('ceiling passed -> skipped (case-set)', 3, ['ceiling', '@b', '@m', '@cs'], br(R('a', ['A p1'], ['A f1'], ['A p2', 'A s1']) + rest), fileIs('cs', 'a\tskipped\tA p2\n'));
    run('ceiling F3 failing id dropped, passed same', 3, ['ceiling', '@b', '@m', '@cs'], br(R('a', ['A p1', 'A p2'], [], ['A s1']) + rest), fileIs('cs', 'a\tremoved\tA f1\n'));
    run('ceiling F3 skipped id dropped, passed same', 3, ['ceiling', '@b', '@m', '@cs'], br(R('a', ['A p1', 'A p2'], ['A f1']) + rest), fileIs('cs', 'a\tremoved\tA s1\n'));
    run('ceiling F3 count rule (2 gone, passed +1)', 3, ['ceiling', '@b', '@m'], br(R('a', ['A f1', 'A p1', 'A p2']) + rest));
    run('ceiling row total dropped', 3, ['ceiling', '@b', '@m'], { m: B(A0 + R('e', ['1', '2'])), b: B(A0 + R('e', ['1'])) });
    run('ceiling row removed (case-set)', 3, ['ceiling', '@b', '@m', '@cs'], br(rest), fileIs('cs', 'a\tremoved\tA p1\na\tremoved\tA p2\na\tremoved\tA f1\na\tremoved\tA s1\n'));
    run('ceiling F4 template-diag row removed', 3, ['ceiling', '@b', '@m'], br(A0 + D0));
    run('ceiling F4 td crash rose', 3, ['ceiling', '@b', '@m'], br(A0 + TD(9, 0) + D0));
    run('ceiling F4 td oracle-error rose', 3, ['ceiling', '@b', '@m'], br(A0 + TD(0, 1) + D0));
    run('ceiling F4 former DIAG vitest row worse', 3, ['ceiling', '@b', '@m'], br(A0 + TD() + R('d', [], ['D p'])));
    run('ceiling unreadable branch', 2, ['ceiling', '@nope', '@m'], { m: base });
    run('ceiling malformed branch', 2, ['ceiling', '@b', '@m'], { m: base, b: 'board-baseline/v1\njs-eval\t2\t1\t0\n' });
    const rep = (st, extra = []) => ({ testResults: [{ status: 'passed', assertionResults: [...st.map(([n, s]) => ({ fullName: n, status: s })), ...extra] }] });
    const tryIds = (name, want, fn) => { let got; try { got = fn(); } catch (e) { got = e instanceof Bad ? 'BAD' : `ERR ${e.message}`; }
        if (got === want) console.log(`  ok   ids ${name}`); else { console.log(`  FAIL ids ${name}: ${JSON.stringify(got)} want ${JSON.stringify(want)}`); fail = 1; } };
    tryIds('block', 'r\tcounts\t1 1 1\nr\tpassed\tP\nr\tfailed\tF\nr\tskipped\tS\n', () => ids('r', rep([['P', 'passed'], ['S', 'skipped'], ['F', 'failed']]), 1, 1, 1));
    tryIds('summary mismatch', 'BAD', () => ids('r', rep([['P', 'passed']]), 2, 0, 0));
    tryIds('duplicate', 'BAD', () => ids('r', rep([['P', 'passed'], ['P', 'failed']]), 1, 1, 0));
    tryIds('-t filter drops out-of-row skips', 'r\tcounts\t1 0 0\nr\tpassed\tx: a\n', () => ids('r', rep([['x: a', 'passed'], ['y: b', 'skipped']]), 1, 0, 1, '^x: '));
    tryIds('-t filter: out-of-row test ran', 'BAD', () => ids('r', rep([['x: a', 'passed'], ['y: b', 'passed']]), 2, 0, 0, '^x: '));
    tryIds('unknown status', 'BAD', () => ids('r', rep([['P', 'weird']]), 1, 0, 0));
    tryIds('load error', 'BAD', () => ids('r', { testResults: [{ status: 'failed', message: 'boom', assertionResults: [] }] }, 0, 0, 0));
    fs.rmSync(t, { recursive: true, force: true });
    console.log(`board-baseline self-test: ${fail ? 'FAILED' : 'ok'}`); return fail;
}
try { process.exitCode = main(process.argv.slice(2)); }
catch (e) { if (e instanceof Bad) { console.error(`!! board-baseline: ${e.message}`); process.exitCode = 2; } else { console.error(e); process.exitCode = 2; } }
