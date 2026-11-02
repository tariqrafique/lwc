// CSR DOM render-parity helpers (used by test/csr.render.test.ts). See CSR_RENDER.md.
//
// Bundling reuses src/ssrRender.ts (`bundleLwc`, the @lwc/rollup-plugin port routed through the CompilerBackend);
// this file only adds: the CSR externals, the template-fixture virtual component + child-stub plugins, the
// template-suite option seam, and a deterministic DOM serializer that inlines shadow roots.
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import type { CompilerBackend, CompileOptions } from './backend';
import { LWC, SSR_RUNTIME_ESM, bundleLwc, fixturePluginOptions } from './ssrRender';

export const ENGINE_DOM_ESM = `${LWC}/engine-dom/dist/index.js`;
export const TEMPLATE_FIXTURES_ROOT = `${LWC}/template-compiler/src/__tests__/fixtures`;
const { isVoidElement, HTML_NAMESPACE } = createRequire(`${LWC}/rollup-plugin/package.json`)('@lwc/shared');

/** `lwc` → the one engine-dom instance the test renders with. `@lwc/ssr-runtime` (one barrel fixture imports it). */
const CSR_EXTERNAL = ['lwc', '@lwc/ssr-runtime', 'vitest'];
const CSR_PATHS = { lwc: ENGINE_DOM_ESM, '@lwc/ssr-runtime': SSR_RUNTIME_ESM };

/** Corpus A: an ssr-compiler fixture bundled exactly like upstream fixtures.spec.ts, but `targetSSR: false`. */
export function bundleSsrFixtureCsr(backend: CompilerBackend, opts: { entry: string; dirname: string; experimentalComplexExpressions?: boolean }) {
    return bundleLwc(backend, {
        entrySource: `export { default } from "${opts.entry}";`,
        external: CSR_EXTERNAL,
        paths: CSR_PATHS,
        pluginOptions: fixturePluginOptions({ ...opts, targetSSR: false }),
    });
}

// Real @lwc/compiler TransformOptions keys (dist/options.d.ts @ b9e8007). The rust seam rejects anything else.
const TRANSFORM_OPTION_KEYS = new Set([
    'name', 'namespace', 'stylesheetConfig', 'dynamicImports', 'experimentalDynamicDirective', 'enableDynamicComponents',
    'enableSyntheticElementInternals', 'experimentalComplexExpressions', 'outputConfig', 'isExplicitImport',
    'preserveHtmlComments', 'scopedStyles', 'enableStaticContentOptimization', 'customRendererConfig', 'enableLwcOn',
    'disableSyntheticShadowSupport', 'enableLightningWebSecurityTransforms', 'instrumentation', 'apiVersion', 'targetSSR',
    'ssrMode', 'experimentalErrorRecoveryMode', 'componentFeatureFlagModulePath', 'enablePrivateMethods',
]);

export interface TemplateFixture { rel: string; name: string; src: string; cfg: Record<string, unknown>; droppedKeys: string[]; light: boolean }

export function listTemplateFixtures(root = TEMPLATE_FIXTURES_ROOT): string[] {
    const out: string[] = [];
    const walk = (d: string) => {
        for (const e of fs.readdirSync(d, { withFileTypes: true })) {
            const p = path.join(d, e.name);
            if (e.isDirectory()) walk(p);
            else if (e.name === 'actual.html') out.push(d);
        }
    };
    walk(root);
    return out.sort();
}

