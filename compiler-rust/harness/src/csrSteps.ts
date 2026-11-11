// CSR interaction steps (D-0285). Plans are derived from fixture source; the runner only applies that plan.
// Identity stamps are assigned when a node is created (createElement / createElementNS / cloneNode), never by a
// post-hoc walk, and are emitted beside html so first-mount html bytes stay free of stamp attributes.
import fs from 'node:fs';
import path from 'node:path';
import { capturedShadowRoot } from './csrRender';
import { describeValue } from './csrRender';

export interface FixtureSources {
    /** Every component module. Lifecycle / event scans use all of them. */
    js: string[];
    html: string[];
    /** The host component module. Prop / list / dynamic writes go through this class only. */
    rootJs: string;
    props: Record<string, unknown>;
}

export type StepSpec =
    | { id: string; class: 'prop'; kind: 'api' | 'track' | 'config'; name: string }
    | { id: string; class: 'event'; type: string }
    | { id: string; class: 'list'; op: 'reorder' | 'insert' | 'remove'; name: string }
    | { id: string; class: 'dynamic'; phase: 'A' | 'B' | 'null'; name: string; alt?: string }
    | { id: string; class: 'slot'; op: 'add' | 'remove' | 'replace' }
    | { id: string; class: 'lifecycle' };

const HOOKS = ['connectedCallback', 'renderedCallback', 'disconnectedCallback', 'errorCallback', 'render'] as const;

interface TraceRec { hook: string; id: string; threw?: string; error?: string }
interface ListenRec { node: EventTarget; type: string; fired: boolean; threw?: string; harness: boolean }

interface RuntimeState {
    trace: TraceRec[];
    root: object | null;
    seq: number;
    ids: WeakMap<object, string>;
    stamps: WeakMap<Node, number>;
    nextStamp: number;
    regs: ListenRec[];
    slotNodes: Element[];
    slotFires: number;
    slotThrew: string[];
}

let state: RuntimeState = freshState();
const wrappedCtors = new WeakSet<Function>();

function freshState(): RuntimeState {
    return {
        trace: [], root: null, seq: 0, ids: new WeakMap(), stamps: new WeakMap(), nextStamp: 1,
        regs: [], slotNodes: [], slotFires: 0, slotThrew: [],
    };
}

function tracePush(rec: TraceRec): void {
    state.trace.push(rec);
}

function idOf(cmp: object): string {
    const existing = state.ids.get(cmp);
    if (existing) return existing;
    const id = `c${++state.seq}`;
    state.ids.set(cmp, id);
    return id;
}

function onConstruct(cmp: object): void {
    try {
        if (!state.root) state.root = cmp;
        tracePush({ hook: 'constructor', id: idOf(cmp) });
    } catch { /* tracing must not change construction */ }
}

function wrapMethods(proto: any): void {
    if (!proto) return;
    for (const hook of HOOKS) {
        let orig: any;
        try { orig = proto[hook]; } catch { continue; }
        if (typeof orig !== 'function' || orig.__csrTraced) continue;
        if (!Object.prototype.hasOwnProperty.call(proto, hook)) continue;
        const traced = function (this: any, ...args: any[]) {
            const rec: TraceRec = { hook, id: idOf(this) };
            if (hook === 'errorCallback' && args.length) rec.error = describeValue(args[0]);
            tracePush(rec);
            try {
                return orig.apply(this, args);
            } catch (e) {
                rec.threw = describeValue(e);
                throw e;
            }
        };
        traced.__csrTraced = true;
        try { proto[hook] = traced; } catch { /* sealed prototype: leave the original */ }
    }
}

function copyStatics(from: any, to: any): void {
    for (const key of [...Object.getOwnPropertyNames(from), ...Object.getOwnPropertySymbols(from)] as (string | symbol)[]) {
        if (key === 'prototype' || key === 'length' || key === 'name') continue;
        const desc = Object.getOwnPropertyDescriptor(from, key);
        if (!desc) continue;
        try { Object.defineProperty(to, key, desc); } catch { /* non-configurable on the subclass */ }
    }
    try { Object.defineProperty(to, 'name', { value: from.name, configurable: true }); } catch { /* keep the wrapper name */ }
}

