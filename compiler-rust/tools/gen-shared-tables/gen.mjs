#!/usr/bin/env node
// Generates the @lwc/shared and @lwc/template-compiler lookup tables (AB-0011, design AB-D0002):
//   crates/lwc-shared/src/generated.rs                                  (@lwc/shared tables)
//   crates/lwc-template/src/parser/constants.rs                         (TC parser/constants.ts, shared/constants.ts)
//   crates/lwc-template/src/parser/utils/html_element_attributes.rs     (TC parser/utils/html-element-attributes.ts)
// Never hand-edit the outputs; rerun: node tools/gen-shared-tables/gen.mjs
//
// usage: node tools/gen-shared-tables/gen.mjs [--check] [--root <rust repo root>]
//   --root   defaults to the repo containing this script; works from any cwd.
//   LWC_ROOT upstream checkout (default /workspace/lwc); its HEAD must equal ./PIN.
//   --check  regenerate in memory and byte-compare; writes nothing.
// exit: 0 ok | 1 stale output (--check) | 2 extraction or self-check failure | 3 LWC_ROOT HEAD != PIN
// Fails closed: any literal it cannot read, any non-ASCII entry, a duplicate key, or a table whose
// answers differ from the upstream predicate (self-check) aborts before anything is written.
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const check = args.includes('--check');
const ri = args.indexOf('--root');
if (ri >= 0 && !args[ri + 1]) die(2, '--root needs a directory');
const ROOT = path.resolve(ri >= 0 ? args[ri + 1] : path.join(HERE, '../..'));
const LWC = path.resolve(process.env.LWC_ROOT || '/workspace/lwc');
const S = 'packages/@lwc/shared/src/';
const TC = 'packages/@lwc/template-compiler/src/';

function die(code, msg) { console.error(`gen-shared-tables: ${msg}`); process.exit(code); }

