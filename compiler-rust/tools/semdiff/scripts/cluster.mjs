#!/usr/bin/env node
// semdiff SD-1B: deterministic clustering of a summary's UNCLASSIFIED cases (notes arch-board/proposals/semdiff-SD-1B-clusters.md).
//
//   node tools/semdiff/scripts/cluster.mjs --summary <summary.json> --manifest <manifest.json> --out <dir> [--banner-whatif <dir>]
//
// Clustering only. It never assigns a verdict: equivalence is semdiff's job (rewrite leg + approved rule table).
// Per case it computes a diff-shape signature from the pair files named by the manifest:
//   - comment features: comments (in source order, as text) present on one side only, digits kept;
//   - token hunks: a Myers diff of the two comment-free token streams (whitespace dropped, which D-0196 makes
//     unobservable; string tokens compared by decoded value, the other D-0196 unobservable). Each hunk becomes
//       hoist-renumber                   the hunk differs only in stcN / $fragmentN numbering,
//       ids:-a,b|+c                      identifiers deleted / inserted by the hunk (not on both sides of it),
//       kinds:-<k..>|+<k..>              otherwise, the token kinds (S string, N number, punctuators and keywords
//                                        verbatim), at most 8 per side.
// Cluster key = semdiff reason + first_diff coords + the sorted unique feature list. Output (all sorted, byte-stable):
//   <out>/clusters.json  [{key, reason, coords, features, count, examples[<=3 ids, smallest first]}] by count desc, key
//   <out>/features.json  [{feature, cases, sole}] by cases desc; sole = cases whose ONLY non-banner feature it is
//   <out>/cases.jsonl    {id, reason, coords, features} per unclassified case, manifest order
// --banner-whatif <dir>: writes <dir>/manifest.json + pairs whose Rust side gets the oracle's LWC version comment
// inserted where the oracle has it (before the closing `}` of `function tmpl`), for a what-if semdiff run. It is a
// measurement aid for the missing-banner cluster, never a verdict.
import fs from 'node:fs';
import path from 'node:path';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => (x.startsWith('--') ? [...a, [x.slice(2), all[i + 1]]] : a), []));
for (const k of ['summary', 'manifest', 'out']) if (!args[k]) { console.error(`cluster.mjs: missing --${k}`); process.exit(2); }

const summary = JSON.parse(fs.readFileSync(args.summary, 'utf8'));
const manifest = JSON.parse(fs.readFileSync(args.manifest, 'utf8'));
const mdir = path.dirname(path.resolve(args.manifest));
const pairOf = new Map(manifest.cases.map((c) => [c.id, c]));
const BANNER = /^\/\*LWC compiler v[\d.]+\*\/$/;

