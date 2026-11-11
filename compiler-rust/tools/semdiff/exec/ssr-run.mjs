// SD-2b thin observer. One (case, side, run) in a fresh process. Trace JSON on stdout only.
// Records serverSideRenderComponent enter/exit, throw name+message, byte-exact HTML, and
// asyncYield chunk length+sha256. formatHTML is stderr under SD_EXEC_DEBUG and is not a trace field.
// Date.now and Math.random are trapped to 0. A trap is a sandbox-trap event (one side only → exec-differs).
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const TRACE_SCHEMA = 'semdiff-exec-trace/v0';

function sha256(s) {
    return createHash('sha256').update(s).digest('hex');
}

// Same canonical JSON as harness/scripts/ssr-emit-pairs.ts and tools/semdiff canonical_json.
function canonicalJson(v) {
    if (v === null) return 'null';
    const t = typeof v;
    if (t === 'string' || t === 'number' || t === 'boolean') return JSON.stringify(v);
    if (Array.isArray(v)) return `[${v.map(canonicalJson).join(',')}]`;
    if (t === 'object') {
        const keys = Object.keys(v).sort();
        return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(v[k])}`).join(',')}}`;
    }
    throw new Error(`semdiff-ssr: non-json value ${t}`);
}

function bump(map, fn) {
    map.set(fn, (map.get(fn) ?? 0) + 1);
}

const trapCounts = new Map();
Date.now = () => {
    bump(trapCounts, 'Date.now');
    return 0;
};
Math.random = () => {
    bump(trapCounts, 'Math.random');
    return 0;
};

const stray = [];
process.on('unhandledRejection', (err) => {
    stray.push(err);
});

const dir = path.resolve(process.argv[2]);
const input = JSON.parse(fs.readFileSync(path.join(dir, 'input.json'), 'utf8'));
const template = process.env.SD_EXEC_TEMPLATE;
const runtimePath = process.env.SD_SSR_RUNTIME;
if (!template || !runtimePath) throw new Error('semdiff-ssr: SD_EXEC_TEMPLATE and SD_SSR_RUNTIME are required');
if (input.engine !== 'ssr-runtime') throw new Error(`semdiff-ssr: engine ${input.engine}`);
if (input.tag !== 'fixture-test') throw new Error(`semdiff-ssr: tag ${input.tag}`);
if (!['sync', 'async', 'asyncYield'].includes(input.mode)) throw new Error(`semdiff-ssr: mode ${input.mode}`);

const runtime = await import(pathToFileURL(runtimePath).href);
// The ssr-render suite installs this hook on the shared runtime (upstream vi.mock). Without it the
// default throws "sanitizeHtmlContent hook must be implemented" the first time a template calls it.
try {
    runtime.setHooks({ sanitizeHtmlContent: (c) => String(c) });
} catch {
    /* already overridden on this instance */
}
const propsHash = sha256(canonicalJson(input.props));
if (propsHash !== input.props_sha256) {
    throw new Error(`semdiff-ssr: props sha256 ${propsHash} != input ${input.props_sha256}`);
}

const events = [];
const chunks = [];
let concat = '';
const sym = runtime.SYMBOL__GENERATE_MARKUP;
let Component;
let loadError;
try {
    Component = (await import(pathToFileURL(template).href)).default;
} catch (err) {
    loadError = err;
}
if (!loadError && input.mode === 'asyncYield' && Component && sym && typeof Component[sym] === 'function') {
    const orig = Component[sym];
    // The runtime installs the symbol non-writable. A subclass owns a wrapper so chunk yields
    // are recorded and then re-yielded unchanged. The original generator still builds the instance.
    const Wrapped = class extends Component {};
    const wrapped = async function* (...args) {
        for await (const seg of orig(...args)) {
            concat += seg;
            const text = typeof seg === 'string' ? seg : String(seg);
            chunks.push(text);
            events.push({ e: 'chunk', length: Buffer.byteLength(text), sha256: sha256(text) });
            yield seg;
        }
    };
    Object.defineProperty(Wrapped, sym, { configurable: true, writable: true, value: wrapped });
    Component = Wrapped;
}

