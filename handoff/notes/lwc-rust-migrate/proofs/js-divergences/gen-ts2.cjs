const pre = "import { LightningElement, api } from 'lwc';\n";
const c = {
  'js-missing-comma-obj': ["const o = {a: 1 b: 2};", 'js'],
  'js-class-two-ids': ["export default class Foo extends LightningElement { x y }", 'js'],
  'js-call-unclosed': ["foo(1, 2;", 'js'],
  'js-if-paren': ["if (x { }", 'js'],
  'js-arr': ["const a = [1 2];", 'js'],
  'js-stmt': ["const a = 1 2;", 'js'],
  'js-arrow-bad': ["const f = (a b) => a;", 'js'],
  'js-import-bad': ["import { a b } from 'x';", 'js'],
  'ts-param-default': ["export default class Foo extends LightningElement { foo(a: string = 'x', b?: number) {} }", 'ts'],
  'ts-ret-generic': ["export default class Foo extends LightningElement { foo(a = (1)): Promise<void> {} }", 'ts'],
  'ts-ml-field': ["export default class Foo extends LightningElement {\n  @api\n  recordId: string;\n}", 'ts'],
  'ts-as-call': ["foo(x as any);\nexport default class Foo extends LightningElement {}", 'ts'],
  'ts-as-array': ["const a = [x as any];\nexport default class Foo extends LightningElement {}", 'ts'],
  'ts-as-ret': ["function f() { return x as any; }\nexport default class Foo extends LightningElement {}", 'ts'],
  'ts-protected': ["export default class Foo extends LightningElement { protected foo() {} }", 'ts'],
  'ts-static-private': ["export default class Foo extends LightningElement { private static x = 1; }", 'ts'],
  'ts-export-interface': ["export interface A { x: number }\nexport default class Foo extends LightningElement {}", 'ts'],
  'ts-arrow-ret': ["const f = (): void => {};\nexport default class Foo extends LightningElement {}", 'ts'],
  'ts-fragment': ["const a = <></>;\nexport default class Foo extends LightningElement {}", 'tsx'],
  'ts-generic-fn': ["function f<T>(a) { return a; }\nexport default class Foo extends LightningElement {}", 'ts'],
  'ts-optional-param-fn': ["function f(a?) { return a; }\nexport default class Foo extends LightningElement {}", 'ts'],
  'ts-wire-typed': ["import { wire } from 'lwc';\nexport default class Foo extends LightningElement { @wire(getX) data: any; }", 'ts'],
};
const out = Object.entries(c).map(([k, [v, ext]]) => ({ name: k, src: pre + v, filename: `foo.${ext}`, opts: {} }));
require('fs').writeFileSync('tsx2.json', JSON.stringify(out, null, 1));
