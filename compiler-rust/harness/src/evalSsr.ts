// Behavioral evaluation of SSR-compiled ESM (targetSSR: true). No byte comparison, no HTML render (0d.3).
// ESM is lowered to CJS by @babel/plugin-transform-modules-commonjs (harness-side eval aid only), then run with stubs:
// - `@lwc/ssr-runtime` / `lwc`: LightningElement base + recording setStaticInternals / registerTemplate; other names → no-op fns
// - `*.css` imports → `default: undefined` (no stylesheet); other imports → recording stub proxies
import { createRequire } from 'node:module';

const req = createRequire('/workspace/lwc/package.json');
const babel = req('@babel/core');
const cjsPlugin = req('@babel/plugin-transform-modules-commonjs');
const nsFromPlugin = req('@babel/plugin-transform-export-namespace-from');

export interface SsrEval {
    imports: string[];
    runtimeImports: string[];
    exports: Record<string, string>;
    defaultExport: null | {
        kind: string; // class | function | asyncgeneratorfunction | generatorfunction | asyncfunction | ...
        extendsLightningElement: boolean;
        staticInternals?: { tagName: unknown; publicProps: unknown; ssrMode: unknown; hasTmpl: boolean };
        templateRegistered?: boolean;
        stylesheetScopeToken?: unknown;
        hasScopedStylesheets?: unknown;
    };
}

function kindOf(v: unknown): string {
    if (v === null) return 'null';
    if (typeof v === 'function') {
        if (/^class\b/.test(Function.prototype.toString.call(v))) return 'class';
        const c = (v as any).constructor?.name as string;
        return c === 'Function' ? 'function' : c.toLowerCase();
    }
    return typeof v;
}

export function evalSsrModule(code: string): SsrEval {
    const { code: cjs } = babel.transformSync(code, {
        babelrc: false, configFile: false, sourceType: 'module',
        plugins: [nsFromPlugin, [cjsPlugin, { importInterop: 'babel' }]],
    });
    const imports: string[] = [];
    const runtimeImports = new Set<string>();
    const internals = new Map<unknown, any>();
    const templates = new Set<unknown>();
    class LightningElement {}
    const rt: Record<string, unknown> = {
        LightningElement,
        setStaticInternals: (C: unknown, tagName: unknown, publicProps: unknown, _priv: unknown, ssrMode: unknown, tmpl: unknown) =>
            internals.set(C, { tagName, publicProps, ssrMode, hasTmpl: tmpl !== undefined }),
        registerTemplate: (t: unknown) => (templates.add(t), t),
    };
    const rtNs = new Proxy(rt, {
        get: (t, k) => {
            if (k === '__esModule') return true;
            if (typeof k === 'string') runtimeImports.add(k);
            return k in t ? t[k as string] : (..._a: unknown[]) => undefined;
        },
    });
    const stub = (spec: string, key: string): any => {
        const f: any = function () { return stub(spec, key + '()'); };
        return new Proxy(f, {
            get: (_t, k) => (k === '__stub' ? `${spec}#${key}` : k === '__esModule' ? false : k === Symbol.toPrimitive ? () => `${spec}#${key}` : stub(spec, `${key}.${String(k)}`)),
        });
    };
    const require = (spec: string) => {
        imports.push(spec);
        if (spec === '@lwc/ssr-runtime' || spec === 'lwc') return rtNs;
        if (/\.css(\?.*)?$/.test(spec)) return { __esModule: true, default: undefined };
        return new Proxy({}, { get: (_t, k) => (k === '__esModule' ? true : stub(spec, String(k))) });
    };
    const module = { exports: {} as Record<string, unknown> };
    const PARAMS = new Set(['require', 'module', 'exports', '__globals']);
    const store: Record<string, unknown> = {};
    const globals = new Proxy(store, {
        has: (_t, k) => typeof k === 'string' && !PARAMS.has(k) && !(k in globalThis),
        get: (t, k) => (k === Symbol.unscopables ? undefined : k in t ? t[k as string] : (x: unknown) => x),
        set: (t, k, v) => ((t[k as string] = v), true),
    });
    new Function('require', 'module', 'exports', '__globals', `with (__globals) {\n${cjs.replace(/^"use strict";/, '')}\n}`)(require, module, module.exports, globals);
    const ex = module.exports;
    const exportsKinds: Record<string, string> = {};
    for (const k of Object.keys(ex).sort()) if (k !== '__esModule') exportsKinds[k] = kindOf(ex[k]);
    const d: any = ex.default;
    let defaultExport: SsrEval['defaultExport'] = null;
    if ('default' in ex) {
        const si = internals.get(d);
        defaultExport = {
            kind: kindOf(d),
            extendsLightningElement: typeof d === 'function' && d.prototype instanceof LightningElement,
            ...(si ? { staticInternals: si } : {}),
            ...(typeof d === 'function' && templates.has(d)
                ? { templateRegistered: true, stylesheetScopeToken: d.stylesheetScopeToken ?? null, hasScopedStylesheets: d.hasScopedStylesheets ?? null }
                : {}),
        };
    }
    return { imports: [...new Set(imports)], runtimeImports: [...runtimeImports].sort(), exports: exportsKinds, defaultExport };
}
