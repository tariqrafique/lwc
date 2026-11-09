// Option-inventory gate: every shipped JS option field maps to a live board row, a rust unit test, or a logged N/A.
// usage: node scripts/option-gate.mjs "<space-separated board rows>"   exit 1 on any unmapped/stale/pending/invalid entry.
// Normally invoked via option_gate (harness/board-rows.sh). Env: CRATES_ROOT, OPTGATE_DIAG, OPTGATE_INVENTORY (inventory file),
// OPTGATE_DECISIONS (decisions.tsv path; default live /workspace/notes/lwc-rust-migrate/decisions.tsv).
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
// OPTGATE_DECISIONS set but empty is a caller bug (e.g. an unset snapshot variable), never a request for the live
// default: exit 2 before reading anything (AB-0006 rework 2, F6).
if (process.env.OPTGATE_DECISIONS === '') { console.error('!! option-gate: OPTGATE_DECISIONS is set but empty (unset it for the live default, or pass a path)'); process.exit(2); }
const here = new URL('..', import.meta.url).pathname;
const fields = JSON.parse(execFileSync('node', [`${here}scripts/option-fields.mjs`], { encoding: 'utf8' }));
const inv = JSON.parse(fs.readFileSync(process.env.OPTGATE_INVENTORY ?? `${here}option-inventory.json`, 'utf8')); delete inv._doc;
const rows = new Set((process.argv[2] ?? '').split(/\s+/).filter(Boolean));
// A field mapped only to a diagnostic (non-gating) row is not covered: no false-pass path.
const diag = new Set((process.env.OPTGATE_DIAG ?? '').split(/\s+/).filter(Boolean));
// decisions.tsv: OPTGATE_DECISIONS (official-board.sh passes the ARCHIVED snapshot notes, $snap/notes/decisions.tsv); the
// default is the live notes, for dev boards and land.sh. A missing/unreadable file throws (exit != 0): fail closed.
const decisionsPath = process.env.OPTGATE_DECISIONS ?? '/workspace/notes/lwc-rust-migrate/decisions.tsv';
const decisions = fs.readFileSync(decisionsPath, 'utf8');
const crates = process.env.CRATES_ROOT ?? `${here}../crates`;  // official-board.sh points this at the frozen snapshot
const bad = [], pending = [];
for (const f of fields) {
    const m = inv[f];
    if (!m) { bad.push(`UNMAPPED ${f}`); continue; }
    if (m.row) { if (!rows.has(m.row)) bad.push(`ROW-MISSING ${f} -> ${m.row}`); else if (diag.has(m.row)) bad.push(`ROW-DIAGNOSTIC ${f} -> ${m.row} (non-gating row)`); }
    else if (m.unit) {
        try { execFileSync('rg', ['-q', `fn ${m.unit}\\b`, `${crates}/${m.crate}`]); } catch { bad.push(`UNIT-MISSING ${f} -> ${m.crate}::${m.unit}`); }
    } else if (m.na) { if (!decisions.includes(`optinv:${f}`)) bad.push(`NA-UNLOGGED ${f} (decisions.tsv needs 'optinv:${f}')`); }
    else if (m.pending) pending.push(`PENDING ${f}: ${m.pending}`);
    else bad.push(`INVALID ${f}`);
}
for (const k of Object.keys(inv)) if (!fields.includes(k)) bad.push(`STALE ${k} (no longer a JS option field)`);
const ok = fields.length - bad.length - pending.length;
console.log(`option-gate: ${fields.length} fields, ${ok} mapped, ${pending.length} pending, ${bad.length} invalid  [decisions ${decisionsPath}]`);
for (const l of [...bad, ...pending]) console.log('  ' + l);
process.exit(bad.length || pending.length ? 1 : 0);
