// Collects the 12 harness error-recovery fixtures (read-only) + extra multi-error / 1212 cases into cases.json.
import fs from 'fs'; import path from 'path';
const FX = '/workspace/lwc/packages/@lwc/babel-plugin-component/src/__tests__/fixtures/error-recovery-mode';
const cases = {};
const walk = (d) => { for (const e of fs.readdirSync(d, { withFileTypes: true })) { const p = path.join(d, e.name);
  if (e.isDirectory()) walk(p); else if (e.name === 'config.json') { const { parserOpts, ...rest } = JSON.parse(fs.readFileSync(p, 'utf8'));
    cases[path.relative(FX, d)] = { src: fs.readFileSync(path.join(d, 'actual.js'), 'utf8'), opts: rest }; } } };
walk(FX);
cases['extra/multi-error'] = { opts: {}, src: `import { api, track, wire, LightningElement } from 'lwc';
import { getRecord } from 'lightning/uiRecordApi';
export default class Test extends LightningElement {
  @api dup;
  @api dup;
  @api data;
  @track
  m() {}
  @wire('notAnId') w1;
  @wire(getRecord, { a: 1 }) @api both;
  @api get g() { return 1; }
  @api set g(v) {}
  load() { return import(this.x); }
}
` };
cases['extra/1212-decorated-private-method'] = { opts: {}, src: `import { api, LightningElement } from 'lwc';
export default class Test extends LightningElement {
  @api #m() {}
}
` };
cases['extra/import-before-and-between-classes'] = { opts: {}, src: `import { api, wire, LightningElement } from 'lwc';
const early = import(someVar);
class A extends LightningElement {
  @api onFoo;
  run() { return import(1); }
}
const mid = import(other);
export default class Test extends LightningElement {
  @wire('x') w;
  @api dataX;
}
` };
cases['extra/decorator-used-as-value'] = { opts: {}, src: `import { api, track, LightningElement } from 'lwc';
const t = track;
export default class Test extends LightningElement {
  @api onBar;
  load() { return import(this.y); }
}
` };
cases['extra/epm-renamed-same-line'] = { opts: {}, src: `import { api, wire, LightningElement } from 'lwc';
export default class Test extends LightningElement {
  #a() { return this.#b(); } #b() {} @wire('nope') w; @api dataQ;
  #c() { return import(this.#a()); }
}
` };
fs.writeFileSync(new URL('./cases.json', import.meta.url), JSON.stringify(cases, null, 1));
console.log(Object.keys(cases).length);
