// Enumerate every option field (recursing into plain object-typed fields) from the shipped JS option types.
// Source of truth for the option-inventory gate. Uses the oracle's own TypeScript.
import { createRequire } from 'node:module';
import { oracleRoot } from './oracle-root.mjs';
const LWC = oracleRoot();
const req = createRequire(`${LWC}/package.json`);
const ts = req('typescript');
const P = `${LWC}/packages/@lwc`;
const ROOTS = [
    ['compiler', `${P}/compiler/src/options.ts`, 'TransformOptions'],
    ['template', `${P}/template-compiler/src/config.ts`, 'Config'],
    ['style', `${P}/style-compiler/src/transform.ts`, 'Config'],
    ['ssr.component', `${P}/ssr-compiler/src/shared.ts`, 'ComponentTransformOptions'],
    ['ssr.template', `${P}/ssr-compiler/src/shared.ts`, 'TemplateTransformOptions'],
];
const program = ts.createProgram(ROOTS.map((r) => r[1]), { strict: true, skipLibCheck: true, noEmit: true, baseUrl: LWC, moduleResolution: ts.ModuleResolutionKind.Bundler, target: ts.ScriptTarget.ES2022 });
const checker = program.getTypeChecker();
const out = [];
const isPlainObj = (t) => {
    const nn = checker.getNonNullableType(t);
    if (nn.isUnion()) return nn.types.every(isPlainObj) && nn.types.length > 0 && nn.types.every((x) => !(x.flags & ts.TypeFlags.Literal));
    if (!(nn.flags & ts.TypeFlags.Object)) return false;
    if (checker.isArrayType(nn) || checker.isTupleType(nn)) return false;
    if (nn.getCallSignatures().length) return false;
    const props = nn.getProperties();
    return props.length > 0 && props.every((p) => !p.valueDeclaration || !ts.isMethodSignature(p.valueDeclaration));
};
function walk(prefix, type, depth) {
    for (const p of checker.getPropertiesOfType(checker.getNonNullableType(type))) {
        const decl = p.valueDeclaration ?? p.declarations?.[0];
        const t = checker.getTypeOfSymbolAtLocation(p, decl);
        const key = `${prefix}.${p.getName()}`;
        out.push(key);
        // Only recurse into config objects (not callback bags like instrumentation, not unions of literals).
        if (depth < 3 && isPlainObj(t) && !/instrumentation/i.test(p.getName())) walk(key, t, depth + 1);
    }
}
for (const [label, file, name] of ROOTS) {
    const sf = program.getSourceFile(file);
    if (!sf) throw new Error(`missing ${file}`);
    let sym;
    ts.forEachChild(sf, (n) => { if ((ts.isInterfaceDeclaration(n) || ts.isTypeAliasDeclaration(n)) && n.name.text === name) sym = checker.getTypeAtLocation(n.name); });
    if (!sym) throw new Error(`type ${name} not found in ${file}`);
    walk(label, sym, 0);
}
console.log(JSON.stringify([...new Set(out)].sort(), null, 1));
