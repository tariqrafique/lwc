import fs from 'fs';
const s = JSON.parse(fs.readFileSync(new URL('./shipped.json', import.meta.url)));
const r = JSON.parse(fs.readFileSync(new URL('./rust.json', import.meta.url)));
let bad = 0;
for (const k of Object.keys(s)) {
  const eqLoc = JSON.stringify(s[k].location) === JSON.stringify(r[k].location) && s[k].code === r[k].code;
  const eqMsg = s[k].message === r[k].message;
  if (!eqLoc || !eqMsg) bad++;
  console.log(`${k}\tcode ${s[k].code}/${r[k].code}\tloc ${eqLoc ? 'SAME' : 'DIFF ' + JSON.stringify(s[k].location) + ' vs ' + JSON.stringify(r[k].location)}\tmessage+frame ${eqMsg ? 'SAME' : 'DIFF'}`);
}
process.exit(bad ? 1 : 0);
