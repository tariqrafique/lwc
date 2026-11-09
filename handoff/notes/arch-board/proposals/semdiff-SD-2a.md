# semdiff SD-2a — EQUIVALENT-BY-EXECUTION, slice a (CSR / engine-dom, template-module pairs)

Status: implementation addendum to semdiff-SD-0.md §5, under AB-D0003 (exec conditions i–iii), D-0195, D-0196.
Scope: CSR only (`@lwc/engine-dom` @ b9e8007 from the oracle checkout), template-module pairs only (the H-1
`template-diag-codegen-only` manifest). SSR (D-0195: ssr-compiler output vs Rust SSR output, both on
`@lwc/ssr-runtime` @ b9e8007; engine-server = HTML cross-check only) is SD-2b and is NOT built here.

## 0. AB-D0003 exec conditions — how SD-2a meets them
| cond | requirement | SD-2a |
|---|---|---|
| (i) | generator reads the UNION of both sides' `$cmp` chains, listeners, iterator sources | §3: static, oxc, in Rust (`tools/semdiff/src/exec.rs`), over BOTH modules; the slot set is the sorted union |
| (ii) | lwc-import interposer is an OBSERVER, in tools/semdiff/exec, out of crates/ | §1: `tools/semdiff/exec/observer.mjs`; records only; return values and identities untouched; nothing in crates/ |
| (iii) | exec-uncovered stays UNCLASSIFIED | §5: equal traces without full coverage → `UNCLASSIFIED reason=exec-uncovered`, never equivalent |

## 1. Seam (H-2 is NOT landed)
On main ab11f76, `render()`/`norm()` are still private to `harness/test/csr.render.test.ts`; there is no
`mountAndObserve`. Per the brief, SD-2a builds on the smallest stable seam, and leaves the test file as it is:
- **Imported from harness (read-only, unchanged):** `harness/src/csrRender.ts` → `ENGINE_DOM_ESM`, `serializeNode`,
  `trackShadowRoots`, `describeValue`. The import is loaded under Node 24 type-stripping, through a resolve hook that adds
  `.ts` to the file's extensionless relative imports.
- **Re-implemented in tools/semdiff/exec (because there is no exported equivalent):** the multi-step mount/flush
  loop (`render()` is single-shot by design) and a minimal `norm()`. The `norm()` only maps `[object:vm X (n)]` → `(#)`.
  Paths need no normalization, because both sides load the module at the same virtual URL.
- When H-2 lands, the loop moves onto `mountAndObserve`. That swap is a follow-up and does not change the trace format.

**Deviation from SD-0 §5, "What runs":** there is no rollup / `FixedOutputBackend` bundle. An H-1 pair is an already
compiled template module, so the exec leg must run **those bytes, unchanged**. Re-bundling would run the module
through `@lwc/rollup-plugin` again. Instead, Node ESM loader hooks (`exec/hooks.mjs`) resolve:
- `./sd.html` → the side's pair module. Its bytes are served unchanged at ONE fixed virtual URL
  (`file:///__semdiff__/x/sd/sd.html.js`) on both sides, so error text and URLs cannot differ by side.
- `lwc` → `exec/observer.mjs`.
- `*.css` / `*.css?scoped=true` → `export default undefined;`. These are the H-1 fixtures' missing implicit
  stylesheets, and the module's `if (_implicitStylesheets)` guards then see the same value on both sides.
- `<ns>/<name>` → a child stub. It is compiled by the oracle `@lwc/compiler` from
  `export default class extends LightningElement {}`, with an oracle-compiled empty `<template></template>`.
- The component `x/sd` is generated source (§3), compiled by the oracle `@lwc/compiler` `transformSync`
  (`{namespace:'x', name:'sd'}`). Its implicit `./sd.html` import is the pair module. So **only the template module
  differs between sides.**

