// Coverage gate (arch-board/coverage-gate-spec.md v2). Node built-ins only.
// usage: node scripts/coverage-gate.mjs [--strict] "<space-separated board rows>"
//        node scripts/coverage-gate.mjs --self-test
//
// Matrix: COVGATE_MATRIX if set (empty is a caller bug, exit 2). Otherwise the
// tracked in-repo snapshot harness/COVERAGE.tsv next to this script. That file
// is what portable and official boards read: arch-board/COVERAGE.tsv stays with
// Arch Board, and this branch's cites (row:style-upstream, row:lbc) live here
// so a stale notes checkout cannot ORPHAN-ROW. NOTES_DIR / OFFICIAL_NOTES_COMMIT
// are consulted only when the in-repo snapshot is absent (no box path default).
// Decisions: notesDecisionsPath() (OPTGATE_DECISIONS, then the notes snapshot).
// dec: cells go through `semdiff check-registry` (AB-L0030). This file does not
// parse decisions.tsv.
import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { notesDecisionsPath } from './oracle-root.mjs';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '../..');

const HEADER = ['source', 'inputs', 'style', 'template_codegen', 'template_diagnostics', 'js_transform', 'whole_component', 'ssr_render', 'csr_render', 'semantic_diff', 'notes'];
const STAGES = HEADER.slice(2, 10);
const INPUT_KINDS = new Set(['css', 'html', 'js', 'component']);
const STAGE_NEEDS = {
    style: ['css'],
    template_codegen: ['html'],
    template_diagnostics: ['html'],
    js_transform: ['js'],
    whole_component: ['component'],
    ssr_render: ['component'],
    csr_render: ['component'],
    semantic_diff: ['css', 'html', 'js'],
};
const ROWID_RE = /^[A-Za-z0-9_-]+$/;
const DECID_RE = /^[A-Za-z0-9_.:-]+$/;
const D_ID_RE = /^D-[0-9]{4}$/;

function failClosed(msg) {
    console.error(`!! coverage-gate: ${msg}`);
    return 2;
}

function notesToken() {
    const official = process.env.OFFICIAL_NOTES_COMMIT || '';
    if (/^[0-9a-f]{40}$/.test(official)) return official.slice(0, 12);
    const dir = process.env.NOTES_DIR || '';
    if (dir && fs.existsSync(path.join(dir, '.git'))) {
        const r = spawnSync('git', ['-C', dir, 'rev-parse', 'HEAD'], { encoding: 'utf8' });
        const h = (r.stdout || '').trim();
        if (r.status === 0 && /^[0-9a-f]{40}$/.test(h)) return h.slice(0, 12);
    }
    const side = path.resolve(scriptDir, '../decisions.snapshot.notes-commit');
    if (fs.existsSync(side)) {
        const h = fs.readFileSync(side, 'utf8').split(/\r?\n/)[0].trim();
        if (/^[0-9a-f]{40}$/.test(h)) return h.slice(0, 12);
    }
    return 'none';
}

function resolveMatrix() {
    if (process.env.COVGATE_MATRIX === '') return { error: 'COVGATE_MATRIX is set but empty (unset it for the in-repo snapshot, or pass a path)' };
    if (process.env.COVGATE_MATRIX) {
        const p = path.resolve(process.env.COVGATE_MATRIX);
        if (!fs.existsSync(p)) return { error: `COVGATE_MATRIX=${p} is missing` };
        return { text: fs.readFileSync(p, 'utf8'), where: p };
    }
    const snap = path.resolve(scriptDir, '../COVERAGE.tsv');
    if (fs.existsSync(snap)) return { text: fs.readFileSync(snap, 'utf8'), where: snap };
    const dir = process.env.NOTES_DIR || '';
    if (dir) {
        const commit = process.env.OFFICIAL_NOTES_COMMIT || '';
        if (/^[0-9a-f]{40}$/.test(commit)) {
            const r = spawnSync('git', ['-C', dir, 'show', `${commit}:arch-board/COVERAGE.tsv`], { encoding: 'utf8' });
            if (r.status === 0) return { text: r.stdout, where: `notes@${commit.slice(0, 12)}:arch-board/COVERAGE.tsv` };
        }
        const p = path.join(dir, 'arch-board', 'COVERAGE.tsv');
        if (fs.existsSync(p)) return { text: fs.readFileSync(p, 'utf8'), where: p };
    }
    return { error: 'no coverage matrix (harness/COVERAGE.tsv missing, and NOTES_DIR has no arch-board/COVERAGE.tsv)' };
}

