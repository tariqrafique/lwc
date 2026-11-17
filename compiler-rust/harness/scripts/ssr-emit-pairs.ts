// SSR pair emitter (PLAN §3 item 7, AB-D0007 F2).
// One emitter: harness bundleFixture, both backends, modes sync | async | asyncYield.
// Semdiff consumes manifest.json + pairs/*.js. It does not bundle.
//
// case_key = ssr-compiler/<rel>/<sha256(entry+modules)>/<sha256(opts)>
// opts = { targetSSR: true, ssrMode, apiVersion?, experimentalComplexExpressions?, propsHash, styleDedupe }
// propsHash = sha256(canonical JSON of config.props ?? {}). styleDedupe defaults to true.
// runtimeEsm is ssrRenderRuntime(rel): TS source for the three barrel-lwc-exports fixtures, suite runtime otherwise.
//
// Completeness: for each mode, every config.json fixture is a pair or a named exclusion. A drop exits 2.
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { JsCompilerBackend, RustCompilerBackend, type CompilerBackend } from '../src/backend.ts';
import { oracleRoot } from './oracle-root.mjs';
import {
    bundleFixture,
    listFixtures,
    ssrRenderRuntime,
    ssrUpstreamSkip,
    FIXTURES_ROOT,
    type SsrMode,
} from '../src/ssrRender.ts';

const CORPUS = 'ssr-compiler';
const MODES: SsrMode[] = ['sync', 'async', 'asyncYield'];

export function canonicalJson(v: unknown): string {
    if (v === null) return 'null';
    const t = typeof v;
    if (t === 'string' || t === 'number' || t === 'boolean') return JSON.stringify(v);
    if (Array.isArray(v)) return `[${v.map(canonicalJson).join(',')}]`;
    if (t === 'object') {
        const o = v as Record<string, unknown>;
        const keys = Object.keys(o).sort();
        return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(o[k])}`).join(',')}}`;
    }
    throw new Error(`ssr-emit-pairs: non-json value ${t}`);
}

export function sha256(data: string | Buffer): string {
    return createHash('sha256').update(data).digest('hex');
}

type InputFile = { rel: string; abs: string };

export function inputFiles(dirname: string, config: { ssrFiles?: { expected?: string; error?: string } }): InputFile[] {
    const expected = config?.ssrFiles?.expected ?? 'expected.html';
    const error = config?.ssrFiles?.error ?? 'error.txt';
    const out: InputFile[] = [];
    const walk = (dir: string) => {
        for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
            if (e.name === 'dist' || e.name === 'node_modules') continue;
            const abs = path.join(dir, e.name);
            const rel = path.relative(dirname, abs).split(path.sep).join('/');
            if (e.isDirectory()) walk(abs);
            else if (e.isFile()) {
                if (rel === 'config.json' || rel === expected || rel === error || e.name === '.skip' || e.name === '.only') continue;
                out.push({ rel, abs });
            }
        }
    };
    walk(dirname);
    out.sort((a, b) => (a.rel < b.rel ? -1 : a.rel > b.rel ? 1 : 0));
    return out;
}

export function hashEntryModules(entry: string, files: InputFile[]): string {
    const h = createHash('sha256');
    h.update(entry);
    h.update('\0');
    for (const f of files) {
        h.update(f.rel);
        h.update('\0');
        h.update(fs.readFileSync(f.abs));
        h.update('\0');
    }
    return h.digest('hex');
}

export function optsFor(config: Record<string, unknown>, mode: SsrMode): Record<string, unknown> {
    const props = (config.props ?? {}) as unknown;
    const opts: Record<string, unknown> = {
        targetSSR: true,
        ssrMode: mode,
        propsHash: sha256(canonicalJson(props)),
        styleDedupe: config.styleDedupe ?? true,
    };
    if (Object.prototype.hasOwnProperty.call(config, 'apiVersion') && config.apiVersion != null) opts.apiVersion = config.apiVersion;
    if (Object.prototype.hasOwnProperty.call(config, 'experimentalComplexExpressions') && config.experimentalComplexExpressions != null) {
        opts.experimentalComplexExpressions = config.experimentalComplexExpressions;
    }
    return opts;
}

