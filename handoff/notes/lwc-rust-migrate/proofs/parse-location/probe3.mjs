import { createRequire } from 'module';
const { transformSync } = createRequire('/workspace/oracle953/')('@lwc/compiler');
const srcs = ["var static = 1;", "function f(yield){}", "({let});", "x = implements;", "class A { static = 1; let = 2; }", "obj.public = 1; ({private: 1});", "function package(){}", "try{}catch(interface){}", "(protected) => 1;", "let x = 1;\nx = let;", "const a = `${let}`;", "import { let as l } from 'm';", "label: x;", "let /* let */ x = 'let';"];
const out = [];
for (const src of srcs) {
  try { transformSync(src, 'foo.js', { name: 'foo', namespace: 'x' }); out.push({ src, ok: true }); }
  catch (e) { out.push({ src, code: e.code, location: e.location, first: e.message.split('\n')[0].replace(process.cwd() + '/', '<cwd>/') }); }
}
console.log(JSON.stringify(out, null, 0).replaceAll('},{', '},\n{'));
import fs from 'fs'; fs.writeFileSync(new URL('./shipped3.json', import.meta.url), JSON.stringify(out, null, 1));
