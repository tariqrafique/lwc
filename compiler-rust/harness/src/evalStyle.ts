// Behavioral evaluation of emitted stylesheet ESM (no printer-byte comparison).
type Sheet = ((token: string | undefined, host: boolean, dir: boolean) => string) & { $scoped$?: boolean; $nativeOnly$?: boolean };

export const COMBOS: Array<[string | undefined, boolean, boolean]> = [
    ['x', true, false], ['x', false, true], ['x', false, false], ['x', true, true], [undefined, true, false],
];

/** Rewrites `import a from "spec";` into stub sheets so the module evaluates standalone. */
export function evalStylesheetModule(code: string) {
    const imports: string[] = [];
    let body = code.replace(/^import\s+(\w+)\s+from\s+("[^"]*"|'[^']*');?$/gm, (_m, id, spec) => {
        imports.push(JSON.parse(spec.replace(/^'|'$/g, '"')));
        return `const ${id} = __imp(${spec});`;
    });
    body = body.replace(/^export default\s+/m, 'return ');
    const __imp = (spec: string) => {
        if (/resolver/.test(spec)) return (name: string, fallback?: string) => `RESOLVED(${name}${fallback ? ',' + fallback : ''})`;
        const s: Sheet = (t) => `/*import:${spec}${t ? ':' + t : ''}*/`;
        return [s];
    };
    const exported = new Function('__imp', body)(__imp) as unknown;
    const flat: Sheet[] = [];
    const walk = (v: unknown) => (Array.isArray(v) ? v.forEach(walk) : typeof v === 'function' && flat.push(v as Sheet));
    walk(exported);
    return {
        imports,
        exportedUndefined: exported === undefined,
        sheets: flat.map((s) => ({
            scoped: !!s.$scoped$,
            nativeOnly: !!s.$nativeOnly$,
            css: Object.fromEntries(COMBOS.map(([t, h, d]) => [`${t ?? '-'}|host=${h}|dir=${d}`, s(t, h, d)])),
        })),
    };
}
