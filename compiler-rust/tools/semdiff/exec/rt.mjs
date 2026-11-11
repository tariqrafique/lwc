// semdiff SD-2a shared runtime: the trace recorder, the stable arg serializer, generator-value builders and the
// determinism shims. One instance per process (observer.mjs, the compiled component and run.mjs all import it).
// Design: notes/arch-board/proposals/semdiff-SD-2a.md §2. Records only: nothing here changes what the engine or the
// template sees, except the values the generator itself supplies.

const MAX_DEPTH = 24;
export const S = {
    events: null, // current step's event list (null = not recording, e.g. while the runner sets up)
    quiet: 0, // >0 while the observer itself is reading (serialization), so its reads are never traced
    fnIds: new WeakMap(),
    nodeIds: new WeakMap(),
    labels: new WeakMap(), // generator value (proxy or function) → its input path
    nextFn: 0,
    nextNode: 0,
    helpers: Object.create(null), // $api key → calls
    reads: new Set(), // normalized read paths (items[0].id → items[].id)
    handled: new Set(), // event types that reached a generated function
    cmp: null, // the raw component instance (first $cmp seen)
    target: null, // {idx, tag} of the element an event step is currently dispatching on
    describe: (v) => String(v),
};

export function record(ev) {
    if (S.events && S.quiet === 0) S.events.push(ev);
}

export const norm = (s) => s.replace(/\[object:vm (\S+) \(\d+\)\]/g, '[object:vm $1 (#)]');
export const describe = (v) => norm(S.describe(v));

function isDom(v) {
    const N = globalThis.Node;
    return typeof N === 'function' && v instanceof N;
}

/** Stable, address-free serialization (design §2.1). */
export function ser(v) {
    S.quiet++;
    try {
        return serIn(v, 0, new Set());
    } finally {
        S.quiet--;
    }
}

function serIn(v, d, seen) {
    if (v === undefined) return { $: 'undefined' };
    if (v === null || typeof v === 'boolean' || typeof v === 'string') return v;
    if (typeof v === 'number') {
        if (Object.is(v, -0)) return { $: 'num', v: '-0' };
        return Number.isFinite(v) ? v : { $: 'num', v: String(v) };
    }
    if (typeof v === 'bigint') return { $: 'bigint', v: String(v) };
    if (typeof v === 'symbol') return { $: 'symbol', v: v.description ?? null };
    const lab = S.labels.get(v);
    if (lab !== undefined) return { $: 'input', path: lab };
    if (typeof v === 'function') return { $: 'fn', id: fnId(v) };
    const nid = S.nodeIds.get(v);
    if (nid !== undefined) return { $: 'node', id: nid };
    if (isDom(v)) return { $: 'dom', tag: v.localName ?? v.nodeName };
    if (typeof globalThis.Event === 'function' && v instanceof globalThis.Event) {
        return { $: 'event', type: v.type, target: S.target };
    }
    if (d >= MAX_DEPTH) return { $: 'depth' };
    if (seen.has(v)) return { $: 'cycle' };
    seen.add(v);
    try {
        return shape(v, d, seen, null);
    } finally {
        seen.delete(v);
    }
}

function get(o, k) {
    try {
        return Reflect.get(o, k);
    } catch (e) {
        return { $: 'throws', e: describe(e) };
    }
}

function shape(v, d, seen, skip) {
    const out = {};
    if (Array.isArray(v)) {
        const a = [];
        for (let i = 0; i < v.length; i++) a.push(i in v ? serIn(get(v, i), d + 1, seen) : { $: 'hole' });
        const extra = Object.keys(v).filter((k) => !/^(0|[1-9]\d*)$/.test(k));
        if (!extra.length && !Object.isFrozen(v)) return a;
        out.$ = 'arr';
        out.items = a;
        if (extra.length) out.p = extra.map((k) => [k, serIn(get(v, k), d + 1, seen)]);
    } else {
        out.$ = 'obj';
        const proto = Object.getPrototypeOf(v);
        if (proto !== Object.prototype && proto !== null) out.proto = proto?.constructor?.name ?? '?';
        out.p = Object.keys(v)
            .filter((k) => !skip || !skip.has(k))
            .map((k) => [k, serIn(get(v, k), d + 1, seen)]);
    }
    if (Object.isFrozen(v)) out.frozen = true;
    return out;
}

function fnId(f) {
    let id = S.fnIds.get(f);
    if (id === undefined) S.fnIds.set(f, (id = S.nextFn++));
    return id;
}

// Engine-made objects (VNodes, static parts, iterator arrays): identity index + shape minus engine back-references.
const NODE_SKIP = new Set(['elm', 'owner', 'vm']);
export function serRet(v) {
    if (v === null || (typeof v !== 'object') || S.labels.has(v) || S.nodeIds.has(v)) return ser(v);
    const id = S.nextNode++;
    S.quiet++;
    try {
        const sh = shape(v, 0, new Set([v]), NODE_SKIP);
        S.nodeIds.set(v, id);
        return { $: 'node', id, shape: sh };
    } finally {
        S.quiet--;
    }
}

