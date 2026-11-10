// 0d.3 SSR render-HTML parity helpers (used by test/ssr.render.test.ts).
//
// `lwcBackendPlugin` is a line-for-line port of upstream `@lwc/rollup-plugin` (packages/@lwc/rollup-plugin/src/index.ts)
// whose `transform` hook calls the harness `CompilerBackend.transform` instead of `@lwc/compiler.transformSync`, so the
// same bundle/render path runs against the JS or rust backend. Resolution (`@lwc/module-resolver`, implicit html/css
// stubs, alias specifiers, `?scoped=true`) and option plumbing mirror upstream; `bundleFixture` mirrors upstream
// `fixtures.spec.ts#compileFixture` (virtual entry, externals, onwarn-throws-unless-CIRCULAR_DEPENDENCY).
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { URLSearchParams } from 'node:url';
import type { CompilerBackend } from './backend';
import { oracleRoot } from './oracleRoot';

export type SsrMode = 'sync' | 'async' | 'asyncYield';
export const LWC = `${oracleRoot()}/packages/@lwc`;
export const FIXTURES_ROOT = `${LWC}/ssr-compiler/src/__tests__/fixtures`;
export const CUSTOM_LOADER = `${LWC}/ssr-compiler/src/__tests__/utils/custom-loader.js`;
/**
 * @lwc/ssr-runtime entry the bundle's `lwc`/`@lwc/ssr-runtime` externals are pinned to (and the test renders with).
 * Default: built ESM dist. `SSR_RUNTIME=src` uses the TS source, which is what upstream's vitest alias
 * (`@lwc/ssr-runtime` → `@lwc/ssr-runtime/src`, vitest.shared.mjs) actually runs against.
 */
export const SSR_RUNTIME_KIND = (process.env.SSR_RUNTIME ?? 'dist') as 'dist' | 'src';
export const SSR_RUNTIME_ESM = SSR_RUNTIME_KIND === 'src' ? `${LWC}/ssr-runtime/src/index.ts` : `${LWC}/ssr-runtime/dist/index.js`;

const req = createRequire(`${LWC}/rollup-plugin/package.json`);
const { rollup } = req('rollup') as typeof import('rollup');
const pluginUtils = req('@rollup/pluginutils');
const { resolveModule, RegistryType } = req('@lwc/module-resolver');
const { getAPIVersionFromNumber } = req('@lwc/shared');

export interface LwcPluginOptions {
    targetSSR?: boolean;
    ssrMode?: SsrMode;
    include?: unknown;
    exclude?: unknown;
    rootDir?: string;
    sourcemap?: boolean | 'inline';
    modules?: any[];
    defaultModules?: any[];
    stylesheetConfig?: unknown;
    preserveHtmlComments?: boolean;
    dynamicImports?: { loader?: string; strictSpecifier?: boolean };
    experimentalDynamicDirective?: boolean;
    enableDynamicComponents?: boolean;
    enableSyntheticElementInternals?: boolean;
    enableLightningWebSecurityTransforms?: boolean;
    experimentalComplexExpressions?: boolean;
    enableLwcOn?: boolean;
    disableSyntheticShadowSupport?: boolean;
    apiVersion?: number;
    enableStaticContentOptimization?: boolean;
    componentFeatureFlagModulePath?: string;
    enablePrivateMethods?: boolean;
}

const PLUGIN_NAME = 'rollup-plugin-lwc-compiler';
const IMPLICIT_DEFAULT_HTML_PATH = ['@lwc', 'resources', 'empty_html.js'].join(path.sep);
const EMPTY_IMPLICIT_HTML_CONTENT = 'export default void 0';
const IMPLICIT_DEFAULT_CSS_PATH = ['@lwc', 'resources', 'empty_css.css'].join(path.sep);
const EMPTY_IMPLICIT_CSS_CONTENT = '';
const SCRIPT_FILE_EXTENSIONS = ['.js', '.mjs', '.jsx', '.ts', '.mts', '.tsx'];
const DEFAULT_MODULES = [{ npm: '@lwc/engine-dom' }, { npm: '@lwc/synthetic-shadow' }, { npm: '@lwc/wire-service' }];