Runtime: plain `node` (v24) + oracle `jsdom` 29.1.1 (`/workspace/lwc/node_modules`). Window globals are installed
before the engine is imported. `NODE_ENV=test` (the same dev-mode branches vitest runs in). The engine is
`ENGINE_DOM_ESM` (one instance: the observer re-exports it, and the component and stubs import it through `lwc`).
Every (case, side, run) is a fresh process, so engine global counters start identical.

## 2. Observer and trace
`observer.mjs` does `export * from ENGINE_DOM_ESM` and overrides these, each one only recording and then delegating
with identical args and return value:
- `registerTemplate(tmpl)` → `E.registerTemplate(new Proxy(tmpl, {apply}))`. Post-registration writes
  (`tmpl.stylesheets = …`, `tmpl.renderMode`) hit the target directly. The `apply` trap records `tmpl` entry/exit,
  wraps `$api` in a recording object (same keys, each helper → record `(key, args)`, call the original, record the
  return), and wraps `$cmp` in a read-recording proxy (`get`/`has`/`ownKeys` on the component, with the receiver
  = target so getters see the real component). `$slotset` and `$ctx` pass through unchanged.
- `parseFragment`, `parseSVGFragment` (strings + values recorded, so static HTML is in the trace and not only in the
  DOM), `freezeTemplate`, `registerComponent`, `registerDecorators`, `sanitizeAttribute`. Each records name + args.
- Not overridden: everything else (`createElement`, `LightningElement`, `api`, …), re-exported as is.

**Events:**
- `call {fn, args}` / `ret {v}` / `throw {e}`, for each `$api` helper and each `lwc` import listed above.
- `get {path}` / `has {path}` / `keys {path}` for component reads, and for reads of generator values. Generator values
  are recording proxies built *before* they reach the engine, so identity is never swapped.
- `handler {path, type, this_is_cmp, target}` / `fncall {path, args}` for generated functions.
- `console {level, args}`, `error {e}` (window `error` + `unhandledrejection`), `nondet {api}` (§2.2).
Each step ends with `dom` = `serializeNode` over body + head nodes not present at start, run through `norm`.

### 2.1 Arg serialization (stable JSON; no addresses)
- Primitives: JSON. Non-JSON values are tagged: `{"$":"undefined"}`, `{"$":"num","v":"NaN"|"Infinity"|"-Infinity"|"-0"}`,
  `{"$":"bigint","v":…}`, `{"$":"symbol","v":description}`.
- **Functions by identity index:** `{"$":"fn","id":k}`, where k is first-seen order over the whole run, not per step
  as in SD-0. That is stricter: memo reuse across re-renders (`$ctx._m0`) shows as the same id, and fresh closures show
  as new ids.
- **Engine-made objects by identity index:** every object returned by an `$api` helper (VNodes, static parts,
  iterator results) gets `{"$":"node","id":k}` when it appears in a later arg. Its `ret` record carries the shape:
  own enumerable keys in insertion order, excluding `elm`/`owner` (DOM and VM, filled in later by the engine and not
  produced by the template). Nested values are serialized recursively. Together with the call records, this is the VNode
  structure, linear in size.
- Plain objects: `{"$":"obj","p":[[key,value]…]}`, with own enumerable string keys in insertion order (order is
  observable), plus `"frozen":true` when frozen. Arrays: JSON arrays (holes → `{"$":"hole"}`). Cycles → `{"$":"cycle"}`.
  Depth cap is 24 (`{"$":"depth"}`). Serialization runs with read-recording suspended, so the observer never adds
  `get` events.
- Thrown values / console args: `describeValue` (stack dropped) + norm.
- Object identity of plain arg objects (a hoisted `stc0` vs a fresh literal) is not serialized. See §6.

### 2.2 Determinism
- `Date` → a fixed clock (0), for `new Date()` and `Date.now`. `performance.now` → 0.
- `Math.random` → seeded splitmix64 that **also records `nondet {api:"Math.random"}`**. If anything reads randomness, it
  shows in the trace and is identical across sides.