function parseInputs(raw) {
    if (!raw || /[\s]/.test(raw)) return { error: 'empty or whitespace' };
    const parts = raw.split(',');
    if (parts.some((p) => !p || !INPUT_KINDS.has(p))) return { error: `kind outside vocabulary: ${raw}` };
    if (new Set(parts).size !== parts.length) return { error: `duplicate kind: ${raw}` };
    const set = new Set(parts);
    if (set.has('component') && !set.has('js')) return { error: 'component without js' };
    return { set };
}

function parseCell(raw) {
    if (raw === 'NA' || raw === 'GAP') return { type: raw };
    if (raw.startsWith('dec:')) {
        const id = raw.slice(4);
        if (!id || !DECID_RE.test(id)) return { type: 'MALFORMED' };
        return { type: 'dec', id };
    }
    if (raw.startsWith('row:')) {
        const ids = [];
        for (const part of raw.split(';')) {
            if (!part.startsWith('row:')) return { type: 'MALFORMED' };
            const id = part.slice(4);
            if (!id || !ROWID_RE.test(id)) return { type: 'MALFORMED' };
            ids.push(id);
        }
        if (!ids.length) return { type: 'MALFORMED' };
        return { type: 'rows', ids };
    }
    return { type: 'MALFORMED' };
}

function applicable(stage, inputs) {
    return STAGE_NEEDS[stage].some((k) => inputs.has(k));
}

function gapOwner(notes, stage) {
    const clause = /GAP owner:\s*([^|]*)/.exec(notes);
    if (!clause) return '?';
    const re = new RegExp(`(?:^|;)\\s*${stage}=([^(;]+)`);
    const m = re.exec(clause[1]);
    return m ? m[1].trim() : '?';
}

function semdiffBin() {
    const env = process.env.SEMDIFF_BIN || '';
    if (env && fs.existsSync(env)) return env;
    const dirs = [];
    if (process.env.CARGO_TARGET_DIR) dirs.push(process.env.CARGO_TARGET_DIR);
    dirs.push(path.join(repoRoot, 'target'));
    for (const dir of dirs) {
        for (const name of ['release/semdiff', 'debug/semdiff']) {
            const p = path.join(dir, name);
            if (fs.existsSync(p)) return p;
        }
    }
    const built = spawnSync('cargo', ['build', '--locked', '-q', '-p', 'semdiff', '--bin', 'semdiff'], { cwd: repoRoot, encoding: 'utf8' });
    if (built.status !== 0) {
        throw new Error(`semdiff build failed (rc=${built.status}): ${(built.stderr || built.stdout || '').slice(0, 600)}`);
    }
    for (const dir of dirs) {
        for (const name of ['release/semdiff', 'debug/semdiff']) {
            const p = path.join(dir, name);
            if (fs.existsSync(p)) return p;
        }
    }
    throw new Error('semdiff build exited 0 but the binary was not found');
}

function registryFindings(bin, file, citedIds) {
    const local = [];
    const req = [];
    for (const id of citedIds) {
        if (!D_ID_RE.test(id)) local.push({ code: 'DEC-UNRESOLVED', id, detail: `${id} does not match ^D-[0-9]{4}$` });
        else req.push(id);
    }
    const args = ['check-registry', '--registry', file];
    for (const id of req) args.push('--require', id);
    const r = spawnSync(bin, args, { encoding: 'utf8' });
    let json;
    try { json = JSON.parse(r.stdout || ''); }
    catch {
        throw new Error(`semdiff check-registry rc=${r.status} produced no JSON: ${(r.stderr || r.stdout || '').slice(0, 400)}`);
    }
    const findings = Array.isArray(json.findings) ? json.findings : [];
    const defective = findings.some((f) => f.code === 'DEC-ID-INVALID' || f.code === 'MALFORMED');
    if (!defective) findings.push(...local);
    return findings;
}

function diagnosticIds(diagText, ratchetPath) {
    if (ratchetPath) {
        if (!fs.existsSync(ratchetPath)) throw new Error(`COVGATE_RATCHET=${ratchetPath} is missing`);
        const set = new Set();
        for (const line of fs.readFileSync(ratchetPath, 'utf8').split(/\n/)) {
            const m = /^([A-Za-z0-9_-]+)\t(diagnostic|nongating|non-gating)\s*$/.exec(line.replace(/\r$/, ''));
            if (m) set.add(m[1]);
        }
        return set;
    }
    return new Set(diagText.split(/\s+/).filter(Boolean));
}

