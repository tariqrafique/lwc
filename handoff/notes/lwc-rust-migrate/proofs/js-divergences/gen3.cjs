const P = { enablePrivateMethods: true };
const c = {
  'anon-api-pm': "export default class { #m() {} @api x; }",
  'anon-api-pm-after': "export default class { @api x; #m() {} }",
  'anon-api-pm-ml': "export default class {\n  #m() { return 1; }\n  #n() {}\n  @api x;\n}",
  'anon-foo-pm': "export default class { #m() {} @foo x; }",
  'anon-api-pf-pm': "export default class { #m() {} #f = 1; @api x; }",
  'nosuper-api-pm-use': "export default class A { #m() {} @api x; y() { this.#m(); } }",
  'two-cls': "class B { #a() {} }\nexport default class { #m() {} @api x; }",
  'pm-before-cls': "class B { #a() {} #b() { this.#a(); } }\nclass C { @api x; }\nexport default B;",
  'cls-deco': "@foo class B { #a() {} }\nexport default B;",
  'cls-deco-long': "class Z { #aaaaaaaaaaaa() {} }\n@foo class B { x() {} }\nexport default B;",
};
const out = [];
for (const [k, v] of Object.entries(c)) { out.push({ name: k, src: v, filename: 'foo.js', opts: P }); out.push({ name: k + '-off', src: v, filename: 'foo.js', opts: {} }); }
require('fs').writeFileSync('t3.json', JSON.stringify(out, null, 1));
