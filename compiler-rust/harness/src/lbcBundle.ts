// Whole-component bundles for the `lbc` board row.
// A bundle is `<name>/<name>.js` or `<name>.ts` whose source extends LightningElement
// (coverage-gate-spec). The component's own js + html + css are compiled together.
// Other specifiers are stubs: this row measures that component, not its dependency tree.
import fs from 'node:fs';
import path from 'node:path';
import type { CompilerBackend } from './backend';
import { bundleLwc, type LwcPluginOptions } from './ssrRender';

const SKIP_DIR = new Set(['node_modules', '.git', 'dist', 'coverage', '__pycache__']);
// Parents that are layout directories, not an LWC namespace.
const NOT_NAMESPACE = new Set(['src', 'lwc', 'components', 'modules', 'main', 'default', 'force-app', 'package', 'packages', 'dist']);
const NAMED_CLASS = /export\s+default\s+class\s+([A-Za-z0-9_]+)\s+extends\s+([A-Za-z0-9_.]+)/;
const ANON_CLASS = /export\s+default\s+class\s+extends\s+([A-Za-z0-9_.]+)/;

export interface LbcBundle {
    corpus: 'lbc' | 'lbc-recipes';
    /** Posix path of the component directory relative to the corpus leaf. */
    rel: string;
    dir: string;
    name: string;
    namespace: string;
    js: string;
    html?: string;
    css?: string;
    id: string;
}

interface ClassDecl { className: string | null; base: string }

function classDecl(text: string): ClassDecl | null {
    const named = NAMED_CLASS.exec(text);
    if (named) return { className: named[1], base: named[2] };
    const anon = ANON_CLASS.exec(text);
    if (anon) return { className: null, base: anon[1] };
    return null;
}

/** True when `base` is LightningElement or a class in this corpus that is. */
function extendsLightningElement(base: string, byName: Map<string, string>, seen: Set<string>): boolean {
    const name = base.startsWith('lwc.') ? base.slice(4) : base;
    if (name === 'LightningElement') return true;
    if (seen.has(name)) return false;
    const next = byName.get(name);
    if (!next) return false;
    seen.add(name);
    return extendsLightningElement(next, byName, seen);
}

export function namespaceFor(dir: string): string {
    const parent = path.basename(path.dirname(dir));
    if (/^[a-z][a-z0-9]*$/.test(parent) && !NOT_NAMESPACE.has(parent)) return parent;
    return 'c';
}

interface Candidate {
    corpus: LbcBundle['corpus'];
    leaf: string;
    dir: string;
    decl: ClassDecl;
}

function walkCandidates(corpus: LbcBundle['corpus'], leaf: string, dir: string, out: Candidate[]) {
    const name = path.basename(dir);
    const js = path.join(dir, `${name}.js`);
    const ts = path.join(dir, `${name}.ts`);
    const file = fs.existsSync(js) ? js : fs.existsSync(ts) ? ts : '';
    if (file) {
        try {
            const decl = classDecl(fs.readFileSync(file, 'utf8'));
            if (decl) out.push({ corpus, leaf, dir, decl });
        } catch { /* unreadable file is not a bundle */ }
    }
    let entries: fs.Dirent[];
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
        if (!e.isDirectory() || SKIP_DIR.has(e.name)) continue;
        walkCandidates(corpus, leaf, path.join(dir, e.name), out);
    }
}

function toBundle(c: Candidate): LbcBundle {
    const name = path.basename(c.dir);
    const js = path.join(c.dir, `${name}.js`);
    const ts = path.join(c.dir, `${name}.ts`);
    const file = fs.existsSync(js) ? js : ts;
    const htmlPath = path.join(c.dir, `${name}.html`);
    const cssPath = path.join(c.dir, `${name}.css`);
    const rel = path.relative(c.leaf, c.dir).split(path.sep).join('/');
    return {
        corpus: c.corpus,
        rel,
        dir: c.dir,
        name,
        namespace: namespaceFor(c.dir),
        js: file,
        ...(fs.existsSync(htmlPath) ? { html: htmlPath } : {}),
        ...(fs.existsSync(cssPath) ? { css: cssPath } : {}),
        id: `${c.corpus}/${rel}`,
    };
}

/** Both leaves. Missing leaf throws: a silent empty corpus is not a pass. */
export function discoverLbcBundles(corpusRoot: string): LbcBundle[] {
    const candidates: Candidate[] = [];
    for (const corpus of ['lbc', 'lbc-recipes'] as const) {
        const leaf = path.join(corpusRoot, corpus);
        if (!fs.existsSync(leaf)) throw new Error(`lbc corpus leaf missing: ${leaf}`);
        walkCandidates(corpus, leaf, leaf, candidates);
    }
    // Class-name → base, per corpus. Named classes only: an anonymous default
    // class is not a superclass other files can extend.
    const byCorpus = new Map<string, Map<string, string>>();
    for (const c of candidates) {
        if (!c.decl.className) continue;
        let m = byCorpus.get(c.corpus);
        if (!m) { m = new Map(); byCorpus.set(c.corpus, m); }
        if (!m.has(c.decl.className)) m.set(c.decl.className, c.decl.base);
    }
    const out: LbcBundle[] = [];
    for (const c of candidates) {
        const table = byCorpus.get(c.corpus) ?? new Map<string, string>();
        if (!extendsLightningElement(c.decl.base, table, new Set())) continue;
        out.push(toBundle(c));
    }
    out.sort((a, b) => a.id.localeCompare(b.id));
    const ids = new Set<string>();
    for (const b of out) {
        if (ids.has(b.id)) throw new Error(`duplicate lbc bundle id ${b.id}`);
        ids.add(b.id);
    }
    return out;
}