// ---- pin ----
const PIN = fs.readFileSync(path.join(HERE, 'PIN'), 'utf8').trim();
let head;
try { head = execFileSync('git', ['-C', LWC, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(); }
catch (e) { die(3, `PIN MISMATCH: cannot read HEAD of LWC_ROOT=${LWC} (${e.message.split('\n')[0]}); PIN=${PIN}`); }
if (head !== PIN) die(3, `PIN MISMATCH: LWC_ROOT=${LWC} HEAD ${head} != PIN ${PIN} (tools/gen-shared-tables/PIN)`);

// ---- module-private literals, read from the TS AST ----
const ts = createRequire(path.join(LWC, 'package.json'))('typescript');
const INPUTS = [
  S + 'void-elements.ts', S + 'html-attributes.ts', S + 'aria.ts', S + 'namespaces.ts',
  TC + 'parser/constants.ts', TC + 'shared/constants.ts',
  TC + 'parser/utils/html-elements.ts', TC + 'parser/utils/svg-elements.ts', TC + 'parser/utils/html-element-attributes.ts',
];
const inputHash = crypto.createHash('sha256');
for (const f of INPUTS) inputHash.update(f + '\0' + fs.readFileSync(path.join(LWC, f), 'utf8') + '\0');
const HASH = inputHash.digest('hex').slice(0, 16);

function topLevelLiterals(rel) {
  const src = ts.createSourceFile(rel, fs.readFileSync(path.join(LWC, rel), 'utf8'), ts.ScriptTarget.Latest, true);
  const decls = new Map();
  for (const st of src.statements) if (ts.isVariableStatement(st))
    for (const d of st.declarationList.declarations) if (ts.isIdentifier(d.name) && d.initializer) decls.set(d.name.text, d.initializer);
  const evalNode = (n, what) => {
    while (ts.isAsExpression(n) || ts.isParenthesizedExpression(n) || ts.isSatisfiesExpression?.(n) || ts.isTypeAssertionExpression?.(n)) n = n.expression;
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) return n.text;
    if (ts.isArrayLiteralExpression(n)) return n.elements.flatMap((e) => {
      if (ts.isSpreadElement(e)) { const v = get(e.expression.getText(), what); if (!Array.isArray(v)) die(2, `${rel}: ${what}: spread of non-array`); return v; }
      return [evalNode(e, what)];
    });
    if (ts.isNewExpression(n) && ts.isIdentifier(n.expression) && (n.expression.text === 'Set' || n.expression.text === 'Map')) {
      const a = n.arguments ?? [];
      if (a.length > 1) die(2, `${rel}: ${what}: unexpected ${n.expression.text} arguments`);
      const items = a.length ? evalNode(a[0], what) : [];
      if (n.expression.text === 'Set') return { set: [...new Set(items)] };
      const m = new Map(); // Map semantics: first position, last value
      for (const kv of items) { if (!Array.isArray(kv) || kv.length !== 2) die(2, `${rel}: ${what}: bad Map entry`); m.set(kv[0], kv[1]); }
      return { map: [...m] };
    }
    die(2, `${rel}: ${what}: unsupported initializer ${ts.SyntaxKind[n.kind]} at ${n.getStart()}`);
  };
  const get = (name, what = name) => { const n = decls.get(name); if (!n) die(2, `${rel}: ${name} not found`); return evalNode(n, what); };
  return get;
}
const voidLit = topLevelLiterals(S + 'void-elements.ts');
const htmlLit = topLevelLiterals(S + 'html-attributes.ts');
const ariaLit = topLevelLiterals(S + 'aria.ts');
const VOID = [...voidLit('VOID_ELEMENTS'), ...voidLit('DEPRECATED_VOID_ELEMENTS')];
const VOID_SET = voidLit('VOID_ELEMENTS_SET').set;
if (JSON.stringify([...new Set(VOID)]) !== JSON.stringify(VOID_SET)) die(2, 'VOID_ELEMENTS_SET != VOID_ELEMENTS + DEPRECATED_VOID_ELEMENTS');
const BOOLEAN = htmlLit('BOOLEAN_ATTRIBUTES').map.map(([k, v]) => [k, v.set]);
const GLOBAL = htmlLit('GLOBAL_ATTRIBUTE').set;
const ARIA_NAMES = ariaLit('AriaPropertyNames');

// ---- exported values + upstream predicate answers, via tsx ----
const keysOf = (pairs) => pairs.map(([k]) => k);
const astKeys = [...VOID, ...keysOf(BOOLEAN), ...GLOBAL, ...ARIA_NAMES];
const EXTRA = ['', 'constructor', '__proto__', 'toString', 'hasOwnProperty', 'valueOf', 'isPrototypeOf', '\u212Aeygen', '\u212A', 'ſ',
  'İnput', 'ARIA-LABEL', 'aria-', 'aria', 'data-x', 'HIDDEN', 'Hidden', 'tabIndex', 'htmlFor', 'fooBar', 'x-foo', 'ß', '😀', 'menuItem'];
function probeNames(extraKeys) {
  const s = new Set([...EXTRA, ...astKeys, ...extraKeys]);
  for (const k of [...s]) for (const v of [k.toUpperCase(), k.slice(0, 1).toUpperCase() + k.slice(1), k + 'x', 'x' + k, k.slice(1), k.replace(/k/g, '\u212A')]) s.add(v);
  return [...s];
}
const tsx = path.join(LWC, 'node_modules/.bin/tsx');
function runExtract(names, tags) {
  const tmp = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'gst-')), 'probes.json');
  fs.writeFileSync(tmp, JSON.stringify({ names, tags }));
  try {
    return JSON.parse(execFileSync(tsx, [path.join(HERE, 'extract.mts'), tmp], { encoding: 'utf8', env: { ...process.env, LWC_ROOT: LWC }, maxBuffer: 1 << 28 }));
  } catch (e) { die(2, `extract.mts failed: ${e.message.split('\n').slice(0, 5).join(' | ')}`); }
  finally { fs.rmSync(path.dirname(tmp), { recursive: true, force: true }); }
}
const tags = [...new Set([...BOOLEAN.flatMap(([, v]) => v), 'div', '', 'INPUT', 'constructor'])];
// Pass 1 gets the exports; pass 2 probes with their keys too.
const { exports: X } = runExtract([], []);
const names = probeNames([...keysOf(X.SPECIAL_PROPERTY_ATTRIBUTE_MAPPING), ...X.SPECIAL_PROPERTY_ATTRIBUTE_MAPPING.map(([, a]) => a),
  ...keysOf(X.AriaPropNameToAttrNameMap), ...keysOf(X.AriaAttrNameToPropNameMap), ...X.REFLECTIVE_GLOBAL_PROPERTY_SET, ...X.ID_REFERENCING_ATTRIBUTES_SET]);
const { results: R } = runExtract(names, tags);