- `setTimeout`/`setInterval`/`requestAnimationFrame`/`queueMicrotask` stay engine-visible: timers go to a virtual
  queue that the flush drains in (delay, seq) order (≤ 1000 callbacks, more = harness failure). Microtasks are real.
- Flush = `await Promise.resolve()` ×8, drain the virtual timers, `await Promise.resolve()` ×8, repeated until quiet (cap 16).
- Each (case, side) runs twice. The two trace hashes must match, or the result is `CRASH reason=exec-nondeterministic`.

## 3. Input generator (Rust, oxc; `src/exec.rs`)
**Why static (oxc) and not trace-driven.** A trace only shows what one side actually executed under the inputs
already chosen. A field the Rust side reads only inside a branch that the current inputs never take (the classic
Rust-only bug) would never show up, so it would never be driven. That is a chicken-and-egg loop that
AB-D0003 (i) rules out. Static extraction over both ASTs sees every read on either side, executed or not. The trace
is used afterwards, for coverage (§5), not for discovery.

**Extraction**, per module. It walks `tmpl` and every nested function with an env `ident → Binding`:
- `$cmp` → `Path([])`. `$api` destructuring (`const {k: api_key} = $api`) builds an alias → helper-key map.
  `$api.k(...)` is resolved too.
- A member chain rooted at an env binding gives a slot path. A computed member with a string/number literal is a
  property. A computed member with any other expression → static gap `computed-member`.
- Iterator: a call to helper `i` whose arg0 resolves to path P, with arg1 a function. Param 0 binds to `P[]`
  (the item); P is marked list.
- for:of record: `const x = {value: xValue, …}`. An object literal whose properties are bound identifiers gives a
  `Rec` binding, so `x.value.id` resolves to `P[].id`.
- Listeners: every `on: {…}` object literal (also under `$ctx` memo `_m0 || ($ctx._m0 = {…})`). The keys are event
  types, and paths inside the values are marked fn.
- A path that is called is marked fn. A path in arg0 of helper `dc` is marked ctor.
- `$cmp` used bare (not as a member object) → gap `bare-cmp`. `dynamicOn` → gap `dynamic-on` (280 of
  the 25k oracle modules use it). `lwc:spread` / `props: $cmp.x` read the whole object, so that slot is a leaf.
- `tmpl.renderMode = "light"` on either side → the component is light DOM.
- Helpers called per side (key set) → for coverage.

**Union:** slots = sorted union of paths (+ kinds) from both sides. Event types = sorted union. Gaps = union.

**Values** are hash-derived per leaf, not drawn from one streaming RNG. Each leaf takes
`splitmix64(sha256(seed ‖ case_key ‖ path ‖ mode)[0..8])` mod the set size. Adding a slot, or sampling a different
case subset, therefore never shifts any other value. The jsdom-side PRNG (Math.random / crypto shims) is seeded with
`rng_seed = splitmix64(sha256(seed ‖ case_key ‖ "rng")[0..8])`, which is recorded in the input set:
- leaf, variant A (mount): one of `"a"`, `"a b"`, `1`, `-1`, `true`, `[]`, `{}`.
- leaf, variant B (alt): one of `undefined`, `null`, `""`, `0`, `false`.
- Under an item, strings/numbers get the item index added (`"a"`→`"a1"`, `1`→`2`), so `key`s are distinct.
- interior → object with its children (B: an object whose children are B values, not a falsy leaf; `x.y` stays
  readable). list → array of `n` items (A: 2, B: 0). fn → recording
  function returning A-leaf `"a"`. ctor → the stub constructor (A) / `null` (B).
- Kind conflict (fn and data on one path) → the path is data, plus gap `kind-conflict`.