function isImplicitHTMLImport(importee: string, importer: string, importerExt: string): boolean {
    return (
        SCRIPT_FILE_EXTENSIONS.includes(importerExt) &&
        path.extname(importee) === '.html' &&
        path.dirname(importer) === path.dirname(importee) &&
        path.basename(importer, importerExt) === path.basename(importee, '.html')
    );
}
function isImplicitCssImport(importee: string, importer: string): boolean {
    return (
        path.extname(importee) === '.css' &&
        path.extname(importer) === '.html' &&
        (path.basename(importee, '.css') === path.basename(importer, '.html') ||
            path.basename(importee, '.scoped.css') === path.basename(importer, '.html'))
    );
}
function parseDescriptorFromFilePath(id: string) {
    const [filename, query] = id.split('?', 2);
    const params = new URLSearchParams(query);
    return { filename, specifier: params.get('specifier'), scoped: params.has('scoped') };
}
function appendAliasSpecifierQueryParam(id: string, specifier: string): string {
    const [filename, query] = id.split('?', 2);
    const params = new URLSearchParams(query);
    params.set('specifier', specifier);
    return `${filename}?${params.toString()}`;
}
function transformWarningToRollupLog(warning: any, src: string, id: string) {
    const result: any = {
        message: `@lwc/rollup-plugin: ${warning.message?.replace(/\n/g, ' ')}`,
        plugin: PLUGIN_NAME,
        pluginCode: `LWC${warning.code}`,
    };
    const { location } = warning;
    if (location) {
        result.loc = { file: id, column: location.column, line: location.line };
        if (typeof location.start === 'number' && typeof location.length === 'number') {
            result.frame = src.substring(location.start, location.start + location.length);
        }
    }
    return result;
}

