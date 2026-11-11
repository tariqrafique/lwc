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

## 10. Observed spelling is never EQUIVALENT-BY-EXECUTION (AB-L0064 rework: bot/semdiff 27bd96c on main dbbb940, pid 3bf36b4b…)
History:
- First submission babaf01 on 9c96ee4, pid e52f32eb…; Arch AB-L0064 REWORK (notes 41d1de9/109d718).
- This section replaces that text.
- Tip **27bd96c** on main **dbbb940**, pid **3bf36b4b06d6c50ce9a914b563b0929a7a9c62a7**. The pid is the same on
  d46d6de, where the corpus was run.

### 10.1 Problem
D-0196 lists only whitespace and quote/escape spelling as unobservable. These are observed, and only statically, since
no trace step sees them (same values, same calls, same DOM):
- numeric-literal spelling and parens (AB-L0041, via Function#toString);
- template raw text (D-0196);
- property-key form (identifier, string, numeric, computed, method), shorthand vs longhand, and TemplateLiteral vs
  StringLiteral (AB-L0064 F9, recorded in ARCHITECTURE.md).

Exec-eligible pairs carry them in two shapes:
- alone: the rewrite leg says `token-spelling-differs` / `printer-equal-ast-differs`, or `printed-differs` when the
  printer keeps the form;
- inside or next to moved or hoisted code: `printed-differs`.

Either way, equal traces made them EQUIVALENT-BY-EXECUTION.

### 10.2 Proof of the false-pass paths
`semdiff run --exec`, seed 1, on a 12-case probe manifest built from the tests/exec.rs fixtures. The pre-fix binary is
§10 v1 (19818d7, the same patch as babaf01); before §10 existed, main also passed the v1 cases.

| probe | §10 v1 (19818d7) | rework (27bd96c) |
|---|---|---|
| `(stc0)` grouping paren next to the hoist (oracle) | **EXEC_EQUAL** | UNCLASSIFIED `unpaired-parens` |
| `({…})` around the inlined object (rust) | **EXEC_EQUAL** | UNCLASSIFIED `unpaired-parens` |
| hoisted `key: 0` vs inlined `key: 0x0` | **EXEC_EQUAL** | DIFFERENT numeric |
| `"title":` vs `title:` alone | **EXEC_EQUAL** | DIFFERENT property-key |
| same, masked by the hoist | **EXEC_EQUAL** | DIFFERENT property-key |
| `const {_m0}` vs `{_m0: _m0}` alone | DIFFERENT (observe_rewrite) | DIFFERENT property-key |
| same, masked by the hoist | **EXEC_EQUAL** | DIFFERENT property-key |
| `` `hi` `` vs `"hi"` alone | **EXEC_EQUAL** | DIFFERENT string-vs-template |
| same, masked by the hoist | **EXEC_EQUAL** | DIFFERENT string-vs-template |
| trailing comma `[x,]` vs `[x]` (M5: only observe_rewrite sees it) | DIFFERENT | DIFFERENT token-spelling-differs |
| control: hoist only | EXEC_EQUAL | EXEC_EQUAL |
| control: `'lang': '\x65n'` vs `"lang": "en"` next to the hoist | EXEC_EQUAL | EXEC_EQUAL |

The original §10 v1 cases (`75`/`0b1001011` alone and masked, template `h\x69`/`hi` masked) were EXEC_EQUAL on
main 9c96ee4 and stay DIFFERENT.

### 10.3 Algorithm (`tools/semdiff/src/spelling.rs`, `InputSet::observe_rewrite`)
1. **Rewrite reason** (approved as is). `token-spelling-differs` / `printer-equal-ast-differs` → static DIFFERENT. This
   also covers spelling outside the sensitive set when it is the only difference (M5).
2. **Tokens.** oxc lexer (`TokensParserConfig`) plus one AST pass. Every token gets a value `key` and an observed
   `spell`. Sensitive tokens are numbers, parens, template parts and property keys.

   | token | key | spell | sensitive |
   |---|---|---|---|
   | string literal | `s` + decoded value | `string` (quotes/escapes unobservable) | no, it is only a counterpart |
   | no-substitution template | `s` + cooked value | `template:` + raw | yes |
   | template head/middle/tail | `t` + part kind + cooked | raw | yes |
   | numeric literal | `n` + value (`number_key`) | raw | yes |
   | `(` `)` | kind | text | yes |
   | property key (ObjectProperty, BindingProperty, AssignmentTarget*) | `pk` + name (numeric keys by value) | `<kind>[+method]:<form>` | yes |
   | other | kind + text | text | no |

   For property keys, `form` is one of `ident`, `string`, `numeric:<raw>`, `computed` or `shorthand`.
   - **Shorthand** is taken from the AST flag, not from tokens.
   - **Computed keys** count only when the key is a literal or a no-substitution template. Any other computed key
     keeps ordinary tokens.
   - **Role keying.** A key token pairs only with key tokens, so `"a":` vs `a:` pair and then differ in spell, and a
     key never pairs with a value string.
3. **Align.** Myers on the key ids, after trimming the common prefix and suffix. Budget: `MAX_EDITS`.
4. **Walk** the alignment, stopping at the first finding:
   - an aligned pair with a different spell → DIFFERENT (`numeric`, `property-key`, `string-vs-template`,
     `template-raw`);
   - an unaligned run that is all parens → DIFFERENT `parens`;
   - an unaligned run that is template parts only on both sides → DIFFERENT `template-raw`.
5. **Moved code.** Pool every unaligned token per side. For each key whose pool holds a sensitive token (sorted, so the
   result is deterministic), compare the two spell multisets:
   - if the equal-spell matches are fewer than min(|O|,|R|), a mismatch is forced → DIFFERENT;
   - else if |O| ≠ |R|, some token cannot be paired → NotProven `unpaired-<what>@<side>:<idx>:<text>`.
6. `decide` order: a trace divergence first, then comments, then spelling DIFFERENT. NotProven is added to the
   coverage misses → UNCLASSIFIED exec-uncovered. With no static difference and no misses → EXEC_EQUAL
   (`branch_coverage: not-proven`). So EXEC_EQUAL is only possible when every sensitive token is aligned with an
   equal spell, or is paired in a moved pool with an equal spell.

### 10.4 Budget: why 4000
`spelling::MAX_EDITS = 4000` Myers edits on the trimmed middle.

Measured on the H-1 codegen-only corpus at d46d6de (16795 pairs, full streams, no trimming; a measurement-only
instrument that was not committed):
- edit distance p50 0, p99 0, p99.9 50, **max 50**;
- largest module 8864 tokens.

So 4000 is about 80× the worst observed distance. It is far below the ~17.7k edits a fully rewritten largest module
would need, and such a pair fails closed as NotProven, which is acceptable because it means rewritten code.

Cost at the cap:
- backtrace about D² i32 = 64 MB per case, so about 0.4 GB at `--exec-jobs 6`;
- time O((N+M)·D) ≈ 7·10⁷ steps (well under 1 s).

A larger cap would buy nothing on this corpus and would multiply worst-case memory quadratically. The cap is a
constant, so the result is deterministic.

### 10.5 Fail closed
NotProven (→ UNCLASSIFIED, never EXEC_EQUAL) covers:
- `alignment-over-4000-edits`;
- `token-lex-failed`;
- `unkeyed-number@…` (a literal that `number_key` cannot value, e.g. an integer past u128);
- `unpaired-<what>@…`.

A DIFFERENT that is found wins over NotProven, and comments win over both. Value changes in sensitive tokens (`1` vs
`2`) are unpaired by design. The trace usually reports them first (exec-differs). When it does not, they stay
UNCLASSIFIED.

### 10.6 Remaining limit (precise)
- **Positional pairing.** In step 5, pairing is by key across all unaligned runs and ignores position. Take a sensitive
  token removed at one place, with a token of the same key and the same spelling added at another. They pair, and
  neither is flagged. Every token is still examined: the pair has equal key and equal spelling, which D-0196 cannot
  distinguish per token. What can differ is where the token sits, and that is a code move the trace judges.
- **Not in the sensitive set.** These are not examined when another difference masks them: statement punctuation
  (`;`, trailing commas), identifier escapes, regex-literal source, and non-literal computed keys.
  - Alone, they are caught by observe_rewrite (M5).
  - Masked, they are code differences, left to the trace.
  - Widening the set is mechanical (add the token kind as sensitive). Each addition turns more value-or-code changes
    into UNCLASSIFIED when the trace is equal.
- Arch accepted this limit (AB-L0064).

### 10.7 Tests (`cargo test -p semdiff --locked`: 82 passed; spelling unit 11, exec 22)
Unit tests in spelling.rs (11):
- the brief's spellings, alone and next to a difference;
- template raw text, and controls for string escapes and quotes;
- `unpaired_sensitive_tokens_are_not_proven_never_same`;
- `moved_or_hoisted_spelling_is_paired_by_value`: the AB-L0064 probes, i.e. the hoisted number, key quoting masked,
  the grouping paren in both directions, shorthand masked, and backtick masked;
- `property_key_form_and_literal_kind_are_observed`: identifier vs string, quote style as the control, shorthand,
  computed, numeric key, method, binding shorthand, assignment-target shorthand, and backtick vs quote;
- over budget, unlexable, and unkeyed numbers;
- default budget, 3800 vs 4200 edits.

CLI `--exec` table in tests/exec.rs (22 rows, as Arch ran it):
- every §10.2 probe except M5, which has its own test;
- num/paren pure and masked;
- `observe_rewrite_alone_makes_a_spelling_pair_different` (M5), which asserts `divergence` says Same and the CLI
  verdict is DIFFERENT;
- `not_proven_spelling_is_unclassified_never_equivalent`.

### 10.8 Mutation pass (each change applied alone, then `cargo test --lib --test exec`)
| mutation | result |
|---|---|
| M6 step-5 forced mismatch disabled | killed (moved_or_hoisted…, brief_negatives…) |
| M7 step-5 unpaired → no NotProven | killed (moved_or_hoisted…, unpaired…) |
| M8 property-key facts dropped | killed (3 tests) |
| M9 shorthand flag ignored | killed (property_key_form…, moved_or_hoisted…) |
| M10 template spelled like a string | killed (3 tests) |
| M11 aligned-pair spell compare off | killed (6 tests) |
| M12 observe_rewrite no-op | killed (observe_rewrite_alone… only), which closes M5 |
| M13 template parts keyed by raw, not cooked | killed (3 tests) |
| M14 NotProven dropped in `decide` | killed (not_proven…, observed_token_spelling… CLI) |

### 10.9 Corpus (live-tree, NON-REPORTABLE)
- Base and pairs:
  - d46d6de + b54cce3 (same pid as 27bd96c);
  - H-1 pairs content 95b1ed4e…, identical to 6016fa4, 9c96ee4 and dbbb940;
  - registry pinned at notes 67a2b74 (sha 71d5d6e6…).
- Runs: jobs 6 and jobs 4 give **byte-identical** summaries, sha 3b3ee9a6fb93997dd06489607dc95562e5e1436a756a67016e3a091fef99d5ab.
- Buckets: equivalent 16549 (printer-equal 16549, exec-equal 0), different_undecided 246, different_decided 0,
  unclassified 0, crash 0.
- Delta vs §10 v1 on 6016fa4 (summary 36446623…): **no verdict changes**. Every class, reason and first_diff is
  identical, and only `decisions.registry_sha256` differs. Expected: no case was exec-equal, and every rework change
  only narrows EXEC_EQUAL.

## 11. AB-L0064 rework 2: premise (bot/semdiff ad6d47f on main 34175ef, pid d2efcf1e…)
Two pairing attempts (babaf01 / 27bd96c) that shared one premise have now failed the same gate: that token pairing
can prove spelling equality next to a real code difference. Arch's second REWORK (notes 57e9662) still found
EXEC_EQUAL on regex source and flags, on forms swapped across sites, on `$cmp.label` vs `$cmp["label"]`, and on a
masked missing `;`. Each hole is a place pairing certified equality that Function#toString can see.

The subtraction: D-0196 makes Function#toString an observation channel. A template module whose printed code differs
by anything other than whitespace or quote/escape spelling has an observed textual difference. The rewrite leg already
proves exactly that (`printed-differs`, `token-spelling-differs`, `printer-equal-ast-differs`). The exec traces cannot
see those spellings, so equal traces are not evidence the texts agree, and exec must never certify EQUIVALENT for those
pairs. The rewrite first_diff *is* the observation: UNCLASSIFIED is only for cases where nothing is proven either way.

### 11.1 New verdict rule
When traces are equal:
- comments still differ → DIFFERENT `exec-static-comments-differ` (unchanged);
- the rewrite reason is `printed-differs`, `token-spelling-differs`, or `printer-equal-ast-differs` → DIFFERENT
  `static-observed-differs`. `first_diff` is the rewrite `first_diff`; `exec.detail` carries the rewrite reason, the
  rewrite `first_diff`, and `trace: "equal"`;
- otherwise coverage gaps → UNCLASSIFIED `exec-uncovered`;
- otherwise EXEC_EQUAL, `branch_coverage: not-proven`.

A prepare gap without a comment difference stays UNCLASSIFIED `exec-uncovered` (no equal traces). A trace divergence
or a throw on one side only is still DIFFERENT (`exec-differs` / `exec-throw-one-side`). Exec still runs on these
cases: it produces DIFFERENT evidence. It cannot produce EXEC_EQUAL.

`semdiff run --exec` only runs rewrite-UNCLASSIFIED cases (`exec_eligible`). Printer-equal and other rewrite-equivalent
pairs never reach exec, so **EXEC_EQUAL is dead on the CLI path**. It remains reachable if someone calls `decide` on a
printer-equal pair (the AB-L0051 trace-parser control does this with rust-vs-rust). No use was invented for it.

A hoist-only pair is `printed-differs`, so it is DIFFERENT `static-observed-differs`. That is correct under a strict
reading of D-0196, and it is exactly what the pending Migrate question in §11.4 about scoping Function#toString would
change. A quote-style-only pair stays EQUIVALENT-BY-REWRITE and never reaches exec.

### 11.2 What was deleted
`tools/semdiff/src/spelling.rs` (Myers alignment, value-keying, multiset pairing of unaligned runs, `MAX_EDITS`,
`Spelling::{Same,Differs,NotProven}`, `divergence`). It existed only to certify equality next to a code difference.
It never landed on main, so the commit vs main does not show the deletion. `Reason::ExecStaticSpellingDiffers` went
with it. `observe_rewrite` now only records the rewrite reason and `first_diff` so `decide` can refuse EXEC_EQUAL.

### 11.3 D-0258 rules (file rt-1, applied-set rt-0 while PENDING)
D-0258 (locked) extends D-0196's UNOBSERVABLE list by two classes, encoded as rewrite-leg token-spelling rules, not as
exec-leg special cases. The token check runs only after a clean parse, identical printed text, and `Program::content_eq`,
so an AST difference never reaches them.

| id | what it skips | required negatives |
|---|---|---|
| R-TRAILING-COMMA-1 | one comma immediately before `]`, `}` or `)` whose previous token is not a comma and not an opener | a hole is not a trailing comma: `[a,,]` vs `[a,]`, `[,]` vs `[]`; the predicate `is_trailing_comma` rejects a comma after a comma or an opener |
| R-ASI-SEMI-1 | a `;` token on exactly one side | an ASI difference that changes the AST: `a\n(b)` (call) vs `a;(b)` (two statements); `return\nx` vs `return x` |

Both live in `tools/semdiff/rules/rt-1.toml` with `arch_review = "PENDING"`, `decision = "D-0258"`. Loading is
fail-closed per AB-D0003 (6): a PENDING rule is loaded (so Arch can review its tests) but is **not applied**. D-0258
being locked does not replace the per-rule Arch review. The applied-set identity (`rule_table_version()` /
`applied_table_version`) is `rt-0` while every listed rule is PENDING, and becomes `rt-1` once any rule's
`arch_review` is an `AB-…` id. Flipping PENDING to that id is the one-line enable; the version follows the applied
set, so the ratchet and the summary record which rules actually applied.

A trailing-comma-only pair therefore stays `token-spelling-differs` today (and DIFFERENT `static-observed-differs`
after exec). Each rule's positives and required negatives are exercised directly via `compare_forcing_rules`, so Arch
can approve on evidence without the rule applying to `semdiff run`.

A ratchet against an rt-0 baseline does not need `--reset-baseline` while nothing is applied. Approving a rule changes
the applied identity to rt-1 and then a ratchet against rt-0 fails with `baseline-rule-table-mismatch` unless the run
is `--reset-baseline` on a board that records the reset. That is expected.

Member-access form (`a.b` vs `a["b"]`), regex source and flags, and property-key form (F9) stay OBSERVED.

### 11.4 Open question for Migrate
D-0196 currently treats Function#toString as an unrestricted observation channel (anything not listed is observed).
The rule in §11.1 follows from that reading: a hoist, a number respelling, a regex literal, and a swapped key form all
change `tmpl.toString()`, so exec cannot call them equivalent.

Should D-0196 scope the Function#toString channel, for example to the banner comment that `checkVersionMismatch` reads
plus known engine reads? That would make exec-equal meaningful for hoists and similar. Until Migrate decides, a
hoist-only pair stays DIFFERENT.

### 11.5 Tests (`cargo test -p semdiff --locked`: 79 passed)
2 lib, 21 cli, 6 decisions, 22 exec, 10 ratchet, 12 rewrite, 6 rules.
- `pending_rule_does_not_apply` / `pending_trailing_comma_stays_token_spelling_differs`: a PENDING rule does not apply;
  a trailing-comma-only pair stays `token-spelling-differs`.
- Rule positives and required negatives in `tests/rules.rs` go through `compare_forcing_rules`.
- Arch probes: every printed-differs / token-spelling-differs / printer-equal-ast-differs fixture is DIFFERENT
  `static-observed-differs` with `detail.trace = "equal"`. Quote-style-only stays rewrite-equivalent. Hoist-only is
  DIFFERENT.
- `without_the_static_cap_printed_differs_probes_become_exec_equal` documents the false pass if the cap is turned off.

### 11.6 Mutation pass (each change applied alone)
| mutation | result |
|---|---|
| printed-differs-to-UNCLASSIFIED (`static_observed` emits Unclassified) | killed (`static_hoisting_pair_is_different_static_observed`) |
| PENDING-applies (`is_applied` always true) | killed (`pending_rule_does_not_apply`, `pending_trailing_comma_stays_token_spelling_differs`) |
| cap-off (`refuse_exec_equal = false`) | killed (`static_hoisting_pair_is_different_static_observed`); the built-in uncap test documents the EXEC_EQUAL false pass |
| comma skip forced off | killed (`trailing_comma_is_unobservable`, via `compare_forcing_rules`) |
| `is_semicolon` always false | killed (`asi_semicolon_with_identical_ast_is_unobservable`, via `compare_forcing_rules`) |
| elision treated as a trailing comma | killed (`rules::tests::trailing_comma_predicate_rejects_elisions`) |

### 11.7 Corpus (live-tree, NON-REPORTABLE)
- Tree: /workspace/wt/semdiff @ **ad6d47f** on main **34175ef**, pid **d2efcf1e81dbea5429d40a379e8f891ce37d190c**. Oracle b9e8007, clean.
- H-1 pairs content **95b1ed4e…** (identical to 6016fa4 / 9c96ee4 / d46d6de / b87b3fb). Manifest ids/paths de43daea….
- `semdiff run --exec`, seed 1, registry pinned to the same file as b87b3fb (sha 7199549d…): jobs 6 and jobs 4 give
  **byte-identical** summaries, sha256 **05ed7cd6b3872bb481e213ff29de7c48bbe7694ce3c8318f9a3fbabbe0d725d8**.
  rule_table_version **rt-0**, exec.cases_run 246.

| Bucket | n | notes |
|---|---|---|
| equivalent | 16549 | all printer-equal; 0 exec-equal (dead on this path) |
| different_undecided | 246 | exec-differs 71 + exec-throw-one-side 35 + static-observed-differs 140 |
| unclassified | 0 | |
| different_decided | 0 | |
| crash | 0 | |

Class changes vs b87b3fb (summary f53ca75c…): exactly 140 cases moved
UNCLASSIFIED `exec-equal-static-observed` → DIFFERENT `static-observed-differs`. All 140 have rewrite reason
`token-spelling-differs` (the C1 numeric and C2 arrow-paren clusters whose traces agreed). The 71+35 trace divergences
are unchanged. No other class moved. No case became rewrite-equivalent (PENDING rules did not apply; this corpus had
no trailing-comma-only or ASI-only pair that would have been rewritten even if they had).


## 11.8 AB-L0064 r4 (D-0266 trailing-comma AST structure; EXEC_EQUAL deleted; ASI approved)

Tree: /workspace/wt/semdiff @ **1dab7cd** on main **2a5cff8**, pid **1e88047f65012dbcbe92e613f9c83179d66e1985**.
Oracle b9e8007, clean. Design for D-0263 is SD-2c (notes only):
`arch-board/proposals/semdiff-SD-2c-toString-channels.md`. No D-0263 code in this tip.

### Verdict
`Reason::ExecEqual` and the `Class::EquivalentByExecution` construction are deleted.
`Class::EquivalentByExecution` remains in the schema as reserved. When traces are
equal: comments → `exec-static-comments-differ`; rewrite `printed-differs` /
`token-spelling-differs` / `printer-equal-ast-differs` → DIFFERENT
`static-observed-differs`; otherwise UNCLASSIFIED `exec-uncovered`
(`branch_coverage: not-proven` if coverage is empty). The AB-L0051 rust-vs-rust
control asserts byte-equal traces, not a verdict. EXEC_EQUAL is gone from the
CLI path and from `decide`.

A hoist-only pair stays DIFFERENT under D-0196 until SD-2c is approved and
implemented. That is the D-0263 opening, not this tip.

### Rules (file rt-1; applied-set rt-1)
A rule is enabled when `arch_review` holds an `AB-` id (a reviewed code change).
There is no `reviews.tsv` check and no `with_pending_applied`. Ids are read from
`rules/rt-1.toml`; Rust does not duplicate the tagged strings.

| id | arch_review | what it skips | required negatives |
|---|---|---|---|
| R-TRAILING-COMMA-1 | PENDING | a comma whose start sits in the trailing region of an allowlisted list | a last-slot hole is not a trailing comma |
| R-ASI-SEMI-1 | AB-L0064 | a `;` token on exactly one side | an ASI difference that changes the AST |

Allowlist = D-0258 ∪ D-0266: ArrayExpression, ObjectExpression, FormalParameters,
CallExpression, NewExpression, ImportDeclaration, ExportNamedDeclaration,
ObjectPattern, ArrayPattern, ObjectAssignmentTarget, ArrayAssignmentTarget.
The visitor does not walk any other list type as allowed. Under `SourceType::mjs()`
oxc has no other JS production that yields a trailing comma (a sequence expression
cannot end in a comma; `(a,)` is FormalParameters only as an arrow; type-position
commas require TypeScript, which this parse does not use).

A trailing comma is distinguished from an elision by AST element structure, not
token text. For `ArrayExpression`, if the last element is `Elision` there is no
trailing region; otherwise the region is `(last.span.end, node.span.end)`. For
`ArrayPattern` / `ArrayAssignmentTarget`, the elements array is `Vec<Option<_>>`;
a last-slot `None` is a hole and there is no trailing region; a last-slot
`Some(el)` yields the region after `el.span.end`. Object lists have no holes.
A `Kind::Comma` is skipped iff its `start` lies in the innermost allowlisted
node's trailing region.

D-0266: `[a,]=o` == `[a]=o` (assignment, declarator, and param). Holes stay
OBSERVED: `[a,,]=o` != `[a,]=o`; `[,a]=o` stays observed; `[,]=o` != `[]=o`;
`[a,,]` nested in a param is observed; two trailing commas are a hole.

The rule stays PENDING, so a trailing-comma-only pair in `semdiff run` remains
`token-spelling-differs`. Positives and negatives run through
`compare_forcing_rules` and through `trailing_comma_in_d0258_or_d0266_context`
(the AST-element predicate). Because ASI applies, the applied identity is **rt-1**.
A ratchet against an rt-0 baseline reports `baseline-rule-table-mismatch` unless
`--reset-baseline`.

### Tests (`cargo test -p semdiff --locked`: 81 passed)
3 lib, 21 cli, 6 decisions, 21 exec, 10 ratchet, 12 rewrite, 8 rules.
`--locked` succeeds on this box because `/workspace/.cargo/config.toml` patches
parse5 to `/workspace/parse5-rust/crates/parse5`. The committed `Cargo.lock`
still has a `parse5` 0.1.0 entry with no git `source` line (predates this tip;
do not fix it from tools/semdiff).

### Mutations (each change applied alone; all killed)
| mutation | killed by |
|---|---|
| printed-differs-to-UNCLASSIFIED | `static_hoisting_pair_is_different_static_observed` |
| PENDING-applies (`is_applied` always true) | `pending_rule_does_not_apply`, `pending_trailing_comma_stays_token_spelling_differs` |
| drop-pattern-contexts (D-0266 parents record no trailing region) | `trailing_comma_is_unobservable` (`let {a,}=o` vs `let {a}=o`), `ast_elements_distinguish_trailing_comma_from_elision` |
| treat-pattern-elision-as-trailing (`Some(None)` yields a trailing region) | `rules::tests::ast_elements_distinguish_trailing_comma_from_elision` (`[a,,]=o` and `[,a]=o`). The compare-level `pattern_elision_is_observed` still passes because print/ContentEq already differ for holes |
| ASI-enabled-vs-pending (`arch_review` of R-ASI-SEMI-1 set back to PENDING) | `asi_rule_applies_when_approved`, `table_lists_asi_applied_and_comma_pending` |
| comma skip forced off | `trailing_comma_is_unobservable` |
| `is_semicolon` always false | `asi_semicolon_with_identical_ast_is_unobservable` |

### Corpus (live-tree, NON-REPORTABLE)
- Tree: /workspace/wt/semdiff @ **1dab7cd** on main **2a5cff8**, pid **1e88047f65012dbcbe92e613f9c83179d66e1985**.
  Oracle b9e8007, clean.
- H-1 pairs content **95b1ed4e…** (identical to 6016fa4 / 9c96ee4 / d46d6de / b87b3fb / ad6d47f). Manifest ids/paths **de43daea…**.
- `semdiff run --exec`, seed 1, registry pinned to the same file as b87b3fb / ad6d47f (sha **7199549d…**): jobs 6 and jobs 4 give
  **byte-identical** summaries, sha256 **008f7583e7ec56aa471f0079bd33ef75272e4391a977cdeb4bc116c7f9363081**.
  `rule_table_version` **rt-1**, exec.cases_run 246.

| Bucket | n | notes |
|---|---|---|
| equivalent | 16549 | all printer-equal; 0 exec-equal (deleted) |
| different_undecided | 246 | exec-differs 71 + exec-throw-one-side 35 + static-observed-differs 140 |
| unclassified | 0 | |
| different_decided | 0 | |
| crash | 0 | |

Class/reason changes vs ad6d47f (summary 05ed7cd6…, `rule_table_version` rt-0): **0**.
The summary bytes differ only because the applied-set identity is now rt-1 (ASI
approved). vs e6637ba (the pre-D-0266 r3 squash): summaries are byte-identical
(008f7583…); D-0266 does not apply while R-TRAILING-COMMA-1 is PENDING, and this
corpus has no trailing-comma-only or ASI-only pair. vs b87b3fb (f53ca75c…): the
same 140 UNCLASSIFIED `exec-equal-static-observed` → DIFFERENT `static-observed-differs`
already reported for ad6d47f.


## 11.9 AB-L0064 merge conditions (both rules applied; Class::EquivalentByExecution deleted)

Tree: /workspace/wt/semdiff @ **e1fd366** on main **0b903dd**, pid **dc2e9503162ab5c0510aa4961c23feba6c570a72**.
Approved r4 was 681aba0 (pid bc8e8661, notes 868a1de). This tip is that tree plus the two merge conditions, squashed.

### What was deleted
`Class::EquivalentByExecution`: the variant, its `as_str` mapping (`"EQUIVALENT-BY-EXECUTION"`), the `bucket()` arm in `lib.rs` that mapped it onto totals bucket `equivalent`, and the `assert_ne!(…, Class::EquivalentByExecution)` in `tests/exec.rs` (and the same guard in `tests/rules.rs`). Comments that named the reserved class as a live path were rewritten. `tools/semdiff/scripts/cluster.mjs` has no reference. README/doc tables in the crate do not list the class. `by_class` is not a summary key; totals use the five buckets.

Historical v1/v2 summaries still read. `read_totals` and the ratchet (`buckets_of`) parse `totals.{equivalent,different_decided,different_undecided,unclassified,crash}` and each case's `bucket` string. They never parse a verdict `class` of `EQUIVALENT-BY-EXECUTION`. A baseline whose `bucket` is not in `TOTALS_KEYS` fails closed (`unknown bucket`). Removing the variant therefore does not break reading a historical baseline, including 681aba0's summary 008f7583… (0 exec-equal cases; its `equivalent` bucket is all EQUIVALENT-BY-REWRITE).

An exhaustive match in `tests/rules.rs::class_has_no_equivalent_by_execution` fails to compile if the variant is restored.

### Trailing comma approved
`R-TRAILING-COMMA-1.arch_review = "AB-L0064"`. Both rules applied. The any-applied name `rt-1` was the ASI-only identity on 681aba0; it must not be reused. `applied_table_version` is now `rt-0` while nothing is applied, otherwise `rt-1:` plus the sorted tagged ids: **`rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1`**. A ratchet against 681aba0 is `baseline-rule-table-mismatch` unless `--reset-baseline`.

### Tests (`cargo test -p semdiff --locked --offline`: 82 passed)
3 lib, 21 cli, 6 decisions, 21 exec, 10 ratchet, 12 rewrite, 9 rules.

### Mutations
| mutation | killed by |
|---|---|
| restore `Class::EquivalentByExecution` (variant + `as_str` + `bucket` arm) | compile fail: `class_has_no_equivalent_by_execution` is non-exhaustive |
| comma pending vs applied (`arch_review` of R-TRAILING-COMMA-1 set back to PENDING) | `approved_trailing_comma_applies`, `table_lists_both_rules_applied` |

### Corpus (live-tree, NON-REPORTABLE)
- H-1 pairs **95b1ed4e…**, manifest **de43daea…**, registry pin **7199549d…**, seed 1, oracle b9e8007.
- jobs 6 and jobs 4 summaries **byte-identical**, sha256 **64f46d36e28706f18228c3da1d7177717cf1b0c08266c8970aed705da86c19ca**.
- `rule_table_version` **rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1**, exec.cases_run 246.
- Buckets unchanged: 16549 equivalent / 246 different_undecided / 0 unclassified / 0 crash
  (printer-equal 16549, exec-differs 71, exec-throw-one-side 35, static-observed-differs 140).
- Class/reason deltas vs 681aba0 summary **008f7583…**: **none**. No case left DIFFERENT. No case moved to EQUIVALENT-BY-REWRITE. The 246 remain C1 numeric (105, token-spelling, equal traces), C2 arrow parens (35 equal-trace + 1 exec-differs), C3 throw (35), C4–C5 exec-differs (70). None of them has a trailing comma as its sole observed difference, so D-0258/D-0266 applying does not rewrite any pair. The summary hash changed only because the applied-set identity is no longer `rt-1`.
