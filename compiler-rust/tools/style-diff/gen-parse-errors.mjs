#!/usr/bin/env node
// Generates crates/lwc-style/tests/parse-errors/<name>/ from the JS oracle (/workspace/lwc @ b9e8007):
//   actual.css            input
//   config.json           style-compiler Config (only when non-default)
//   error.json            style-compiler `transform` throw, normalized like the upstream fixture spec
//                         ({name, reason, column, line} or an array for AggregateError)
//   expected.js           when the oracle accepts the input (version replaced with X.X.X)
//   compiler-error.json   `@lwc/compiler` transformSync(src, 'x.css') throw: message (cwd -> <cwd>), code,
//                         filename, location, level, url; aggregates: message + errors[]
// Never hand-edit the outputs; rerun: node tools/style-diff/gen-parse-errors.mjs
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const LWC = process.env.LWC_ROOT || '/workspace/lwc';
const { transform } = require(`${LWC}/packages/@lwc/style-compiler/dist/index.cjs`);
const { transformSync } = require(`${LWC}/packages/@lwc/compiler/dist/index.cjs`);
const LWC_VERSION = require(`${LWC}/packages/@lwc/style-compiler/package.json`).version;
const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../crates/lwc-style/tests/parse-errors');

const ERM = { experimentalErrorRecoveryMode: true };
// [name, css, config?]
export const CASES = [
  // parser.js unclosedBlock (innermost current.source.start)
  ['unclosed-block-host', ':host{'],
  ['unclosed-block-rule', 'a{color:red'],
  ['unclosed-block-semicolon', 'a{b:c;'],
  ['unclosed-block-nested', 'a{b{c{'],
  ['unclosed-block-media', '@media screen{a{}'],
  ['unclosed-block-media-inner', '@media screen{\n  a{color:red;\n'],
  ['unclosed-block-empty-rule', '{'],
  ['unclosed-block-custom-prop', 'a{--x:{a:b}'],
  ['unclosed-block-multiline', 'a {}\n\nb {\n  color: red;\n'],
  ['unclosed-block-after-astral', '/* 😀😀 */ a{}\n😀{'],
  ['unclosed-block-crlf', 'a{}\r\nb{\r\n'],
  ['unclosed-block-bom', '\uFEFFa{'],
  ['unclosed-block-keyframes', '@keyframes x{from{opacity:0}'],
  ['unclosed-block-erm', ':host{', ERM],
  ['unclosed-block-scoped', ':host{', { scoped: true }],
  ['unclosed-block-dss', ':host{', { disableSyntheticShadowSupport: true }],
  // parser.js unexpectedClose
  ['unexpected-close', '}'],
  ['unexpected-close-after-rule', 'a{b:c}}'],
  ['unexpected-close-at-rule', '@media x}'],
  ['unexpected-close-erm', 'a{}\n}', ERM],
  // tokenize.js unclosed('bracket') + parser.js unclosedBracket
  ['unclosed-bracket-url', 'a{b:url(x}'],
  ['unclosed-bracket-paren', 'a{b:(c}'],
  ['unclosed-bracket-square', 'a{[b}'],
  ['unclosed-bracket-selector', 'a(b{'],
  ['unclosed-bracket-outer-first', 'a{b:(c[d}'],
  ['unclosed-bracket-erm', 'a{b:(c}', ERM],
  // tokenize.js unclosed('string')
  ['unclosed-string-double', 'a{b:"x}'],
  ['unclosed-string-single', "a{content:'x}"],
  ['unclosed-string-escaped-quote', 'a{b:"x\\"}'],
  ['unclosed-string-utf16', 'é{b:"'],
  ['unclosed-string-astral', '😀{b:"'],
  ['unclosed-string-erm', 'a{b:"x}', ERM],
  // tokenize.js unclosed('comment')
  ['unclosed-comment', '/* x'],
  ['unclosed-comment-in-rule', 'a{color:red;/* x}'],
  ['unclosed-comment-erm', '/* x', ERM],
  // parser.js unknownWord
  ['unknown-word-selector-semicolon', 'a b;'],
  ['unknown-word-no-colon', 'a{color red}'],
  ['unknown-word-eof', 'a'],
  ['unknown-word-between', 'a{b c:d}'],
  ['unknown-word-no-position', 'a{: ;}'],
  ['unknown-word-erm', 'a{color red}', ERM],
  ['unknown-word-no-position-erm', 'a{: ;}', ERM],
  // parser.js unnamedAtrule
  ['at-rule-without-name', '@{}'],
  ['at-rule-without-name-semicolon', 'a{}\n@;'],
  // parser.js doubleColon / checkMissedSemicolon
  ['double-colon', 'a{b: :c}'],
  ['missed-semicolon', 'a{b:c d:e}'],
  ['missed-semicolon-important', 'a{b:c!important d:e}'],
  ['missed-semicolon-multiline', 'a{\n  color: red\n  background: blue\n}'],
  ['missed-semicolon-erm', 'a{b:c d:e}', ERM],
  // accepted by PostCSS: must compile (expected.js from the oracle)
  ['ok-media-unclosed-paren', '@media (x {}'],
  ['ok-progid', 'a{b: progid:x}'],
  ['ok-star-prefix', 'a{_b:c}'],
  ['ok-custom-prop-colon', 'a{--x: a:b}'],
  ['ok-url-paren', 'a{b:url(x)}'],
  ['ok-at-rule-eof', '@import "x"'],
  ['ok-decl-eof', 'a{}b:c'],
  // serialize.ts `throw new Error('Unexpected host rules ordering')`: a rule ownSemicolon (`};`) after a
  // :host rule ends the native host variant early. Plain Error (no position), not CssSyntaxError; not
  // wrapped in AggregateError in recovery mode (thrown after the PostCSS pipeline).
  ['host-ordering', ':host{a:b};x{}'],
  ['host-ordering-empty', ':host{};'],
  ['host-ordering-trailing', ':host{a:b};'],
  ['host-ordering-space', ':host{a:b} ;x{}'],
  ['host-ordering-media', '@media x{:host{a:b};}'],
  ['host-ordering-host-fn', ':host(.a){a:b};x{}'],
  ['host-ordering-erm', ':host{a:b};x{}', ERM],
  ['host-ordering-scoped', ':host{a:b};x{}', { scoped: true }],
  ['host-ordering-dss', ':host{a:b};x{}', { disableSyntheticShadowSupport: true }],
  ['ok-own-semicolon-before-host', 'a{};:host{}'],
  // parse error wins over plugin errors in recovery mode
  ['parse-before-plugin-erm', '#foo{} :host-context(.a){} a{', ERM],
];

