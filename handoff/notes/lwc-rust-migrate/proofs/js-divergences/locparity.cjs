// node locparity.cjs <napi> <cases.json>...  -> per case: outcome/code/class+filename/location parity vs live 9.5.3
const { createRequire } = require('module'); const fs = require('fs');
const oracle = createRequire('/workspace/lwc/packages/@lwc/compiler/')('@lwc/compiler');
const nat = require(process.argv[2]); const cwd = process.cwd() + '/';
const strip = (s) => s.split(cwd).join('');
const rows = [];
for (const f of process.argv.slice(3)) for (const c of JSON.parse(fs.readFileSync(f, 'utf8'))) {
  const opts = { namespace: 'x', name: 'foo', ...(c.opts || {}), ...(c.recover ? { experimentalErrorRecoveryMode: true } : {}) };
  let js = null; try { oracle.transformSync(c.src, c.filename, opts); } catch (e) { js = e; }
  const r = nat.transform(c.src, c.filename, opts); const rs = r.error || null;
  const row = { file: f.split('/').pop(), id: c.name || c.id, filename: c.filename, src: c.src };
  row.outcome = (js === null) === (rs === null);
  if (js && rs) {
    row.code = js.code === rs.code; row.jsCode = js.code; row.rsCode = rs.code;
    const pre = `SyntaxError: LWC${js.code}: ${c.filename}: `;
    row.klass = strip(js.message).startsWith(pre) === strip(rs.message).startsWith(pre) && strip(js.message).startsWith(pre);
    const L = (l) => l ? [l.line, l.column, l.index ?? null] : null;
    row.jsLoc = L(js.location); row.rsLoc = L(rs.location);
    row.loc = JSON.stringify(row.jsLoc) === JSON.stringify(row.rsLoc);
    row.msg = strip(js.message) === strip(rs.message);
    row.jsMsg = strip(js.message).split('\n')[0]; row.rsMsg = strip(rs.message).split('\n')[0];
  }
  rows.push(row);
}
if (process.env.OUT) fs.writeFileSync(process.env.OUT, JSON.stringify(rows, null, 1));
const errs = rows.filter(r => r.jsLoc !== undefined && r.code);
console.log(`cases ${rows.length}; outcome match ${rows.filter(r => r.outcome).length}; both-throw ${rows.filter(r => r.jsCode).length}; code match ${rows.filter(r => r.code).length}; 1007 class+filename ${rows.filter(r => r.jsCode === 1007 && r.klass).length}/${rows.filter(r => r.jsCode === 1007).length}; location match ${errs.filter(r => r.loc).length}/${errs.length}; message match ${errs.filter(r => r.msg).length}/${errs.length}`);
for (const r of rows) if (!r.outcome || (r.jsCode && (!r.code || !r.loc))) console.log(' ', r.file, r.id, r.outcome ? '' : 'OUTCOME', r.code === false ? `code ${r.jsCode}/${r.rsCode}` : '', r.loc === false ? `loc ${JSON.stringify(r.jsLoc)} vs ${JSON.stringify(r.rsLoc)}` : '');
