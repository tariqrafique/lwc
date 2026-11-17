#!/usr/bin/env node
// Generates crates/lwc-shared/src/instrumentation/generated.rs (J3, D-0205 / D-0212) from the pinned @lwc checkout:
//   CompilerMetric  <- @lwc/errors src/compiler/instrumentation.ts `CompilerMetrics`
//   DeprecationId   <- @lwc/compiler src/options.ts, every `console.warn(<literal>)` (D-0212's deprecation warnings)
// Never hand-edit the output; rerun: node tools/gen-instrumentation/gen.mjs
//
// usage: node tools/gen-instrumentation/gen.mjs [--check | --stdout | --oracle] [--root <rust repo root>]
//   --check   regenerate in memory and byte-compare; writes nothing.
//   --stdout  print the generated file.
//   --oracle  print what the upstream RUNTIME does (not the AST), one TSV row per fact, for lwc-shared's oracle test:
//               metric<TAB><upstream key><TAB><value>
//               deprecation<TAB><option path><TAB><warnings over two validateTransformOptions calls><TAB><text>
//   LWC_ORACLE upstream checkout (via harness/scripts/oracle-root.mjs); its HEAD must equal tools/gen-shared-tables/PIN.
// exit: 0 ok | 1 stale output (--check) | 2 extraction or self-check failure | 3 LWC_ROOT HEAD != PIN
// Fails closed: a non-literal value or warning, an unrecognised latch, or an AST table that disagrees with the
// upstream runtime (self-check) aborts before anything is written.
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url'; import { oracleRoot } from '../../harness/scripts/oracle-root.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const mode = args.includes('--check') ? 'check' : args.includes('--stdout') ? 'stdout' : args.includes('--oracle') ? 'oracle' : 'write';
const ri = args.indexOf('--root');
if (ri >= 0 && !args[ri + 1]) die(2, '--root needs a directory');
const ROOT = path.resolve(ri >= 0 ? args[ri + 1] : path.join(HERE, '../..'));
const LWC = oracleRoot();
const METRICS_TS = 'packages/@lwc/errors/src/compiler/instrumentation.ts';
const OPTIONS_TS = 'packages/@lwc/compiler/src/options.ts';
const OUT = 'crates/lwc-shared/src/instrumentation/generated.rs';

function die(code, msg) { console.error(`gen-instrumentation: ${msg}`); process.exit(code); }