events.unshift({
    e: 'enter',
    tag: input.tag,
    props_sha256: propsHash,
    styleDedupe: input.styleDedupe,
    mode: input.mode,
});

let html;
let thrown;
if (loadError) {
    thrown = { name: loadError?.name ? String(loadError.name) : 'Error', message: String(loadError?.message ?? loadError) };
} else {
    try {
        const result = await runtime.serverSideRenderComponent(
            input.tag,
            Component,
            input.props,
            input.styleDedupe,
            input.mode,
        );
        if (typeof result !== 'string') throw new Error(`semdiff-ssr: render returned ${typeof result}`);
        html = result;
    } catch (err) {
        thrown = { name: err?.name ? String(err.name) : 'Error', message: String(err?.message ?? err) };
    }
}

// Give a rejection queued during the render a turn before we record the trace.
await new Promise((r) => setImmediate(r));
await new Promise((r) => setImmediate(r));

// Async mode can reject outside the awaited call (the runtime stringifies the template promise).
// Record that rejection and keep the returned HTML. The listener above keeps the process alive.
if (html !== undefined && input.mode === 'asyncYield' && concat !== html) {
    throw new Error(
        `semdiff-ssr: asyncYield chunks do not concatenate to the HTML event (chunks=${chunks.length} concat=${concat.length} html=${html.length})`,
    );
}
if (html !== undefined && stray.length) {
    const err = stray[0];
    events.push({ e: 'html', html });
    events.push({
        e: 'throw',
        name: err?.name ? String(err.name) : 'Error',
        message: String(err?.message ?? err),
    });
} else if (html !== undefined) {
    events.push({ e: 'html', html });
} else {
    events.push({ e: 'throw', name: thrown.name, message: thrown.message });
}

for (const fn of [...trapCounts.keys()].sort()) {
    events.push({ e: 'sandbox-trap', fn, count: trapCounts.get(fn) });
}
events.push({ e: 'exit' });

// Test-only observer post-hooks. Unset in production. They edit the recorded trace, not the runtime return.
const side = path.basename(template).startsWith('rust') ? 'rust' : 'oracle';
const mutate = process.env.SD_EXEC_MUTATE ?? '';
if (side === 'rust' && mutate === 'html-suffix') {
    const ev = events.find((e) => e.e === 'html');
    if (!ev) throw new Error('semdiff-ssr: html-suffix mutation found no html event');
    ev.html += ' ';
}
if (side === 'rust' && mutate === 'throw-rust') {
    const i = events.findIndex((e) => e.e === 'html');
    if (i < 0) throw new Error('semdiff-ssr: throw-rust mutation found no html event');
    events.splice(i, 1, { e: 'throw', name: 'Error', message: 'mutated-throw' });
}
if (side === 'rust' && mutate === 'random-rust') {
    const exit = events.pop();
    events.push({ e: 'sandbox-trap', fn: 'Math.random', count: 1 });
    events.push(exit);
}

if (process.env.SD_EXEC_DEBUG === '1' && html !== undefined) {
    const fmt = path.join(process.env.LWC_ORACLE ?? '', 'scripts/test-utils/format-html.ts');
    try {
        const mod = await import(pathToFileURL(fmt).href);
        console.error(`exec.debug formatHTML sha256=${sha256(String(mod.formatHTML(html)))}`);
    } catch (err) {
        console.error(`exec.debug formatHTML skipped: ${err?.message ?? err}`);
    }
}

const trace = {
    schema: TRACE_SCHEMA,
    handled: [],
    helpers: {},
    reads: [],
    steps: [
        {
            kind: 'render',
            label: 'serverSideRenderComponent',
            events,
            dom: { body: null, head: [] },
        },
    ],
};
process.stdout.write(JSON.stringify(trace));
