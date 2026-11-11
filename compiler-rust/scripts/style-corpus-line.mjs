// Turn tools/style-diff report.json into the style-corpus footer payload.
// Exit 3 when the run was capped, a source count is short, or any job failed / skipped / was allowlisted.
// argv: report.json expect.json (object of source -> file count, key order is the payload order)
import fs from 'node:fs';

const reportPath = process.argv[2];
const expectPath = process.argv[3];
if (!reportPath || !expectPath) {
    console.error('usage: style-corpus-line.mjs <report.json> <expect.json>');
    process.exit(3);
}
const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
const expect = JSON.parse(fs.readFileSync(expectPath, 'utf8'));
const s = report.summary;
if (!s || s.uncapped !== true) {
    console.error('style-corpus: style-diff was capped or wrote no summary');
    process.exit(3);
}
const fail = s.fail || 0;
const skip = s.skip || 0;
const allowed = s.allowed || 0;
if (fail || skip || allowed) {
    console.error(`style-corpus: fail=${fail} skip=${skip} allowed=${allowed}`);
    process.exit(3);
}
const parts = [];
let dedup = 0;
for (const [k, want] of Object.entries(expect)) {
    const b = s.bySource && s.bySource[k];
    if (!b || b.files !== want || b.unique + b.deduped !== b.files) {
        console.error(`style-corpus: ${k} bySource=${JSON.stringify(b)} want files=${want}`);
        process.exit(3);
    }
    dedup += b.deduped;
    parts.push(`${k}=${b.files}/${want}`);
}
process.stdout.write(`${parts.join(' ')} dedup=${dedup} jobs=${s.jobs} fail=0 (uncapped style-diff, hash-dedup, direct oracle)\n`);