export function caseKey(rel: string, entryHash: string, optsHash: string): string {
    return `${CORPUS}/${rel}/${entryHash}/${optsHash}`;
}

export type Account = { rel: string; kind: 'pair' | 'exclusion' };

/** fixtures == pairs + exclusions, neither overlapping nor duplicated. Returns a problem list (empty = complete). */
export function completenessProblems(fixtures: string[], accounts: Account[]): string[] {
    const problems: string[] = [];
    const pairs = new Set<string>();
    const exclusions = new Set<string>();
    for (const a of accounts) {
        const dest = a.kind === 'pair' ? pairs : exclusions;
        if (dest.has(a.rel)) problems.push(`duplicate ${a.kind} ${a.rel}`);
        dest.add(a.rel);
    }
    const known = new Set(fixtures);
    for (const rel of pairs) if (!known.has(rel)) problems.push(`pair not in fixtures ${rel}`);
    for (const rel of exclusions) if (!known.has(rel)) problems.push(`exclusion not in fixtures ${rel}`);
    for (const rel of fixtures) {
        const p = pairs.has(rel);
        const e = exclusions.has(rel);
        if (p && e) problems.push(`both pair and exclusion ${rel}`);
        if (!p && !e) problems.push(`dropped ${rel}`);
    }
    if (pairs.size + exclusions.size !== fixtures.length && problems.length === 0) {
        problems.push(`count ${pairs.size}+${exclusions.size} != ${fixtures.length}`);
    }
    return problems;
}

type CompileResult = { ok: true; code: string } | { ok: false; empty: boolean; error: string };

function clip(err: unknown): string {
    const msg = err instanceof Error ? err.message : String(err);
    return msg.length > 400 ? msg.slice(0, 400) : msg;
}

async function compileOne(backend: CompilerBackend, dirname: string, config: Record<string, unknown>, mode: SsrMode, runtimeEsm: string): Promise<CompileResult> {
    try {
        const code = await bundleFixture(backend, {
            entry: String(config.entry),
            dirname,
            experimentalComplexExpressions: config.experimentalComplexExpressions as boolean | undefined,
            ssrMode: mode,
            runtimeEsm,
            ...(config.apiVersion != null ? { apiVersion: config.apiVersion as number } : {}),
        });
        if (typeof code !== 'string' || code.length === 0) return { ok: false, empty: true, error: 'empty bundle' };
        return { ok: true, code };
    } catch (err) {
        return { ok: false, empty: false, error: clip(err) };
    }
}

function exclusionReason(js: CompileResult, rust: CompileResult): string {
    const kind = (r: CompileResult) => (r.ok ? 'ok' : r.empty ? 'missing' : 'compile');
    const j = kind(js);
    const r = kind(rust);
    if (j === 'missing' && r === 'missing') return 'missing-bundle-both';
    if (j === 'missing' && r === 'ok') return 'missing-bundle-oracle';
    if (r === 'missing' && j === 'ok') return 'missing-bundle-rust';
    if (j === 'compile' && r === 'compile') return 'compile-error-both';
    if (j === 'compile' && r === 'ok') return 'compile-error-oracle';
    if (r === 'compile' && j === 'ok') return 'compile-error-rust';
    return 'both-error';
}

type PairRec = {
    kind: 'pair';
    rel: string;
    mode: SsrMode;
    id: string;
    oracle: string;
    rust: string;
    meta: Record<string, unknown>;
};
type ExRec = { kind: 'exclusion'; rel: string; mode: SsrMode; reason: string; detail: string };
type Rec = PairRec | ExRec;

function modeIndex(m: string): number {
    return MODES.indexOf(m as SsrMode);
}