**Step script** (deterministic from the slots; capped at 48 steps, overflow → gap `script-truncated`):
`load` (import the template module) → `mount` (all roots = A, set as `@api` props before append) →
`event:<type>` ×(each union type, in A-state) → `set:<root>=B` ×(each non-fn root, in name order; cumulative: earlier
roots stay B) → `len:<list>=3`, `len:<list>=1`
×(each list path; the root is rebuilt in A with only that list's length overridden) → `reset` (all roots = fresh A: a re-render with new identities) → `unmount`.
An event step dispatches `new Event(type, {bubbles:false, composed:false, cancelable:true})` on **every element of the
host's composed tree, in document order**. The action does not depend on either side's recorded listeners, so a dropped
listener shows up as a missing `handler` event, never as a different script.
Component: `@api` per non-fn root; one method per fn root (`handler`/`fncall` recorder, `this === component` checked).
Values are only ever replaced at the root, never mutated (`@api` values are not deep-tracked).

**Input set** = stable JSON `{schema:"semdiff-exec-input/v0", seed, rng_seed, light, roots:[{name, kind:prop|fn}],
stubs, dyn_ctor, events, slots, gaps, steps:[{kind, label, props?: [[root, spec]], type?}]}`. Here `spec` is the value
builder `{"u":1}` (undefined) | `{"l":json}` | `{"o":[[k,spec]]}` | `{"a":[spec…]}` | `{"f":1}` (recording fn) |
`{"c":1}` (stub ctor). It is inlined in the verdict (`exec.input_set`) along with `input_set_sha256`.

**Eligibility.** The leg runs only on cases the rewrite leg left UNCLASSIFIED, excluding parse errors. EQUIVALENT-BY-REWRITE
is already decided. An unparsable side is the rewrite leg's finding, not something the engine would load the same way.
The rewrite verdict's `reason`/`first_diff` are kept under `rewrite`.

## 4. Runner protocol
`semdiff run … --exec [--exec-seed N] [--exec-jobs J]`. It applies only to cases whose rewrite verdict is UNCLASSIFIED
and not a parse error. For each such case Rust does the following:
1. Write `job.json` (input set + side module path).
2. Spawn `node --import exec/register.mjs exec/run.mjs job.json <side>` 2× per side.
3. Read one stable-JSON trace from stdout: `{steps:[{kind, events, dom}], helpers, reads, handled}`.
Runner exit ≠ 0, no/invalid JSON, or a 60 s timeout → harness failure. Rust checks that the oracle HEAD is b9e8007
before any run, and fails closed (internal error) otherwise.

## 5. Verdict rules
Steps are compared in order, by sha256 of each step's stable JSON:
1. Any harness failure on any run → `CRASH reason=exec-harness-failure`.
2. The two runs of a side differ → `CRASH reason=exec-nondeterministic`.
3. The first divergent step, where exactly one side has a `throw`/`error` event the other lacks → `DIFFERENT
   reason=exec-throw-one-side`.
4. Any other divergent step → `DIFFERENT reason=exec-differs`. `first_diff` = `{coords:"exec-step", index, kind,
   event_index, oracle, rust}`. A divergence always counts, whatever the coverage: one input where the behaviour
   differs is enough.
4b. Traces equal (or a prepare gap), but the comment texts in source order differ → `DIFFERENT
   reason=exec-static-comments-differ`, `first_diff={coords:"static-comments", index, oracle, rust}`. D-0196 observes
   comments, statically: they are visible through Function#toString, and engine-dom's version-banner check reads
   `tmpl.toString()`. No trace step sees them, so the leg compares them itself. This rule was found in the smoke: without it, the 155
   banner-only cases would have been EQUIVALENT-BY-EXECUTION, which is a false equivalence under D-0196 (fork F7).
5. All steps equal, but coverage is incomplete → `UNCLASSIFIED reason=exec-uncovered` with `uncovered:[…]`. Coverage
   is complete when ALL of these hold:
   - no static gaps;
   - every `$api` key whose alias side S references was executed on S (`helper-not-run:<side>:<key>`);
   - every union slot was read on at least one side;
   - every union event type produced ≥1 `handler` event.
6. Otherwise → `EQUIVALENT-BY-EXECUTION reason=exec-equal`, `trace_sha256` (= both sides) and `input_set_sha256` set.

Buckets: EQUIVALENT-BY-EXECUTION → `equivalent`. DIFFERENT → `different_decided` / `different_undecided` (the decisions
map applies, as in SD-1). Schemas: verdict `semdiff-verdict/v1` (adds the `exec` object), summary `semdiff-summary/v2`
(adds `exec`: `null` when `--exec` is off, else `{runner_sha256, oracle_commit, engine, seed, cases_run}`). Ratchet
baselines of v1 or v2 are accepted.

## 6. NOT observed (anything not listed here is observed, D-0196)
- Identity of plain (non-function, non-engine-made) objects passed to the engine. Example: a hoisted `stc0` reused
  across renders vs a fresh equal literal. The engine and user code cannot tell them apart except by identity, which
  the engine does not expose. Frozenness IS observed.
- `elm`/`owner` fields of engine-made objects (engine-produced, not template-produced).
- Error stacks (paths/line numbers; same as render()).
- VM counter values in log text (`(#)`).
- Wall-clock timing and timer delays (virtualized). Delay *order* is observed through the drain order.
- `$ctx` writes as such. Their effect is observed through function/node identity in later steps.

## 7. Forks (conservative choice made; Arch to rule)
- **F1, component reads observed.** `get`/`has`/`keys` on `$cmp` and generator values are trace events, so a read
  the oracle does not make (read twice, read eagerly, read both branches) is DIFFERENT. Reason: user getters observe
  reads, and D-0196 says what is unlisted is observed. Alternative: rule reads unobservable, so only API calls + DOM
  count.
- **F2, coverage is stricter than "the diff region was executed".** SD-0 §5 asks that each traced helper the diff
  touches be reached. SD-2a has no reliable diff-region→helper map for re-ordered codegen, so it requires full
  per-side helper coverage + union-slot reads + every event type handled. This gives more UNCLASSIFIED, and never a
  false equivalence. Branch coverage is NOT proven: two branches using the same helpers are not told apart. The A/B
  variants drive every leaf truthy and falsy, which is the cheap mitigation. Arch may require a branch coverage
  instrument (one that would have to rewrite module bytes, which is why it is not done here).
- **F3, fn ids are run-global, not per step.** Stricter than SD-0 (memo reuse across steps is visible).
- **F4, structured script instead of K=6 random steps.** It is deterministic in the slots, so coverage is predictable.
  The seed varies the values only.
- **F5, no rollup / FixedOutputBackend (§1).** The pair bytes run unchanged at a fixed virtual URL.
- **F7, comment difference = DIFFERENT, not UNCLASSIFIED (§5 4b).** Comments are observed (D-0196), so a differing
  comment list is a seen difference. The alternative is UNCLASSIFIED (the SD-1 rewrite-leg stance). Both are
  `--gate strict` failures. DIFFERENT routes them through the decisions map like any other difference. Today this is
  the whole missing-`/*LWC compiler v9.5.3*/` banner population (the Rust emitter drops it; owner LWC Template Rust).
- **F8, oracle location is fixed.** The JS side reaches the oracle through harness/src/ssrRender.ts `LWC`, which is
  hard-coded, so `exec::ORACLE_DIR` is a constant (derived from that line since AB-L0051, §9) that preflight checks (HEAD == b9e8007…, no tracked modifications).
  An env override would check one tree and run another, so there is none.
- **F6, `--exec` is opt-in.** Default `run` behaviour is unchanged apart from the schema bump. The full corpus is not
  run in SD-2a.

## 8. Smoke (live-tree, NON-REPORTABLE)
### 8.1 Current: bot/semdiff ee72af9 on main 9cc1c0a
H-1 manifest regenerated on the rebased tree: 16795 codegen-only cases (diag-exact 63112/63224). Sample of 200 by
sha256(id), sample file sha256 aad453ed…; `semdiff run --exec`, seed 1. Bucket counts: equivalent 192,
different_undecided 8, different_decided 0, unclassified 0, crash 0.
- by_reason: printer-equal 190 (rewrite leg; exec not run), exec-equal 2, exec-differs 8. exec.cases_run = 10.
- All 8 exec-differs first diverge at mount(A):
  - 7 are slot-scoped key namespacing. The oracle passes `"@:N"`, `"@other:N"` or `"@head:N"` as the key to `$api.st`/`$api.dc`;
    rust passes the bare number N. Route: LWC Template Rust ("@slot keys", already routed 23:23).
  - 1 is `$api.d(undefined)` where the oracle first reads `$cmp.name` (complex-expr value loss, already routed).
- jobs 8 vs jobs 5: byte-identical. Wall time is 9s on 8 cores, because exec only runs on rewrite-UNCLASSIFIED cases.

### 8.2 History: main ab11f76 (before the rebase)
H-1 manifest regenerated on bot/semdiff == main ab11f76 (`TDD_EMIT_PAIRS`): 25195 codegen-only cases.
The deterministic sample is `node tools/semdiff/scripts/sample.mjs <manifest> 200 <out>`: the first 200 by
sha256(case id), sample sha256 723baace…. Then `semdiff run --exec`, seed 1. Bucket counts:
equivalent 0, different_undecided 200, different_decided 0, unclassified 0, crash 0.
- by_reason: exec-static-comments-differ 174, exec-differs 26.
- Of the 174, the traces are equal in 173, and 155 of those have complete coverage (they would be EQUIVALENT-BY-EXECUTION once the banner
  matches). 18 are uncovered; 1 is a prepare gap (component-compile).
- In the 26 exec-differs, 25 first diverge at mount(A) and 1 at `set:condition=B`. Seen there: classMap
  vs `attrs.class`, `slotAssignment` vs `attrs.slot`, a missing `svg: true`, a missing `attrs.name` on `$api.s`,
  string vs number `key` (`"@:3"` vs `3`), and one `$api.d(undefined)` where the oracle reads `name`.
- Coverage misses seen: helper-not-run 44, slot-not-read 13, gap:dynamic-on 3, event-not-handled 2,
  gap:component-compile 1.
- Two runs (jobs 8 and jobs 5) are byte-identical. Wall time is about 2m40s for 200 cases on 8 cores.

## 9. AB-L0051 rework (bot/semdiff 65b0e76 on main 5739df7, pid 208b48a3…)
- **Typed trace boundary.** `parse_trace(text, input)` is the only reader of trace JSON. It parses into
  `Trace {handled, helpers, reads, schema, steps}` and `Step {kind, label, body: Ran{events, dom} | Skipped(why)}`,
  with `Dom {body, head}` and `Event` (an object with a string `e`), all `deny_unknown_fields` except the event payload.
  The five top-level fields are required. A step carries `{events, dom}` or `{skipped}` and nothing else, and `load`
  is never skipped. `steps` must be non-empty. It must match the input script position for position (same count, kind
  and label), so a trace starts `load`, `mount(A)`. The ruling says "step 0 = mount(A)". In the script, step 0 is `load`,
  where module-evaluation throws are recorded, so mount(A) is step 1. The check enforces that exact prefix and the whole
  script. A trace that fails any of this is `CRASH exec-harness-failure`.
- **Deleted.** These fallbacks are gone: `unwrap_or(&empty)` on steps and events in `first_divergence`, the
  `as_array().into_iter().flatten()` empties in `has_throw`, and `unwrap_or_default` on helpers plus the
  flatten-empties on reads and handled in `coverage_missing`. All three now take `&Trace`. A length mismatch is still
  reported as `missing-step`, and a kind/label mismatch as `step`. Neither can come out of `parse_trace`.
- **Hash stability.** Fields are declared in alphabetical order, so a parsed trace re-serializes to the same bytes as
  the old sorted `Value`, and `trace_sha256` does not change. A test asserts this.
- **ORACLE_DIR, one source.** `tools/semdiff/build.rs` reads `harness/src/ssrRender.ts` and needs exactly one
  `export const LWC = '<abs>/packages/@lwc';`. It exports `<abs>` as `SEMDIFF_ORACLE_DIR`, and
  `exec::ORACLE_DIR = env!(…)`. Any other shape fails the build. A rebuild runs when that file changes. The F8
  preflight (HEAD == b9e8007…, no tracked modifications) is unchanged.
- **F2.** Every EXEC_EQUAL verdict has detail `{"branch_coverage": "not-proven"}`.
- **Tests (tests/exec.rs, +7).** Each negative feeds one malformed trace to all four runs, both sides alike. That is
  the case that used to compare equal. Covered: steps `[]` or missing, `{}`, empty output; events missing on mount(A),
  on every step, not a list, or an event without `e`; helpers, reads, handled or schema missing or null; load dropped
  (so mount(A) comes first), mount(A) dropped, step 1 relabelled or re-kinded, steps 0 and 1 swapped; a step missing or
  added, unknown fields, a ran step without dom, a skipped load, a wrong schema. All give CRASH exec-harness-failure.
  The control (the real trace) gives EXEC_EQUAL with branch_coverage not-proven. The CLI test checks the emitted
  verdict too. `cargo test -p semdiff --locked`: 69 passed (exec 19).
- The §8 smoke has not been re-run on 65b0e76.

## 10. Observed spelling is never EQUIVALENT-BY-EXECUTION (bot/semdiff babaf01 on main 9c96ee4, pid e52f32eb…)
### 10.1 Problem
D-0196 lists only whitespace and quote/escape spelling as unobservable. Arch (AB-L0041) ruled numeric-literal spelling
and parens inside `tmpl` OBSERVED (Function#toString); D-0196 also observes template raw text exactly. No trace step
can see any of them: the values, calls and DOM are the same. The rewrite leg leaves such pairs UNCLASSIFIED
(`token-spelling-differs`, `printer-equal-ast-differs`), or hides them behind a real code difference
(`printed-differs`). Both shapes are exec-eligible, and equal traces then made them EQUIVALENT-BY-EXECUTION.

### 10.2 Proof it was a false-pass path
Pre-fix binary built from main 9c96ee4 (`semdiff run --exec`, seed 1, 6-case manifest from the tests/exec.rs fixtures):

| case | rewrite reason | 9c96ee4 (pre-fix) | babaf01 |
|---|---|---|---|
| `key: 75` vs `key: 0b1001011` | token-spelling-differs | **EQUIVALENT-BY-EXECUTION exec-equal** | DIFFERENT exec-static-spelling-differs |
| same, next to the stc0 hoist | printed-differs | **EQUIVALENT-BY-EXECUTION exec-equal** | DIFFERENT exec-static-spelling-differs |
| `` api_text(`h\x69`) `` vs `` api_text(`hi`) ``, next to the hoist | printed-differs | **EQUIVALENT-BY-EXECUTION exec-equal** | DIFFERENT exec-static-spelling-differs (template-raw) |
| `.map((foo)=>foo)` vs `.map(foo=>foo)` | token-spelling-differs | UNCLASSIFIED exec-uncovered (callback gap only) | DIFFERENT exec-static-spelling-differs |
| same, next to the hoist | printed-differs | UNCLASSIFIED exec-uncovered | DIFFERENT exec-static-spelling-differs |
| control: `'lang': '\x65n'` vs `"lang": "en"`, next to the hoist | printed-differs | EQUIVALENT-BY-EXECUTION | EQUIVALENT-BY-EXECUTION (unchanged) |

The paren cases only escaped a false pass because of an unrelated coverage gap.

### 10.3 Algorithm (`tools/semdiff/src/spelling.rs`, `InputSet::observe_rewrite`)
1. **Rewrite reason.** If the rewrite leg said `token-spelling-differs` or `printer-equal-ast-differs`, the pair is
   printer-equal and differs only in spelling the printer drops, with quotes and whitespace already ignored. That is a
   static difference whatever exec finds (`what` = the rewrite reason, `rewrite_first_diff` kept).
2. **Masked spelling.** `divergence(oracle, rust)` lexes both modules with oxc (`TokensParserConfig`) and keys every
   token: string literals by decoded value (the AST `StringLiteral.value` at the token's span), numeric literals by
   value (`number_key`: radix/legacy-octal/separators/bigint → `big:<u128>` or `f:<f64 bits>`), everything else by
   kind plus exact text. It removes the common prefix and suffix, then aligns the middle with Myers. Walking the
   alignment, it reports the first of these:
   - a matched number whose raw spelling differs (`numeric`);
   - an unmatched run that is parens only on both sides, one side possibly empty (`parens`: grouping parens, arrow
     parameter parens, `new X` vs `new X()`);
   - an unmatched run that is template-literal parts only on both sides, both non-empty (`template-raw`: equal raw
     text would have matched).
3. `decide` order: comments (§5 4b), then spelling → `DIFFERENT exec-static-spelling-differs`,
   `first_diff.coords = "static-spelling"`. As with comments, a seen static difference does not depend on coverage.

### 10.4 Budget
`spelling::MAX_EDITS = 4000` Myers edits on the middle (after prefix/suffix trimming). The worst-case backtrace is
about MAX_EDITS² i32 (~64 MB). Time is O((N+M)·D). The budget is a constant, so the result is deterministic.

### 10.5 Fail closed
`Spelling::NotProven(reason)` is added to the coverage misses, giving `UNCLASSIFIED exec-uncovered` and never
EXEC_EQUAL. Three causes produce it:
- `static-spelling-not-proven:alignment-over-4000-edits`;
- `…:token-lex-failed` (cannot happen, because exec_eligible already drops parse errors, but it never reads as Same);
- `…:unkeyed-number@i/j`, a numeric literal in an unmatched run that `number_key` cannot value (an integer past u128).
  Such a literal is keyed by spelling, so an equal-valued respelling could hide there.

A found `Differs` wins over not-proven, and comments win over both.

### 10.6 Known limit (stated, not hidden)
A paren token inside an unmatched run that also has other differing tokens on both sides is attributed to that code
difference, which the trace judges. The minimal alignment only produces such a run when the paren sits next to a
replaced token. Property-key spelling (`a:` vs `"a":`) and shorthand (`{a}` vs `{a: a}`) are not checked here: they
are not quote/escape spelling, so by D-0196's default they are observed. **Fork F9 for Arch:** should
`spelling::divergence` also flag them? On the current corpus their population inside exec-equal is measured in
SD-1B-clusters (2026-11-08).

### 10.7 Tests (`cargo test -p semdiff --locked`: 78 passed)
- spelling unit tests (+4):
  - `75`/`0b1001011`, `(foo)=>foo`/`foo=>foo` (both directions), `new X`/`new X()`, `1_000`/`1000`, `1.0`/`1`, each
    alone and next to a real difference;
  - template raw `` `a\x41` ``/`` `aA` `` including a `${}` tail;
  - controls: string escapes, quotes and `\u{41}` give Same;
  - over budget gives NotProven (and the same pair within budget gives Same), unlexable gives NotProven, and an
    unkeyed number gives NotProven while a found difference still wins;
  - default budget: 3800 edits → Same, 4200 → NotProven.
- tests/exec.rs CLI table (+6 rows): num-75 and paren-foo, alone and masked; masked-template-raw; and the
  masked-quotes control, which stays EXEC_EQUAL.
- `not_proven_spelling_is_unclassified_never_equivalent`: real equal traces plus NotProven give UNCLASSIFIED
  exec-uncovered carrying the not-proven marker.
