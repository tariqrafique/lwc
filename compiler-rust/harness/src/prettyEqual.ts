// pretty-equal (D-0292): CSR bundle of the lbc corpora, the three sample apps,
// and six more public corpora, formatted by the pinned Prettier on both sides
// and compared byte for byte. A bundle dir is `<name>/<name>.js` (html/css
// optional) directly under the corpus's lwc folder. Comment placement and
// blank lines before comments stay on the allowlist (item 17). Literal
// spelling does not. Both-sides compile failure is reported separately; a
// one-side failure is a difference (`compile-one-side`).
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { JsCompilerBackend, RustCompilerBackend } from './backend';
import { bundleLbcComponent, discoverBundles, namespaceFor, type LbcBundle } from './lbcBundle';
import { prettierRoot, sampleCorpusRoot, styleCorpusRoot } from './oracleRoot';
import { ENGINE_DOM_ESM } from './csrRender';
import { SSR_RUNTIME_ESM } from './ssrRender';

const PRETTIER_VERSION = '3.9.9';
const ALLOWLIST = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../pretty-equal-allowlist.txt');

export interface PrettyReport {
    text: string;
    failures: string[];
}

interface Hunk {
    classes: string[];
}

function compact(s: string): string {
    let out = '';
    for (let i = 0; i < s.length; i++) {
        const c = s.charCodeAt(i);
        if (c === 32 || c === 9 || c === 10 || c === 13 || c === 12 || c === 11) continue;
        out += s[i];
    }
    return out;
}

const NUM = /(?<![\w$.])(?:0[xX][0-9a-fA-F_]+n?|0[oO][0-7_]+n?|0[bB][01_]+n?|(?:\d[\d_]*\.?[\d_]*|\.\d[\d_]*)(?:[eE][+-]?\d+)?n?)(?![\w$])/g;
const STR = /"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'/g;
const TRAIL = /^(.*?[;,)\]}{(])\s*(\/\/.*|\/\*.*\*\/)\s*$/;
const INLINE = /\/\*(?!#__PURE__|@__PURE__).*?\*\//g;

function isComment(l: string): boolean {
    const s = l.trim();
    return (s.startsWith('//') || s.startsWith('/*') || s.startsWith('*') || s.startsWith('*/')) && !s.startsWith('/*#__PURE__*/');
}

function numval(tok: string): string {
    let t = tok.replace(/_/g, '');
    const big = t.endsWith('n');
    if (big) t = t.slice(0, -1);
    let v: number;
    try {
        if (/^0[xXoObB]/.test(t)) v = Number(BigInt(t));
        else if (/^\d+$/.test(t) && big) v = Number(t);
        else v = Number(t);
    } catch {
        return tok;
    }
    if (!Number.isFinite(v)) return `N<${t}${big ? 'n' : ''}>`;
    if (Number.isInteger(v) && Math.abs(v) < 2 ** 53) v = Math.trunc(v);
    return `N<${v}${big ? 'n' : ''}>`;
}

function strval(tok: string): string {
    const body = tok.slice(1, -1);
    let v = '';
    try {
        v = JSON.parse(`"${body.replace(/"/g, '\\"').replace(/\\'/g, "'")}"`);
    } catch {
        v = body;
    }
    return `S<${v.replace(/\\/g, '')}>`;
}

function normNum(s: string): string {
    return s.replace(NUM, (m) => numval(m));
}
function normStr(s: string): string {
    return s.replace(STR, (m) => strval(m));
}
function shorthand(s: string): string {
    return s.replace(/\b([A-Za-z_$][\w$]*): \1\b(?=\s*[,}])/g, '$1');
}

function splitLine(l: string): [string, string[]] {
    let s = l.trim();
    if (!s) return ['', []];
    if (isComment(l)) return ['', [s]];
    const cm: string[] = [];
    const m = TRAIL.exec(s);
    if (m) {
        s = m[1];
        cm.push(m[2].trim());
    }
    const inl = s.match(INLINE) ?? [];
    if (inl.length) {
        s = s.replace(INLINE, '');
        cm.unshift(...inl);
    }
    return [s, cm];
}

function counter(xs: string[]): Map<string, number> {
    const m = new Map<string, number>();
    for (const x of xs) m.set(x, (m.get(x) ?? 0) + 1);
    return m;
}
function sub(a: Map<string, number>, b: Map<string, number>): string[] {
    const out: string[] = [];
    for (const [k, n] of a) {
        const left = n - (b.get(k) ?? 0);
        for (let i = 0; i < left; i++) out.push(k);
    }
    return out;
}