// ---- generator values (design §3) --------------------------------------------------------------------------------
// spec: {"u":1} undefined | {"l":json} literal | {"o":[[k,spec]...]} object | {"a":[spec...]} array | {"f":1} fn | {"c":1} ctor
const normPath = (p) => p.replace(/\[\d+\]/g, '[]');

function readTrap(kind, label, key) {
    if (S.quiet) return;
    const p = typeof key === 'symbol' ? `${label}[@@${key.description}]` : /^(0|[1-9]\d*)$/.test(key) ? `${label}[${key}]` : label ? `${label}.${key}` : key;
    if (typeof key !== 'symbol') S.reads.add(normPath(p));
    record({ e: kind, path: p });
}

export function recordingProxy(target, label) {
    const p = new Proxy(target, {
        get(t, k) {
            readTrap('get', label, k);
            return Reflect.get(t, k, t);
        },
        has(t, k) {
            readTrap('has', label, k);
            return Reflect.has(t, k);
        },
        ownKeys(t) {
            if (!S.quiet) record({ e: 'keys', path: label });
            return Reflect.ownKeys(t);
        },
    });
    if (label) S.labels.set(p, label);
    return p;
}

export function makeFn(label) {
    const f = function (...args) {
        return method(this, label, args);
    };
    S.labels.set(f, label);
    return f;
}

export function method(self, path, args) {
    for (const a of args) if (typeof globalThis.Event === 'function' && a instanceof globalThis.Event) S.handled.add(a.type);
    record({ e: 'fncall', path, this_is_cmp: self === S.cmp, args: args.map(ser) });
    return 'a';
}

export function build(spec, label, ctor) {
    if ('u' in spec) return undefined;
    if ('l' in spec) return spec.l;
    if ('f' in spec) return makeFn(label);
    if ('c' in spec) return ctor;
    if ('a' in spec) {
        const arr = spec.a.map((s, i) => build(s, `${label}[${i}]`, ctor));
        for (const [k, s] of spec.p ?? []) arr[k] = build(s, `${label}.${k}`, ctor);
        return recordingProxy(arr, label);
    }
    if ('o' in spec) {
        const o = {};
        for (const [k, s] of spec.o) o[k] = build(s, `${label}.${k}`, ctor);
        return recordingProxy(o, label);
    }
    throw new Error(`semdiff-exec: bad value spec ${JSON.stringify(spec)}`);
}

// ---- determinism shims (design §2.2) -----------------------------------------------------------------------------
export function splitmix64(seed) {
    let s = BigInt.asUintN(64, BigInt(seed));
    return () => {
        s = BigInt.asUintN(64, s + 0x9e3779b97f4a7c15n);
        let z = s;
        z = BigInt.asUintN(64, (z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n);
        z = BigInt.asUintN(64, (z ^ (z >> 27n)) * 0x94d049bb133111ebn);
        z = z ^ (z >> 31n);
        return Number(z >> 11n) / 2 ** 53;
    };
}

const timers = { q: [], seq: 0, now: 0, ran: 0 };
const MAX_TIMER_CALLBACKS = 1000;
function addTimer(fn, delay, args, interval) {
    const id = ++timers.seq;
    const d = Math.max(0, Number(delay) || 0);
    timers.q.push({ id, at: timers.now + d, d, fn, args, interval });
    return id;
}
function clearTimer(id) {
    timers.q = timers.q.filter((t) => t.id !== id);
}
/** Run every due timer in (at, id) order, advancing the virtual clock; returns how many ran. */
export function drainTimers() {
    let n = 0;
    while (timers.q.length) {
        timers.q.sort((a, b) => a.at - b.at || a.id - b.id);
        const t = timers.q.shift();
        timers.now = t.at;
        if (++timers.ran > MAX_TIMER_CALLBACKS) throw new Error('semdiff-exec: timer storm (> 1000 callbacks)');
        if (t.interval) timers.q.push({ ...t, id: ++timers.seq, at: timers.now + Math.max(1, t.d) });
        n++;
        if (typeof t.fn === 'function') t.fn(...t.args);
    }
    return n;
}

export function installShims(win, seed) {
    const rnd = splitmix64(seed);
    const RealDate = Date;
    class FixedDate extends RealDate {
        constructor(...a) {
            super(...(a.length ? a : [0]));
        }
        static now() {
            return 0;
        }
    }
    const random = () => {
        record({ e: 'nondet', api: 'Math.random' });
        return rnd();
    };
    for (const g of [globalThis, win]) {
        g.Date = FixedDate;
        g.setTimeout = (fn, d, ...a) => addTimer(fn, d, a, false);
        g.setInterval = (fn, d, ...a) => addTimer(fn, d, a, true);
        g.clearTimeout = g.clearInterval = clearTimer;
        g.requestAnimationFrame = (fn) => addTimer(() => fn(0), 16, [], false);
        g.cancelAnimationFrame = clearTimer;
        if (g.performance) {
            try {
                Object.defineProperty(g.performance, 'now', { value: () => 0, configurable: true });
            } catch {}
        }
    }
    Math.random = random;
    win.Math.random = random;
}