/**
 * Run the gate. Returns { code, lines } and also prints lines to stdout
 * (the board log). `code` is 0, 1, or 2.
 */
export function runCoverageGate(opts) {
    const lines = [];
    const say = (s) => { lines.push(s); console.log(s); };
    const strict = !!opts.strict;
    const rowSet = new Set((opts.rows || '').split(/\s+/).filter(Boolean));
    let diag;
    try { diag = diagnosticIds(opts.diag || '', opts.ratchet || ''); }
    catch (e) { console.error(`!! coverage-gate: ${e.message}`); return { code: 2, lines }; }

    const matrix = opts.matrixText != null ? { text: opts.matrixText, where: opts.matrixWhere || 'memory' } : null;
    const resolved = matrix || resolveMatrix();
    if (resolved.error) { console.error(`!! coverage-gate: ${resolved.error}`); return { code: 2, lines }; }
    const text = resolved.text.endsWith('\n') || resolved.text.length === 0 ? resolved.text : resolved.text + '\n';
    const sha12 = crypto.createHash('sha256').update(text).digest('hex').slice(0, 12);

    let decisions;
    try { decisions = opts.decisionsPath || notesDecisionsPath(); }
    catch (e) { console.error(`!! coverage-gate: ${e.message}`); return { code: 2, lines }; }
    if (!fs.existsSync(decisions)) { console.error(`!! coverage-gate: decisions file missing: ${decisions}`); return { code: 2, lines }; }

    const findings = [];
    const rawLines = text.split(/\n/);
    if (rawLines.length && rawLines[rawLines.length - 1] === '') rawLines.pop();
    if (!rawLines.length) {
        findings.push({ kind: 'BAD-HEADER', line: 'header missing' });
    } else {
        const header = rawLines[0].replace(/\r$/, '').split('\t');
        if (header.join('\t') !== HEADER.join('\t')) findings.push({ kind: 'BAD-HEADER', line: 'header is not the v2 header' });
    }
    const headerOk = findings.length === 0;

    const cited = new Set();
    const decIds = [];
    const decCells = [];
    const counts = { covered: 0, diag: 0, gap: 0, na: 0, dec: 0, invalid: 0 };
    let sources = 0;
    const seen = new Set();
    const cellInvalid = (source, stage) => { counts.invalid++; };

    if (headerOk) {
        rawLines.slice(1).forEach((raw, idx) => {
            const lineNo = idx + 2;
            const fields = raw.replace(/\r$/, '').split('\t');
            if (fields.length !== 11) {
                findings.push({ kind: 'BAD-LINE', line: `line ${lineNo} has ${fields.length} fields` });
                counts.invalid++;
                return;
            }
            const [source, inputsRaw, ...rest] = fields;
            const notes = rest[8];
            const stageCells = rest.slice(0, 8);
            if (!source) findings.push({ kind: 'DUP-SOURCE', line: `line ${lineNo} source is empty` });
            else if (seen.has(source)) findings.push({ kind: 'DUP-SOURCE', line: `${source} repeated` });
            else seen.add(source);
            if (!notes) findings.push({ kind: 'NO-EVIDENCE', line: `${source || `line ${lineNo}`} notes empty` });
            const inputs = parseInputs(inputsRaw);
            if (inputs.error) {
                findings.push({ kind: 'BAD-INPUTS', line: `${source || `line ${lineNo}`} ${inputs.error}` });
                counts.invalid++;
                return;
            }
            sources++;
            stageCells.forEach((cell, i) => {
                const stage = STAGES[i];
                const parsed = parseCell(cell);
                const applies = applicable(stage, inputs.set);
                if (parsed.type === 'MALFORMED') {
                    findings.push({ kind: 'MALFORMED', line: `${source} x ${stage} ${cell || '<blank>'}` });
                    cellInvalid();
                    return;
                }
                if (parsed.type === 'NA') {
                    if (applies) {
                        findings.push({ kind: 'NA-ILLEGAL', line: `${source} x ${stage}` });
                        cellInvalid();
                    } else counts.na++;
                    return;
                }
                if (parsed.type === 'GAP') {
                    if (!applies) {
                        findings.push({ kind: 'GAP-MUST-BE-NA', line: `${source} x ${stage}` });
                        cellInvalid();
                    } else {
                        findings.push({ kind: 'GAP', line: `${source} x ${stage} owner=${gapOwner(notes, stage)}`, report: true });
                        counts.gap++;
                    }
                    return;
                }
                if (!applies) {
                    findings.push({ kind: 'CITE-MUST-BE-NA', line: `${source} x ${stage}` });
                    cellInvalid();
                    return;
                }
                if (parsed.type === 'dec') {
                    decIds.push(parsed.id);
                    decCells.push({ source, stage, id: parsed.id });
                    return;
                }
                let gating = false;
                let unknown = false;
                for (const id of parsed.ids) {
                    cited.add(id);
                    if (!rowSet.has(id)) {
                        findings.push({ kind: 'UNKNOWN-ROW', line: `${source} x ${stage} -> row:${id}` });
                        unknown = true;
                    } else if (!diag.has(id)) gating = true;
                }
                if (unknown) { cellInvalid(); return; }
                if (gating) counts.covered++;
                else {
                    findings.push({ kind: 'DIAG-ONLY', line: `${source} x ${stage} rows=${parsed.ids.join(',')}`, report: true });
                    counts.diag++;
                }
            });
        });
        for (const id of rowSet) {
            if (!cited.has(id)) findings.push({ kind: 'ORPHAN-ROW', line: `row:${id}` });
        }
    }

    if (headerOk) {
        let reg;
        try { reg = registryFindings(semdiffBin(), decisions, [...new Set(decIds)]); }
        catch (e) { console.error(`!! coverage-gate: ${e.message}`); return { code: 2, lines }; }
        const byId = new Map();
        for (const c of decCells) {
            if (!byId.has(c.id)) byId.set(c.id, []);
            byId.get(c.id).push(c);
        }
        const unresolved = new Set();
        let registryDefective = false;
        for (const f of reg) {
            const code = f.code || 'MALFORMED';
            if (code === 'DEC-ID-INVALID' || code === 'MALFORMED') registryDefective = true;
            if (code === 'DEC-UNRESOLVED' || code === 'DEC-NOT-LOCKED') {
                const cells = byId.get(f.id) || [];
                if (!cells.length) findings.push({ kind: code, line: `${f.id || '?'} ${f.detail || ''}`.trim() });
                for (const c of cells) {
                    findings.push({ kind: code, line: `${c.source} x ${c.stage} dec:${c.id}${f.detail ? ` ${f.detail}` : ''}` });
                    unresolved.add(`${c.source}\t${c.stage}`);
                }
            } else {
                findings.push({ kind: code, line: `${f.detail || f.id || code}` });
            }
        }
        for (const c of decCells) {
            if (registryDefective || unresolved.has(`${c.source}\t${c.stage}`)) counts.invalid++;
            else counts.dec++;
        }
    }

    const hard = findings.filter((f) => !f.report);
    const report = findings.filter((f) => f.report);
    const mode = strict ? 'strict' : 'report-only';
    const label = opts.ratchet ? 'ratchet gating' : 'declared gating';
    const cells = sources * STAGES.length;
    say(`coverage-gate: notes ${notesToken()} matrix ${sha12} ${mode}: ${sources} sources x ${STAGES.length} stages = ${cells} cells, ${counts.covered} covered (${label}), ${counts.diag} diagnostic-only, ${counts.gap} gap, ${counts.na} na, ${counts.dec} dec, ${counts.invalid} invalid`);
    for (const f of [...hard, ...report]) say(`  ${f.kind} ${f.line}`.trimEnd());
    const fail = hard.length > 0 || (strict && report.length > 0);
    return { code: fail ? 1 : 0, lines, counts };
}