// ---- pin (the one pin file, shared with gen-shared-tables) ----
const PIN = fs.readFileSync(path.join(HERE, '../gen-shared-tables/PIN'), 'utf8').trim();
let head;
try { head = execFileSync('git', ['-C', LWC, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(); }
catch (e) { die(3, `PIN MISMATCH: cannot read HEAD of LWC_ROOT=${LWC} (${e.message.split('\n')[0]}); PIN=${PIN}`); }
if (head !== PIN) die(3, `PIN MISMATCH: LWC_ROOT=${LWC} HEAD ${head} != PIN ${PIN}`);

// ---- AST extraction ----
const ts = createRequire(path.join(LWC, 'package.json'))('typescript');
const parse = (rel) => ts.createSourceFile(rel, fs.readFileSync(path.join(LWC, rel), 'utf8'), ts.ScriptTarget.Latest, true);
const lineOf = (src, node) => src.getLineAndCharacterOfPosition(node.getStart(src)).line + 1;
const strOf = (n) => (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n) ? n.text : undefined);

// CompilerMetrics: `export const CompilerMetrics = { Key: 'value', ... } as const;`
const msrc = parse(METRICS_TS);
let metrics, metricsLine;
for (const st of msrc.statements) if (ts.isVariableStatement(st)) for (const d of st.declarationList.declarations) {
  if (d.name.getText(msrc) !== 'CompilerMetrics') continue;
  let init = d.initializer;
  while (init && (ts.isAsExpression(init) || ts.isSatisfiesExpression?.(init))) init = init.expression;
  if (!init || !ts.isObjectLiteralExpression(init)) die(2, `${METRICS_TS}: CompilerMetrics is not an object literal`);
  metricsLine = lineOf(msrc, d);
  metrics = init.properties.map((p) => {
    const v = ts.isPropertyAssignment(p) && strOf(p.initializer);
    if (typeof v !== 'string') die(2, `${METRICS_TS}:${lineOf(msrc, p)}: non-literal CompilerMetrics entry`);
    return { key: p.name.getText(msrc), value: v, line: lineOf(msrc, p) };
  });
}
if (!metrics?.length) die(2, `${METRICS_TS}: no CompilerMetrics`);

// Deprecations: every console.warn(<literal>) in options.ts (D-0212, amended by D-0250). Each warn must sit in the
// THEN branch of exactly one `if` (up to its function) whose condition is an `&&` of recognised terms:
//   `!isUndefined(<a>.<b>...)`  presence of the deprecated option  -> warns on every call, or
//   `!alreadyWarned*`           a module-level `let` latch the same branch sets `= true` -> once per process.
// At least one presence term is required. Any other guard shape (else branch, nested ifs, other terms, a latch the
// branch does not set, a loop/try/switch/conditional around the warn) is a failure: unknown semantics.
const osrc = parse(OPTIONS_TS);
const latches = new Set();
for (const st of osrc.statements) if (ts.isVariableStatement(st) && (st.declarationList.flags & ts.NodeFlags.Let))
  for (const d of st.declarationList.declarations) if (/^alreadyWarned/.test(d.name.getText(osrc))) latches.add(d.name.getText(osrc));
const deprecations = [];
// `true` = latched (once per process), `false` = every call; dies on any other shape (see the comment above).
function guardOf(call) {
  const where = `${OPTIONS_TS}:${lineOf(osrc, call)}`;
  const ifs = [];
  let child = call;
  for (let p = call.parent; p && !ts.isFunctionLike(p) && !ts.isSourceFile(p); child = p, p = p.parent) {
    if (ts.isIfStatement(p)) {
      if (p.thenStatement !== child) die(2, `${where}: console.warn outside an if's then-branch (unknown guard shape)`);
      ifs.push(p);
    } else if (!(ts.isBlock(p) || ts.isExpressionStatement(p))) die(2, `${where}: console.warn under ${ts.SyntaxKind[p.kind]} (unknown guard shape)`);
  }
  if (ifs.length !== 1) die(2, `${where}: ${ifs.length} enclosing ifs, want exactly 1 (unknown guard shape)`);
  const terms = [];
  (function split(e) {
    while (ts.isParenthesizedExpression(e)) e = e.expression;
    if (ts.isBinaryExpression(e) && e.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken) { split(e.left); split(e.right); }
    else terms.push(e);
  })(ifs[0].expression);
  let presence = 0; const used = [];
  for (const t of terms) {
    const neg = ts.isPrefixUnaryExpression(t) && t.operator === ts.SyntaxKind.ExclamationToken ? t.operand : undefined;
    if (neg && ts.isIdentifier(neg) && latches.has(neg.text)) used.push(neg.text);
    else if (neg && ts.isCallExpression(neg) && neg.expression.getText(osrc) === 'isUndefined' && neg.arguments.length === 1 && ts.isPropertyAccessExpression(neg.arguments[0])) presence++;
    else die(2, `${where}: unrecognised guard term \`${t.getText(osrc)}\` (unknown guard shape)`);
  }
  if (!presence) die(2, `${where}: guard has no !isUndefined(<option>) presence term (unknown guard shape)`);
  if (used.length > 1) die(2, `${where}: more than one latch (unknown guard shape)`);
  if (used.length) {
    const sets = ifs[0].thenStatement.getText(osrc).replace(/\s+/g, ' ').includes(`${used[0]} = true`);
    if (!sets) die(2, `${where}: latch ${used[0]} is tested but not set in the same branch (unknown guard shape)`);
  }
  return used.length === 1;
}
let warnCalls = 0;
(function visit(n) {
  if (ts.isCallExpression(n) && n.expression.getText(osrc) === 'console.warn') {
    warnCalls++;
    const text = n.arguments.length === 1 ? strOf(n.arguments[0]) : undefined;
    if (typeof text !== 'string') die(2, `${OPTIONS_TS}:${lineOf(osrc, n)}: console.warn without a single literal argument`);
    const once = guardOf(n);
    const m = /^"([A-Za-z.]+)" property is deprecated\./.exec(text);
    if (!m) die(2, `${OPTIONS_TS}:${lineOf(osrc, n)}: warning does not name a property: ${JSON.stringify(text)}`);
    const segs = m[1].split('.');
    deprecations.push({
      id: segs.map((s) => s[0].toUpperCase() + s.slice(1)).join(''),
      optionPath: segs.map((s) => s[0].toLowerCase() + s.slice(1)).join('.'),
      text, once, line: lineOf(osrc, n.arguments[0]),
    });
  }
  ts.forEachChild(n, visit);
})(osrc);
if (!deprecations.length || deprecations.length !== warnCalls) die(2, `${OPTIONS_TS}: ${warnCalls} console.warn calls, ${deprecations.length} extracted`);
for (const t of [metrics.map((m) => m.key), metrics.map((m) => m.value), deprecations.map((d) => d.id)])
  if (new Set(t).size !== t.length) die(2, `duplicate entry in ${JSON.stringify(t)}`);

// ---- upstream runtime (via tsx): the values @lwc/errors exports and the warnings validateTransformOptions prints ----
function runtime() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'gen-instr-'));
  try {
    const probe = path.join(dir, 'probe.mts');
    fs.writeFileSync(probe, `import { pathToFileURL } from 'node:url';
const imp = (p: string) => import(pathToFileURL(${JSON.stringify(LWC)} + '/packages/@lwc/' + p).href);
const errors = await imp('errors/src/index.ts');
const paths: string[] = ${JSON.stringify(deprecations.map((d) => d.optionPath))};
const warnings: Record<string, string[]> = {};
const orig = console.warn;
for (const p of paths) {
    const fresh = await import(pathToFileURL(${JSON.stringify(path.join(LWC, OPTIONS_TS))}).href + '?' + p);
    const got: string[] = [];
    console.warn = (...a: unknown[]) => { got.push(a.map(String).join(' ')); };
    for (let i = 0; i < 2; i++) {
        const o: Record<string, unknown> = { name: 'a', namespace: 'x' };
        let cur = o; const segs = p.split('.');
        segs.slice(0, -1).forEach((s) => { cur = (cur[s] = {}) as Record<string, unknown>; });
        cur[segs[segs.length - 1]] = {};
        fresh.validateTransformOptions(o);
    }
    console.warn = orig;
    warnings[p] = got;
}
process.stdout.write(JSON.stringify({ metrics: Object.entries(errors.CompilerMetrics), warnings }));
`);
    return JSON.parse(execFileSync(path.join(LWC, 'node_modules/.bin/tsx'), [probe], { encoding: 'utf8', cwd: LWC }));
  } catch (e) { die(2, `runtime probe failed: ${e.message.split('\n')[0]}`); }
  finally { fs.rmSync(dir, { recursive: true, force: true }); }
}
const R = runtime();

