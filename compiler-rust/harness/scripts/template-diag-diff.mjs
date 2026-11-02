#!/usr/bin/env node
// template-diag-diff: differential diagnostics corpus for LWC templates, JS oracle (/workspace/lwc b9e8007, read-only)
// vs Rust. Same pattern as tools/style-diff (notes/postcss-rust/05-06): deterministic seedless corpus, sha256 dedup,
// per-fixture caps, crash/hang-isolated child workers, exact compare, bucketed JSON report + text summary.
//
//   node harness/scripts/template-diag-diff.mjs                         # full corpus, engine=crate
//   TDD_ENGINE=napi node harness/scripts/template-diag-diff.mjs         # same corpus through the transformSync seam
//   node harness/scripts/template-diag-diff.mjs --engine=both           # crate + napi; report-both.json + BOARD line
//   TDD_SOURCES=fixtures,mutations node harness/scripts/template-diag-diff.mjs
//
// Engines:
//   crate (default): @lwc/template-compiler compile() vs lwc_template::compile via the standalone driver in
//     tools/template-diag-diff/driver (built on demand, own target dir). Compares the complete warnings[] list at
//     every level (code, message, level, line/column/start/length) plus a codegen hash.
//   napi: @lwc/compiler transformSync vs the napi `transform`, both with experimentalErrorRecoveryMode:true
//     (errors[] of the aggregate when it throws, else warnings[]). Catches seam issues (option decoding,
//     aggregate shape) on top of the template compiler.
//
// Env: --engine= / TDD_ENGINE (crate|napi|both), TDD_NAPI (napi engine; default harness/lwc-compiler-napi.node),
//      TDD_OUT (default harness/.template-diag-diff/report-<engine>.json; a directory for both), TDD_JOBS (default nproc-1),
//      TDD_SOURCES (comma list: fixtures, real, mutations or a source key; default all),
//      TDD_CAP (mutations per fixture per family, default 40), TDD_HANG_MS (per-case watchdog, default 20000),
//      TDD_DRIVER (prebuilt driver binary), TDD_DRIVER_TARGET (driver cargo target dir).
//      Self-test hooks: TDD_ORACLE_DIST (oracle module path: template-compiler dist for crate, @lwc/compiler for napi),
//      TDD_ORACLE_INJECT=throw (oracle call throws a TypeError). `--self-test` runs both on both engines
//      (TDD_SOURCES default fixtures) and asserts every case lands in oracle-error.
//
// Bucket oracle-error (AB-X0001): the JS side failed, not Rust. Kinds: load (oracle dist did not load), js-crash
// (oracle worker aborted/hung on the case), js-missing (no oracle record), non-compiler-error (the oracle threw
// something without the @lwc/errors CompilerError shape). It is the case's only bucket and skips the Rust compare.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { createRequire } from 'node:module';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const HARNESS = path.resolve(HERE, '..');
const LWC = '/workspace/lwc';
const TPL_FIX = `${LWC}/packages/@lwc/template-compiler/src/__tests__/fixtures`;
const API_VERSIONS = [58, 59, 60, 61, 62, 63, 64, 65, 66]; // lwc_shared::api_version::ALL_VERSIONS
// Clamp / snap-down edges of getAPIVersionFromNumber (shared/src/api-version.ts).
const API_EDGES = [0, 57, 60.5, 63.9, 67, 999];
const sha = (s) => crypto.createHash('sha256').update(s).digest('hex');

// ---------------------------------------------------------------- corpus
function walk(dir, pred, out = []) {
    if (!fs.existsSync(dir)) return out;
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
        if (e.name === 'node_modules' || e.name === '.git' || e.name === 'dist') continue;
        const p = path.join(dir, e.name);
        if (e.isDirectory()) walk(p, pred, out);
        else if (pred(p)) out.push(p);
    }
    return out.sort();
}
const isTemplate = (src) => /^\s*(?:<!--[\s\S]*?-->\s*)*<template[\s>]/.test(src);

// Real-world sources (read-only). Order = dedup priority.
const REAL_SOURCES = [
    ['lwc-integration-wtr', `${LWC}/packages/@lwc/integration-wtr`],
    ['lwc-integration-wdio', `${LWC}/packages/@lwc/integration-wdio`],
    ['lwc-ssr-compiler-fixtures', `${LWC}/packages/@lwc/ssr-compiler`],
    ['lwc-other', [`${LWC}/packages/@lwc/perf-benchmarks`, `${LWC}/packages/@lwc/perf-benchmarks-components`,
        `${LWC}/packages/@lwc/rollup-plugin`, `${LWC}/packages/@lwc/engine-server`, `${LWC}/packages/@lwc/module-resolver`,
        `${LWC}/packages/@lwc/integration-types`, `${LWC}/playground`]],
    ['lightning-base-components', '/workspace/style-corpus/lbc'],
    ['lbc-recipes', '/workspace/style-corpus/lbc-recipes'],
];

