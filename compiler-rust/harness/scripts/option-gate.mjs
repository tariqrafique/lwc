// Option-inventory gate: every shipped JS option field maps to a live board row, a rust unit test, or a logged N/A.
// usage: node scripts/option-gate.mjs "<space-separated board rows>"   exit 1 on any unmapped/stale/pending/invalid entry.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
const here = new URL('..', import.meta.url).pathname;
const fields = JSON.parse(execFileSync('node', [`${here}scripts/option-fields.mjs`], { encoding: 'utf8' }));
const inv = JSON.parse(fs.readFileSync(`${here}option-inventory.json`, 'utf8')); delete inv._doc;
const rows = new Set((process.argv[2] ?? '').split(/\s+/).filter(Boolean));
// A field mapped only to a diagnostic (non-gating) row is not covered: no false-pass path.
const diag = new Set((process.env.OPTGATE_DIAG ?? '').split(/\s+/).filter(Boolean));
const decisions = fs.readFileSync('/workspace/notes/lwc-rust-migrate/decisions.tsv', 'utf8');
const crates = `${here}../crates`;
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
console.log(`option-gate: ${fields.length} fields, ${ok} mapped, ${pending.length} pending, ${bad.length} invalid`);
for (const l of [...bad, ...pending]) console.log('  ' + l);
process.exit(bad.length || pending.length ? 1 : 0);