if (mode === 'oracle') {
  const rows = [...R.metrics.map(([k, v]) => `metric\t${k}\t${v}`),
    ...Object.entries(R.warnings).flatMap(([p, ws]) => [...new Set(ws)].map((w) => `deprecation\t${p}\t${ws.filter((x) => x === w).length}\t${w}`))];
  process.stdout.write(rows.join('\n') + '\n');
  process.exit(0);
}

// ---- self-check: the AST tables equal the runtime ----
const bad = [];
if (JSON.stringify(R.metrics) !== JSON.stringify(metrics.map((m) => [m.key, m.value]))) bad.push(`CompilerMetrics: runtime ${JSON.stringify(R.metrics)}`);
for (const d of deprecations) {
  const ws = R.warnings[d.optionPath] || [];
  const want = Array(d.once ? 1 : 2).fill(d.text);
  if (JSON.stringify(ws) !== JSON.stringify(want)) bad.push(`${d.optionPath}: runtime warned ${JSON.stringify(ws)}, AST says ${want.length}x`);
}
if (bad.length) die(2, `SELF-CHECK FAILED:\n  ${bad.join('\n  ')}`);

// ---- emit ----
const q = (s) => JSON.stringify(s);
const variant = (k) => k.replace(/^LWC/, 'Lwc');
const rs = `// @generated by tools/gen-instrumentation/gen.mjs from @lwc ${PIN.slice(0, 12)}. Do not edit.
// rerun: node tools/gen-instrumentation/gen.mjs

/// \`@lwc/errors\` \`CompilerMetrics\` (${METRICS_TS}:${metricsLine}), in upstream order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilerMetric {
${metrics.map((m) => `    /// \`CompilerMetrics.${m.key}\` (instrumentation.ts:${m.line}).\n    ${variant(m.key)},`).join('\n')}
}