/** Port of `@lwc/rollup-plugin` default export with the compiler call routed through `backend`. */
export function lwcBackendPlugin(backend: CompilerBackend, pluginOptions: LwcPluginOptions = {}): any {
    const filter = pluginUtils.createFilter(pluginOptions.include, pluginOptions.exclude);
    let { rootDir, modules = [] } = pluginOptions;
    const {
        targetSSR, ssrMode, stylesheetConfig, sourcemap = false, preserveHtmlComments, dynamicImports,
        experimentalDynamicDirective, enableDynamicComponents, enableSyntheticElementInternals, enableLwcOn,
        enableLightningWebSecurityTransforms, experimentalComplexExpressions, disableSyntheticShadowSupport,
        apiVersion, defaultModules = DEFAULT_MODULES, componentFeatureFlagModulePath, enablePrivateMethods,
    } = pluginOptions;

    return {
        name: PLUGIN_NAME,
        buildStart(this: any, { input }: any) {
            if (rootDir === undefined) {
                if (Array.isArray(input)) {
                    rootDir = path.dirname(path.resolve(input[0]));
                    if (input.length > 1) this.warn(`The "rootDir" option should be explicitly set when passing an "input" array to rollup. The "rootDir" option is implicitly resolved to ${rootDir}.`);
                } else {
                    rootDir = path.dirname(path.resolve(Object.values(input as Record<string, string>)[0]));
                    this.warn(`The "rootDir" option should be explicitly set when passing "input" object to rollup. The "rootDir" option is implicitly resolved to ${rootDir}.`);
                }
            } else rootDir = path.resolve(rootDir);
            modules = [...modules, ...defaultModules, { dir: rootDir }];
        },
        resolveId(importee: string, importer?: string) {
            if (!importer) return;
            const { filename: importerFilename } = parseDescriptorFromFilePath(importer);
            if (importee.startsWith('.') || importee.startsWith('/')) {
                const importerExt = path.extname(importerFilename);
                const importeeExt = path.extname(importee) || importerExt;
                const importeeResolvedPath = path.resolve(path.dirname(importerFilename), importee);
                const importeeAbsPath = pluginUtils.addExtension(importeeResolvedPath, importeeExt);
                const { filename: norm } = parseDescriptorFromFilePath(importeeAbsPath);
                if (isImplicitHTMLImport(norm, importerFilename, importerExt) && !fs.existsSync(norm)) return IMPLICIT_DEFAULT_HTML_PATH;
                if (isImplicitCssImport(norm, importerFilename) && !fs.existsSync(norm)) return IMPLICIT_DEFAULT_CSS_PATH;
                return importeeAbsPath;
            }
            try {
                const { entry, specifier, type } = resolveModule(importee, importer, { modules, rootDir });
                if (type === RegistryType.alias) {
                    const [namespace, name, ...rest] = specifier.split('/');
                    if (!!namespace?.length && !!name?.length && !rest?.length) return appendAliasSpecifierQueryParam(entry, specifier);
                }
                return entry;
            } catch (err: any) {
                if (err && err.code !== 'NO_LWC_MODULE_FOUND') throw err;
            }
        },
        load(this: any, id: string) {
            if (id === IMPLICIT_DEFAULT_HTML_PATH) return EMPTY_IMPLICIT_HTML_CONTENT;
            if (id === IMPLICIT_DEFAULT_CSS_PATH) return EMPTY_IMPLICIT_CSS_CONTENT;
            const { filename, specifier: hasAlias } = parseDescriptorFromFilePath(id);
            const isCSS = path.extname(filename) === '.css';
            if (isCSS || hasAlias) {
                if (fs.existsSync(filename)) return fs.readFileSync(filename, 'utf8');
                if (isCSS) {
                    this.warn(`The imported CSS file ${filename} does not exist: Importing it as undefined. This behavior may be removed in a future version of LWC. Please avoid importing a CSS file that does not exist.`);
                    return EMPTY_IMPLICIT_CSS_CONTENT;
                }
            }
        },
        transform(this: any, src: string, id: string) {
            const { scoped, filename, specifier } = parseDescriptorFromFilePath(id);
            if (!filter(filename)) return;
            const [namespace, name] = specifier?.split('/') ?? path.dirname(filename).split(path.sep).slice(-2);
            if (!namespace || !name) {
                this.warn(`The component namespace and name (${JSON.stringify(namespace)} and ${JSON.stringify(name)}) could not be determined from the specifier ${JSON.stringify(specifier)} or filename ${JSON.stringify(path.dirname(filename))}`);
            }
            const { code, warnings } = backend.transform(src, filename, {
                name, namespace,
                outputConfig: { sourcemap },
                stylesheetConfig, dynamicImports, experimentalDynamicDirective, enableDynamicComponents,
                enableSyntheticElementInternals, enableLwcOn, enableLightningWebSecurityTransforms,
                experimentalComplexExpressions, preserveHtmlComments,
                scopedStyles: scoped,
                disableSyntheticShadowSupport,
                apiVersion: getAPIVersionFromNumber(apiVersion),
                enableStaticContentOptimization: 'enableStaticContentOptimization' in pluginOptions ? pluginOptions.enableStaticContentOptimization : true,
                targetSSR, ssrMode, componentFeatureFlagModulePath, enablePrivateMethods,
            });
            if (warnings) for (const w of warnings) this.warn(transformWarningToRollupLog(w, src, filename));
            // sourcemap:false upstream; drop maps so backend map normalization can't affect rollup.
            return { code, map: null };
        },
    };
}

/**
 * Shared rollup core for the SSR and CSR render suites: virtual `input` entry re-exporting `entrySource`, then
 * `extraPlugins` (resolved first, e.g. virtual component files), then `lwcBackendPlugin`. Mirrors upstream
 * fixtures.spec.ts `compileFixture` (onwarn throws unless CIRCULAR_DEPENDENCY). Returns the ESM bundle text.
 * `onWarning` (CSR template corpus only) collects non-circular warnings instead of throwing.
 */
