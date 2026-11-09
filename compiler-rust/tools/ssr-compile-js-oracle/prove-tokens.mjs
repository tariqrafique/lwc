import { createRequire } from 'node:module';
import { makeCompare } from './compare.mjs';

const LWC = process.env.LWC_ROOT || '/workspace/lwc';
const acorn = createRequire(import.meta.url)(`${LWC}/node_modules/acorn`);
const { tokens } = makeCompare(acorn);

const pairs = [
  ['template vs string', '`x`', '"x"'],
  ['ident vs quoted key', '({foo:1})', '({"foo":1})'],
  ['ident vs computed key', '({foo:1})', '({["foo"]:1})'],
  ['quoted vs computed key', '({"foo":1})', '({["foo"]:1})'],
  ['shorthand vs longhand', '({foo})', '({foo: foo})'],
];
let differ = 0, same = 0;
for (const [name, a, b] of pairs) {
  const ta = tokens(a), tb = tokens(b);
  const eq = ta === tb;
  if (eq) same++; else differ++;
  console.log(`${eq ? 'SAME' : 'DIFFER'} ${name}`);
}
console.log(`${differ} pairs differ, ${same} pairs match`);
if (process.argv.includes('--expect-collapsed')) {
  if (same !== pairs.length) process.exit(1);
} else if (differ !== pairs.length) process.exit(1);
