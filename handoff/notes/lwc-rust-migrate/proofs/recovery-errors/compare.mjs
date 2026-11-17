// errors[] equality after dropping shipped's duplicate LWC1096 entries for a property that is also an
// @api get/set pair (the 1112 case); the 1212 crash case is reported, not compared.
import fs from 'fs';
const s = JSON.parse(fs.readFileSync(new URL('./shipped.json', import.meta.url)));
const r = JSON.parse(fs.readFileSync(new URL('./rust.json', import.meta.url)));
const rows = []; let bad = 0;
for (const k of Object.keys(s)) {
  if (k.includes('1212')) { rows.push({ k, verdict: 'EXCLUDED (shipped crash)', s: s[k], r: r[k] }); continue; }
  const pairProps = new Set(s[k].errors.filter((e) => e.code === 1112).map((e) => /@api get (\S+)/.exec(e.message)[1]));
  const sf = s[k].errors.filter((e) => !(e.code === 1096 && pairProps.has(/property "(.*)"/.exec(e.message)[1])));
  const same = JSON.stringify(sf) === JSON.stringify(r[k].errors);
  if (!same) bad++;
  rows.push({ k, verdict: same ? (sf.length !== s[k].errors.length ? `IDENTICAL (after dropping ${s[k].errors.length - sf.length} shipped dup LWC1096)` : 'IDENTICAL') : 'MISMATCH', s: s[k], r: r[k], sf });
}
fs.writeFileSync(new URL('./compare.json', import.meta.url), JSON.stringify(rows, null, 1));
for (const x of rows) console.log(x.k, '\t', x.verdict);
process.exit(bad ? 1 : 0);
