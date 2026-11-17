// generate-markup.ts (`__setStaticInternals`, the template import) and registerPublicProperties (index.ts
// ClassDeclaration/ClassExpression), compared as the output's token stream.
const H = `import { LightningElement, api, track, wire } from 'lwc';\n`;
export const cases = {
  bare: H + `export default class C extends LightningElement {}\n`,
  props: H + `export default class C extends LightningElement {\n  @api a;\n  @api get b() { return 1; }\n  @track t;\n}\n`,
  quotedProp: H + `export default class C extends LightningElement {\n  @api ['x"y'];\n}\n`,
  styles: H + `import s from './foo.css';\nexport default class C extends LightningElement {\n  static stylesheets = [s];\n}\n`,
  defaultExpr: H + `export default class extends LightningElement {\n  @api a;\n}\n`,
  wrapped: H + `const C = class extends LightningElement {\n  @api a;\n};\nexport default wrap(C);\n`,
  baseExpr: H + `const B = class extends Object {\n  @api x;\n};\nexport default class C extends B {\n  @api y;\n}\n`,
  baseDecl: H + `class B {\n  @api x;\n  @api y;\n}\nexport default class C extends LightningElement {}\n`,
  baseAnon: H + `foo(class {\n  @api q;\n});\nexport default class C extends LightningElement {}\n`,
  baseNested: H + `export default class C extends LightningElement {\n  @api a;\n  m() {\n    return class {\n      @api n;\n    };\n  }\n}\n`,
  baseMatched: H + `const M = class extends LightningElement {\n  @api x;\n};\nexport default class C extends M {}\n`,
  decoratorOnlyImport: `import { api } from 'lwc';\n` + H + `export default class C extends LightningElement {\n  @api a;\n}\n`,
  notLwc: `export const x = 1;\n`,
};
