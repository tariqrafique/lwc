// Single (non-aggregate) error cases, normal mode. ids name shipped 9.5.3's code.
export const CASES = [
  { id: 'js-lwc-1107', filename: 'foo.js', src: 'import {api,LightningElement} from "lwc";\nexport default class T extends LightningElement{ @api dataA; }\n' },
  { id: 'js-lwc-1097-wire', filename: 'foo.js', src: 'import {wire,LightningElement} from "lwc";\nexport default class T extends LightningElement{ @wire("x") w; }\n' },
  { id: 'js-lwc-1121-dyn-import', filename: 'foo.js', src: 'export function f(x){ return import(x); }\n' },
  { id: 'js-lwc-1103-private-method', filename: 'foo.js', src: 'import { api, LightningElement } from "lwc";\nexport default class Test extends LightningElement {\n  @api #m() {}\n}\n' },
  { id: 'js-parse-1007', filename: 'foo.js', src: 'let = ;\n' },
  { id: 'js-1007-decorators-not-enabled', filename: 'foo.js', src: 'export default class { @foo x; }\n' },
  { id: 'js-1198-decorator-sniff', filename: 'foo.js', src: 'export default class { @api x; }\n' },
  { id: 'html-1058', filename: 'foo.html', src: '<template><div></template>' },
  { id: 'html-1058-duplicate-attr', filename: 'foo.html', src: '<template><div if:true={a} if:true={b}></div></template>' },
  { id: 'css-1009', filename: 'foo.css', src: ':host{' },
  { id: 'css-1009-host-context', filename: 'foo.css', src: ':host-context(.a){}' },
  { id: 'opts-1021', filename: 'foo.js', src: 'export const a = 1;\n', opts: { outputConfig: { sourcemap: 'foo' } } },
];