function traceWrap(Ctor: any): any {
    if (typeof Ctor !== 'function' || wrappedCtors.has(Ctor)) return Ctor;
    wrappedCtors.add(Ctor);
    wrapMethods(Ctor.prototype);
    let Wrapped: any;
    try {
        Wrapped = class extends Ctor {
            constructor(...args: any[]) {
                super(...args);
                onConstruct(this);
            }
        };
    } catch {
        return Ctor;
    }
    copyStatics(Ctor, Wrapped);
    wrappedCtors.add(Wrapped);
    return Wrapped;
}

/** Bundle rewrite: route `registerComponent` through the tracer. Rollup rewrites `lwc` to the engine-dom path. */
export function instrumentCsrBundle(code: string): string {
    if (code.includes('__csrRegister')) return code;
    return code.replace(/import\s*\{([\s\S]*?)\}\s*from\s*(['"])([^'"]+)\2\s*;?/g, (full, inner: string, q: string, from: string) => {
        if (from !== 'lwc' && !from.includes('engine-dom')) return full;
        const specs = inner.split(',').map((s) => s.trim()).filter(Boolean);
        const wrappers: string[] = [];
        let changed = false;
        const next = specs.map((spec) => {
            const m = spec.match(/^([A-Za-z_$][\w$]*)(?:\s+as\s+([A-Za-z_$][\w$]*))?$/);
            if (!m) return spec;
            const imported = m[1];
            const local = m[2] ?? m[1];
            if (imported !== 'registerComponent') return spec;
            changed = true;
            const renamed = `__csr_${local}`;
            wrappers.push(`function ${local}(Ctor, meta){ return globalThis.__csrRegister(Ctor, meta, ${renamed}); }`);
            return `registerComponent as ${renamed}`;
        });
        if (!changed) return full;
        return `import { ${next.join(', ')} } from ${q}${from}${q};\n${wrappers.join('\n')}\n`;
    });
}

const g = globalThis as any;
g.__csrRegister = (Ctor: any, meta: unknown, orig: (c: any, m: unknown) => unknown) => orig(traceWrap(Ctor), meta);

/** Stamp a node created outside document.createElement (the host from `new` inside lwc.createElement). */
export function stampNode(node: Node | null | undefined): void {
    stamp(node);
}

function stamp(node: Node | null | undefined): number | undefined {
    if (!node || node.nodeType !== 1) return undefined;
    const existing = state.stamps.get(node);
    if (existing !== undefined) return existing;
    const id = state.nextStamp++;
    state.stamps.set(node, id);
    return id;
}

function stampTree(node: Node): void {
    stamp(node);
    for (const child of Array.from(node.childNodes)) stampTree(child);
}

let patches: (() => void) | null = null;

/** Install creation stamps and listener observation for one render(). Call `restore` in a finally. */
export function installStepRuntime(): { restore: () => void } {
    state = freshState();
    if (patches) patches();
    const doc = document;
    const origCreate = doc.createElement;
    const origCreateNS = doc.createElementNS;
    const origClone = Node.prototype.cloneNode;
    const origAttach = Element.prototype.attachShadow;
    const origAdd = EventTarget.prototype.addEventListener;
    const origRemove = EventTarget.prototype.removeEventListener;
    const wrappedFns = new WeakMap<Function, EventListener>();

    doc.createElement = function (this: Document, tag: string, options?: ElementCreationOptions) {
        const el = origCreate.call(this, tag, options);
        stamp(el);
        return el;
    } as typeof doc.createElement;
    doc.createElementNS = function (this: Document, ns: string | null, tag: string, options?: ElementCreationOptions) {
        const el = origCreateNS.call(this, ns, tag, options);
        stamp(el);
        return el;
    } as typeof doc.createElementNS;
    Node.prototype.cloneNode = function (this: Node, deep?: boolean) {
        const clone = origClone.call(this, deep);
        stampTree(clone);
        return clone;
    };
    // Child components are constructed with `new`, not document.createElement. Shadow attach is that construction.
    Element.prototype.attachShadow = function (this: Element, init: ShadowRootInit) {
        stamp(this);
        return origAttach.call(this, init);
    };

    EventTarget.prototype.addEventListener = function (this: EventTarget, type: string, listener: any, options?: any) {
        if (typeof listener !== 'function') return origAdd.call(this, type, listener, options);
        const harness = !!listener.__csrHarness;
        let wrapped = wrappedFns.get(listener);
        if (!wrapped) {
            wrapped = function (this: any, ev: Event) {
                const rec = state.regs.find((r) => r.node === this && r.type === ev.type);
                if (rec) {
                    rec.fired = true;
                    // throws are recorded by the wrapper below; slotchange uses the same path
                }
                try {
                    return listener.call(this, ev);
                } catch (e) {
                    if (rec) rec.threw = describeValue(e);
                    throw e;
                }
            };
            wrappedFns.set(listener, wrapped);
        }
        if (!state.regs.some((r) => r.node === this && r.type === type)) {
            state.regs.push({ node: this, type, fired: false, harness });
        }
        return origAdd.call(this, type, wrapped, options);
    };
    EventTarget.prototype.removeEventListener = function (this: EventTarget, type: string, listener: any, options?: any) {
        const wrapped = typeof listener === 'function' ? wrappedFns.get(listener) ?? listener : listener;
        return origRemove.call(this, type, wrapped, options);
    };

    const restore = () => {
        doc.createElement = origCreate;
        doc.createElementNS = origCreateNS;
        Node.prototype.cloneNode = origClone;
        Element.prototype.attachShadow = origAttach;
        EventTarget.prototype.addEventListener = origAdd;
        EventTarget.prototype.removeEventListener = origRemove;
        if (patches === restore) patches = null;
    };
    patches = restore;
    return { restore };
}

export function lifecycleTrace(): TraceRec[] {
    return state.trace.map((r) => ({ ...r }));
}

export function traceLength(): number {
    return state.trace.length;
}

function stripComments(s: string): string {
    // Built as a RegExp so fixture analysis can ignore comments. Not a comparator.
    const block = new RegExp('/\\*' + '[\\s\\S]*?' + '\\*' + '/', 'g');
    const line = new RegExp('(^|[^:\\\\])//.*$', 'gm');
    return s.replace(block, '').replace(line, '$1');
}

function fieldsOf(js: string, kind: 'api' | 'track'): string[] {
    const src = stripComments(js);
    const out: string[] = [];
    const re = new RegExp(`@${kind}\\b(?:\\s*\\([^)]*\\))?\\s+(?:(get|set)\\s+)?([A-Za-z_$][\\w$]*)\\s*(\\(|[=;]|\\n)`, 'g');
    let m: RegExpExecArray | null;
    while ((m = re.exec(src))) {
        const accessor = m[1];
        const name = m[2];
        const tail = m[3];
        if (tail === '(' && !accessor) continue; // @api method, not a field
        if (!out.includes(name)) out.push(name);
    }
    return out;
}

function dynamicBindings(html: string): string[] {
    const out: string[] = [];
    const re = /lwc:(?:is|dynamic)\s*=\s*\{\s*([A-Za-z_$][\w$]*)\s*\}/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(html))) if (!out.includes(m[1])) out.push(m[1]);
    return out;
}

