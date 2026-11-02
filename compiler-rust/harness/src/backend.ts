// Mirrors Rust `lwc_compiler::CompilerBackend` (crates/lwc-compiler/src/lib.rs).
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export interface CompileOptions {
    name: string;
    namespace: string;
    scopedStyles?: boolean;
    disableSyntheticShadowSupport?: boolean;
    apiVersion?: number;
    targetSSR?: boolean;
    ssrMode?: 'sync' | 'async' | 'asyncYield';
    experimentalErrorRecoveryMode?: boolean;
    enablePrivateMethods?: boolean;
    enableLightningWebSecurityTransforms?: boolean;
    isExplicitImport?: boolean;
    outputConfig?: { sourcemap?: boolean | 'inline'; minify?: unknown };
    stylesheetConfig?: { customProperties?: { resolution?: { type: 'native' } | { type: 'module'; name: string } } };
    customRendererConfig?: { directives: string[]; elements: { tagName: string; namespace?: string; attributes?: string[] }[] };
    [extra: string]: unknown;
}
export interface Diagnostic { code: number; level: number; message?: string }
export interface CompileResult { code: string; map?: string | null; warnings?: Diagnostic[]; cssScopeTokens?: string[] }
export interface CompilerBackend { readonly id: string; transform(src: string, filename: string, options: CompileOptions): CompileResult }

export class JsCompilerBackend implements CompilerBackend {
    readonly id = 'js';
    constructor(private readonly lwc: typeof import('@lwc/compiler')) {}
    transform(src: string, filename: string, options: CompileOptions): CompileResult {
        const { code, map, warnings, cssScopeTokens } = this.lwc.transformSync(src, filename, options as any) as any;
        const m: any = map;
        const norm = m == null ? m : typeof m === 'string' ? m : m.mappings === '' ? '' : JSON.stringify(m);
        return { code, map: norm, warnings, cssScopeTokens };
    }
}

type NativeAddon = {
    transform(src: string, filename: string, options: Record<string, unknown>): {
        code?: string | null;
        map?: string | null;
        cssScopeTokens?: string[];
        warnings?: Diagnostic[] | null;
        error?: {
            name?: string | null;
            message: string;
            code?: number | null;
            line?: number | null;
            column?: number | null;
        } | null;
    };
};

function loadNative(): NativeAddon {
    const req = createRequire(import.meta.url);
    const here = path.dirname(fileURLToPath(import.meta.url));
    const candidates = [
        process.env.LWC_COMPILER_NAPI_PATH,
        path.resolve(here, '../lwc-compiler-napi.node'),
        path.resolve(here, '../../target/release/liblwc_compiler_napi.so'),
        path.resolve(here, '../../target/debug/liblwc_compiler_napi.so'),
    ].filter(Boolean) as string[];
    let last: unknown;
    for (const p of candidates) {
        try { return req(p) as NativeAddon; } catch (e) { last = e; }
    }
    throw new Error(`rust backend: native addon not found (tried ${candidates.join(', ')}); last error: ${last}`);
}

// Allowlist mirroring napi `JsCompileOptions` (crates/lwc-compiler-napi/src/lib.rs). Every key here is
// forwarded; any other key throws (never silently dropped). napi re-checks the same set (serde deny_unknown_fields).
const RUST_OPTION_KEYS = [
    'name', 'namespace', 'scopedStyles', 'disableSyntheticShadowSupport', 'apiVersion', 'targetSSR', 'ssrMode',
    'experimentalErrorRecoveryMode', 'enablePrivateMethods', 'experimentalComplexExpressions', 'preserveHtmlComments',
    'enableStaticContentOptimization', 'enableDynamicComponents', 'enableLwcOn', 'experimentalDynamicDirective',
    'enableSyntheticElementInternals', 'componentFeatureFlagModulePath', 'dynamicImports',
    'enableLightningWebSecurityTransforms', 'isExplicitImport', 'outputConfig', 'customRendererConfig', 'stylesheetConfig',
] as const;
// Nested shapes (same keys napi accepts). `[]` = array of objects with these keys.
type Shape = { [k: string]: Shape | true } | { '[]': Shape };
const NESTED_SHAPES: Record<string, Shape> = {
    dynamicImports: { loader: true, strictSpecifier: true },
    outputConfig: { sourcemap: true, minify: true },
    customRendererConfig: { directives: true, elements: { '[]': { tagName: true, namespace: true, attributes: true } } },
    stylesheetConfig: { customProperties: { resolution: { type: true, name: true } } },
};
function assertShape(v: unknown, shape: Shape, where: string): void {
    if (v == null || typeof v !== 'object') return;
    if ('[]' in shape) {
        if (Array.isArray(v)) v.forEach((e, i) => assertShape(e, (shape as any)['[]'], `${where}[${i}]`));
        return;
    }
    for (const [k, sub] of Object.entries(v)) {
        if (!(k in shape)) throw new Error(`rust backend: unsupported compile option "${where}.${k}" (not forwarded by the Rust seam)`);
        if (sub !== undefined && (shape as any)[k] !== true) assertShape(sub, (shape as any)[k], `${where}.${k}`);
    }
}
function assertKnownKeys(options: Record<string, unknown>): void {
    for (const k of Object.keys(options)) {
        if (k === 'instrumentation' && options[k] != null) {
            // Genuine TransformOptions key, but a JS callback object can't cross the JSON napi seam (Rust consumers
            // use lwc_shared::CompileOptions::instrumentation). Dropping it would silently lose metrics.
            throw new Error('rust backend: compile option "instrumentation" (JS callbacks) is not supported over the napi seam');
        }
        if (k === 'instrumentation') continue;
        if (!(RUST_OPTION_KEYS as readonly string[]).includes(k)) throw new Error(`rust backend: unsupported compile option "${k}" (not forwarded by the Rust seam)`);
        if (k in NESTED_SHAPES) assertShape(options[k], NESTED_SHAPES[k], k);
    }
}

