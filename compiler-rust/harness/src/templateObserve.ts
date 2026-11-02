// Template observation shared by the template option rows (template-options, template-api). Same observed shape as the
// `template` row ({ok, ...evalTemplateModule, warnings, cssScopeTokens}); `detail` adds version-gated codegen signals.
import type { CompilerBackend } from './backend';
import { evalTemplateModule } from './evalTemplate';
import { errorObservation, errorMultiset } from './optionRows';

export const TEMPLATE_ROOT = '/workspace/lwc/packages/@lwc/template-compiler/src/__tests__/fixtures';
const diag = (ws: any[] = []) => ws.map((w) => ({ code: w.code, level: w.level }));

/**
 * apiVersion-gated template codegen signals:
 *  - apiHelpers: `$api` members the render function binds (USE_FRAGMENTS_FOR_LIGHT_DOM_SLOTS drops `f` flatten,
 *    TEMPLATE_CLASS_NAME_OBJECT_BINDING adds `ncls`), sorted.
 *  - fragments: static HTML handed to parseFragment/parseSVGFragment, in order (LOWERCASE_SCOPE_TOKENS gates the
 *    static SVG optimization and the scope token placeholders).
 *  - tokens: `tmpl.stylesheetToken` / `tmpl.legacyStylesheetToken` values (LOWERCASE_SCOPE_TOKENS); these assignments
 *    follow `export default`, so evalTemplateModule never sees them.
 */
export function templateDetail(code: string) {
    const apiHelpers = [...code.matchAll(/const\s*\{([^}]*)\}\s*=\s*\$api\b/g)]
        .flatMap((m) => m[1].split(',').map((p) => p.split(':')[0].trim()).filter(Boolean));
    const fragments = [...code.matchAll(/\b(parseFragment|parseSVGFragment)\s*`((?:\\.|[^`\\])*)`/g)].map((m) => `${m[1]}:${m[2]}`);
    const tokens = Object.fromEntries([...code.matchAll(/\btmpl\.(stylesheetToken|legacyStylesheetToken)\s*=\s*("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')/g)]
        .map((m) => [m[1], JSON.parse(m[2].startsWith("'") ? `"${m[2].slice(1, -1)}"` : m[2])]));
    return { apiHelpers: [...new Set(apiHelpers)].sort(), fragments, tokens };
}

export function observeTemplate(backend: CompilerBackend, src: string, filename: string, opts: Record<string, unknown>, { detail = false } = {}) {
    try {
        const res = backend.transform(src, filename, opts as any);
        let ev: unknown;
        try { ev = evalTemplateModule(res.code); } catch (e: any) { ev = { evalError: e?.name ?? 'Error' }; }
        return { ok: true, ...(ev as object), warnings: diag(res.warnings), cssScopeTokens: (res as any).cssScopeTokens ?? null, ...(detail ? templateDetail(res.code) : {}) };
    } catch (e: any) {
        return { ...errorObservation(e), errors: errorMultiset(e) };
    }
}