function altCtor(rootJs: string, binding: string): string | undefined {
    const src = stripComments(rootJs);
    const re = /@(?:api|track)\b(?:\s*\([^)]*\))?\s+([A-Za-z_$][\w$]*)\s*=\s*([A-Za-z_$][\w$]*)/g;
    const alts: string[] = [];
    let m: RegExpExecArray | null;
    while ((m = re.exec(src))) {
        if (m[1] !== binding && !alts.includes(m[1])) alts.push(m[1]);
    }
    alts.sort();
    return alts[0];
}

function eventTypes(js: string[], html: string[]): string[] {
    const out: string[] = [];
    const add = (t: string) => { if (t && !out.includes(t)) out.push(t); };
    for (const src of html) {
        const text = stripComments(src);
        let m: RegExpExecArray | null;
        const onDir = /(?:^|[^\w])on:([A-Za-z][\w-]*)\s*=\s*\{/g;
        while ((m = onDir.exec(text))) add(m[1]);
        const onAttr = /(?:^|[^\w])on([A-Za-z][\w-]*)\s*=\s*\{/g;
        while ((m = onAttr.exec(text))) add(m[1]);
        const lwcOn = /lwc:on\s*=\s*\{\s*(['"])([^'"]+)\1\s*\}/g;
        while ((m = lwcOn.exec(text))) add(m[2]);
    }
    for (const src of js) {
        const text = stripComments(src);
        const re = /\.addEventListener\(\s*(['"`])([^'"`]+)\1/g;
        let m: RegExpExecArray | null;
        while ((m = re.exec(text))) add(m[2]);
    }
    return out.sort();
}

function listBindings(html: string[]): string[] {
    const out: string[] = [];
    const re = /(?:for:each|iterator:\w+)\s*=\s*\{\s*([A-Za-z_$][\w$]*)\s*\}/g;
    for (const src of html) {
        let m: RegExpExecArray | null;
        while ((m = re.exec(src))) if (!out.includes(m[1])) out.push(m[1]);
    }
    return out.sort();
}

function hasSlot(html: string[]): boolean {
    return html.some((src) => /<slot\b/i.test(src));
}

function hasLifecycle(js: string[]): boolean {
    return js.some((src) => {
        const text = stripComments(src);
        return /\b(?:connectedCallback|renderedCallback|disconnectedCallback|errorCallback|constructor)\s*\(/.test(text)
            || /\brender\s*\(/.test(text);
    });
}

/** Deterministic step list. A class with nothing to drive contributes no steps. */
export function analyzeFixture(sources: FixtureSources): StepSpec[] {
    const steps: StepSpec[] = [];
    const api = fieldsOf(sources.rootJs, 'api');
    const track = fieldsOf(sources.rootJs, 'track').filter((n) => !api.includes(n));
    const dyn = sources.html.flatMap(dynamicBindings).filter((n, i, a) => a.indexOf(n) === i).sort();
    const propNames = new Set<string>([...api, ...track]);
    const configKeys = Object.keys(sources.props).filter((k) => !propNames.has(k)).sort();

    const propSteps: StepSpec[] = [
        ...api.filter((n) => !dyn.includes(n)).sort().map((name) => ({ id: `prop:api:${name}`, class: 'prop' as const, kind: 'api' as const, name })),
        ...track.filter((n) => !dyn.includes(n)).sort().map((name) => ({ id: `prop:track:${name}`, class: 'prop' as const, kind: 'track' as const, name })),
        ...configKeys.filter((n) => !dyn.includes(n)).map((name) => ({ id: `prop:config:${name}`, class: 'prop' as const, kind: 'config' as const, name })),
    ];
    steps.push(...propSteps);

    for (const type of eventTypes(sources.js, sources.html)) steps.push({ id: `event:${type}`, class: 'event', type });

    const lists = listBindings(sources.html).filter((n) => propNames.has(n) || Object.prototype.hasOwnProperty.call(sources.props, n));
    for (const name of lists) {
        steps.push({ id: `list:${name}:reorder`, class: 'list', op: 'reorder', name });
        steps.push({ id: `list:${name}:insert`, class: 'list', op: 'insert', name });
        steps.push({ id: `list:${name}:remove`, class: 'list', op: 'remove', name });
    }

    for (const name of dyn) {
        const alt = altCtor(sources.rootJs, name);
        steps.push({ id: `dynamic:${name}:A`, class: 'dynamic', phase: 'A', name, ...(alt ? { alt } : {}) });
        if (alt) steps.push({ id: `dynamic:${name}:B`, class: 'dynamic', phase: 'B', name, alt });
        steps.push({ id: `dynamic:${name}:null`, class: 'dynamic', phase: 'null', name });
    }

    if (hasSlot(sources.html)) {
        steps.push({ id: 'slot:add', class: 'slot', op: 'add' });
        steps.push({ id: 'slot:remove', class: 'slot', op: 'remove' });
        steps.push({ id: 'slot:replace', class: 'slot', op: 'replace' });
    }

    if (hasLifecycle(sources.js.length ? sources.js : [sources.rootJs])) steps.push({ id: 'lifecycle', class: 'lifecycle' });
    return steps;
}

function alternate(value: unknown): unknown {
    if (Array.isArray(value)) {
        if (value.length >= 2) return value.slice().reverse();
        return value.concat(['step']);
    }
    if (value === null || value === undefined) return 'step';
    switch (typeof value) {
        case 'string': return `${value}\u00b7`;
        case 'number': return Number.isFinite(value) ? value + 1 : 0;
        case 'boolean': return !value;
        case 'object': {
            try {
                const o = { ...(value as object) } as Record<string, unknown>;
                o.__step = typeof o.__step === 'number' ? (o.__step as number) + 1 : 1;
                return o;
            } catch {
                return { __step: 1 };
            }
        }
        default: return 'step';
    }
}

function readField(cmp: object, name: string): unknown {
    return (cmp as any)[name];
}

function writeField(cmp: object, name: string, value: unknown): void {
    (cmp as any)[name] = value;
}

function hostCmp(): object | null {
    return state.root;
}

function insertedItem(arr: unknown[]): unknown {
    const last = arr[arr.length - 1];
    if (typeof last === 'string') return 'step-insert';
    if (typeof last === 'number') return last + 1;
    if (typeof last === 'boolean') return !last;
    if (last && typeof last === 'object') {
        try { return { ...(last as object), __step: 'insert' }; } catch { return 'step-insert'; }
    }
    return 'step-insert';
}

export function treePath(node: Node, host: Node): string {
    if (node === host) return 'host';
    const parts: string[] = [];
    let cur: Node | null = node;
    while (cur && cur !== host) {
        const parent: Node | null = cur.parentNode;
        if (!parent) break;
        const idx = Array.prototype.indexOf.call(parent.childNodes, cur);
        parts.push(String(idx));
        if (parent.nodeType === 11) {
            parts.push('s');
            cur = (parent as ShadowRoot).host;
            continue;
        }
        cur = parent;
    }
    parts.reverse();
    return parts.join('/');
}

function walkElements(node: Node, visit: (el: Element) => void): void {
    if (node.nodeType === 1) {
        const el = node as Element;
        visit(el);
        const sr = capturedShadowRoot(el);
        if (sr) for (const c of Array.from(sr.childNodes)) walkElements(c, visit);
    }
    if (node.nodeType === 1 || node.nodeType === 11) {
        for (const c of Array.from(node.childNodes)) walkElements(c, visit);
    }
}

export function identityMap(host: Element): Record<string, number> {
    const out: Record<string, number> = {};
    walkElements(host, (el) => {
        const id = state.stamps.get(el);
        if (id !== undefined) out[treePath(el, host)] = id;
    });
    return out;
}

function listenerNodes(host: Element, type: string): ListenRec[] {
    const found: ListenRec[] = [];
    walkElements(host, (el) => {
        for (const rec of state.regs) {
            if (rec.node === el && rec.type === type && !rec.harness) found.push(rec);
        }
    });
    // Also listeners on shadow roots themselves (rare).
    found.sort((a, b) => treePath(a.node as Node, host).localeCompare(treePath(b.node as Node, host)));
    return found;
}

function findSlots(host: Element): Element[] {
    const slots: Element[] = [];
    walkElements(host, (el) => {
        if (el.localName === 'slot') slots.push(el);
    });
    return slots;
}

const slotListener = function (this: EventTarget, ev: Event) {
    state.slotFires += 1;
    if ((ev as any).__csrThrew) state.slotThrew.push((ev as any).__csrThrew);
};
(slotListener as any).__csrHarness = true;

function armSlots(host: Element): void {
    for (const slot of findSlots(host)) {
        if (!state.slotNodes.includes(slot)) {
            state.slotNodes.push(slot);
            slot.addEventListener('slotchange', slotListener as EventListener);
        }
    }
}

let slotCarrier: Element | null = null;

function makeSlotNode(text: string): HTMLElement {
    const el = document.createElement('span');
    el.setAttribute('data-csr-step', text);
    el.textContent = text;
    return el;
}

/** Apply one step's stimulus. Observations (html, fired flags, slotchange) are read after the flush. */
export function actStep(host: Element | undefined, spec: StepSpec): string | undefined {
    const cmp = hostCmp();
    try {
        if (spec.class === 'prop' || spec.class === 'list' || spec.class === 'dynamic') {
            if (!cmp) return 'no component instance';
            if (spec.class === 'prop') {
                const cur = readField(cmp, spec.name);
                if (typeof cur === 'function') return 'function value left to the dynamic steps';
                writeField(cmp, spec.name, alternate(cur));
                return;
            }
            if (spec.class === 'list') {
                const cur = readField(cmp, spec.name);
                if (!Array.isArray(cur)) return `list ${spec.name} is not an array`;
                if (spec.op === 'reorder') writeField(cmp, spec.name, cur.length >= 2 ? cur.slice().reverse() : cur.concat(['step-reorder']));
                else if (spec.op === 'insert') writeField(cmp, spec.name, cur.concat([insertedItem(cur)]));
                else writeField(cmp, spec.name, cur.slice(0, Math.max(0, cur.length - 1)));
                return;
            }
            const cur = readField(cmp, spec.name);
            if (spec.phase === 'A') {
                if (typeof cur === 'function') writeField(cmp, spec.name, cur);
                return;
            }
            if (spec.phase === 'B') {
                const alt = spec.alt ? readField(cmp, spec.alt) : undefined;
                if (typeof alt !== 'function') return `no second ctor on ${spec.alt ?? spec.name}`;
                writeField(cmp, spec.name, alt);
                return;
            }
            writeField(cmp, spec.name, null);
            return;
        }
        if (spec.class === 'event') {
            if (!host) return 'no element';
            for (const rec of state.regs) {
                if (rec.type === spec.type) { rec.fired = false; delete rec.threw; }
            }
            const nodes = listenerNodes(host, spec.type);
            if (!nodes.length) return 'no-listener';
            for (const rec of nodes) {
                try {
                    rec.node.dispatchEvent(new Event(spec.type, { bubbles: true, composed: true, cancelable: true }));
                } catch (e) {
                    rec.threw = describeValue(e);
                }
            }
            return;
        }
        if (spec.class === 'slot') {
            if (!host) return 'no element';
            state.slotFires = 0;
            state.slotThrew = [];
            armSlots(host);
            if (spec.op === 'add') {
                slotCarrier = makeSlotNode('slot-add');
                host.appendChild(slotCarrier);
            } else if (spec.op === 'remove') {
                if (slotCarrier?.parentNode) slotCarrier.parentNode.removeChild(slotCarrier);
                slotCarrier = null;
            } else {
                const prev = makeSlotNode('slot-old');
                const next = makeSlotNode('slot-new');
                host.appendChild(prev);
                prev.replaceWith(next);
                slotCarrier = next;
            }
            return;
        }
        return;
    } catch (e) {
        return describeValue(e);
    }
}

export interface StepExtras {
    note?: string;
    handlers?: { type: string; path: string; fired: boolean; threw?: string }[];
    ids?: Record<string, number>;
    slotchange?: { fires: number; threw?: string[] };
    lifecycle?: TraceRec[];
}

export function observeStep(host: Element | undefined, spec: StepSpec, note: string | undefined): StepExtras {
    const extra: StepExtras = {};
    if (note && note !== 'no-listener') extra.note = note;
    if (spec.class === 'event' && host) {
        const nodes = listenerNodes(host, spec.type);
        extra.handlers = nodes.map((rec) => ({
            type: spec.type,
            path: treePath(rec.node as Node, host),
            fired: rec.fired,
            ...(rec.threw ? { threw: rec.threw } : {}),
        }));
        if (!nodes.length) extra.handlers = [];
    }
    if (spec.class === 'list' && host) extra.ids = identityMap(host);
    if (spec.class === 'slot') {
        extra.slotchange = {
            fires: state.slotFires,
            ...(state.slotThrew.length ? { threw: state.slotThrew.slice() } : {}),
        };
    }
    if (spec.class === 'lifecycle') extra.lifecycle = lifecycleTrace();
    return extra;
}

export function readSsrSources(dir: string, entry: string, props: Record<string, unknown>): FixtureSources {
    const jsFiles: string[] = [];
    const htmlFiles: string[] = [];
    const walk = (d: string) => {
        if (!fs.existsSync(d)) return;
        const ents = fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name));
        for (const e of ents) {
            const p = path.join(d, e.name);
            if (e.isDirectory()) walk(p);
            else if (e.name.endsWith('.js')) jsFiles.push(p);
            else if (e.name.endsWith('.html')) htmlFiles.push(p);
        }
    };
    walk(dir);
    jsFiles.sort();
    htmlFiles.sort();
    const parts = entry.split('/').filter(Boolean);
    const leaf = parts[parts.length - 1] ?? '';
    const rootPath = path.join(dir, 'modules', ...parts, `${leaf}.js`);
    const rootJs = fs.existsSync(rootPath) ? fs.readFileSync(rootPath, 'utf8') : '';
    return {
        js: jsFiles.map((p) => fs.readFileSync(p, 'utf8')),
        html: htmlFiles.map((p) => fs.readFileSync(p, 'utf8')),
        rootJs,
        props,
    };
}
