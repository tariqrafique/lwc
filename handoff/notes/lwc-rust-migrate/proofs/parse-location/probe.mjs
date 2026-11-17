import { createRequire } from 'module';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const srcs = ["let = ;", "let x = ;", "a b", "class {", "function (", "}", "const x = 1;\nconst y = 2 +;\n", "var 😀 = 1;", "const s = \"é\"; let = ;", "const s = \"😀\"; let x = ;"];
const out = [];
for (const rec of [false, true]) for (const src of srcs) {
  try { transformSync(src, 'foo.js', { name: 'foo', namespace: 'x', experimentalErrorRecoveryMode: rec }); out.push({ src, rec, ok: true }); }
  catch (e) { out.push({ src, rec, code: e.code, location: e.location, locProto: e.location && Object.getPrototypeOf(e.location)?.constructor?.name, locKeys: e.location && Object.keys(e.location), first: e.message.split('\n')[0].replace(process.cwd() + '/', '<cwd>/') }); }
}
console.log(JSON.stringify(out, null, 0).replaceAll('},{', '},\n{'));
import fs from 'fs'; fs.writeFileSync(new URL('./shipped.json', import.meta.url), JSON.stringify(out, null, 1));
