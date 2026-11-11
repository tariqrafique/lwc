import { createRequire } from 'node:module';
import { makeCompare } from './compare.mjs';
import { oracleRoot } from '../../harness/scripts/oracle-root.mjs';

const LWC = oracleRoot();
const acorn = createRequire(import.meta.url)(`${LWC}/node_modules/acorn`);
const { extractors } = makeCompare(acorn);
const mode = process.argv[2] || 'nested';

const tryExtract = (code, c = {}) => {
  try {
    const v = extractors.props(code, c);
    console.log(`MATCHED ${v}`);
    return 0;
  } catch (e) {
    if (e.extractMissing) { console.log(`EXTRACT ${e.message}`); return 2; }
    throw e;
  }
};

if (mode === 'nested') {
  const left = '__setStaticInternals(C, "x-foo", ["a", ["x"], "LEFT"], null);';
  const right = '__setStaticInternals(C, "x-foo", ["a", ["x"], "RIGHT"], null);';
  const a = extractors.props(left, { expectAbsent: ['reg'] });
  const b = extractors.props(right, { expectAbsent: ['reg'] });
  const cmp = a === b ? 'SAME' : 'DIFFER';
  console.log(`${cmp} ${a} vs ${b}`);
  const bare = 'export default {};';
  try {
    const v = extractors.props(bare, {});
    console.log(`MATCHED ${v}`);
  } catch (e) {
    if (e.extractMissing) console.log(`MISSING ${e.message}`);
    else throw e;
  }
} else if (mode === 'dup') {
  const code = '__setStaticInternals(C, "x-foo", ["a"], null);\n__setStaticInternals(C, "x-foo", ["b"], null);';
  process.exit(tryExtract(code, { expectAbsent: ['reg'] }));
} else if (mode === 'present-one') {
  const withReg = '__setStaticInternals(C, "x-foo", [], null);\n__registerPublicProperties(this, ["x"]);';
  const noReg = '__setStaticInternals(C, "x-foo", [], null);';
  const c = { expectAbsent: ['reg'] };
  const side = (code) => {
    try { return { kind: 'ok', value: extractors.props(code, c) }; }
    catch (e) { if (e.extractMissing) return { kind: 'missing', value: e.message }; throw e; }
  };
  const up = side(withReg), us = side(noReg);
  if (up.kind === 'missing' || us.kind === 'missing') {
    console.log(`EXTRACT stale or false up=${up.value} ours=${us.value}`);
    process.exit(2);
  }
  console.log('ok');
  process.exit(0);
} else {
  throw new Error(`unknown mode ${mode}`);
}