export async function bundleLwc(backend: CompilerBackend, opts: {
    entrySource: string; pluginOptions: LwcPluginOptions; external: string[]; paths: Record<string, string>;
    extraPlugins?: any[]; afterPlugins?: any[]; onWarning?: (message: string) => void;
}): Promise<string> {
    const input = 'virtual/fixture/test.js';
    const bundle = await rollup({
        input,
        external: opts.external,
        plugins: [
            { name: 'virtual', resolveId: (id: string) => (id === input ? id : undefined), load: (id: string) => (id === input ? opts.entrySource : undefined) },
            ...(opts.extraPlugins ?? []),
            lwcBackendPlugin(backend, opts.pluginOptions),
            ...(opts.afterPlugins ?? []),
        ],
        onwarn({ message, code }: any) {
            if (code === 'CIRCULAR_DEPENDENCY') return;
            if (opts.onWarning) opts.onWarning(message);
            else throw new Error(message);
        },
    });
    try {
        const { output } = await bundle.generate({ format: 'esm', exports: 'named', paths: opts.paths });
        return output[0].code;
    } finally {
        await bundle.close();
    }
}

/** Upstream fixtures.spec.ts plugin options for an ssr-compiler fixture; `targetSSR:false` drops ssrMode (CSR). */
export function fixturePluginOptions(opts: { dirname: string; experimentalComplexExpressions?: boolean; targetSSR: boolean; ssrMode?: SsrMode; apiVersion?: number }): LwcPluginOptions {
    return {
        targetSSR: opts.targetSSR,
        ...(opts.targetSSR ? { ssrMode: opts.ssrMode } : {}),
        enableDynamicComponents: true,
        enableLwcOn: true,
        experimentalDynamicDirective: true,
        modules: [{ dir: path.resolve(opts.dirname, './modules') }],
        dynamicImports: { loader: CUSTOM_LOADER, strictSpecifier: false },
        experimentalComplexExpressions: opts.experimentalComplexExpressions,
        // ssr_render_api row only; absent otherwise so existing rows are unchanged.
        ...(opts.apiVersion !== undefined ? { apiVersion: opts.apiVersion } : {}),
    };
}

/** Mirrors upstream fixtures.spec.ts `compileFixture`; returns the ESM bundle text (externals pinned via `paths`). */
export async function bundleFixture(backend: CompilerBackend, opts: {
    entry: string; dirname: string; experimentalComplexExpressions?: boolean; ssrMode: SsrMode; apiVersion?: number;
}): Promise<string> {
    return bundleLwc(backend, {
        entrySource: `export { default } from "${opts.entry}";`,
        external: ['lwc', '@lwc/ssr-runtime', 'vitest'],
        pluginOptions: fixturePluginOptions({ ...opts, targetSSR: true }),
        // Upstream lets node resolve these from the fixture dir to the same @lwc/ssr-runtime instance the spec imports;
        // pin to the same absolute dist file the test imports so hooks (sanitizeHtmlContent) are shared.
        paths: { lwc: SSR_RUNTIME_ESM, '@lwc/ssr-runtime': SSR_RUNTIME_ESM },
    });
}

/** Upstream testFixtureDir `.skip`/`.only` lookup walks up to `__tests__`. */
export function existsUp(dir: string, file: string): boolean {
    while (true) {
        if (fs.existsSync(path.join(dir, file))) return true;
        dir = path.join(dir, '..');
        const b = path.basename(dir);
        if (b === '__tests__' || b === '') return false;
    }
}

export function listFixtures(root = FIXTURES_ROOT): string[] {
    const out: string[] = [];
    const walk = (d: string) => {
        for (const e of fs.readdirSync(d, { withFileTypes: true })) {
            const p = path.join(d, e.name);
            if (e.isDirectory()) walk(p);
            else if (e.name === 'config.json') out.push(d);
        }
    };
    walk(root);
    return out.sort();
}