impl CompilerMetric {
    /// Every metric, in upstream declaration order.
    pub const ALL: &[Self] = &[${metrics.map((m) => `Self::${variant(m.key)}`).join(', ')}];

    /// The string upstream passes to \`incrementCounter\`.
    pub fn js_value(self) -> &'static str {
        match self {
${metrics.map((m) => `            Self::${variant(m.key)} => ${q(m.value)},`).join('\n')}
        }
    }

    /// Inverse of [\`Self::js_value\`].
    pub fn from_js(s: &str) -> Option<Self> { Self::ALL.iter().copied().find(|m| m.js_value() == s) }
}

/// A deprecation \`console.warn\` of \`@lwc/compiler\` \`validateTransformOptions\` (${OPTIONS_TS}, D-0212).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeprecationId {
${deprecations.map((d) => `    /// options.ts:${d.line}${d.once ? ', once per process' : ', on every call'}.\n    ${d.id},`).join('\n')}
}

impl DeprecationId {
    /// Every deprecation, in upstream source order.
    pub const ALL: &[Self] = &[${deprecations.map((d) => `Self::${d.id}`).join(', ')}];

    /// The exact text upstream passes to \`console.warn\`.
    pub fn message(self) -> &'static str {
        match self {
${deprecations.map((d) => `            Self::${d.id} => ${q(d.text)},`).join('\n')}
        }
    }

    /// The \`TransformOptions\` key path whose presence (not \`undefined\`) triggers the warning.
    pub fn option_path(self) -> &'static str {
        match self {
${deprecations.map((d) => `            Self::${d.id} => ${q(d.optionPath)},`).join('\n')}
        }
    }

    /// \`true\`: upstream latches it (\`alreadyWarnedOn*\`) and warns once per process; \`false\`: on every call.
    pub fn once_per_process(self) -> bool {
        match self {
${deprecations.map((d) => `            Self::${d.id} => ${d.once},`).join('\n')}
        }
    }
}
`;
const p = path.join(ROOT, OUT);
if (mode === 'stdout') process.stdout.write(rs);
else if (mode === 'check') {
  if (!fs.existsSync(p) || fs.readFileSync(p, 'utf8') !== rs) die(1, `STALE: ${OUT}; rerun: node tools/gen-instrumentation/gen.mjs`);
  console.log(`gen-instrumentation: fresh (${metrics.length} metrics, ${deprecations.length} deprecations, pin ${PIN.slice(0, 12)})`);
} else {
  fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, rs);
  console.log(`gen-instrumentation: wrote ${OUT} (${metrics.length} metrics, ${deprecations.length} deprecations, pin ${PIN.slice(0, 12)})`);
}
