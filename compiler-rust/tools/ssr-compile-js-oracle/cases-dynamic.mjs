// index.ts ImportExpression: `dynamicImports.strictSpecifier` (LWC1121, its argument and location) and
// `dynamicImports.loader` (the `load as __lwcLoad` import and every `import()` rewritten), as token streams.
const H = `import { LightningElement } from 'lwc';\n`;
const C = (body) => H + `export default class C extends LightningElement {\n${body}}\n`;
const strict = { strictSpecifier: true };
const loader = { loader: 'my/loader', strictSpecifier: false };
export const cases = {
  // No `dynamicImports`: transformSync's defaults (strict on, no loader).
  strictOk: { src: C(`  m() { return import('x/y'); }\n`), dynamicImports: strict },
  strictParen: { src: C(`  m() { return import(('x/y')); }\n`), dynamicImports: strict },
  strictIdent: { src: C(`  m() { return import(y); }\n`), dynamicImports: strict },
  strictTemplate: { src: C('  m() { return import(`x`); }\n'), dynamicImports: strict },
  strictNumber: { src: C(`  m() { return import(1e21); }\n`), dynamicImports: strict },
  strictSmall: { src: C(`  m() { return import(1.5e-7); }\n`), dynamicImports: strict },
  strictBigInt: { src: C(`  m() { return import(10n); }\n`), dynamicImports: strict },
  strictRegExp: { src: C(`  m() { return import(/a/g); }\n`), dynamicImports: strict },
  strictNull: { src: C(`  m() { return import(null); }\n`), dynamicImports: strict },
  strictBool: { src: C(`  m() { return import(true); }\n`), dynamicImports: strict },
  strictNotLwc: { src: `export function f() { return import(y); }\n`, dynamicImports: strict },
  strictReservedFirst: { src: `const __lwcX = 1;\nexport function f() { return import(y); }\n`, dynamicImports: strict },
  strictReservedAfter: { src: `export function f() { return import(y); }\nconst __lwcX = 1;\n`, dynamicImports: strict },
  strictDefault: { src: C(`  m() { return import(y); }\n`) },
  strictOff: { src: C(`  m() { return import(y); }\n`), dynamicImports: { strictSpecifier: false } },
  loader: { src: C(`  m() { return import('x/y'); }\n`), dynamicImports: loader },
  loaderTwice: { src: C(`  m() { import('a'); return import('b'); }\n`), dynamicImports: loader },
  loaderNested: { src: C(`  m() { return import(import('a')); }\n`), dynamicImports: loader },
  loaderNestedStrict: { src: C(`  m() { return import(import('a')); }\n`), dynamicImports: { ...loader, ...strict } },
  loaderOptions: { src: C(`  m() { return import('a', { with: { type: 'json' } }); }\n`), dynamicImports: loader },
  loaderNotLwc: { src: `export function f() { return import('a'); }\n`, dynamicImports: loader },
  loaderRemovedMember: { src: C(`  renderedCallback() { import('a'); }\n`), dynamicImports: loader },
  loaderAndStrict: { src: C(`  m() { return import(y); }\n`), dynamicImports: { ...loader, ...strict } },
  loaderBaseRegister: { src: `import { LightningElement, api } from 'lwc';\nclass B { @api p; }\nexport default class C extends LightningElement {\n` + `  m() { return import('a'); }\n}\n`, dynamicImports: loader },
};
