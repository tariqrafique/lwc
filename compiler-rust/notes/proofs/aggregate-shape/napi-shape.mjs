// JS-visible shape of the installed napi error objects (Phase A), for the Harness hand-off.
import { createRequire } from 'module'; import fs from 'fs';
const napi = createRequire(import.meta.url)('/workspace/lwc-compiler-rust/harness/lwc-compiler-napi.node');
const cases = JSON.parse(fs.readFileSync(new URL('../recovery-errors/cases.json', import.meta.url)));
const keys = (o) => Object.keys(o).map((k) => `${k}${o[k] === undefined ? '(undefined)' : o[k] === null ? '(null)' : ''}`);
const r = napi.transform(cases['extra/decorator-used-as-value'].src, 'foo.js', { name: 'foo', namespace: 'x', experimentalErrorRecoveryMode: true });
console.log('aggregate keys:', keys(r.error).join(', '));
console.log('entry keys:', keys(r.error.errors[0]).join(', '));
console.log(JSON.stringify({ ...r.error, message: r.error.message.split('\n') }, null, 1));
const s = napi.transform('import {api,LightningElement} from "lwc";\nexport default class T extends LightningElement{ @api dataA; }\n', 'foo.js', { name: 'foo', namespace: 'x' });
console.log('single keys:', keys(s.error).join(', '), JSON.stringify(s.error.location));