function classify(rem: string[], add: string[], fileRem: Map<string, number>, fileAdd: Map<string, number>): Set<string> {
    const cls = new Set<string>();
    const rs = rem.map(splitLine);
    const as = add.map(splitLine);
    const rc = rs.flatMap(([, cs]) => cs);
    const ac = as.flatMap(([, cs]) => cs);
    const rcode = rs.map(([c]) => c).filter(Boolean);
    const acode = as.map(([c]) => c).filter(Boolean);
    const rb = rem.filter((l) => !l.trim()).length;
    const ab = add.filter((l) => !l.trim()).length;
    if (rb !== ab) cls.add('blank-line');
    const rcs = counter(rc);
    const acs = counter(ac);
    const gone = sub(rcs, acs);
    const neu = sub(acs, rcs);
    const sameComments = gone.length === 0 && neu.length === 0;
    if (sameComments && rc.length && compact(rcode.join('')) === compact(acode.join(''))
        && JSON.stringify(rem.filter((l) => l.trim()).map(splitLine)) !== JSON.stringify(add.filter((l) => l.trim()).map(splitLine))) {
        cls.add('comment-placement');
    }
    for (const c of gone) {
        if (c.includes('__PURE__')) cls.add('pure-annotation');
        else if (c.includes('@license') || c.startsWith('/*!')) cls.add('license-comment');
        else if (c.includes('sourceMappingURL')) cls.add('sourcemap-url');
        cls.add(fileAdd.has(c) ? 'comment-placement' : 'comment-dropped');
    }
    for (const c of neu) {
        if (c.includes('__PURE__')) cls.add('pure-annotation');
        cls.add(fileRem.has(c) ? 'comment-placement' : 'comment-added');
    }
    if (gone.length && neu.length && compact(gone.join('')) === compact(neu.join(''))) cls.add('comment-text-whitespace');
    if (rcode.length || acode.length) {
        const R = rcode.join('\n');
        const A = acode.join('\n');
        const rPure = R.split('__PURE__').length - 1;
        const aPure = A.split('__PURE__').length - 1;
        if (compact(R) === compact(A)) {
            /* whitespace inside code already covered, or identical */
        } else if (compact(normNum(R)) === compact(normNum(A))) cls.add('numeric-spelling');
        else if (compact(normStr(R)) === compact(normStr(A))) cls.add('string-raw');
        else if (compact(normStr(normNum(R))) === compact(normStr(normNum(A)))) {
            cls.add('numeric-spelling');
            cls.add('string-raw');
        } else if (compact(shorthand(normNum(R))) === compact(shorthand(normNum(A)))) cls.add('object-shorthand');
        else if (rPure !== aPure) cls.add('pure-annotation');
        else if ([...rcode, ...acode].every((l) => l.trimStart().startsWith('import'))) cls.add('import-shape');
        else if (JSON.stringify([...rcode].map(compact).sort()) === JSON.stringify([...acode].map(compact).sort())) cls.add('line-order');
        else if ((!acode.length && rcode.every((l) => /^[\w$"']+: /.test(l.trim())))
            || (!rcode.length && acode.every((l) => /^[\w$"']+: /.test(l.trim())))) cls.add('duplicate-key');
        else if (/\b_?(?:classPrivate|defineProperty|objectWithoutProperties|asyncToGenerator|objectSpread)/.test(R + A)) cls.add('helper-shape');
        else cls.add('other');
    }
    if (cls.size === 0) cls.add('layout');
    return cls;
}

function parseHunks(diff: string): { removed: string[]; added: string[] }[] {
    const hunks: { removed: string[]; added: string[] }[] = [];
    let cur: { removed: string[]; added: string[] } | null = null;
    for (const line of diff.split('\n')) {
        if (line.startsWith('@@')) {
            if (cur) hunks.push(cur);
            cur = { removed: [], added: [] };
            continue;
        }
        if (!cur || line.startsWith('\\')) continue;
        if (line.startsWith('-')) cur.removed.push(line.slice(1));
        else if (line.startsWith('+')) cur.added.push(line.slice(1));
    }
    if (cur) hunks.push(cur);
    return hunks;
}

function diffHunks(a: string, b: string): { removed: string[]; added: string[] }[] {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'pretty-eq-'));
    const af = path.join(dir, 'a.js');
    const bf = path.join(dir, 'b.js');
    fs.writeFileSync(af, a.endsWith('\n') ? a : `${a}\n`);
    fs.writeFileSync(bf, b.endsWith('\n') ? b : `${b}\n`);
    const r = spawnSync('diff', ['-U0', '--', af, bf], { encoding: 'utf8' });
    fs.rmSync(dir, { recursive: true, force: true });
    if (r.status !== 0 && r.status !== 1) {
        throw new Error(`diff failed (${r.status}): ${r.stderr}`);
    }
    if (r.status === 0) return [];
    return parseHunks(r.stdout);
}

function classesOf(a: string, b: string): { hunks: Hunk[]; caseClasses: string[] } {
    const raw = diffHunks(a, b);
    const frc = counter(raw.flatMap((h) => h.removed.flatMap((l) => splitLine(l)[1])));
    const fac = counter(raw.flatMap((h) => h.added.flatMap((l) => splitLine(l)[1])));
    const hunks = raw.map((h) => ({ classes: [...classify(h.removed, h.added, frc, fac)].sort() }));
    const set = new Set<string>();
    for (const h of hunks) for (const c of h.classes) set.add(c);
    return { hunks, caseClasses: [...set].sort() };
}

function readAllowlist(file: string): Map<string, string[]> {
    const text = fs.readFileSync(file, 'utf8');
    const out = new Map<string, string[]>();
    for (const [n, raw] of text.split(/\n/).entries()) {
        const line = raw.trim();
        if (!line || line.startsWith('#')) continue;
        const tab = raw.indexOf('\t');
        if (tab < 0) throw new Error(`${file}:${n + 1}: want case-id<TAB>class[,class]`);
        const id = raw.slice(0, tab).trim();
        const classes = raw.slice(tab + 1).trim().split(',').map((s) => s.trim()).filter(Boolean);
        if (!id || classes.length === 0) throw new Error(`${file}:${n + 1}: empty case id or class`);
        if (out.has(id)) throw new Error(`${file}:${n + 1}: duplicate case ${id}`);
        const sorted = [...classes].sort();
        if (sorted.join(',') !== classes.join(',')) {
            throw new Error(`${file}:${n + 1}: classes must be sorted (${id})`);
        }
        out.set(id, sorted);
    }
    return out;
}

async function mapPool<T>(items: T[], n: number, fn: (item: T) => Promise<void>): Promise<void> {
    let i = 0;
    async function worker(): Promise<void> {
        for (;;) {
            const k = i++;
            if (k >= items.length) return;
            await fn(items[k]);
        }
    }
    await Promise.all(Array.from({ length: Math.min(n, items.length) }, () => worker()));
}

const SKIP_DIR = new Set(['node_modules', '.git', 'mdapi', 'mdapiout']);

function walkDirs(root: string, visit: (abs: string, relPosix: string) => void): void {
    const walk = (abs: string, rel: string) => {
        let entries: fs.Dirent[];
        try { entries = fs.readdirSync(abs, { withFileTypes: true }); } catch { return; }
        for (const e of entries) {
            if (!e.isDirectory() || SKIP_DIR.has(e.name)) continue;
            const childRel = rel ? `${rel}/${e.name}` : e.name;
            const childAbs = path.join(abs, e.name);
            visit(childAbs, childRel);
            walk(childAbs, childRel);
        }
    };
    walk(root, '');
}

/** Direct children of an `lwc` folder that are `<name>/<name>.js|.ts` (html/css optional). */
function bundlesUnder(corpus: string, leaf: string, idBase: string): LbcBundle[] {
    if (!fs.existsSync(leaf)) throw new Error(`pretty-equal leaf missing: ${leaf} (${corpus})`);
    const out: LbcBundle[] = [];
    for (const name of fs.readdirSync(leaf)) {
        const dir = path.join(leaf, name);
        if (!fs.statSync(dir).isDirectory()) continue;
        const js = path.join(dir, `${name}.js`);
        const ts = path.join(dir, `${name}.ts`);
        const file = fs.existsSync(js) ? js : fs.existsSync(ts) ? ts : '';
        const htmlPath = path.join(dir, `${name}.html`);
        const cssPath = path.join(dir, `${name}.css`);
        if (!file && !fs.existsSync(htmlPath) && !fs.existsSync(cssPath)) continue;
        if (!file) continue;
        const rel = idBase ? `${idBase}/${name}` : name;
        out.push({
            corpus,
            rel,
            dir,
            name,
            namespace: namespaceFor(dir),
            js: file,
            ...(fs.existsSync(htmlPath) ? { html: htmlPath } : {}),
            ...(fs.existsSync(cssPath) ? { css: cssPath } : {}),
            id: `${corpus}/${rel}`,
        });
    }
    return out;
}

function extraBundles(samples: string): LbcBundle[] {
    const out: LbcBundle[] = [];
    const one = (corpus: string, repo: string, relLeaf: string) => {
        out.push(...bundlesUnder(corpus, path.join(samples, repo, relLeaf), relLeaf));
    };
    one('b2b-commerce-open-source-components', 'b2b-commerce-open-source-components', 'force-app/main/default/sfdc_cms__lwc');
    one('NPSP', 'NPSP', 'force-app/main/default/lwc');
    one('visualforce-to-lwc', 'visualforce-to-lwc', 'force-app/main/default/lwc');
    const flowRepo = path.join(samples, 'LightningFlowComponents');
    walkDirs(flowRepo, (abs, rel) => {
        if (!/^flow_[^/]+(?:\/.*)?\/force-app\/main\/default\/lwc$/.test(rel)) return;
        out.push(...bundlesUnder('LightningFlowComponents', abs, rel));
    });
    const revenueRepo = path.join(samples, 'revenue-cloud-foundations');
    walkDirs(revenueRepo, (abs, rel) => {
        if (!/^unpackaged\/post_[^/]+\/lwc$/.test(rel)) return;
        out.push(...bundlesUnder('revenue-cloud-foundations', abs, rel));
    });
    for (const top of ['utils-core', 'utils-recipes']) {
        const topAbs = path.join(samples, 'lwc-utils', top);
        walkDirs(topAbs, (abs, rel) => {
            if (rel !== 'lwc' && !rel.endsWith('/lwc')) return;
            out.push(...bundlesUnder('lwc-utils', abs, `${top}/${rel}`));
        });
    }
    return out;
}

export async function runPrettyEqual(): Promise<PrettyReport> {
    const samples = sampleCorpusRoot();
    const prettierDir = prettierRoot();
    const pkg = JSON.parse(fs.readFileSync(path.join(prettierDir, 'package.json'), 'utf8')) as { version?: string };
    if (pkg.version !== PRETTIER_VERSION) {
        throw new Error(`prettier ${pkg.version} != pinned ${PRETTIER_VERSION} (${prettierDir})`);
    }
    const prettier = await import(pathToFileURL(path.join(prettierDir, 'index.mjs')).href) as {
        format(code: string, opts: { parser: string; objectWrap: string }): Promise<string>;
    };
    const style = styleCorpusRoot();
    const bundles = [
        ...discoverBundles([
            { corpus: 'lbc', leaf: path.join(style, 'lbc') },
            { corpus: 'lbc-recipes', leaf: path.join(style, 'lbc-recipes') },
            { corpus: 'lwc-recipes', leaf: path.join(samples, 'lwc-recipes', 'force-app') },
            { corpus: 'ebikes-lwc', leaf: path.join(samples, 'ebikes-lwc', 'force-app') },
            { corpus: 'dreamhouse-lwc', leaf: path.join(samples, 'dreamhouse-lwc', 'force-app') },
        ]),
        ...extraBundles(samples),
    ];
    const seen = new Set<string>();
    for (const b of bundles) {
        if (seen.has(b.id)) throw new Error(`duplicate pretty-equal bundle id ${b.id}`);
        seen.add(b.id);
    }
    const js = new JsCompilerBackend(await import('@lwc/compiler'));
    const rust = new RustCompilerBackend();
    type Row = {
        id: string;
        corpus: string;
        both: 'ok' | 'both-fail' | 'one-fail';
        identical?: boolean;
        classes?: string[];
        hunks?: Hunk[];
        detail?: string;
    };
    const rows: Row[] = new Array(bundles.length);
    await mapPool(bundles.map((b, i) => ({ b, i })), 6, async ({ b, i }) => {
        rows[i] = await one(js, rust, prettier, b);
    });

    // Both-fail is outside the identical ratio. One-side failure is a difference.
    const compared = rows.filter((r) => r.both !== 'both-fail');
    const identical = compared.filter((r) => r.identical);
    const differs = compared.filter((r) => !r.identical);
    const classHunks = new Map<string, number>();
    for (const r of differs) {
        if (r.both === 'one-fail') {
            classHunks.set('compile-one-side', (classHunks.get('compile-one-side') ?? 0) + 1);
            continue;
        }
        for (const h of r.hunks ?? []) {
            for (const c of h.classes) classHunks.set(c, (classHunks.get(c) ?? 0) + 1);
        }
    }
    const byCorpus = new Map<string, { compared: number; same: number; bothFail: number }>();
    for (const r of rows) {
        const slot = byCorpus.get(r.corpus) ?? { compared: 0, same: 0, bothFail: 0 };
        if (r.both === 'both-fail') slot.bothFail++;
        else {
            slot.compared++;
            if (r.identical) slot.same++;
        }
        byCorpus.set(r.corpus, slot);
    }
    const allow = readAllowlist(ALLOWLIST);
    const failures: string[] = [];
    const observed = new Map(differs.map((r) => [r.id, r.classes ?? []]));
    for (const [id, classes] of allow) {
        const got = observed.get(id);
        if (!got) failures.push(`allowlist entry is now identical (remove it): ${id}\t${classes.join(',')}`);
        else if (got.join(',') !== classes.join(',')) {
            failures.push(`allowlist classes stale for ${id}: listed ${classes.join(',')} observed ${got.join(',')}`);
        }
    }
    for (const [id, classes] of observed) {
        if (!allow.has(id)) failures.push(`unlisted case differs: ${id}\t${classes.join(',')}`);
    }
    const classLines = [...classHunks.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    const lines = [
        `pretty-equal: ${identical.length}/${compared.length} identical, allowlist ${allow.size}`,
        ...classLines.map(([c, n]) => `pretty-equal class ${c} ${n}`),
        ...[...byCorpus.entries()].sort((a, b) => a[0].localeCompare(b[0])).map(([c, s]) => {
            const both = s.bothFail ? ` both-fail ${s.bothFail}` : '';
            return `pretty-equal corpus ${c} ${s.same}/${s.compared}${both}`;
        }),
    ];
    const bothFail = rows.filter((r) => r.both === 'both-fail').length;
    if (bothFail) lines.push(`pretty-equal both-fail ${bothFail}`);
    return { text: `${lines.join('\n')}\n`, failures };
}

async function one(
    js: JsCompilerBackend,
    rust: RustCompilerBackend,
    prettier: { format(code: string, opts: { parser: string; objectWrap: string }): Promise<string> },
    b: LbcBundle,
): Promise<{
    id: string; corpus: string; both: 'ok' | 'both-fail' | 'one-fail';
    identical?: boolean; classes?: string[]; hunks?: Hunk[]; detail?: string;
}> {
    const compile = async (be: JsCompilerBackend | RustCompilerBackend) => {
        try {
            return { ok: true as const, code: await bundleLbcComponent(be, b, { targetSSR: false, lwcPath: ENGINE_DOM_ESM, ssrRuntimePath: SSR_RUNTIME_ESM }) };
        } catch (e) {
            const msg = e instanceof Error ? e.message : String(e);
            return { ok: false as const, error: msg.split('\n')[0] };
        }
    };
    const [j, r] = [await compile(js), await compile(rust)];
    if (!j.ok && !r.ok) return { id: b.id, corpus: b.corpus, both: 'both-fail' };
    if (!j.ok || !r.ok) {
        return {
            id: b.id, corpus: b.corpus, both: 'one-fail', identical: false,
            classes: ['compile-one-side'], hunks: [],
            detail: `js=${j.ok ? 'ok' : j.error} rust=${r.ok ? 'ok' : r.error}`,
        };
    }
    const fmt = async (code: string) => prettier.format(code, { parser: 'babel', objectWrap: 'collapse' });
    let pj: string;
    let pr: string;
    try {
        [pj, pr] = [await fmt(j.code), await fmt(r.code)];
    } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        return {
            id: b.id, corpus: b.corpus, both: 'one-fail', identical: false,
            classes: ['compile-one-side'], hunks: [],
            detail: `prettier ${msg.split('\n')[0]}`,
        };
    }
    if (pj === pr) return { id: b.id, corpus: b.corpus, both: 'ok', identical: true, classes: [], hunks: [] };
    const { hunks, caseClasses } = classesOf(pj, pr);
    return { id: b.id, corpus: b.corpus, both: 'ok', identical: false, classes: caseClasses, hunks };
}
