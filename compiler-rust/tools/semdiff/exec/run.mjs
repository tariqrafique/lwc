// semdiff SD-2a runner: ONE (case, side, run) in a fresh process. Emits one trace JSON on stdout.
//   SD_EXEC_CASE=<case-dir> SD_EXEC_TEMPLATE=<side module> NODE_ENV=test node --import ./register.mjs run.mjs <case-dir>
// <case-dir>/input.json is the input set (generated in Rust, src/exec.rs); <case-dir>/map.json comes from prepare.mjs.
// Exit 0 = trace written (template throws are trace events, not failures). Anything else = harness failure.
// Design: notes/arch-board/proposals/semdiff-SD-2a.md §2–§4.
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { ENGINE_DOM_ESM, serializeNode, trackShadowRoots, describeValue } from '../../../harness/src/csrRender.ts';
import { S, record, build, describe, installShims, drainTimers } from 'semdiff:rt';
import { TEMPLATE_URL } from './hooks.mjs';

const TRACE_SCHEMA = 'semdiff-exec-trace/v0';
const dir = path.resolve(process.argv[2]);
const input = JSON.parse(fs.readFileSync(path.join(dir, 'input.json'), 'utf8'));
const map = JSON.parse(fs.readFileSync(path.join(dir, 'map.json'), 'utf8'));
if (map.engine !== ENGINE_DOM_ESM) throw new Error(`engine mismatch: ${map.engine} != ${ENGINE_DOM_ESM}`);
const realSetImmediate = setImmediate;

// ---- environment: jsdom globals before the engine is evaluated ----------------------------------------------------
const { JSDOM, VirtualConsole } = createRequire(ENGINE_DOM_ESM)('jsdom');
const vc = new VirtualConsole();
vc.on('jsdomError', (e) => record({ e: 'error', src: 'jsdom', err: describe(e) }));
const dom = new JSDOM('<!doctype html><html><head></head><body></body></html>', { virtualConsole: vc });
const win = dom.window;
// DOM interfaces (capitalized names) come from jsdom even where Node 24 has its own (Event, EventTarget, CustomEvent…),
// so `instanceof` agrees with what the engine creates; Node's URL/encoding/Blob stay (module loading uses them).
const KEEP_NODE = new Set(['URL', 'URLSearchParams', 'TextEncoder', 'TextDecoder', 'Blob', 'Buffer']);
for (const k of Object.getOwnPropertyNames(win)) {
    if (k in globalThis && !(/^[A-Z]/.test(k) && !KEEP_NODE.has(k))) continue;
    try {
        globalThis[k] = win[k];
    } catch {}
}
globalThis.window = win;
globalThis.document = win.document;
S.describe = describeValue;
installShims(win, BigInt.asUintN(64, BigInt(input.rng_seed)));
trackShadowRoots();
for (const level of ['log', 'info', 'warn', 'error', 'debug', 'trace']) {
    console[level] = (...args) => record({ e: 'console', level, args: args.map((a) => describe(a)) });
}
win.addEventListener('error', (ev) => {
    record({ e: 'error', src: 'window', err: describe(ev.error ?? ev.message) });
    ev.preventDefault();
});
process.on('unhandledRejection', (r) => record({ e: 'error', src: 'rejection', err: describe(r) }));
process.on('uncaughtException', (r) => record({ e: 'error', src: 'uncaught', err: describe(r) }));

const headStart = new Set(win.document.head.childNodes);
const tick = () => new Promise((r) => realSetImmediate(r));
async function flush() {
    for (let round = 0; round < 16; round++) {
        for (let i = 0; i < 8; i++) await Promise.resolve();
        await tick();
        if (drainTimers() === 0) return;
    }
    throw new Error('semdiff-exec: flush did not quiesce in 16 rounds');
}
function snapshot() {
    return {
        body: serializeNode(win.document.body),
        head: Array.from(win.document.head.childNodes).filter((n) => !headStart.has(n)).map(serializeNode),
    };
}
// Composed-tree preorder of the host: the element, its shadow root's subtree, then its light children.
function walk(el, out) {
    out.push(el);
    const sr = el.shadowRoot;
    if (sr) for (const c of sr.children) walk(c, out);
    for (const c of el.children) walk(c, out);
    return out;
}

// ---- steps ----------------------------------------------------------------------------------------------------------
let E = null;
let C = null;
let host = null;
let ctor = null;
let loaded = false;
const caseUrl = pathToFileURL(dir).href + '/';
const props = (list) => {
    for (const [name, spec] of list) {
        try {
            host[name] = build(spec, name, ctor);
        } catch (err) {
            record({ e: 'throw', at: `set ${name}`, err: describe(err) });
        }
    }
};
const ACTIONS = {
    async load() {
        try {
            await import(TEMPLATE_URL);
        } catch (err) {
            // Resolution failures are the harness's (hooks.mjs / a missing stub), never the module's: fail the run.
            if (err?.code === 'ERR_MODULE_NOT_FOUND' || String(err?.message).includes('semdiff-exec:')) throw err;
            record({ e: 'throw', at: 'load', err: describe(err) });
            return;
        }
        E = await import('semdiff:engine');
        C = (await import(new URL(map.component, caseUrl).href)).default;
        if (map.dyn) ctor = (await import(new URL(map.dyn, caseUrl).href)).default;
        loaded = true;
    },
    async mount(step) {
        host = E.createElement('x-sd', { is: C });
        props(step.props);
        try {
            win.document.body.appendChild(host);
        } catch (err) {
            record({ e: 'throw', at: 'mount', err: describe(err) });
        }
    },
    async set(step) {
        props(step.props);
    },
    async event(step) {
        const els = walk(host, []);
        els.forEach((el, idx) => {
            S.target = { idx, tag: el.localName };
            try {
                el.dispatchEvent(new win.Event(step.type, { bubbles: false, composed: false, cancelable: true }));
            } catch (err) {
                record({ e: 'throw', at: `dispatch ${step.type}`, err: describe(err) });
            }
        });
        S.target = null;
    },
    async unmount() {
        try {
            host.remove();
        } catch (err) {
            record({ e: 'throw', at: 'unmount', err: describe(err) });
        }
    },
};

const steps = [];
for (const step of input.steps) {
    const out = { kind: step.kind, label: step.label };
    if (step.kind !== 'load' && !loaded) {
        out.skipped = 'load-threw';
        steps.push(out);
        continue;
    }
    out.events = S.events = [];
    await ACTIONS[step.kind](step);
    await flush();
    S.events = null;
    out.dom = snapshot();
    steps.push(out);
}

const trace = {
    schema: TRACE_SCHEMA,
    steps,
    helpers: S.helpers,
    reads: [...S.reads].sort(),
    handled: [...S.handled].sort(),
};
process.stdout.write(JSON.stringify(trace));
// The engine may hold timers/handles; the trace is complete, so leave deterministically.
process.exit(0);
