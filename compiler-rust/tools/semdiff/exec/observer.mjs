// semdiff SD-2a observer (AB-D0003 (ii)): the template's `lwc` import. It re-exports the ONE oracle engine-dom instance
// and overrides only the entry points the compiled template calls, each of which records and then delegates with the
// same arguments and the same return value. Never imported from crates/. Design: semdiff-SD-2a.md §2.
import * as E from 'semdiff:engine';
import { S, record, ser, serRet, recordingProxy, describe } from 'semdiff:rt';

export * from 'semdiff:engine';

function traced(name, f) {
    return function (...args) {
        record({ e: 'call', fn: name, args: args.map(ser) });
        let r;
        try {
            r = f.apply(this, args);
        } catch (err) {
            record({ e: 'throw', fn: name, err: describe(err) });
            throw err;
        }
        record({ e: 'ret', fn: name, v: serRet(r) });
        return r;
    };
}

function wrapApi(api) {
    const w = {};
    for (const k of Object.keys(api)) {
        const f = api[k];
        w[k] = typeof f === 'function'
            ? traced(`$api.${k}`, function (...a) {
                  S.helpers[k] = (S.helpers[k] ?? 0) + 1;
                  return f.apply(this, a);
              })
            : f;
    }
    return Object.freeze(w);
}

const cmpProxies = new WeakMap();
function wrapCmp(cmp) {
    if (cmp === null || typeof cmp !== 'object') return cmp;
    S.cmp ??= cmp;
    let p = cmpProxies.get(cmp);
    if (!p) cmpProxies.set(cmp, (p = recordingProxy(cmp, '')));
    return p;
}

export function registerTemplate(tmpl) {
    record({ e: 'call', fn: 'registerTemplate', args: [ser(tmpl)] });
    const p = new Proxy(tmpl, {
        apply(t, self, args) {
            const [$api, $cmp, ...rest] = args;
            record({ e: 'tmpl', fn: ser(t) });
            const r = Reflect.apply(t, self, [wrapApi($api), wrapCmp($cmp), ...rest]);
            record({ e: 'tmpl-ret', v: serRet(r) });
            return r;
        },
    });
    const r = E.registerTemplate(p);
    record({ e: 'ret', fn: 'registerTemplate', v: ser(r) });
    return r;
}

export const parseFragment = traced('parseFragment', E.parseFragment);
export const parseSVGFragment = traced('parseSVGFragment', E.parseSVGFragment);
export const freezeTemplate = traced('freezeTemplate', E.freezeTemplate);
export const registerComponent = traced('registerComponent', E.registerComponent);
export const registerDecorators = traced('registerDecorators', E.registerDecorators);
export const sanitizeAttribute = traced('sanitizeAttribute', E.sanitizeAttribute);