// ---- tables as emitted (sorted by UTF-8 bytes = Rust str Ord; all ASCII, checked below) ----
const cmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const sorted = (xs) => [...xs].sort(cmp);
const sortedPairs = (ps) => [...ps].sort((a, b) => cmp(a[0], b[0]));
const T = {
  VOID_ELEMENTS: sorted(VOID),
  BOOLEAN_ATTRIBUTES: sortedPairs(BOOLEAN.map(([k, v]) => [k, sorted(v)])),
  GLOBAL_ATTRIBUTE: sorted(GLOBAL),
  SPECIAL_PROPERTY_TO_ATTRIBUTE: sortedPairs(X.SPECIAL_PROPERTY_ATTRIBUTE_MAPPING),
  SPECIAL_ATTRIBUTE_TO_PROPERTY: sortedPairs(X.SPECIAL_PROPERTY_ATTRIBUTE_MAPPING.map(([p, a]) => [a, p])),
  REFLECTIVE_GLOBAL_PROPERTY_SET: sorted(X.REFLECTIVE_GLOBAL_PROPERTY_SET),
  AMBIGUOUS_PROP_SET: sortedPairs(X.AMBIGUOUS_PROP_SET),
  DISALLOWED_PROP_SET: sorted(X.DISALLOWED_PROP_SET),
  ARIA_PROPERTY_NAMES: ARIA_NAMES, // upstream order (a list)
  ARIA_PROP_TO_ATTR: sortedPairs(X.AriaPropNameToAttrNameMap),
  ARIA_ATTR_TO_PROP: sortedPairs(X.AriaAttrNameToPropNameMap),
  ID_REFERENCING_ATTRIBUTES_SET: sorted(X.ID_REFERENCING_ATTRIBUTES_SET),
  KNOWN_HTML_AND_SVG_ELEMENTS: sorted(X.KNOWN_HTML_AND_SVG_ELEMENTS),
  SUPPORTED_SVG_TAGS: sorted(X.SUPPORTED_SVG_TAGS),
  DASHED_TAGNAME_ELEMENT_SET: sorted(X.DASHED_TAGNAME_ELEMENT_SET),
  HTML_ATTRIBUTE_ELEMENT_MAP: sortedPairs(X.HTML_ATTRIBUTE_ELEMENT_MAP), // element lists keep upstream order
};
const NS = ['HTML_NAMESPACE', 'XML_NAMESPACE', 'SVG_NAMESPACE', 'MATHML_NAMESPACE', 'XLINK_NAMESPACE'];