export function readTemplateFixture(dir: string, root = TEMPLATE_FIXTURES_ROOT): TemplateFixture {
    const cfgPath = path.join(dir, 'config.json');
    const raw = fs.existsSync(cfgPath) ? JSON.parse(fs.readFileSync(cfgPath, 'utf8')) : {};
    const cfg: Record<string, unknown> = {};
    const droppedKeys: string[] = [];
    for (const [k, v] of Object.entries(raw)) (TRANSFORM_OPTION_KEYS.has(k) ? ((cfg[k] = v), 0) : droppedKeys.push(k));
    const src = fs.readFileSync(path.join(dir, 'actual.html'), 'utf8');
    // Pair light-DOM root templates with a light-DOM component; otherwise the engine rejects the pairing outright.
    const light = /^\s*(?:<!--[\s\S]*?-->\s*)*<template\b[^>]*\blwc:render-mode\s*=\s*["']?light\b/.test(src);
    return { rel: path.relative(root, dir), name: path.basename(dir), src, cfg, droppedKeys, light };
}

const VROOT = '/virtual-csr';
const STUB_ROOT = '/virtual-csr-stub';

/**
 * Corpus B: `x/<name>` = minimal LightningElement + the fixture template. The template is compiled with exactly the
 * template.behavior suite's call: `transform(src, '<name>.html', { namespace: 'x', name, ...config })`; the component
 * .js and stubs go through the plugin's normal options. Unknown `x/child` imports become empty stub components.
 */
export function bundleTemplateFixtureCsr(backend: CompilerBackend, fx: TemplateFixture, onWarning: (message: string) => void) {
    const dir = `${VROOT}/x/${fx.name}`;
    const files: Record<string, string> = {
        [`${dir}/${fx.name}.js`]: `import { LightningElement } from 'lwc';\nexport default class extends LightningElement {${fx.light ? "\n    static renderMode = 'light';" : ''}\n}\n`,
        [`${dir}/${fx.name}.html`]: fx.src,
    };
    const htmlId = `${dir}/${fx.name}.html`;
    const seam: CompilerBackend = {
        id: backend.id,
        transform(src: string, filename: string, options: CompileOptions) {
            if (filename === htmlId) return backend.transform(src, `${fx.name}.html`, { namespace: 'x', name: fx.name, ...fx.cfg } as CompileOptions);
            return backend.transform(src, filename, options);
        },
    };
    const virtualFiles = {
        name: 'csr-virtual-component',
        resolveId(importee: string, importer?: string) {
            if (importee in files) return importee;
            if (importer && importer.startsWith(VROOT) && importee.startsWith('.')) {
                const p = path.posix.resolve(path.posix.dirname(importer.split('?')[0]), importee);
                if (p in files) return p;
            }
        },
        load: (id: string) => files[id],
    };
    const childStubs = {
        name: 'csr-child-stubs',
        resolveId(importee: string) {
            const m = /^([a-z][\w]*)\/([\w-]+)$/i.exec(importee);
            if (m) return `${STUB_ROOT}/${m[1]}/${m[2]}/${m[2]}.js`;
        },
        load(id: string) {
            if (id.startsWith(STUB_ROOT)) return `import { LightningElement } from 'lwc';\nexport default class extends LightningElement {}\n`;
        },
    };
    const apiVersion = typeof fx.cfg.apiVersion === 'number' ? fx.cfg.apiVersion : undefined;
    return bundleLwc(seam, {
        entrySource: `export { default } from "x/${fx.name}";`,
        external: CSR_EXTERNAL,
        paths: CSR_PATHS,
        // Stubs resolve before the LWC plugin: its module resolver throws for `ns/name` with no lwc config on disk.
        extraPlugins: [virtualFiles, { name: 'csr-entry', resolveId: (id: string) => (id === `x/${fx.name}` ? `${dir}/${fx.name}.js` : undefined) }, childStubs],
        onWarning,
        pluginOptions: { targetSSR: false, apiVersion },
    });
}

// ── Deterministic DOM serializer ────────────────────────────────────────────────────────────────────────────────
// HTML-fragment serialization (WHATWG "serializing HTML fragments" escaping), attributes in DOM order, and every
// shadow root (open or closed: captured by `trackShadowRoots`) inlined as `<template shadowrootmode=...>` first child.
const RAW_TEXT = new Set(['style', 'script', 'xmp', 'iframe', 'noembed', 'noframes', 'plaintext', 'noscript']);
const escText = (s: string) => s.replace(/&/g, '&amp;').replace(/\u00a0/g, '&nbsp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const escAttr = (s: string) => s.replace(/&/g, '&amp;').replace(/\u00a0/g, '&nbsp;').replace(/"/g, '&quot;');

// jsdom 29 has no ShadowRoot#delegatesFocus, so it is taken from the attachShadow init instead.
const shadowRoots = new WeakMap<Element, ShadowRoot>();
const delegatesFocus = new WeakSet<ShadowRoot>();
let tracking = false;
export function trackShadowRoots(): void {
    if (tracking) return;
    tracking = true;
    const orig = Element.prototype.attachShadow;
    Element.prototype.attachShadow = function (this: Element, init: ShadowRootInit) {
        const sr = orig.call(this, init);
        shadowRoots.set(this, sr);
        if (init?.delegatesFocus) delegatesFocus.add(sr);
        return sr;
    };
}

function sheetText(sheet: CSSStyleSheet): string {
    try { return Array.from(sheet.cssRules, (r) => r.cssText).join('\n'); } catch { return '<unreadable>'; }
}

// Fixtures may shadow DOM accessors on the host (e.g. `@api attributes`, `@api shadowRoot`, `@api children`), so the
// serializer reads everything through the native prototype getters, never through the instance.
function nativeGetter(proto: object, key: string): (this: unknown) => any {
    for (let p: any = proto; p; p = Object.getPrototypeOf(p)) {
        const d = Object.getOwnPropertyDescriptor(p, key);
        if (d?.get) return d.get;
    }
    throw new Error(`no native getter ${key}`);
}
let G: Record<string, (this: unknown) => any> | undefined;
function g(): Record<string, (this: unknown) => any> {
    return (G ??= {
        nodeType: nativeGetter(Node.prototype, 'nodeType'),
        childNodes: nativeGetter(Node.prototype, 'childNodes'),
        parentNode: nativeGetter(Node.prototype, 'parentNode'),
        data: nativeGetter(CharacterData.prototype, 'data'),
        attributes: nativeGetter(Element.prototype, 'attributes'),
        localName: nativeGetter(Element.prototype, 'localName'),
        namespaceURI: nativeGetter(Element.prototype, 'namespaceURI'),
        tagName: nativeGetter(Element.prototype, 'tagName'),
        shadowRoot: nativeGetter(Element.prototype, 'shadowRoot'),
        content: nativeGetter(HTMLTemplateElement.prototype, 'content'),
        attrName: nativeGetter(Attr.prototype, 'name'),
        attrValue: nativeGetter(Attr.prototype, 'value'),
        mode: nativeGetter(ShadowRoot.prototype, 'mode'),
    });
}
const get = (n: unknown, k: string) => g()[k].call(n);

export function serializeNode(node: Node): string {
    switch (get(node, 'nodeType')) {
        case 3: {
            const p = get(node, 'parentNode') as Element | null;
            const raw = p && get(p, 'nodeType') === 1 && get(p, 'namespaceURI') === HTML_NAMESPACE && RAW_TEXT.has(get(p, 'localName'));
            const data = get(node, 'data') as string;
            return raw ? data : escText(data);
        }
        case 8: return `<!--${get(node, 'data')}-->`;
        case 11: return serializeChildren(node);
        case 1: {
            const el = node as Element;
            const html = get(el, 'namespaceURI') === HTML_NAMESPACE;
            const localName = get(el, 'localName') as string;
            const tag = html ? localName : (get(el, 'tagName') as string);
            let s = `<${tag}`;
            for (const a of Array.from(get(el, 'attributes') as NamedNodeMap)) s += ` ${get(a, 'attrName')}="${escAttr(get(a, 'attrValue'))}"`;
            s += '>';
            const sr = shadowRoots.get(el) ?? (get(el, 'shadowRoot') as ShadowRoot | null);
            if (sr) {
                s += `<template shadowrootmode="${get(sr, 'mode')}"${delegatesFocus.has(sr) ? ' shadowrootdelegatesfocus' : ''}>`;
                const adopted = (sr as any).adoptedStyleSheets as CSSStyleSheet[] | undefined;
                if (adopted?.length) s += adopted.map((sh) => `<!--adoptedStyleSheet:${sheetText(sh)}-->`).join('');
                s += serializeChildren(sr) + '</template>';
            }
            if (html && isVoidElement(localName, HTML_NAMESPACE)) return s;
            const kids = html && localName === 'template' ? (get(el, 'content') as DocumentFragment) : el;
            return s + serializeChildren(kids) + `</${tag}>`;
        }
        default: return '';
    }
}
const serializeChildren = (n: Node) => Array.from(get(n, 'childNodes') as NodeList, serializeNode).join('');

/** Stable text for a thrown value / console argument. Error stacks are dropped (they carry bundle paths). */
export function describeValue(v: unknown): string {
    if (v instanceof Error || (v && typeof v === 'object' && 'message' in (v as any) && 'name' in (v as any))) return `${(v as any).name}: ${(v as any).message}`;
    if (typeof v === 'string') return v;
    try { return typeof v === 'object' ? JSON.stringify(v) ?? String(v) : String(v); } catch { return String(v); }
}