function selfTest() {
    const root = path.join(scriptDir, 'coverage-gate.fixtures');
    const sharedDec = path.join(root, '_ok', 'decisions.tsv');
    if (!fs.existsSync(sharedDec)) {
        console.error(`!! coverage-gate: missing ${sharedDec}`);
        return 1;
    }
    const cases = fs.readdirSync(root, { withFileTypes: true }).filter((d) => d.isDirectory() && !d.name.startsWith('_')).map((d) => d.name).sort();
    let failed = 0;
    const check = (name, result, expectExit, kinds) => {
        const got = result.lines.filter((l) => l.startsWith('  ')).map((l) => l.trim().split(/\s+/)[0]);
        const set = [...new Set(got)].sort();
        const want = [...new Set(kinds)].sort();
        const same = result.code === expectExit && set.join(',') === want.join(',');
        if (!same) {
            failed++;
            console.error(`FAIL ${name}: exit ${result.code} (want ${expectExit}) kinds ${set.join(',') || '-'} (want ${want.join(',') || '-'})`);
            for (const l of result.lines) console.error(`    ${l}`);
        } else console.log(`ok   ${name}`);
    };
    for (const name of cases) {
        const dir = path.join(root, name);
        const expectPath = path.join(dir, 'expect');
        if (!fs.existsSync(expectPath)) { console.error(`FAIL ${name}: no expect`); failed++; continue; }
        const expectLines = fs.readFileSync(expectPath, 'utf8').split(/\n/).map((l) => l.trim()).filter(Boolean);
        const matrixPath = path.join(dir, 'COVERAGE.tsv');
        const decPath = fs.existsSync(path.join(dir, 'decisions.tsv')) ? path.join(dir, 'decisions.tsv') : sharedDec;
        const rows = fs.existsSync(path.join(dir, 'rows')) ? fs.readFileSync(path.join(dir, 'rows'), 'utf8').trim() : '';
        const diag = fs.existsSync(path.join(dir, 'diag')) ? fs.readFileSync(path.join(dir, 'diag'), 'utf8').trim() : '';
        const argv = fs.existsSync(path.join(dir, 'argv')) ? fs.readFileSync(path.join(dir, 'argv'), 'utf8') : '';
        const strict = argv.split(/\s+/).includes('--strict');
        if (expectLines[0] === 'mode' && expectLines[1] === 'status-derivation') {
            const text = fs.readFileSync(matrixPath, 'utf8');
            const a = runCoverageGate({ rows: 'a', diag: '', matrixText: text, decisionsPath: decPath });
            const b = runCoverageGate({ rows: 'a', diag: 'a', matrixText: text, decisionsPath: decPath });
            const aCovered = a.lines[0]?.includes('8 covered') && !a.lines.some((l) => l.startsWith('  DIAG-ONLY'));
            const bDiag = b.lines[0]?.includes('0 covered') && b.lines[0]?.includes('8 diagnostic-only') && b.lines.filter((l) => l.startsWith('  DIAG-ONLY')).length === 8;
            if (a.code === 0 && b.code === 0 && text === fs.readFileSync(matrixPath, 'utf8') && aCovered && bDiag) console.log(`ok   ${name}`);
            else { failed++; console.error(`FAIL ${name}: status derivation`); console.error(a.lines[0]); console.error(b.lines[0]); }
            continue;
        }
        const exitLine = expectLines[0];
        const m = /^exit (\d+)$/.exec(exitLine);
        if (!m) { console.error(`FAIL ${name}: bad expect`); failed++; continue; }
        const result = runCoverageGate({ strict, rows, diag, matrixText: fs.readFileSync(matrixPath, 'utf8'), decisionsPath: decPath });
        check(name, result, Number(m[1]), expectLines.slice(1));
    }
    if (!cases.length) { console.error('FAIL self-test: no fixtures'); return 1; }
    console.log(failed ? `coverage-gate self-test: ${failed} failed` : `coverage-gate self-test: ok (${cases.length})`);
    return failed ? 1 : 0;
}

const argv = process.argv.slice(2);
if (process.env.OPTGATE_DECISIONS === '') {
    failClosed('OPTGATE_DECISIONS is set but empty (unset it for the live default, or pass a path)');
    process.exit(2);
}
if (argv.includes('--self-test')) {
    process.exit(selfTest());
}
let strict = false;
const pos = [];
for (const a of argv) {
    if (a === '--strict') strict = true;
    else if (a === '--help' || a === '-h') {
        console.log('usage: node scripts/coverage-gate.mjs [--strict] "<space-separated board rows>" | --self-test');
        process.exit(0);
    } else if (a.startsWith('-')) {
        failClosed(`unknown flag ${a}`);
        process.exit(2);
    } else pos.push(a);
}
if (pos.length > 1) {
    failClosed('usage: node scripts/coverage-gate.mjs [--strict] "<space-separated board rows>"');
    process.exit(2);
}
const ratchet = process.env.COVGATE_RATCHET || '';
const result = runCoverageGate({
    strict,
    rows: pos[0] || '',
    diag: process.env.COVGATE_DIAG || '',
    ratchet,
});
process.exit(result.code);
