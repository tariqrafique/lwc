import { createRequire } from 'module';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const srcs = ["let = 1;", "let  = ;", "let /*c*/ = ;", "let;", "let\n= 3;", "x; let = ;", "a.let = ;", "let(1);", "let.x = 1;", "if (let) {}", "let\n  x = ;", "yield = ;", "static = ;", "let + ;"];
const out = [];
for (const rec of [false, true]) for (const src of srcs) {
  try { transformSync(src, 'foo.js', { name: 'foo', namespace: 'x', experimentalErrorRecoveryMode: rec }); out.push({ src, rec, ok: true }); }
  catch (e) { out.push({ src, rec, code: e.code, location: e.location, first: e.message.split('\n')[0].replace(process.cwd() + '/', '<cwd>/') }); }
}
console.log(JSON.stringify(out, null, 0).replaceAll('},{', '},\n{'));
import fs from 'fs'; fs.writeFileSync(new URL('./shipped2.json', import.meta.url), JSON.stringify(out, null, 1));
