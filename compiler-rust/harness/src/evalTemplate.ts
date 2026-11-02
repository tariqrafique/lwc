// Behavioral evaluation of emitted template ESM (no printer-byte comparison).
// Imports are stubbed: `lwc` named imports become recording no-ops (registerTemplate = identity),
// default imports (stylesheets, child components) become stubs. DOM render is 0d.1 follow-up.
export interface TemplateEval {
    imports: string[];
    lwcImports: string[];
    defaultExportKind: string;
    isFunction: boolean;
    registered: boolean;
    stylesheetToken?: string;
    stylesheetsIsArray: boolean;
    /** customRendererConfig signal: `renderer: renderer` hooks on element data (absent when 0). */
    rendererHooks?: number;
}

export function evalTemplateModule(code: string): TemplateEval {
    const imports: string[] = [];
    const lwcImports = new Set<string>();
    let registered = false;
    const lwcStub = (name: string) => {
        lwcImports.add(name);
        if (name === 'registerTemplate') return (t: unknown) => ((registered = true), t);
        if (name === 'freezeTemplate') return () => {};
        return (..._a: unknown[]) => ({ __lwc: name });
    };
    const imp = (spec: string) => (imports.push(spec), spec);
    let body = code
        .replace(/^import\s+\{([^}]*)\}\s+from\s+("[^"]*"|'[^']*');?/gm, (_m, names: string, spec) => {
            const s = imp(JSON.parse(spec.replace(/^'|'$/g, '"')));
            return names.split(',').map((n) => n.trim()).filter(Boolean).map((n) => {
                const [orig, local = orig] = n.split(/\s+as\s+/).map((x) => x.trim());
                return `const ${local} = __named(${JSON.stringify(s)}, ${JSON.stringify(orig)});`;
            }).join('\n');
        })
        .replace(/^import\s+\*\s+as\s+(\w+)\s+from\s+("[^"]*"|'[^']*');?/gm, (_m, id, spec) => `const ${id} = __ns(${JSON.stringify(imp(JSON.parse(spec.replace(/^'|'$/g, '"'))))});`)
        .replace(/^import\s+(\w+)\s+from\s+("[^"]*"|'[^']*');?/gm, (_m, id, spec) => `const ${id} = __def(${JSON.stringify(imp(JSON.parse(spec.replace(/^'|'$/g, '"'))))});`);
    if (/^\s*(import|export)\s+(?!default\b)/m.test(body)) throw new Error('evalTemplate: unsupported import/export form');
    body = body.replace(/^export default\s+/m, 'return ');
    const __named = (spec: string, n: string) => (spec === 'lwc' ? lwcStub(n) : () => ({ __import: `${spec}#${n}` }));
    const __def = (spec: string) => (/\.css(\?|$)/.test(spec) ? undefined : function Stub() {});
    const __ns = (spec: string) => new Proxy({}, { get: (_t, k) => __named(spec, String(k)) });
    const exp: any = new Function('__named', '__def', '__ns', body)(__named, __def, __ns);
    return {
        imports,
        lwcImports: [...lwcImports].sort(),
        defaultExportKind: exp === null ? 'null' : typeof exp,
        isFunction: typeof exp === 'function',
        registered,
        ...(typeof exp?.stylesheetToken === 'string' ? { stylesheetToken: exp.stylesheetToken } : {}),
        stylesheetsIsArray: Array.isArray(exp?.stylesheets),
        ...rendererHooks(code),
    };
}

// Count of element-data `renderer: renderer` properties (template-compiler `isCustomRendererHookRequired`).
// Only present when nonzero, so templates compiled without customRendererConfig keep their golden shape.
function rendererHooks(code: string): { rendererHooks?: number } {
    // `renderer: renderer` property (or `{ renderer }` shorthand) on element data; import lines excluded.
    const body = code.replace(/^\s*import\b[^;]*;?$/gm, '');
    const n = (body.match(/\brenderer\s*:\s*renderer\b|[{,]\s*renderer\s*(?=[,}])/g) ?? []).length;
    return n ? { rendererHooks: n } : {};
}