const TOK = /\s+|\/\*[\s\S]*?\*\/|\/\/[^\n]*|"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`(?:[^`\\]|\\.)*`|[A-Za-z_$][\w$]*|\d[\w.]*|\.\.\.|===|!==|=>|\?\.|\?\?|&&|\|\||[{}()[\];,.:?=+\-*/%!<>&|^~]|./g;
const KEYWORDS = new Set(['true', 'false', 'null', 'undefined', 'const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'of', 'in', 'new', 'import', 'export', 'default', 'from', 'this', 'typeof', 'void', 'delete']);
const strValue = (t) => {
    try {
        if (t[0] === '"') return JSON.parse(t);
        if (t[0] === "'") return JSON.parse('"' + t.slice(1, -1).replace(/\\'/g, "'").replace(/"/g, '\\"') + '"');
    } catch { /* fall back to the exact spelling: fail closed */ }
    return t;
};
function lex(src) {
    const comments = [], toks = [];
    for (const t of src.match(TOK) || []) {
        if (/^\s+$/.test(t)) continue;
        if (t.startsWith('/*') || t.startsWith('//')) comments.push(t);
        else toks.push(/^["']/.test(t) ? `S${JSON.stringify(strValue(t))}` : t);
    }
    return { comments, toks };
}
const kind = (t) => (t[0] === 'S' && t[1] === '"' ? 'S' : t[0] === '`' ? 'T' : /^\d/.test(t) ? 'N' : /^[A-Za-z_$]/.test(t) && !KEYWORDS.has(t) ? 'I' : t);
const renum = (t) => t.replace(/^(stc|\$fragment)\d+$/, '$1#');

// Myers O(ND) diff -> hunks of {del[], ins[]}. Deterministic (fixed tie-break: prefer deletion).
function hunks(a, b) {
    const n = a.length, m = b.length, off = n + m + 1, v = new Int32Array(2 * off + 2), trace = [];
    let done = false;
    for (let d = 0; d <= n + m && !done; d++) {
        trace.push(v.slice());
        for (let k = -d; k <= d; k += 2) {
            let x = k === -d || (k !== d && v[off + k - 1] < v[off + k + 1]) ? v[off + k + 1] : v[off + k - 1] + 1;
            let y = x - k;
            while (x < n && y < m && a[x] === b[y]) { x++; y++; }
            v[off + k] = x;
            if (x >= n && y >= m) { done = true; break; }
        }
    }
    const ops = [];
    let x = n, y = m;
    for (let d = trace.length - 1; d >= 0; d--) {
        const vv = trace[d], k = x - y;
        const pk = k === -d || (k !== d && vv[off + k - 1] < vv[off + k + 1]) ? k + 1 : k - 1;
        const px = vv[off + pk], py = px - pk;
        while (x > px && y > py) { x--; y--; ops.push(0); }
        if (d > 0) { if (x === px) { y--; ops.push([1, b[y]]); } else { x--; ops.push([-1, a[x]]); } }
        x = px; y = py;
    }
    ops.reverse();
    const out = [];
    let cur = null;
    for (const op of ops) {
        if (op === 0) { if (cur) out.push(cur); cur = null; continue; }
        cur ??= { del: [], ins: [] };
        (op[0] < 0 ? cur.del : cur.ins).push(op[1]);
    }
    if (cur) out.push(cur);
    return out;
}
function hunkShape({ del, ins }) {
    const d = del.map(renum), i = ins.map(renum);
    if (d.join(' ') === i.join(' ')) return 'hoist-renumber';
    const ids = (xs) => new Set(xs.filter((t) => kind(t) === 'I'));
    const di = ids(d), ii = ids(i);
    const minus = [...di].filter((t) => !ii.has(t)).sort(), plus = [...ii].filter((t) => !di.has(t)).sort();
    if (minus.length || plus.length) return `ids:-${minus.join(',')}|+${plus.join(',')}`;
    return `kinds:-${d.slice(0, 8).map(kind).join(' ')}|+${i.slice(0, 8).map(kind).join(' ')}`;
}
function commentFeatures(oc, rc) {
    const norm = (c) => (BANNER.test(c) ? 'LWC-version-banner' : c.slice(0, 60));
    const count = (xs) => xs.reduce((m, c) => m.set(norm(c), (m.get(norm(c)) || 0) + 1), new Map());
    const o = count(oc), r = count(rc), f = [];
    for (const [c, n] of o) if (n > (r.get(c) || 0)) f.push(`comment-missing-rust:${c}`);
    for (const [c, n] of r) if (n > (o.get(c) || 0)) f.push(`comment-extra-rust:${c}`);
    if (!f.length && oc.join('\n') !== rc.join('\n')) f.push('comment-order');
    return f;
}

fs.mkdirSync(args.out, { recursive: true });
const clusters = new Map(), feats = new Map(), lines = [];
const unclassified = summary.cases.filter((c) => c.bucket === 'unclassified');
for (const c of unclassified) {
    const p = pairOf.get(c.id);
    if (!p) { console.error(`cluster.mjs: ${c.id} not in manifest`); process.exit(2); }
    const o = lex(fs.readFileSync(path.join(mdir, p.oracle), 'utf8'));
    const r = lex(fs.readFileSync(path.join(mdir, p.rust), 'utf8'));
    const features = [...new Set([...commentFeatures(o.comments, r.comments), ...hunks(o.toks, r.toks).map(hunkShape)])].sort();
    const reason = c.verdict.reason, coords = c.verdict.first_diff?.coords ?? '-';
    const key = `${reason}|${coords}|${features.join(' && ')}`;
    const cl = clusters.get(key) ?? { key, reason, coords, features, count: 0, examples: [] };
    cl.count++; cl.examples.push(c.id); clusters.set(key, cl);
    const rest = features.filter((f) => f !== 'comment-missing-rust:LWC-version-banner');
    for (const f of features) {
        const e = feats.get(f) ?? { feature: f, cases: 0, sole: 0 };
        e.cases++; if (rest.length === 1 && rest[0] === f) e.sole++; feats.set(f, e);
    }
    lines.push(JSON.stringify({ id: c.id, reason, coords, features }));
}
const byCount = (a, b) => b.count - a.count || (a.key < b.key ? -1 : 1);
const out = [...clusters.values()].sort(byCount).map((c) => ({ ...c, examples: c.examples.sort().slice(0, 3) }));
fs.writeFileSync(path.join(args.out, 'clusters.json'), JSON.stringify(out, null, 1) + '\n');
fs.writeFileSync(path.join(args.out, 'features.json'), JSON.stringify([...feats.values()].sort((a, b) => b.cases - a.cases || (a.feature < b.feature ? -1 : 1)), null, 1) + '\n');
fs.writeFileSync(path.join(args.out, 'cases.jsonl'), lines.join('\n') + '\n');
console.log(`cluster.mjs: ${unclassified.length} unclassified of ${summary.cases.length} cases -> ${out.length} clusters, ${feats.size} features (${args.out})`);

if (args['banner-whatif']) {
    const wd = args['banner-whatif'];
    fs.mkdirSync(path.join(wd, 'pairs'), { recursive: true });
    const cases = [];
    let patched = 0;
    for (const c of manifest.cases) {
        const o = fs.readFileSync(path.join(mdir, c.oracle), 'utf8');
        let r = fs.readFileSync(path.join(mdir, c.rust), 'utf8');
        const banner = o.match(/\n( *)(\/\*LWC compiler v[\d.]+\*\/)\n\}/);
        const fn = r.indexOf('function tmpl(');
        const close = fn < 0 ? -1 : r.indexOf('\n}', fn);
        if (banner && close >= 0 && !/\/\*LWC compiler v/.test(r)) { r = `${r.slice(0, close)}\n${banner[1]}${banner[2]}${r.slice(close)}`; patched++; }
        const base = path.basename(c.rust);
        fs.copyFileSync(path.join(mdir, c.oracle), path.join(wd, 'pairs', path.basename(c.oracle)));
        fs.writeFileSync(path.join(wd, 'pairs', base), r);
        cases.push({ id: c.id, oracle: `pairs/${path.basename(c.oracle)}`, rust: `pairs/${base}` });
    }
    fs.writeFileSync(path.join(wd, 'manifest.json'), JSON.stringify({ schema: manifest.schema, corpus: manifest.corpus, cases }) + '\n');
    console.log(`cluster.mjs: banner what-if: patched ${patched}/${manifest.cases.length} Rust files (${wd})`);
}