export class RustCompilerBackend implements CompilerBackend {
    readonly id = 'rust';
    private native = loadNative();
    transform(src: string, filename: string, options: CompileOptions): CompileResult {
        assertKnownKeys(options);
        const {
            name, namespace, scopedStyles, disableSyntheticShadowSupport,
            apiVersion, targetSSR, ssrMode, experimentalErrorRecoveryMode, enablePrivateMethods,
            experimentalComplexExpressions, preserveHtmlComments, enableStaticContentOptimization, enableDynamicComponents, enableLwcOn, experimentalDynamicDirective, enableSyntheticElementInternals, componentFeatureFlagModulePath,
            dynamicImports, enableLightningWebSecurityTransforms, isExplicitImport, outputConfig, customRendererConfig, stylesheetConfig,
        } = options as any;
        const dropNulls = (o: any) => (o && typeof o === 'object' ? Object.fromEntries(Object.entries(o).filter(([, v]) => v != null)) : o);
        const raw: Record<string, unknown> = {
            name, namespace, scopedStyles, disableSyntheticShadowSupport,
            apiVersion, targetSSR, ssrMode, experimentalErrorRecoveryMode, enablePrivateMethods,
            experimentalComplexExpressions, preserveHtmlComments, enableStaticContentOptimization, enableDynamicComponents, enableLwcOn, experimentalDynamicDirective, enableSyntheticElementInternals, componentFeatureFlagModulePath,
            dynamicImports: dropNulls(dynamicImports),
            enableLightningWebSecurityTransforms, isExplicitImport,
            outputConfig: dropNulls(outputConfig),
            customRendererConfig,
            stylesheetConfig,
        };
        // napi Option<T> rejects null (e.g. fixture apiVersion: null) — treat null as unset like JS defaults.
        for (const k of Object.keys(raw)) if (raw[k] == null) delete raw[k];
        const r = this.native.transform(src, filename, raw);
        if (r.error) {
            // Recovery mode: napi `errors[]` (JsAggregateEntry) -> AggregateError members shaped like @lwc/errors
            // CompilerError (code, filename, location {line,column,start,length}, level, url). Never dropped.
            const subs = Array.isArray(r.error.errors)
                ? r.error.errors.map((x: any) => Object.assign(new Error(x.message), {
                    code: x.code ?? undefined, filename: x.filename ?? undefined, location: x.location ?? undefined, level: x.level, url: x.url,
                }))
                : null;
            const err: any = subs ? new AggregateError(subs, r.error.message ?? '') : new Error(r.error.message ?? 'rust compile failed');
            if (subs) err.errors = subs;
            err.name = r.error.name ?? (subs ? 'AggregateError' : 'Error');
            if (r.error.code != null) err.code = r.error.code;
            if (r.error.location != null) err.location = r.error.location;
            else if (r.error.line != null) err.location = { line: r.error.line, column: r.error.column ?? null };
            if (r.error.filename != null) err.filename = r.error.filename;
            err.line = r.error.line ?? null;
            err.column = r.error.column ?? null;
            throw err;
        }
        if (r.code == null || r.code === undefined) {
            throw new Error('rust compile failed: empty result');
        }
        const m: any = r.map;
        const norm = m == null ? m : typeof m === 'string' ? m : m.mappings === '' ? '' : JSON.stringify(m);
        return {
            code: r.code ?? '',
            map: norm,
            warnings: r.warnings ?? undefined,
            cssScopeTokens: r.cssScopeTokens ?? undefined,
        };
    }
}

export async function getBackend(): Promise<CompilerBackend> {
    const which = process.env.LWC_COMPILER_BACKEND ?? 'js';
    if (which === 'rust') return new RustCompilerBackend();
    return new JsCompilerBackend(await import('@lwc/compiler'));
}
