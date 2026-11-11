import fs from 'fs';
// shipped resolves the relative filename against cwd; normalize it so messages compare.
const s = JSON.parse(fs.readFileSync(new URL('./shipped.json', import.meta.url), 'utf8').split(process.cwd() + '/test.js').join('test.js'));
const r = JSON.parse(fs.readFileSync(new URL('./rust.json', import.meta.url)));
let bad = 0;
for (const k of Object.keys(s)) { const same = JSON.stringify(s[k]) === JSON.stringify(r[k]); if (!same) bad++;
  console.log(same ? 'SAME' : 'DIFF', k); if (!same && process.argv[2]) console.log('  S', JSON.stringify(s[k]), '\n  R', JSON.stringify(r[k])); }
process.exit(bad ? 1 : 0);