async function pool<T, R>(items: T[], jobs: number, fn: (item: T, index: number) => Promise<R>): Promise<R[]> {
    const ret = new Array<R>(items.length);
    let next = 0;
    const n = Math.max(1, Math.min(jobs, items.length || 1));
    await Promise.all(
        Array.from({ length: n }, async () => {
            for (;;) {
                const i = next++;
                if (i >= items.length) return;
                ret[i] = await fn(items[i], i);
            }
        }),
    );
    return ret;
}

function proveDrop(): never {
    const problems = completenessProblems(['a', 'b'], [{ rel: 'a', kind: 'pair' }]);
    if (!problems.some((p) => p === 'dropped b')) {
        console.error(`ssr-emit-pairs: prove-drop checker did not notice the drop: ${problems.join('; ')}`);
        process.exit(1);
    }
    console.error('completeness: dropped fixture b');
    process.exit(2);
}

function selfTest(): void {
    const canon = canonicalJson({ b: 1, a: [true, 'x'] });
    if (canon !== '{"a":[true,"x"],"b":1}') throw new Error(`canonicalJson ${canon}`);
    const quoted = canonicalJson({ b: 'a"b', a: true });
    if (quoted !== '{"a":true,"b":"a\\"b"}') throw new Error(`canonicalJson quote ${quoted}`);
    const ok = completenessProblems(['a', 'b'], [
        { rel: 'a', kind: 'pair' },
        { rel: 'b', kind: 'exclusion' },
    ]);
    if (ok.length) throw new Error(`complete set reported ${ok}`);
    const drop = completenessProblems(['a', 'b'], [{ rel: 'a', kind: 'pair' }]);
    if (!drop.some((p) => p.startsWith('dropped'))) throw new Error('drop not detected');
    const both = completenessProblems(['a'], [
        { rel: 'a', kind: 'pair' },
        { rel: 'a', kind: 'exclusion' },
    ]);
    if (!both.length) throw new Error('overlap not detected');
    const key = caseKey('for-each-block/basic', sha256('e'), sha256('o'));
    if (!key.startsWith('ssr-compiler/for-each-block/basic/')) throw new Error(key);
    console.log('ssr-emit-pairs self-test ok');
}

function arg(name: string, argv: string[]): string | undefined {
    const i = argv.indexOf(name);
    if (i < 0) return undefined;
    return argv[i + 1];
}

function args(argv: string[], name: string): string[] {
    const out: string[] = [];
    for (let i = 0; i < argv.length; i++) if (argv[i] === name && argv[i + 1]) out.push(argv[++i]);
    return out;
}

