# isExplicitImport proof (shipped 9.5.3 vs Rust lwc-js)

Options: `transformSync(src, 'foo.js', { name: 'foo', namespace: 'x', isExplicitImport: true })`, run on shipped
(`/workspace/oracle953`) and on Rust (`cargo run -p lwc-js --example opts -- <input> explicit`).
Rerun with `./run-all.sh`.

| input | contents | shipped form | Rust form | eval |
|---|---|---|---|---|
| `input.js` | @api field, @api get/set, @api method, @track, @wire with a dynamic config, a static field, a named export | `class Foo …` + `export { Foo as default }` | `export default class Foo …` | OBSERVABLY IDENTICAL (`eval-input.json`) |
| `input-reassign.js` | the same, plus `export function swap() { Foo = class Swapped … }` | as above | as above | OBSERVABLY IDENTICAL: `default` is live in both; after `swap()` it reads `Swapped` |
| `input-anon.js` | anonymous `export default class extends …` | `export default _registerDecorators((_Class = class …))` | same structure | OBSERVABLY IDENTICAL (name `_Class` in both) |

`eval/eval.mjs` loads each output as an ES module. It uses stub `lwc` (LightningElement, plus a registerDecorators
that records its calls) and `x/fooAdapter` modules, and deep-compares:
- the export names, and the named export values (`helper(3)`);
- the default's type, `name`, `length` and source head;
- the prototype chain, plus `instanceof LightningElement` and `instanceof Default`;
- static, prototype and instance own-property descriptors, and the instance JSON;
- the number of registerDecorators calls, with every call registering the default;
- the registerDecorators meta: the adapter identity and the wire `config($cmp)` result;
- binding liveness after `swap()`.

**Outcome:** no observable difference, so shipped's `class Foo {}` + `export { Foo as default }` form was not
ported. Per ECMA-262, `export default class Foo {}` exports the local mutable binding `Foo`, which is the same
binding that `export { Foo as default }` exports. Liveness and `.name` therefore agree, and the eval confirms it.
The text still differs (quote style, indentation, and lwc import specifiers split into two declarations). This is
the same accepted whitespace/format class as the non-explicit path, and the harness compares normalized code.
