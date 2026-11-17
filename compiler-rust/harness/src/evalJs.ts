// Behavioral evaluation of emitted component JS ESM (no Babel-byte comparison).
// The emitted ESM is lowered to a CJS-shaped function body by @babel/plugin-transform-modules-commonjs
// (harness-side evaluation aid only, not part of the backend under test), then run with stubbed imports:
// - `lwc`: LightningElement base class + recording registerComponent / registerDecorators / track / api / wire stubs
// - everything else (templates, css, @salesforce/*, other modules): recording stub proxies
// Engine event smoke (actually rendering via @lwc/engine-dom) is the 0d.2 follow-up.
import { createRequire } from 'node:module';
import { oracleRoot } from './oracleRoot';

const req = createRequire(`${oracleRoot()}/package.json`);
const babel = req('@babel/core');
const cjsPlugin = req('@babel/plugin-transform-modules-commonjs');
const nsFromPlugin = req('@babel/plugin-transform-export-namespace-from');

export interface JsEval {
    imports: string[];
    lwcImports: string[];
    exports: Record<string, string>; // export name -> kind
    defaultExport: null | {
        kind: string;
        extendsLightningElement: boolean;
        registered: boolean;
        registerOptions?: { sel?: string; apiVersion?: string; hasTmpl: boolean; enableSyntheticElementInternals?: boolean; hasFeatureFlag?: boolean; apiVersionValue?: unknown; enablePrivateMethods?: unknown };
    };
    decorators: Array<{ className: string; meta: Record<string, unknown> }>;
}

function kindOf(v: unknown): string {
    if (v === null) return 'null';
    if (typeof v === 'function') return /^class\b/.test(Function.prototype.toString.call(v)) ? 'class' : 'function';
    return typeof v;
}
// Stable, behavior-level view of registerDecorators metadata (shape only, no functions/values).
function shape(v: any, depth = 0): unknown {
    if (v === null || depth > 6) return v === null ? null : '…';
    if (typeof v === 'function') return '[fn]';
    // JSON-stable encodings for values a golden file can't hold
    if (typeof v === 'symbol') return `[symbol ${v.description ?? ''}]`;
    if (v === undefined) return '[undefined]';
    if (typeof v === 'bigint') return `[bigint ${v}]`;
    if (Array.isArray(v)) return v.map((x) => shape(x, depth + 1));
    if (typeof v === 'object') {
        if (v.__stub) return `[import ${v.__stub}]`;
        return Object.fromEntries(Object.keys(v).sort().map((k) => [k, shape(v[k], depth + 1)]));
    }
    return v;
}

// detail: also record option-driven registerComponent values (apiVersion value, enablePrivateMethods key).
// Off by default so the js row's goldens keep their original shape; option rows (js-options) turn it on.
export function evalJsModule(code: string, { detail = false }: { detail?: boolean } = {}): JsEval {
    const { code: cjs } = babel.transformSync(code, {
        babelrc: false, configFile: false, sourceType: 'module',
        plugins: [nsFromPlugin, [cjsPlugin, { importInterop: 'babel' }]],
    });
    const imports: string[] = [];
    const lwcImports = new Set<string>();
    const registered = new Map<unknown, any>();
    const decorators: JsEval['decorators'] = [];
    class LightningElement {}
    const lwc: Record<string, unknown> = {
        LightningElement,
        registerComponent: (C: unknown, opts: any) => (registered.set(C, opts), C),
        registerDecorators: (C: any, meta: unknown) => (decorators.push({ className: C?.name ?? '?', meta: shape(meta) as any }), C),
        registerTemplate: (t: unknown) => t,
        sanitizeAttribute: (_a: unknown, _b: unknown, _c: unknown, v: unknown) => v,
    };
    const stub = (spec: string, key: string): any => {
        const f: any = function () { return stub(spec, key + '()'); };
        return new Proxy(f, {
            get: (_t, k) => (k === '__stub' ? `${spec}#${key}` : k === '__esModule' ? false : k === Symbol.toPrimitive ? () => `${spec}#${key}` : stub(spec, `${key}.${String(k)}`)),
        });
    };
    const lwcNs = new Proxy(lwc, {
        get: (t, k) => {
            if (k === '__esModule') return true;
            if (typeof k === 'string') lwcImports.add(k);
            return k in t ? t[k as string] : (..._a: unknown[]) => undefined;
        },
    });
    const require = (spec: string) => {
        imports.push(spec);
        if (spec === 'lwc') return lwcNs;
        return new Proxy({}, { get: (_t, k) => (k === '__esModule' ? true : stub(spec, String(k))) });
    };
    const module = { exports: {} as Record<string, unknown> };
    // Free identifiers the fixture never declares (e.g. `mixin(LightningElement)`) resolve to an identity stub.
    const PARAMS = new Set(['require', 'module', 'exports', '__globals']);
    // `var` bindings emitted by the CJS lowering also resolve through the `with` scope, so keep a real store.
    const store: Record<string, unknown> = {};
    const globals = new Proxy(store, {
        has: (_t, k) => typeof k === 'string' && !PARAMS.has(k) && !(k in globalThis),
        get: (t, k) => (k === Symbol.unscopables ? undefined : k in t ? t[k as string] : (x: unknown) => x),
        set: (t, k, v) => ((t[k as string] = v), true),
    });
    const body = `with (__globals) {\n${cjs.replace(/^"use strict";/, '')}\n}`;
    new Function('require', 'module', 'exports', '__globals', body)(require, module, module.exports, globals);
    const ex = module.exports;
    const exportsKinds: Record<string, string> = {};
    for (const k of Object.keys(ex).sort()) if (k !== '__esModule') exportsKinds[k] = kindOf(ex[k]);
    const d: any = ex.default;
    let defaultExport: JsEval['defaultExport'] = null;
    if ('default' in ex) {
        const opts = registered.get(d);
        defaultExport = {
            kind: kindOf(d),
            extendsLightningElement: typeof d === 'function' && d.prototype instanceof LightningElement,
            registered: !!opts,
            ...(opts ? { registerOptions: {
                ...(opts.sel !== undefined ? { sel: opts.sel } : {}),
                ...(opts.apiVersion !== undefined ? { apiVersion: typeof opts.apiVersion } : {}),
                hasTmpl: opts.tmpl !== undefined,
                ...(opts.enableSyntheticElementInternals !== undefined ? { enableSyntheticElementInternals: opts.enableSyntheticElementInternals } : {}),
                ...(opts.componentFeatureFlag !== undefined ? { hasFeatureFlag: true } : {}),
                ...(detail && opts.apiVersion !== undefined ? { apiVersionValue: opts.apiVersion } : {}),
                ...(detail && 'enablePrivateMethods' in opts ? { enablePrivateMethods: opts.enablePrivateMethods } : {}),
            } } : {}),
        };
    }
    return { imports: [...new Set(imports)], lwcImports: [...lwcImports].sort(), exports: exportsKinds, defaultExport, decorators };
}