const VIRTUAL_EXT = new Set(['.js', '.ts', '.mjs', '.html', '.css']);
const SKIP_VIRTUAL_DIR = new Set(['__examples__', '__tests__', '__docs__', '__mocks__', 'node_modules', '.git']);

/** Component directory files, keyed by a posix virtual path under `vdir`. */
function virtualFiles(dir: string, vdir: string): Record<string, string> {
    const files: Record<string, string> = {};
    const walk = (abs: string, relPosix: string) => {
        for (const e of fs.readdirSync(abs, { withFileTypes: true })) {
            if (e.isDirectory()) {
                if (SKIP_VIRTUAL_DIR.has(e.name)) continue;
                walk(path.join(abs, e.name), relPosix ? `${relPosix}/${e.name}` : e.name);
                continue;
            }
            const ext = path.extname(e.name);
            if (!VIRTUAL_EXT.has(ext)) continue;
            const rel = relPosix ? `${relPosix}/${e.name}` : e.name;
            files[`${vdir}/${rel}`] = fs.readFileSync(path.join(abs, e.name), 'utf8');
        }
    };
    walk(dir, '');
    return files;
}

/**
 * Compile one bundle's js + html + css through `bundleLwc`.
 * Throws the rollup/compiler error (caller records compile-error).
 */
export function bundleLbcComponent(backend: CompilerBackend, bundle: LbcBundle, opts: {
    targetSSR: boolean;
    /** Absolute module the bundle's `lwc` external is pinned to. */
    lwcPath: string;
    ssrRuntimePath?: string;
}): Promise<string> {
    const ns = bundle.namespace;
    const name = bundle.name;
    const vdir = `/virtual-lbc/${ns}/${name}`;
    const files = virtualFiles(bundle.dir, vdir);
    const entryFile = `${vdir}/${name}${path.extname(bundle.js)}`;
    if (!(entryFile in files)) files[entryFile] = fs.readFileSync(bundle.js, 'utf8');
    const own = `${ns}/${name}`;
    const stubCmp = (id: string) => {
        const parts = id.split('/').filter(Boolean);
        const comp = parts[parts.length - 1] || 'Stub';
        const safe = comp.replace(/[^\w$]/g, '_') || 'Stub';
        return `import { LightningElement } from 'lwc';\nexport default class ${safe} extends LightningElement {}\n`;
    };
    const virtual = {
        name: 'lbc-virtual',
        resolveId(importee: string, importer?: string) {
            if (!importer && importee !== own) return null;
            if (importee === 'lwc' || importee.startsWith('lwc/') || importee === '@lwc/ssr-runtime' || importee === 'vitest') return null;
            if (importee === own) return entryFile;
            if (importer && (importee.startsWith('.') || importee.startsWith('/'))) {
                const [spec, query] = importee.split('?');
                const base = path.posix.resolve(path.posix.dirname(importer.split('?')[0]), spec);
                const candidates = [base, `${base}.js`, `${base}.ts`, `${base}.mjs`, `${base}.html`, `${base}.css`, `${base}/index.js`];
                for (const c of candidates) {
                    if (c in files) return query ? `${c}?${query}` : c;
                }
                return null;
            }
            if (!importee || importee.startsWith('.') || importee.startsWith('/') || importee.startsWith('\0')) return null;
            if (/^[A-Za-z][\w-]*\/[\w.-]+$/.test(importee)) return `/virtual-lbc-stub/${importee}.js`;
            return `/virtual-lbc-mod/${encodeURIComponent(importee)}.js`;
        },
        load(id: string) {
            const base = id.split('?')[0];
            if (base in files) return files[base];
            if (base.startsWith('/virtual-lbc-stub/')) return stubCmp(base.slice('/virtual-lbc-stub/'.length).replace(/\.js$/, ''));
            if (base.startsWith('/virtual-lbc-mod/')) return 'const stub = {};\nexport default stub;\n';
            return null;
        },
    };
    const runtime = opts.ssrRuntimePath ?? opts.lwcPath;
    const paths: Record<string, string> = opts.targetSSR
        ? { lwc: runtime, '@lwc/ssr-runtime': runtime }
        : { lwc: opts.lwcPath, '@lwc/ssr-runtime': runtime };
    const pluginOptions: LwcPluginOptions = {
        targetSSR: opts.targetSSR,
        ...(opts.targetSSR ? { ssrMode: 'sync' as const } : {}),
        rootDir: vdir,
        enableStaticContentOptimization: true,
    };
    return bundleLwc(backend, {
        entrySource: `export { default } from "${own}";`,
        external: ['lwc', '@lwc/ssr-runtime', 'vitest'],
        paths,
        extraPlugins: [virtual],
        onWarning() { /* compiler warnings are not a crash */ },
        pluginOptions,
        // Stubs are empty components. Named helper imports (`lightning/utilsPrivate`)
        // are not real modules here; rollup supplies `undefined` so the component's
        // own js/html/css still compile. Both backends see the same stubs.
        shimMissingExports: true,
        inlineDynamicImports: true,
    });
}