function normalizeError(err) {
  if (err.name === 'AggregateError') return err.errors.map(normalizeError);
  if (err.name === 'CssSyntaxError') return { name: err.name, reason: err.reason, column: err.column, line: err.line };
  return { name: err.name, message: err.message };
}

const cwd = process.cwd() + '/';
const norm = (s) => (typeof s === 'string' ? s.split(cwd).join('<cwd>/') : s);
function compilerError(e) {
  const one = (x) => ({ name: x.name, message: norm(x.message), code: x.code, filename: x.filename, location: x.location, level: x.level, url: x.url });
  if (Array.isArray(e.errors)) return { name: e.name, message: e.message, ownKeys: Object.keys(e), errors: e.errors.map(one) };
  return { ...one(e), ownKeys: Object.keys(e) };
}

fs.rmSync(OUT, { recursive: true, force: true });
for (const [name, css, config = {}] of CASES) {
  const dir = path.join(OUT, name);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'actual.css'), css);
  if (Object.keys(config).length) fs.writeFileSync(path.join(dir, 'config.json'), JSON.stringify(config, null, 4) + '\n');
  try {
    const { code } = transform(css, 'actual.css', config);
    fs.writeFileSync(path.join(dir, 'expected.js'), code.replace(new RegExp(LWC_VERSION.replace(/\./g, '\\.'), 'g'), 'X.X.X'));
  } catch (err) {
    fs.writeFileSync(path.join(dir, 'error.json'), JSON.stringify(normalizeError(err), null, 4));
  }
  try {
    transformSync(css, 'x.css', { name: 'x', namespace: 'x', ...config, scopedStyles: config.scoped });
  } catch (e) {
    fs.writeFileSync(path.join(dir, 'compiler-error.json'), JSON.stringify(compilerError(e), null, 4) + '\n');
  }
}
console.log(`wrote ${CASES.length} cases to ${OUT}`);