// ---- fail-closed structural checks ----
const ascii = (s) => typeof s === 'string' && /^[\x20-\x7e]*$/.test(s) && !/["\\]/.test(s);
for (const [name, t] of Object.entries(T)) {
  const keys = name === 'ARIA_PROPERTY_NAMES' ? sorted(t) : t.map((e) => (Array.isArray(e) ? e[0] : e));
  for (const e of t.flat(2)) if (!ascii(e)) die(2, `${name}: non-ASCII or unquotable entry ${JSON.stringify(e)}`);
  for (let i = 1; i < keys.length; i++) if (!(keys[i - 1] < keys[i])) die(2, `${name}: duplicate key ${JSON.stringify(keys[i])}`);
  if (!t.length) die(2, `${name}: empty`);
}
for (const n of NS) if (!ascii(X[n])) die(2, `${n}: bad value`);
if (JSON.stringify(ARIA_NAMES) !== JSON.stringify(keysOf(X.AriaPropNameToAttrNameMap))) die(2, 'AriaPropertyNames (AST) != keys of AriaPropNameToAttrNameMap');

// ---- self-check: emitted tables answer exactly like the upstream functions ----
const has = (xs, k) => xs.includes(k);
const pairGet = (ps, k) => ps.find(([a]) => a === k)?.[1];
const mine = {
  isVoidHtml: (n) => has(T.VOID_ELEMENTS, n.toLowerCase()),
  isVoidSvg: () => false,
  isBoolean: (n, t) => { const v = pairGet(T.BOOLEAN_ATTRIBUTES, n); return v !== undefined && (v.length === 0 || v.includes(t)); },
  isGlobal: (n) => has(T.GLOBAL_ATTRIBUTE, n),
  isAria: (n) => pairGet(T.ARIA_ATTR_TO_PROP, n) !== undefined,
  propToAttr: (n) => pairGet(T.ARIA_PROP_TO_ATTR, n) ?? pairGet(T.SPECIAL_PROPERTY_TO_ATTRIBUTE, n) ??
    [...n].map((c) => (c >= 'A' && c <= 'Z' ? '-' + c.toLowerCase() : c)).join(''),
};
let bad = 0, checked = 0;
names.forEach((n, i) => {
  for (const k of ['isVoidHtml', 'isVoidSvg', 'isGlobal', 'isAria', 'propToAttr']) {
    checked++;
    if (mine[k](n) !== R[k][i]) { if (bad++ < 10) console.error(`  ${k}(${JSON.stringify(n)}): upstream ${JSON.stringify(R[k][i])}, tables ${JSON.stringify(mine[k](n))}`); }
  }
  tags.forEach((t, j) => { checked++; if (mine.isBoolean(n, t) !== R.isBoolean[i][j]) { if (bad++ < 10) console.error(`  isBooleanAttribute(${JSON.stringify(n)}, ${JSON.stringify(t)}) differs`); } });
});
if (bad) die(2, `SELF-CHECK FAILED: ${bad}/${checked} answers differ from upstream`);

// ---- emit ----
const header = `// @generated by tools/gen-shared-tables/gen.mjs from @lwc ${PIN.slice(0, 12)}; sha256(inputs)=${HASH}. Do not edit.\n// rerun: node tools/gen-shared-tables/gen.mjs\n`;
const q = (s) => JSON.stringify(s);
const strList = (name, xs, doc, vis = 'pub') => `${doc.map((d) => `/// ${d}\n`).join('')}${vis} static ${name}: &[&str] = &[\n${xs.map((x) => `    ${q(x)},\n`).join('')}];\n`;
const pairList = (name, ps, doc, vis = 'pub') => `${doc.map((d) => `/// ${d}\n`).join('')}${vis} static ${name}: &[(&str, &str)] = &[\n${ps.map(([a, b]) => `    (${q(a)}, ${q(b)}),\n`).join('')}];\n`;
const listMap = (name, ps, doc, vis = 'pub') => `${doc.map((d) => `/// ${d}\n`).join('')}${vis} static ${name}: &[(&str, &[&str])] = &[\n${ps.map(([a, v]) => `    (${q(a)}, &[${v.map(q).join(', ')}]),\n`).join('')}];\n`;
const SORTED = 'Sorted by key (byte order) for `binary_search`.';

const sharedRs = [
  header,
  '//! Data for the hand-written `lwc_shared` modules that mirror @lwc/shared `src/*.ts`',
  '//! (`void_elements`, `html_attributes`, `aria`, `namespaces`). Look values up through those modules.',
  '',
  ...NS.map((n) => `/// @lwc/shared namespaces.ts \`${n}\`.\npub const ${n}: &str = ${q(X[n])};`),
  '',
  strList('VOID_ELEMENTS', T.VOID_ELEMENTS, ['void-elements.ts `VOID_ELEMENTS` + `DEPRECATED_VOID_ELEMENTS` (`VOID_ELEMENTS_SET`). ' + SORTED]),
  listMap('BOOLEAN_ATTRIBUTES', T.BOOLEAN_ATTRIBUTES, ['html-attributes.ts `BOOLEAN_ATTRIBUTES` (attr -> allowed tag names; empty = any tag). ' + SORTED + ' Tag lists sorted too.']),
  strList('GLOBAL_ATTRIBUTE', T.GLOBAL_ATTRIBUTE, ['html-attributes.ts `GLOBAL_ATTRIBUTE`. ' + SORTED]),
  pairList('SPECIAL_PROPERTY_TO_ATTRIBUTE', T.SPECIAL_PROPERTY_TO_ATTRIBUTE, ['html-attributes.ts `SPECIAL_PROPERTY_ATTRIBUTE_MAPPING` (prop -> attr). ' + SORTED]),
  pairList('SPECIAL_ATTRIBUTE_TO_PROPERTY', T.SPECIAL_ATTRIBUTE_TO_PROPERTY, ['The same mapping inverted (attr -> prop), as template-compiler parser/constants.ts `ATTRS_PROPS_TRANFORMS` builds it. ' + SORTED]),
  strList('REFLECTIVE_GLOBAL_PROPERTY_SET', T.REFLECTIVE_GLOBAL_PROPERTY_SET, ['html-attributes.ts `REFLECTIVE_GLOBAL_PROPERTY_SET`. ' + SORTED]),
  pairList('AMBIGUOUS_PROP_SET', T.AMBIGUOUS_PROP_SET, ['html-attributes.ts `AMBIGUOUS_PROP_SET` (lowercase -> camelCase). ' + SORTED]),
  strList('DISALLOWED_PROP_SET', T.DISALLOWED_PROP_SET, ['html-attributes.ts `DISALLOWED_PROP_SET`. ' + SORTED]),
  strList('ARIA_PROPERTY_NAMES', T.ARIA_PROPERTY_NAMES, ['aria.ts `AriaPropertyNames`, in upstream order (a list, not a lookup).']),
  pairList('ARIA_PROP_TO_ATTR', T.ARIA_PROP_TO_ATTR, ['aria.ts `AriaPropNameToAttrNameMap`. ' + SORTED]),
  pairList('ARIA_ATTR_TO_PROP', T.ARIA_ATTR_TO_PROP, ['aria.ts `AriaAttrNameToPropNameMap`. ' + SORTED]),
  strList('ID_REFERENCING_ATTRIBUTES_SET', T.ID_REFERENCING_ATTRIBUTES_SET, ['aria.ts `ID_REFERENCING_ATTRIBUTES_SET`. ' + SORTED]),
  '/// Every sorted key list above, for the `tables_sorted` test.',
  '#[cfg(test)]',
  'pub(crate) fn sorted_key_lists() -> Vec<(&\'static str, Vec<&\'static str>)> {',
  '    vec![',
  ...['VOID_ELEMENTS', 'GLOBAL_ATTRIBUTE', 'REFLECTIVE_GLOBAL_PROPERTY_SET', 'DISALLOWED_PROP_SET', 'ID_REFERENCING_ATTRIBUTES_SET'].map((n) => `        (${q(n)}, ${n}.to_vec()),`),
  ...['BOOLEAN_ATTRIBUTES', 'SPECIAL_PROPERTY_TO_ATTRIBUTE', 'SPECIAL_ATTRIBUTE_TO_PROPERTY', 'AMBIGUOUS_PROP_SET', 'ARIA_PROP_TO_ATTR', 'ARIA_ATTR_TO_PROP'].map((n) => `        (${q(n)}, ${n}.iter().map(|e| e.0).collect()),`),
  ...T.BOOLEAN_ATTRIBUTES.map(([k]) => `        (${q('BOOLEAN_ATTRIBUTES.' + k)}, BOOLEAN_ATTRIBUTES.iter().find(|e| e.0 == ${q(k)}).unwrap().1.to_vec()),`),
  '    ]',
  '}',
  '',
].join('\n');

const constantsRs = [
  header,
  '//! @lwc/template-compiler tables: parser/constants.ts `KNOWN_HTML_AND_SVG_ELEMENTS` and',
  '//! `SUPPORTED_SVG_TAGS`, shared/constants.ts `DASHED_TAGNAME_ELEMENT_SET`.',
  '',
  strList('KNOWN_HTML_AND_SVG_ELEMENTS', T.KNOWN_HTML_AND_SVG_ELEMENTS, ['parser/constants.ts `KNOWN_HTML_AND_SVG_ELEMENTS` (utils/html-elements.ts + utils/svg-elements.ts). ' + SORTED], 'pub(crate)'),
  strList('SUPPORTED_SVG_TAGS', T.SUPPORTED_SVG_TAGS, ['parser/constants.ts `SUPPORTED_SVG_TAGS`. ' + SORTED], 'pub(crate)'),
  strList('DASHED_TAGNAME_ELEMENT_SET', T.DASHED_TAGNAME_ELEMENT_SET, ['shared/constants.ts `DASHED_TAGNAME_ELEMENT_SET`. ' + SORTED], 'pub(crate)'),
].join('\n');

const haemRs = [
  header,
  '//! @lwc/template-compiler parser/utils/html-element-attributes.ts `HTML_ATTRIBUTE_ELEMENT_MAP`',
  '//! (attribute -> elements that accept it; global attributes map to `[]`). Own keys only.',
  '',
  listMap('HTML_ATTRIBUTE_ELEMENT_MAP', T.HTML_ATTRIBUTE_ELEMENT_MAP, [SORTED + ' Element lists keep upstream order.'], 'pub(crate)'),
].join('\n');

const OUT = {
  'crates/lwc-shared/src/generated.rs': sharedRs,
  'crates/lwc-template/src/parser/constants.rs': constantsRs,
  'crates/lwc-template/src/parser/utils/html_element_attributes.rs': haemRs,
};
if (check) {
  const stale = Object.entries(OUT).filter(([f, s]) => { const p = path.join(ROOT, f); return !fs.existsSync(p) || fs.readFileSync(p, 'utf8') !== s; }).map(([f]) => f);
  if (stale.length) {
    for (const f of stale) console.error(`STALE: ${f}`);
    die(1, `STALE OUTPUT (${stale.length} file(s)) under ${ROOT}; rerun: node tools/gen-shared-tables/gen.mjs`);
  }
  console.log(`gen-shared-tables: fresh (${Object.keys(OUT).length} files, pin ${PIN.slice(0, 12)}, self-check ${checked} answers)`);
} else {
  for (const [f, s] of Object.entries(OUT)) { const p = path.join(ROOT, f); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, s); }
  console.log(`gen-shared-tables: wrote ${Object.keys(OUT).length} files under ${ROOT} (pin ${PIN.slice(0, 12)}, self-check ${checked} answers)`);
}