function templateFixtures() {
    return walk(TPL_FIX, (p) => p.endsWith('/actual.html')).map((f) => {
        const dir = path.dirname(f);
        const cfgPath = path.join(dir, 'config.json');
        const cfg = fs.existsSync(cfgPath) ? JSON.parse(fs.readFileSync(cfgPath, 'utf8')) : {};
        return { rel: path.relative(TPL_FIX, dir), src: fs.readFileSync(f, 'utf8'), cfg };
    });
}

// Start tags and their attributes, with absolute offsets. Expressions with `>` inside quotes are handled by the
// quoted-value alternatives; unquoted `{a > b}` is rare and only makes a mutation miss, never a wrong compare.
const TAG_RE = /<([a-zA-Z][\w:.-]*)((?:\s+(?:[^\s=/>"']+)(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*)\s*(\/?)>/g;
const ATTR_RE = /\s+([^\s=/>"']+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s"'=<>`]+))?/g;
function scanTags(src) {
    const tags = [];
    for (const m of src.matchAll(TAG_RE)) {
        const attrsStart = m.index + 1 + m[1].length;
        const attrs = [];
        for (const a of m[2].matchAll(ATTR_RE)) {
            attrs.push({ name: a[1], value: a[2], start: attrsStart + a.index, end: attrsStart + a.index + a[0].length });
        }
        tags.push({ tag: m[1], start: m.index, nameEnd: attrsStart, attrs });
    }
    return tags;
}
const isDirectiveAttr = (n) => n.includes(':') || ['key', 'slot', 'tabindex', 'is'].includes(n) || /^on[a-z]/.test(n);
const splice = (s, a, b, ins) => s.slice(0, a) + ins + s.slice(b);

const BAD_VALUES = [null, '""', '"x"', '"{x}"', '"{x.y}"', '"{x[0]}"', '"{ x }"', '"{x} y"', '"true"', '"{true}"', '"{x()}"', '{x}'];
const ROOT_INSERTS = [
    'lwc:render-mode="light"', 'lwc:render-mode="shadow"', 'lwc:render-mode="bad"', 'lwc:render-mode="{m}"', 'lwc:render-mode',
    'lwc:preserve-comments', 'lwc:preserve-comments="x"', 'lwc:if={c}', 'class="c"', 'lwc:ref="r"', 'lwc:slot-data="d"',
];
const ELEM_INSERTS = [
    'tabindex="0"', 'tabindex="-1"', 'tabindex="5"', 'tabindex="{t}"', 'tabindex="x"', 'key={k}', 'key="k"',
    'lwc:ref="r"', 'lwc:ref=""', 'lwc:ref={r}', 'lwc:ref', 'lwc:slot-bind={b}', 'lwc:slot-bind="b"', 'lwc:spread={s}',
    'lwc:spread="s"', 'lwc:dom="manual"', 'lwc:dom="bad"', 'lwc:inner-html={h}', 'lwc:inner-html="h"', 'lwc:external',
    'lwc:is={c}', 'lwc:on={o}', 'lwc:slot-data="d"', 'lwc:else', 'lwc:elseif={c}', 'lwc:if={c}', 'lwc:if="c"',
    'for:each={l}', 'for:item="i"', 'for:index="j"', 'iterator:it={l}', 'if:true={c}', 'if:false="c"', 'if:bad={c}',
    'slot="s"', 'slot={s}', 'lwc:dynamic={d}', 'lwc:render-mode="light"', 'lwc:preserve-comments', 'lwc:bad',
    'onclick={h}', 'onclick="h"', 'onClick={h}', 'id="a"', 'id={i}', 'href="#a"', 'class={c}', 'style={s}',
    'data-x={d}', 'aria-x="y"', 'xlink:href="#a"', 'foo:bar="x"', 'Bad-Name="x"',
];
const OPTION_TOGGLES = [
    ['experimentalComplexExpressions', [true, false]], ['preserveHtmlComments', [true, false]],
    ['enableStaticContentOptimization', [true, false]], ['enableDynamicComponents', [true, false]],
    ['experimentalDynamicDirective', [true, false]], ['enableLwcOn', [true, false]],
    ['disableSyntheticShadowSupport', [true, false]],
    ['dynamicImports', [{ loader: 'x/loader', strictSpecifier: true }, { strictSpecifier: false }]],
    ['customRendererConfig', [{ directives: ['lwc:inner-html'], elements: [{ tagName: 'svg', attributes: ['href'] }] }, { directives: [], elements: [] }]],
];

function mutationsFor(fx, cap) {
    const out = []; // {family, label, src, opts}
    const base = fx.cfg;
    const { apiVersion: _pin, ...noApi } = base;
    void _pin;
    for (const v of API_VERSIONS) out.push({ family: 'apiVersion', label: `api=${v}`, src: fx.src, opts: { ...noApi, apiVersion: v } });
    for (const v of API_EDGES) out.push({ family: 'apiVersion-edge', label: `api=${v}`, src: fx.src, opts: { ...noApi, apiVersion: v } });
    out.push({ family: 'apiVersion-edge', label: 'api=omitted', src: fx.src, opts: { ...noApi } });
    for (const [k, vals] of OPTION_TOGGLES) for (const v of vals) {
        out.push({ family: 'options', label: `${k}=${JSON.stringify(v)}`, src: fx.src, opts: { ...base, [k]: v } });
    }
    for (const v of [59, 65, 66]) {
        out.push({ family: 'options', label: `complex@api${v}`, src: fx.src, opts: { ...base, experimentalComplexExpressions: true, apiVersion: v } });
    }
    const tags = scanTags(fx.src);
    const dirAttrs = tags.flatMap((t) => t.attrs.filter((a) => isDirectiveAttr(a.name)).map((a) => ({ t, a })));
    const take = (arr) => arr.slice(0, cap);
    out.push(...take(dirAttrs.flatMap(({ a }) => [
        { family: 'attr-drop', label: `drop ${a.name}@${a.start}`, src: splice(fx.src, a.start, a.end, ''), opts: base },
        { family: 'attr-dup', label: `dup ${a.name}@${a.start}`, src: splice(fx.src, a.end, a.end, fx.src.slice(a.start, a.end)), opts: base },
    ])));
    out.push(...take(dirAttrs.flatMap(({ a }) => BAD_VALUES.filter((v) => v !== a.value).map((v) => ({
        family: 'attr-value', label: `${a.name}=${v ?? '<bool>'}@${a.start}`,
        src: splice(fx.src, a.start, a.end, ` ${a.name}${v == null ? '' : '=' + v}`), opts: base,
    })))));
    const root = tags.find((t) => t.tag === 'template');
    const elem = tags.find((t) => t !== root && t.start > (root?.start ?? -1));
    const ins = [];
    if (root) for (const s of ROOT_INSERTS) ins.push({ family: 'insert-root', label: `root+${s}`, src: splice(fx.src, root.nameEnd, root.nameEnd, ' ' + s), opts: base });
    if (elem) for (const s of ELEM_INSERTS) ins.push({ family: 'insert-elem', label: `${elem.tag}+${s}`, src: splice(fx.src, elem.nameEnd, elem.nameEnd, ' ' + s), opts: base });
    out.push(...ins.filter((m) => m.family === 'insert-root').slice(0, cap), ...ins.filter((m) => m.family === 'insert-elem').slice(0, cap));
    return out;
}

function buildCorpus(want, cap) {
    const on = (k) => !want || want.includes(k);
    const cases = [];
    const sources = {};
    const families = {};
    const seen = new Set();
    const fixtures = templateFixtures();
    if (on('fixtures')) {
        sources['template-fixtures'] = { files: fixtures.length, templates: fixtures.length, unique: 0 };
        for (const fx of fixtures) {
            seen.add(sha(fx.src));
            sources['template-fixtures'].unique++;
            cases.push({ source: 'template-fixtures', family: 'base', file: fx.rel, label: 'config', src: fx.src, opts: fx.cfg });
        }
    }
    for (const [key, roots] of REAL_SOURCES) {
        if (!on(key) && !on('real')) continue;
        const files = [roots].flat().flatMap((r) => walk(r, (p) => p.endsWith('.html')));
        const s = (sources[key] = { files: files.length, templates: 0, unique: 0 });
        for (const f of files) {
            const src = fs.readFileSync(f, 'utf8');
            if (!isTemplate(src)) continue;
            s.templates++;
            const h = sha(src);
            if (seen.has(h)) continue;
            seen.add(h);
            s.unique++;
            cases.push({ source: key, family: 'base', file: f, label: 'default', src, opts: {} });
        }
    }
    if (on('mutations')) {
        const mseen = new Set();
        for (const fx of fixtures) {
            for (const m of mutationsFor(fx, cap)) {
                const h = sha(m.src + '\0' + JSON.stringify(m.opts));
                if (mseen.has(h)) continue;
                mseen.add(h);
                families[m.family] = (families[m.family] ?? 0) + 1;
                cases.push({ source: 'mutation', family: m.family, file: fx.rel, label: m.label, src: m.src, opts: m.opts });
            }
        }
    }
    cases.forEach((c, i) => (c.id = i));
    return { cases, sources, families };
}

// ---------------------------------------------------------------- observation + compare
const nameOf = (file) => path.basename(file, '.html').replace(/[^\w-]/g, '_') || 'x';
const locOf = (l) => (l == null ? null : { line: l.line ?? null, column: l.column ?? null, start: l.start ?? null, length: l.length ?? null });
const diagOf = (d) => ({ code: d.code ?? null, level: d.level ?? null, message: d.message ?? null, location: locOf(d.location) });
// FNV-1a 32 over UTF-8 of banner/whitespace-stripped code (same as the Rust driver) so codegen equality needs no code transfer.
function fnv1a(s) {
    s = s.replace(/\/\*LWC compiler v[\s\S]*?\*\//g, '').replace(/[ \t\n\r\f\v]+/g, '');
    let h = 0x811c9dc5;
    for (const b of Buffer.from(s, 'utf8')) { h ^= b; h = Math.imul(h, 0x01000193) >>> 0; }
    return h >>> 0;
}
const ERROR = 1; // DiagnosticLevel.Error
// transformSync's option forwarding into template-compiler compile() (compiler/src/transformers/template.ts).
function templateConfig(c) {
    const { dynamicImports, experimentalDynamicDirective, ...rest } = c.opts;
    const cfg = { namespace: 'x', name: nameOf(c.file), ...rest };
    cfg.experimentalDynamicDirective = experimentalDynamicDirective ?? Boolean(dynamicImports);
    return cfg;
}

// engine=crate: @lwc/template-compiler compile() vs lwc_template::compile (driver). Full warnings[] at every level.
function observeJsCompile(compile, c) {
    try {
        const r = compile(c.src, `${nameOf(c.file)}.html`, templateConfig(c));
        const diags = r.warnings.map(diagOf);
        return { ok: !diags.some((d) => d.level === ERROR), diags, codeHash: fnv1a(r.code), codeLen: Buffer.byteLength(r.code) };
    } catch (e) {
        if (!isCompilerError(e)) return oracleError('non-compiler-error', e);
        return { ok: false, threw: String(e?.message ?? e), diags: [] };
    }
}
// engine=napi: transformSync seam (recovery mode) on both sides.
function observeJsTransform(lwc, c, opts) {
    try {
        const r = lwc.transformSync(c.src, `${nameOf(c.file)}.html`, opts);
        return { ok: true, diags: (r.warnings ?? []).map(diagOf), codeHash: fnv1a(r.code) };
    } catch (e) {
        if (Array.isArray(e?.errors) && e.errors.every(isCompilerError)) return { ok: false, diags: e.errors.map(diagOf) };
        if (!isCompilerError(e)) return oracleError('non-compiler-error', e);
        return { ok: false, threw: String(e?.message ?? e), diags: [diagOf(e)] };
    }
}
// @lwc/errors CompilerError shape (errors/src/compiler/utils.ts): an Error with numeric code and level. Anything
// else thrown by the oracle is an oracle failure, not an expected diagnostic.
function isCompilerError(e) { return e instanceof Error && Number.isInteger(e.code) && Number.isInteger(e.level); }
function oracleError(kind, e) { return { ok: false, oracle: { kind, msg: String(e?.stack?.split('\n')[0] ?? e?.message ?? e) }, diags: [] }; }
function observeNapi(native, c, opts) {
    let r;
    try { r = native.transform(c.src, `${nameOf(c.file)}.html`, JSON.parse(JSON.stringify(opts))); } catch (e) {
        return { ok: false, seam: String(e?.message ?? e), diags: [] }; // option deserialize reject or surfaced panic
    }
    if (r.error) {
        if (Array.isArray(r.error.errors)) return { ok: false, diags: r.error.errors.map(diagOf) };
        return { ok: false, threw: String(r.error.message), diags: [diagOf(r.error)] };
    }
    return { ok: true, diags: (r.warnings ?? []).map(diagOf), codeHash: fnv1a(r.code) };
}

const PANIC_RE = /panick|panic|RuntimeError|unreachable/i;
function compare(js, rs) {
    if (js.oracle || js.crash) {
        const { kind, msg } = js.oracle ?? { kind: js.crash === 'js-missing' ? 'js-missing' : 'js-crash', msg: js.crash };
        return { buckets: ['oracle-error'], keys: [`oracle-error:${kind}:${String(msg).slice(0, 140)}`] };
    }
    if (rs.crash) return { buckets: ['crash'], keys: [`crash:${rs.crash}`] };
    if (rs.panic != null) return { buckets: ['crash'], keys: [`crash:panic:${rs.panic.slice(0, 140)}`] };
    if (rs.seam != null) {
        const b = PANIC_RE.test(rs.seam) ? 'crash' : 'seam-reject';
        return { buckets: [b], keys: [`${b}:${rs.seam.slice(0, 120)}`] };
    }
    const buckets = new Set();
    const keys = [];
    if (js.threw != null || rs.threw != null) {
        if (js.threw != null && rs.threw != null && js.threw === rs.threw) return { buckets: [], keys: [] };
        if (js.threw == null || rs.threw == null || js.threw !== rs.threw) {
            buckets.add(js.threw == null ? 'extra-diag' : rs.threw == null ? 'missing-diag' : 'message');
            keys.push(`threw:${js.threw == null ? 'rust-only' : rs.threw == null ? 'js-only' : 'differs'}:${String(js.threw ?? rs.threw).slice(0, 100)}`);
            return { buckets: [...buckets], keys };
        }
    }
    const jd = js.diags, rd = rs.diags;
    if (!js.ok && rs.ok) buckets.add('missing-diag');
    if (js.ok && !rs.ok) buckets.add('extra-diag');
    // Pair by code+message, then by code, then by position.
    const usedR = new Set();
    const pairs = [];
    const unJ = [];
    const pick = (pred) => rd.findIndex((x, k) => !usedR.has(k) && pred(x, k));
    jd.forEach((d, i) => {
        let j = pick((x) => x.code === d.code && x.message === d.message);
        if (j < 0) j = pick((x) => x.code === d.code);
        if (j < 0) { unJ.push([d, i]); return; }
        usedR.add(j); pairs.push([d, rd[j]]);
    });
    for (const [d, i] of unJ) {
        if (i < rd.length && !usedR.has(i) && !jd.some((x) => x.code === rd[i].code)) { usedR.add(i); pairs.push([d, rd[i]]); }
        else { buckets.add('missing-diag'); keys.push(`missing-diag:${d.code}`); }
    }
    for (const d of rd.filter((_, k) => !usedR.has(k))) { buckets.add('extra-diag'); keys.push(`extra-diag:${d.code}`); }
    for (const [a, b] of pairs) {
        if (a.code !== b.code) { buckets.add('code'); keys.push(`code:${a.code}->${b.code}`); continue; }
        if (a.level !== b.level) { buckets.add('level'); keys.push(`level:${a.code}:${a.level}->${b.level}`); }
        if (a.message !== b.message) { buckets.add('message'); keys.push(`message:${a.code}`); }
        if (JSON.stringify(a.location) !== JSON.stringify(b.location)) {
            const f = ['line', 'column', 'start', 'length'].filter((k) => a.location?.[k] !== b.location?.[k]);
            buckets.add('location'); keys.push(`location:${a.code}:${a.location == null || b.location == null ? 'null' : f.join('+')}`);
        }
    }
    if (!buckets.size && JSON.stringify(jd) !== JSON.stringify(rd)) { buckets.add('order'); keys.push('order'); }
    if (!buckets.size && js.ok && rs.ok && js.codeHash !== rs.codeHash) buckets.add('codegen-only');
    if (buckets.has('missing-diag') && !keys.some((k) => k.startsWith('missing'))) keys.push('missing-diag:<error-vs-ok>');
    if (buckets.has('extra-diag') && !keys.some((k) => k.startsWith('extra'))) keys.push('extra-diag:<error-vs-ok>');
    return { buckets: [...buckets], keys };
}

// Worker: `--worker <side> casesFile from to outFile napiPath`; side = js | napi (napi = both seam sides).
async function worker(side, casesFile, from, to, outFile, napiPath) {
    const req = createRequire(`${HARNESS}/package.json`);
    const fd = fs.openSync(outFile, 'a');
    const lines = fs.readFileSync(casesFile, 'utf8').split('\n');
    // The oracle module: a load failure is recorded per case as oracle-error:load (not a worker abort).
    let oracle = null, loadErr = null;
    try { oracle = req(process.env.TDD_ORACLE_DIST ?? (side === 'js' ? `${LWC}/packages/@lwc/template-compiler/dist/index.cjs` : '@lwc/compiler')); }
    catch (e) { loadErr = e; }
    const inject = process.env.TDD_ORACLE_INJECT === 'throw'
        ? () => { throw new TypeError('TDD_ORACLE_INJECT: injected non-CompilerError'); } : null;
    const compile = side === 'js' && oracle ? (inject ?? oracle.compile) : null;
    const lwc = side === 'napi' && oracle ? (inject ? { transformSync: inject } : oracle) : null;
    const native = side === 'napi' ? req(napiPath) : null;
    for (let i = from; i < to; i++) {
        const c = JSON.parse(lines[i]);
        fs.writeSync(fd, JSON.stringify({ start: c.id }) + '\n');
        let rec;
        if (loadErr && side === 'js') rec = { id: c.id, ...oracleError('load', loadErr) };
        else if (side === 'js') rec = { id: c.id, ...observeJsCompile(compile, c) };
        else {
            const { experimentalComputedMemberExpression: _cme, ...o } = c.opts; void _cme; // not a TransformOptions key
            const opts = { namespace: 'x', name: nameOf(c.file), experimentalErrorRecoveryMode: true, ...o };
            rec = { id: c.id, js: loadErr ? oracleError('load', loadErr) : observeJsTransform(lwc, c, opts), rust: observeNapi(native, c, opts) };
        }
        fs.writeSync(fd, JSON.stringify(rec) + '\n');
    }
    fs.closeSync(fd);
}

// ---------------------------------------------------------------- main (spawns workers, survives aborts/hangs)
const DRIVER_DIR = path.resolve(HERE, '../../tools/template-diag-diff/driver');
function buildDriver() {
    const target = process.env.TDD_DRIVER_TARGET ?? path.join(os.tmpdir(), 'template-diag-diff-target');
    execFileSync('cargo', ['build', '--release', '-q', '--manifest-path', path.join(DRIVER_DIR, 'Cargo.toml')], {
        stdio: 'inherit', env: { ...process.env, CARGO_TARGET_DIR: target },
    });
    return path.join(target, 'release/template-diag-driver');
}

async function main() {
    const arg = process.argv.slice(2).find((a) => a.startsWith('--engine='));
    const engine = arg ? arg.slice('--engine='.length) : (process.env.TDD_ENGINE ?? 'crate');
    if (!['crate', 'napi', 'both'].includes(engine)) throw new Error(`--engine / TDD_ENGINE must be crate|napi|both, got ${engine}`);
    if (engine !== 'both') return void (await runEngine(engine, process.env.TDD_OUT));
    // both: one report per engine (TDD_OUT is a directory here) plus report-both.json with the board row.
    const dir = path.resolve(process.env.TDD_OUT ?? `${HARNESS}/.template-diag-diff`);
    const row = {};
    for (const e of ['crate', 'napi']) {
        const r = await runEngine(e, path.join(dir, `report-${e}.json`));
        row[e] = { totals: r.totals, buckets: r.buckets, rust: r.meta.rust };
    }
    const both = { schema: 'template-diag-diff-both/v1', generated: new Date().toISOString(), ...row };
    fs.writeFileSync(path.join(dir, 'report-both.json'), JSON.stringify(both, null, 1));
    const cell = (e) => `${e} ${row[e].totals.diagPass}/${row[e].totals.cases} (crash ${row[e].buckets.crash})`;
    console.log(`BOARD template-diag-diff  ${cell('crate')}  ${cell('napi')}  napi ${row.napi.rust.sha256}`);
}

async function runEngine(engine, out) {
    const napiPath = path.resolve(process.env.TDD_NAPI ?? `${HARNESS}/lwc-compiler-napi.node`);
    const outPath = path.resolve(out ?? `${HARNESS}/.template-diag-diff/report-${engine}.json`);
    const jobs = Number(process.env.TDD_JOBS ?? Math.max(1, os.cpus().length - 1));
    const cap = Number(process.env.TDD_CAP ?? 40);
    const hangMs = Number(process.env.TDD_HANG_MS ?? 20000);
    const want = process.env.TDD_SOURCES ? process.env.TDD_SOURCES.split(',') : null;
    const t0 = Date.now();
    const driver = engine === 'crate' ? (process.env.TDD_DRIVER ?? buildDriver()) : null;
    const { cases, sources, families } = buildCorpus(want, cap);
    const work = fs.mkdtempSync(path.join(os.tmpdir(), 'tdd-'));
    const casesFile = path.join(work, 'cases.jsonl');
    fs.writeFileSync(casesFile, cases.map((c) => JSON.stringify({ id: c.id, file: c.file, filename: `${nameOf(c.file)}.html`, src: c.src, opts: c.opts })).join('\n') + '\n');
    // Crate driver reads templateConfig()-shaped opts; rewrite a copy for it.
    let driverCases = casesFile;
    if (driver) {
        driverCases = path.join(work, 'cases-driver.jsonl');
        fs.writeFileSync(driverCases, cases.map((c) => JSON.stringify({ id: c.id, filename: `${nameOf(c.file)}.html`, src: c.src, opts: templateConfig(c) })).join('\n') + '\n');
    }
    const chunk = Math.ceil(cases.length / jobs);
    const ranges = [];
    for (let i = 0; i < cases.length; i += chunk) ranges.push([i, Math.min(cases.length, i + chunk)]);

    // Runs [from,to) of one side in child processes; an abort/hang marks the open case and resumes after it.
    const runSide = (side, results) => {
        const runRange = (from, to, w) => new Promise((resolve) => {
            if (from >= to) return resolve();
            const outFile = path.join(work, `${side}-w${w}-${from}.jsonl`);
            const fd = fs.openSync(outFile, 'a');
            const child = side === 'rust'
                ? spawn(driver, [driverCases, String(from), String(to)], { stdio: ['ignore', fd, 'pipe'] })
                : spawn(process.execPath, [fileURLToPath(import.meta.url), '--worker', side, casesFile, String(from), String(to), outFile, napiPath], { stdio: ['ignore', 'ignore', 'pipe'] });
            let stderr = '';
            child.stderr.on('data', (b) => { stderr = (stderr + b).slice(-4000); });
            let lastSize = -1, lastChange = Date.now();
            const timer = setInterval(() => {
                const sz = fs.statSync(outFile).size;
                if (sz !== lastSize) { lastSize = sz; lastChange = Date.now(); }
                else if (Date.now() - lastChange > hangMs) child.kill('SIGKILL');
            }, 1000);
            child.on('exit', (code, signal) => {
                clearInterval(timer);
                fs.closeSync(fd);
                let last = from - 1, open = null;
                for (const line of fs.readFileSync(outFile, 'utf8').split('\n')) {
                    if (!line) continue;
                    let r; try { r = JSON.parse(line); } catch { continue; }
                    if ('start' in r) { open = r.start; continue; }
                    results[r.id] = r; last = r.id; open = null;
                }
                fs.writeFileSync(outFile, '');
                if (last + 1 >= to) return resolve();
                const bad = open ?? last + 1;
                const why = signal === 'SIGKILL' ? 'hang' : `abort(${code ?? signal})`;
                const tail = (stderr.match(/panicked at[^\n]*\n?[^\n]*/) ?? [stderr.split('\n').filter(Boolean).slice(-1)[0] ?? ''])[0];
                const crash = { id: bad, crash: `${why}:${tail.slice(0, 140)}`, stderr: stderr.slice(-1500), diags: [] };
                results[bad] = side === 'napi' ? { id: bad, js: { ok: true, diags: [] }, rust: crash } : crash;
                stderr = '';
                runRange(bad + 1, to, w).then(resolve);
            });
        });
        return Promise.all(ranges.map(([a, b], w) => runRange(a, b, w)));
    };
    const results = new Array(cases.length);
    if (engine === 'crate') {
        const js = new Array(cases.length), rs = new Array(cases.length);
        await Promise.all([runSide('js', js), runSide('rust', rs)]);
        const norm = (r) => (r.crash || r.panic != null || r.threw != null ? { ...r, diags: [] }
            : { ...r, diags: r.diags.map(diagOf), ok: !r.diags.some((d) => d.level === ERROR) });
        for (let i = 0; i < cases.length; i++) results[i] = { js: js[i] ?? { crash: 'js-missing', diags: [] }, rust: norm(rs[i] ?? { crash: 'missing', diags: [] }) };
    } else {
        const both = new Array(cases.length);
        await runSide('napi', both);
        for (let i = 0; i < cases.length; i++) results[i] = both[i] ?? { js: { ok: true, diags: [] }, rust: { crash: 'missing', diags: [] } };
    }
    fs.rmSync(work, { recursive: true, force: true });
    for (const r of results) Object.assign(r, compare(r.js, r.rust));

    // ------------------------------------------------------------ aggregate
    const slim = (o) => { const { id: _i, codeHash: _h, codeLen: _l, ...rest } = o ?? {}; void _i; void _h; void _l; return rest; };
    const BUCKETS = ['oracle-error', 'crash', 'seam-reject', 'missing-diag', 'extra-diag', 'code', 'level', 'message', 'location', 'order', 'codegen-only'];
    const buckets = Object.fromEntries(BUCKETS.map((b) => [b, 0]));
    const primary = Object.fromEntries(BUCKETS.map((b) => [b, 0]));
    const clusters = new Map();
    const perSource = {};
    let pass = 0, diagPass = 0;
    const mismatches = [];
    for (const c of cases) {
        const r = results[c.id];
        const ps = (perSource[c.source === 'mutation' ? `mutation:${c.family}` : c.source] ??= { cases: 0, pass: 0, diagMismatch: 0, codegenOnly: 0 });
        ps.cases++;
        const bs = r?.buckets ?? ['crash'];
        if (!bs.length) { pass++; diagPass++; ps.pass++; continue; }
        for (const b of bs) buckets[b]++;
        primary[BUCKETS.find((b) => bs.includes(b))]++;
        if (bs.length === 1 && bs[0] === 'codegen-only') { diagPass++; ps.codegenOnly++; continue; }
        ps.diagMismatch++;
        for (const k of new Set(r.keys)) {
            const cl = clusters.get(k) ?? { key: k, count: 0, examples: [] };
            cl.count++;
            if (cl.examples.length < 3 || c.src.length < cl.examples[cl.examples.length - 1].src.length) {
                cl.examples.push({ id: c.id, source: c.source, family: c.family, file: c.file, label: c.label, opts: c.opts, src: c.src, js: slim(r.js), rust: slim(r.rust) });
                cl.examples.sort((a, b) => a.src.length - b.src.length);
                cl.examples.length = Math.min(cl.examples.length, 3);
            }
            clusters.set(k, cl);
        }
        mismatches.push({ id: c.id, source: c.source, family: c.family, file: c.file, label: c.label, opts: c.opts, buckets: bs, keys: r.keys, js: slim(r.js), rust: slim(r.rust) });
    }
    const report = {
        schema: 'template-diag-diff/v1',
        meta: {
            generated: new Date().toISOString(), elapsedMs: Date.now() - t0,
            engine,
            rust: engine === 'crate' ? { driver: path.resolve(driver), driverSource: process.env.TDD_DRIVER ? 'TDD_DRIVER' : 'built', sha256: sha(fs.readFileSync(driver)).slice(0, 12), lwcTemplate: 'lwc_template::compile' } : { napi: napiPath, sha256: sha(fs.readFileSync(napiPath)).slice(0, 12) },
            oracle: { repo: LWC, entry: engine === 'crate' ? '@lwc/template-compiler compile()' : '@lwc/compiler transformSync (experimentalErrorRecoveryMode)' },
            options: { namespace: 'x', cap, sources: want ?? 'all' },
        },
        totals: { cases: cases.length, pass, diagPass, diagMismatch: cases.length - diagPass, codegenOnly: buckets['codegen-only'], oracleError: buckets['oracle-error'] },
        sources, families, perSource, buckets, primary,
        clusters: [...clusters.values()].sort((a, b) => b.count - a.count),
        mismatches,
    };
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, JSON.stringify(report, null, 1));
    // ------------------------------------------------------------ summary
    const L = [];
    L.push(`template-diag-diff  engine ${engine}${engine === 'napi' ? ' ' + report.meta.rust.sha256 : ''}  ${cases.length} cases  ${(report.meta.elapsedMs / 1000).toFixed(1)}s`);
    L.push(`diag-exact ${diagPass}/${cases.length}  (fully identical ${pass}, codegen-only ${buckets['codegen-only']})  crash ${buckets.crash}  oracle-error ${buckets['oracle-error']}`);
    L.push('sources: ' + Object.entries(sources).map(([k, v]) => `${k} ${v.unique}/${v.templates}/${v.files}`).join(', ') + '  (unique/templates/files)');
    L.push('mutation families: ' + Object.entries(families).map(([k, v]) => `${k} ${v}`).join(', '));
    L.push('buckets (cases touching): ' + BUCKETS.map((b) => `${b} ${buckets[b]}`).join(', '));
    L.push('per source:');
    for (const [k, v] of Object.entries(perSource)) L.push(`  ${k.padEnd(34)} ${String(v.cases).padStart(6)}  diag-mismatch ${v.diagMismatch}  codegen-only ${v.codegenOnly}`);
    L.push('top clusters:');
    for (const c of report.clusters.slice(0, 30)) L.push(`  ${String(c.count).padStart(6)}  ${c.key}  e.g. ${c.examples[0].file} [${c.examples[0].label}]`);
    L.push(`report: ${outPath}`);
    console.log(L.join('\n'));
    return report;
}

if (process.argv[2] === '--worker') {
    const [, , , side, casesFile, from, to, outFile, napiPath] = process.argv;
    await worker(side, casesFile, Number(from), Number(to), outFile, napiPath);
} else if (process.argv[2] === '--self-test') {
    await selfTest();
} else {
    await main();
}

// AB-X0001 self-test: a broken oracle (bogus dist, injected non-CompilerError throw) must land every case in
// oracle-error, never in missing-diag, on both engines.
async function selfTest() {
    process.env.TDD_SOURCES ??= 'fixtures';
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'tdd-selftest-'));
    const hooks = [['load', { TDD_ORACLE_DIST: path.join(tmp, 'no-such-dist.cjs') }], ['non-compiler-error', { TDD_ORACLE_INJECT: 'throw' }]];
    let failed = 0;
    for (const engine of ['crate', 'napi']) {
        for (const [kind, env] of hooks) {
            Object.assign(process.env, env);
            const r = await runEngine(engine, path.join(tmp, `${engine}-${kind}.json`));
            for (const k of Object.keys(env)) delete process.env[k];
            const keysOk = r.clusters.every((c) => c.key.startsWith(`oracle-error:${kind}:`));
            const ok = r.totals.cases > 0 && r.totals.oracleError === r.totals.cases && r.buckets['missing-diag'] === 0 && r.totals.diagPass === 0 && keysOk;
            if (!ok) failed++;
            console.log(`SELFTEST ${ok ? 'ok  ' : 'FAIL'} ${engine} ${kind}: oracleError ${r.totals.oracleError}/${r.totals.cases}, missing-diag ${r.buckets['missing-diag']}, clusters ${r.clusters.map((c) => c.key.slice(0, 60)).join(' | ')}`);
        }
    }
    fs.rmSync(tmp, { recursive: true, force: true });
    if (failed) { console.error(`SELFTEST ${failed} failed`); process.exit(1); }
}
