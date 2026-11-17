const pre = "import { LightningElement, api } from 'lwc';\n";
const c = {
  'field-anno': "export default class Foo extends LightningElement { x: string; }",
  'field-anno-init': "export default class Foo extends LightningElement { @api x: string = 'a'; }",
  'field-opt': "export default class Foo extends LightningElement { x?: string; }",
  'field-bang': "export default class Foo extends LightningElement { x!: string; }",
  'private-mod': "export default class Foo extends LightningElement { private x = 1; }",
  'public-mod': "export default class Foo extends LightningElement { public x = 1; }",
  'readonly-mod': "export default class Foo extends LightningElement { readonly x = 1; }",
  'method-param': "export default class Foo extends LightningElement { foo(a: string) {} }",
  'method-ret': "export default class Foo extends LightningElement { foo(): void {} }",
  'fn-param': "function f(a: number) { return a; }\nexport default class Foo extends LightningElement {}",
  'fn-ret': "function f(): number { return 1; }\nexport default class Foo extends LightningElement {}",
  'const-anno': "const a: number = 1;\nexport default class Foo extends LightningElement {}",
  'let-anno': "let a: number;\nexport default class Foo extends LightningElement {}",
  'interface': "interface A { x: number }\nexport default class Foo extends LightningElement {}",
  'type-alias': "type A = string;\nexport default class Foo extends LightningElement {}",
  'as-cast': "const a = (1 as any);\nexport default class Foo extends LightningElement {}",
  'as-cast2': "const a = b as any;\nexport default class Foo extends LightningElement {}",
  'generic-call': "const a = f<string>(1);\nexport default class Foo extends LightningElement {}",
  'generic-class': "export default class Foo<T> extends LightningElement {}",
  'implements': "export default class Foo extends LightningElement implements A {}",
  'import-type': "import type { A } from './a';\nexport default class Foo extends LightningElement {}",
  'export-type': "export type A = string;\nexport default class Foo extends LightningElement {}",
  'enum': "enum E { A }\nexport default class Foo extends LightningElement {}",
  'non-null': "const a = b!;\nexport default class Foo extends LightningElement {}",
  'arrow-param': "const f = (a: string) => a;\nexport default class Foo extends LightningElement {}",
  'declare': "declare const x: number;\nexport default class Foo extends LightningElement {}",
  'abstract': "export default abstract class Foo extends LightningElement {}",
  'getter-ret': "export default class Foo extends LightningElement { @api get x(): string { return ''; } }",
  'satisfies': "const a = {} satisfies A;\nexport default class Foo extends LightningElement {}",
  'jsx': "const a = <div />;\nexport default class Foo extends LightningElement {}",
  'jsx2': "export default class Foo extends LightningElement { render() { return <div>hi</div>; } }",
  'namespace': "namespace N {}\nexport default class Foo extends LightningElement {}",
};
const out = [];
for (const [k, v] of Object.entries(c)) for (const ext of ['ts', 'tsx', 'jsx']) {
  if (ext !== 'ts' && !k.startsWith('jsx') && !['field-anno','interface','as-cast'].includes(k)) continue;
  out.push({ name: `${k}.${ext}`, src: pre + v, filename: `foo.${ext}`, opts: {} });
}
out.push({ name: 'field-anno.ts-rec', src: pre + c['field-anno'], filename: 'foo.ts', opts: { experimentalErrorRecoveryMode: true } });
out.push({ name: 'interface.ts-rec', src: pre + c['interface'], filename: 'foo.ts', opts: { experimentalErrorRecoveryMode: true } });
require('fs').writeFileSync('tsx.json', JSON.stringify(out, null, 1));