async function main(): Promise<void> {
    const argv = process.argv.slice(2);
    if (argv.includes('--prove-drop')) proveDrop();
    if (argv.includes('--self-test')) {
        selfTest();
        return;
    }
    const outDir = arg('--out', argv);
    if (!outDir) {
        console.error('usage: ssr-emit-pairs.ts --out DIR [--expect-fixtures N] [--only rel]... [--mode sync|async|asyncYield]... [--jobs N] [--js-only]');
        process.exit(2);
    }
    const expectFixtures = arg('--expect-fixtures', argv);
    const only = new Set(args(argv, '--only'));
    const modeArgs = args(argv, '--mode');
    const modes: SsrMode[] = modeArgs.length ? (modeArgs as SsrMode[]) : [...MODES];
    for (const m of modes) {
        if (!MODES.includes(m)) {
            console.error(`ssr-emit-pairs: bad --mode ${m}`);
            process.exit(2);
        }
    }
    const jobs = Number(arg('--jobs', argv) ?? Math.min(16, os.availableParallelism?.() ?? 8));
    const jsOnly = argv.includes('--js-only');
    // bundleFixture's virtual entry is resolved from cwd. The ssr-render vitest root is the harness
    // directory, whose package.json is the LWC config root the module resolver walks up to.
    process.chdir(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..'));
    const allDirs = listFixtures();
    if (expectFixtures != null && allDirs.length !== Number(expectFixtures)) {
        console.error(`ssr-emit-pairs: fixture count ${allDirs.length} != --expect-fixtures ${expectFixtures}`);
        process.exit(2);
    }
    const dirs = allDirs.filter((d) => {
        if (!only.size) return true;
        return only.has(path.relative(FIXTURES_ROOT, d).split(path.sep).join('/'));
    });
    if (only.size && dirs.length !== only.size) {
        console.error(`ssr-emit-pairs: --only matched ${dirs.length} of ${only.size}`);
        process.exit(2);
    }
    // Absolute oracle dist. A bare package import resolves only when some node_modules
    // is reachable from this file; the land worktree has none.
    const jsCompiler = createRequire(import.meta.url)(`${oracleRoot()}/packages/@lwc/compiler/dist/index.cjs`);
    const jsBackend: CompilerBackend = new JsCompilerBackend(jsCompiler);
    const rustBackend: CompilerBackend | null = jsOnly ? null : new RustCompilerBackend();
    type Job = { dirname: string; rel: string; config: Record<string, unknown>; mode: SsrMode; runtimeEsm: string; runtimeKind: string; entryHash: string };
    const jobsList: Job[] = [];
    const early: Rec[] = [];
    const rels: string[] = [];
    for (const dirname of dirs) {
        const rel = path.relative(FIXTURES_ROOT, dirname).split(path.sep).join('/');
        rels.push(rel);
        let config: Record<string, unknown>;
        try {
            config = JSON.parse(fs.readFileSync(path.join(dirname, 'config.json'), 'utf8'));
        } catch (err) {
            for (const mode of modes) early.push({ kind: 'exclusion', rel, mode, reason: 'bad-config', detail: clip(err) });
            continue;
        }
        if (typeof config.entry !== 'string' || !config.entry) {
            for (const mode of modes) early.push({ kind: 'exclusion', rel, mode, reason: 'bad-config', detail: 'config.entry is not a string' });
            continue;
        }
        if (ssrUpstreamSkip(dirname, rel)) {
            for (const mode of modes) early.push({ kind: 'exclusion', rel, mode, reason: 'upstream-skip', detail: 'upstream .skip still applies' });
            continue;
        }
        const rt = ssrRenderRuntime(rel);
        const files = inputFiles(dirname, config as { ssrFiles?: { expected?: string; error?: string } });
        const entryHash = hashEntryModules(config.entry, files);
        for (const mode of modes) jobsList.push({ dirname, rel, config, mode, runtimeEsm: rt.esm, runtimeKind: rt.kind, entryHash });
    }
    let done = 0;
    const compiled = await pool(jobsList, jobs, async (job) => {
        const js = await compileOne(jsBackend, job.dirname, job.config, job.mode, job.runtimeEsm);
        const rust = rustBackend ? await compileOne(rustBackend, job.dirname, job.config, job.mode, job.runtimeEsm) : js.ok ? { ok: true as const, code: js.code } : js;
        done++;
        if (done % 50 === 0 || done === jobsList.length) console.error(`ssr-emit-pairs: compiled ${done}/${jobsList.length}`);
        const opts = optsFor(job.config, job.mode);
        const id = caseKey(job.rel, job.entryHash, sha256(canonicalJson(opts)));
        if (js.ok && rust.ok) {
            const props = (job.config.props ?? {}) as unknown;
            const meta = {
                id,
                rel: job.rel,
                mode: job.mode,
                props,
                styleDedupe: job.config.styleDedupe ?? true,
                runtimeKind: job.runtimeKind,
            };
            return { kind: 'pair', rel: job.rel, mode: job.mode, id, oracle: js.code, rust: rust.code, meta } as Rec;
        }
        const reason = exclusionReason(js, rust);
        const detail = `oracle: ${js.ok ? 'ok' : js.error} | rust: ${rust.ok ? 'ok' : rust.error}`;
        return { kind: 'exclusion', rel: job.rel, mode: job.mode, reason, detail } as Rec;
    });
    const recs = [...early, ...compiled];
    recs.sort((a, b) => modeIndex(a.mode) - modeIndex(b.mode) || (a.rel < b.rel ? -1 : a.rel > b.rel ? 1 : 0));
    const problems: string[] = [];
    for (const mode of modes) {
        const accounts: Account[] = recs.filter((r) => r.mode === mode).map((r) => ({ rel: r.rel, kind: r.kind }));
        for (const p of completenessProblems(rels, accounts)) problems.push(`${mode}: ${p}`);
    }
    const seen = new Set<string>();
    for (const r of recs) {
        if (r.kind !== 'pair') continue;
        if (seen.has(r.id)) problems.push(`duplicate case_key ${r.id}`);
        seen.add(r.id);
    }
    if (problems.length) {
        console.error(`ssr-emit-pairs: completeness failed (${problems.length})`);
        for (const p of problems.slice(0, 50)) console.error(`  ${p}`);
        process.exit(2);
    }
    const dest = path.resolve(outDir);
    const tmp = `${dest}.tmp-${process.pid}`;
    fs.rmSync(tmp, { recursive: true, force: true });
    fs.mkdirSync(path.join(tmp, 'pairs'), { recursive: true });
    const cases: { id: string; oracle: string; rust: string }[] = [];
    const metaLines: string[] = [];
    const exclusions: ExRec[] = [];
    for (const r of recs) {
        if (r.kind === 'exclusion') {
            exclusions.push(r);
            continue;
        }
        const k = sha256(r.id).slice(0, 24);
        fs.writeFileSync(path.join(tmp, 'pairs', `${k}.oracle.js`), r.oracle);
        fs.writeFileSync(path.join(tmp, 'pairs', `${k}.rust.js`), r.rust);
        cases.push({ id: r.id, oracle: `pairs/${k}.oracle.js`, rust: `pairs/${k}.rust.js` });
        metaLines.push(JSON.stringify(r.meta));
    }
    fs.writeFileSync(path.join(tmp, 'meta.jsonl'), metaLines.join('\n') + (metaLines.length ? '\n' : ''));
    const byReason: Record<string, number> = {};
    for (const e of exclusions) byReason[e.reason] = (byReason[e.reason] ?? 0) + 1;
    fs.writeFileSync(
        path.join(tmp, 'exclusions.json'),
        JSON.stringify({ schema: 'ssr-pair-exclusions/v0', fixtures: rels.length, modes, byReason, cases: exclusions }, null, 1) + '\n',
    );
    const manifest = { schema: 'semdiff-manifest/v0', corpus: CORPUS, cases };
    fs.writeFileSync(path.join(tmp, 'manifest.json.tmp'), JSON.stringify(manifest, null, 1) + '\n');
    fs.renameSync(path.join(tmp, 'manifest.json.tmp'), path.join(tmp, 'manifest.json'));
    fs.rmSync(dest, { recursive: true, force: true });
    fs.renameSync(tmp, dest);
    const perMode = modes.map((m) => {
        const pairs = recs.filter((r) => r.mode === m && r.kind === 'pair').length;
        const ex = recs.filter((r) => r.mode === m && r.kind === 'exclusion').length;
        return `${m}:pairs=${pairs},exclusions=${ex}`;
    });
    console.log(`ssr-emit-pairs corpus=${CORPUS} fixtures=${rels.length} pairs=${cases.length} exclusions=${exclusions.length} ${perMode.join(' ')} reasons=${JSON.stringify(byReason)} -> ${dest}/manifest.json`);
}

const here = path.basename(process.argv[1] ?? '');
if (here === 'ssr-emit-pairs.ts' || here === 'ssr-emit-pairs.mjs') {
    main().catch((err) => {
        console.error(err?.stack ?? err);
        process.exit(2);
    });
}
