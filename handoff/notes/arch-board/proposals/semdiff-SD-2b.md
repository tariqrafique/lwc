# semdiff SD-2b — SSR exec leg (D-0195), design only

Owner: LWC Semantic Diff (`bot/semdiff`). Status: **design APPROVED with required
amendments (AB-D0007 @ notes 6373f6c)** — this file is **r2** (doc only; no code until
the amended design is the one Arch would land against). Companion to SD-2a
(`arch-board/proposals/semdiff-SD-2a.md` §1–§11, including r4 / AB-L0064). Decisions:
**D-0195** (SSR reference), **D-0196** (observation model), **D-0028** (sync-first /
engine-server deleted from ship), **D-0275** (scratch under `/workspace/scratch/semdiff/`
only). Board row: `semantic-diff`.

Reuses SD-2a’s runner protocol, verdict schema (`semdiff-verdict/v1`,
`semdiff-summary/v2`), and r4 rule that equal traces never produce
`EQUIVALENT-BY-EXECUTION` when printed source still differs.
`Class::EquivalentByExecution` stays reserved / deleted from the decide path.

Scratch for this workstream: `/workspace/scratch/semdiff/sd2b-premise/` and
`/workspace/scratch/semdiff/sd2b-premise/f1-bundles/` (F1 re-measure).

---

## Changelog

| Rev | Notes commit / review | What changed |
|---|---|---|
| r1 | notes 9b6fc46 | Initial design; module-grain premise (630 reach exec); forks F1–F9 open. |
| **r2** | AB-D0007 @ 6373f6c (APPROVE + amendments) | Premise restated (r4 ⇒ would_certify=0; value = triage + byte-exact HTML lock). F1-grain re-measure. F2 REJECTED (one harness emitter). F6 deleted (§1.3, crosscheck flag, N7). F7 = reuse SD-2a decide path, no new rule-table entry. F5 thin APPROVE with D-0196 gap note. F1/F3/F4/F8/F9 APPROVE. |

---

## 0. Premise (AB-D0007) — value, not would_certify

Under SD-2a **r4** the exec leg has **no equivalent class**. Every pair that reaches
exec ends `DIFFERENT`, `UNCLASSIFIED`, or `CRASH`. So **would_certify = 0 by
construction**. The earlier “630 reach exec” figure (module grain, r1) was a **cost**
estimate, not a value argument, and must not be read as a certification pool.

**Real value of SD-2b:**

1. **Triage.** Separate observable SSR differences (raw HTML or throw →
   `exec-differs` / `exec-throw-one-side`) from **static-only** ones
   (`static-observed-differs`, `exec-static-comments-differ`, spelling). That split
   routes compiler work; the board cannot get it from rewrite alone.
2. **Lock runtime semantics** with **byte-exact** oracle-vs-Rust HTML per mode
   (`sync` / `async` / `asyncYield`). The `ssr-render` row cannot give this lock: it
   judges `formatHTML` against goldens, so whitespace/attribute pretty-printing can
   hide drift that D-0196 still wants observed at the HTML event.

That value is enough **not to defer** (F9 APPROVE), even when today’s raw sync HTML
already matches on successful renders (see §0.1).

No H-1 SSR pairs exist on the board today. H-1 (`TDD_EMIT_PAIRS`) emits
template-compiler modules only. SD-2b needs the harness ssr-render emitter extended
like H-1 (F2 REJECTED as a semdiff-side bundler — §2).

### 0.1 F1-grain re-measure (fixture bundles, research-only)

Grain = **fixture bundle** (`bundleFixture` → `compiled-ssr.mjs`), not per-module
transforms. Measured 2026-11-09 under `/workspace/scratch/semdiff/sd2b-premise/f1-bundles/`:

| Step | How |
|---|---|
| Emit | Harness `test/ssr.render.test.ts` twice (`LWC_COMPILER_BACKEND=js\|rust`, `SSR_MODE=sync`), napi **c4cd4cb09eac…**, oracle b9e8007; copy `.ssr-render-out/{js,rust}-sync-dist` → scratch |
| Rewrite | `semdiff run --gate none` on 359 overlapping bundles → `bundles-rewrite.json` |
| Raw HTML | `raw-html-compare.mjs`: `serverSideRenderComponent(..., 'sync')` on each non-printer-equal pair, **no** `formatHTML` |

| Quantity | N |
|---|---:|
| Fixtures with `config.json` | 387 |
| Vitest ssr-render sync | 384 passed / 5 skipped |
| Overlapping `compiled-ssr.mjs` pairs (js ∩ rust) | **359** |
| Byte-equal bundles | **0** |
| Printer-equal (rewrite) | **6** |
| **Not printer-equal** | **353** |
| Of those 353: raw sync HTML **byte-differ** | **0** |
| Of those 353: raw sync HTML byte-equal | 334 |
| Of those 353: throw both sides | 19 |
| Of those 353: throw one side | 0 |

Rewrite reasons among the 359: `printed-differs` 286, `comments-differ` 49,
`token-spelling-differs` 18, `printer-equal` 6.

**Artifact shas (scratch):**

| File | sha256 |
|---|---|
| `f1-bundles/raw-html-compare.mjs` | `eabd2d7297e1828e9e2ab7ae234cc51ee90c66e0d4a7c0dce5889b2f3e9e40ba` |
| `f1-bundles/bundles-rewrite.json` | `c4cb47fb1f7080018671d5d010336c3b8e5c521950da8cb03bd197de6602699c` |
| `f1-bundles/raw-html-summary.json` | `875153442b38978340e5b651f6cad589b6d913a239003ad6036d734bb1a7ae2c` |

Script: `/workspace/scratch/semdiff/sd2b-premise/f1-bundles/raw-html-compare.mjs` (plus the
vitest emits and `semdiff run` above). Research-only; not a board input.

**Reading:** today every successful sync render is already byte-exact across
compilers, so the exec leg’s HTML lock is a **ratchet against future drift**, and
nearly all of the 353 non-printer-equal bundles are **static-only** triage (or shared
throws). Module-grain r1 numbers (1183 byte-differ / 630 not printer-equal) remain
historical cost context only; they are not the F1 product grain (F8).

---

## 1. Scope

### 1.1 Pairs (primary)

| Side | Compiler | Runtime |
|---|---|---|
| Oracle | `@lwc/ssr-compiler` @ b9e8007 | `@lwc/ssr-runtime` @ b9e8007 |
| Rust | `crates/lwc-ssr` (napi / crate emit) | **same** `@lwc/ssr-runtime` @ b9e8007 |

Both sides run through `serverSideRenderComponent(tag, Component, props, styleDedupe, mode)`
exported by ssr-runtime (also aliased as `renderComponent` in the ESM export list).
Because the runtime is shared, any trace/HTML divergence is a **compiler** difference
(D-0195).

Corpus: the upstream ssr-compiler fixture tree
`packages/@lwc/ssr-compiler/src/__tests__/fixtures/**/config.json` (387). Each case
carries `entry`, optional `props`, optional `styleDedupe`, optional
`experimentalComplexExpressions`, optional `ssrFiles`. Slotted content is **not** a
config key; it comes from child modules under `modules/` (same as upstream
`fixtures.spec.ts` / harness `bundleFixture`).

### 1.2 Runtimes and modes

ssr-runtime accepts `mode ∈ { sync, async, asyncYield }` (DEFAULT `sync`). Harness
already mirrors this (`SSR_MODE`, `bundleFixture`, `ssr.render.test.ts`). Rust
`lwc-ssr` emits all three (`crates/lwc-ssr/src/lib.rs:16`).

**F4 APPROVE:** run **all three modes** as separate case_keys (mode in opts hash).

### 1.3 Out of scope

- CSR / `@lwc/engine-dom` (SD-2a, on main).
- SD-2c / Function#toString channels (DROPPED / DEFERRED per D-0270; would_certify 0).
- Style-only eval fallback.
- `@lwc/engine-server` as a pair product or diagnostic flag (F6 deleted in r2; D-0028 /
  D-0195 ship story unchanged — engine-server is not what this leg runs).
- Landing any `tools/semdiff` SSR exec code before the amended design is the landed
  reference Arch expects.
- Any semdiff-side bundler (F2 REJECTED).

---

## 2. Seam (pair emission) — F2 REJECTED

**Exactly one emitter:** harness `ssrRender.ts` `bundleFixture`, extended the way H-1
`TDD_EMIT_PAIRS` was. Semdiff **only consumes** the manifest + pair bytes. A
semdiff-side bundler (even “temporary, hand off later”) is a second source of truth
and is forbidden. Ownership and sequencing of the emitter extension are routed to
**Migrate** (Arch AB-D0007).

Shape the harness emitter must produce (consumed unchanged by semdiff):

1. For each fixture × mode: `{case_key}.oracle.js` / `{case_key}.rust.js` = the
   **bundled** ESM entry (`bundleFixture`; only the module under test varies across
   sides via FixedOutputBackend when that pattern is used for exec — the official
   pairs are full backend bundles as in ssr-render).
2. **case_key** = `ssr-compiler/<rel>/<sha256(entry+modules)>/<sha256(opts)>` with
   `opts = { targetSSR: true, ssrMode, apiVersion?, experimentalComplexExpressions?,
   propsHash, styleDedupe }`.
3. Manifest schema stays `semdiff-manifest/v0`.

**F1 APPROVE:** pair unit = **fixture bundle**, not per-module transform.

---

## 3. Deterministic inputs and the step script

### 3.1 Single render (plus mode variants)

SSR has no multi-step CSR lifecycle. One logical step per case:

```
serverSideRenderComponent(
  'fixture-test',          // tag, matches harness/upstream
  Component,               // default export of the bundle
  config.props ?? {},      // from config.json; 20 fixtures have props today
  config.styleDedupe ?? true,
  mode                     // sync | async | asyncYield
) → HTML string (or async iterable for asyncYield)
```

- **sync:** one HTML string.
- **async:** `await` the promise → one HTML string.
- **asyncYield:** concatenate async-iterable chunks in order → one HTML string; also
  record per-chunk events in the trace (thin observer, §4).

No DOM mount, no `connectedCallback` CSR sequence. If a future fixture needs multiple
renders with different props, that is a new step list under the same case_key family;
none exist today.

### 3.2 Props and slots

- **Props:** taken verbatim from `config.json` `props` (JSON-serializable). Hashed into
  opts so two prop bags never collide. Empty object when absent.
- **Slots:** driven by the fixture’s child modules / light-DOM content under
  `modules/`, not by a config field. The bundle includes those modules identically on
  both sides when using a FixedOutputBackend research setup; official pairs use full
  backend bundles. Trace records slotted HTML as part of the final string.
- **styleDedupe:** from config; default `true` to match `ssr.render.test.ts`.

### 3.3 Input generator

Reuse SD-2a `InputSet` shape where it still applies (seed, jobs), but the SSR input
set is thinner:

- `seed` (u64) — reserved; v1 render path does not sample.
- `jobs` — same invariance proof as SD-2a: `jobs=1` vs `jobs=N` bit-identical summary
  (P5).
- No union-event / `$api` helper matrix (those are engine-dom). Coverage claims for
  SSR are listed in §7 and are **not** branch-coverage proofs.

---

## 4. Trace: what is recorded

Observer lives under `tools/semdiff/exec/` (not `crates/`, not shipped), re-exports
oracle `@lwc/ssr-runtime`, records without changing return values (AB-D0003).

### 4.1 Thin observer (F5 APPROVE)

**v1 records only:**

1. `serverSideRenderComponent` enter/exit (tag, props sha256, styleDedupe, mode).
2. Throws / rejections (message + name; no stack addresses).
3. Final HTML string (byte-exact).
4. For `asyncYield`: one `chunk` event per yielded string (length + sha256), then the
   final concatenate matching the HTML event.

It does **not** record the full D-0196 call-trace surface (`renderAttrs`,
`addSlottedContent`, `sanitizeHtmlContent`, etc.).

**Gap rule (required by AB-D0007):** the thin observer **observes less than D-0196’s
call trace**. That gap can **only change a DIFFERENT reason label** (e.g. miss a
helper-level divergence and classify as `static-observed-differs` or a coarser
`exec-differs` on HTML). It must **never** produce a clean / equivalent verdict that
a fuller observer would have refused. Under r4 there is no exec-equivalent class
anyway; the thin observer must not invent one, and must not turn a would-be DIFFERENT
into UNCLASSIFIED-as-success.

Anything not listed remains **observed in principle** under D-0196; thinning is a
deliberate instrumentation limit, not a new UNOBSERVABLE set.

### 4.2 HTML output and normalization

**F3 APPROVE: byte-exact HTML.** D-0196’s UNOBSERVABLE set is whitespace and
quote/escape spelling **inside emitted JavaScript**, including through
`Function#toString`. It does **not** authorize HTML pretty-printing. `formatHTML` is
diagnostic-only under `exec.debug` and must not feed `decide`. Kill test 8.2 #2 stays.

### 4.3 Malformed traces

Same as SD-2a: runner protocol failure, missing events, or non-parseable JSON →
process **exit 2** (`CRASH` / harness failure), never a soft UNCLASSIFIED.

---

## 5. Verdict mapping (matches SD-2a r4)

`decide` path after AB-L0064 merge: **no** `EQUIVALENT-BY-EXECUTION` construction.
`Reason::ExecEqual` deleted. Mapping for SSR:

| Condition | Class | reason | bucket |
|---|---|---|---|
| Harness/runner failure, malformed trace | CRASH | `exec-harness-failure` | crash (exit 2) |
| Two runs of one side disagree | CRASH | `exec-nondeterministic` | crash |
| Traces diverge; exactly one side throws | DIFFERENT | `exec-throw-one-side` | different_* |
| Traces diverge otherwise (incl. HTML event) | DIFFERENT | `exec-differs` | different_* |
| Traces equal; comment texts differ | DIFFERENT | `exec-static-comments-differ` | different_* |
| Traces equal; rewrite `printed-differs` / `token-spelling-differs` / `printer-equal-ast-differs` | DIFFERENT | `static-observed-differs` | different_* |
| Traces equal; coverage incomplete / prepare gap | UNCLASSIFIED | `exec-uncovered` (or gap-specific) | unclassified |
| Traces equal; rewrite already equivalent | (rewrite short-circuit; exec not required) | `printer-equal` | equivalent |
| Gaps the runner cannot classify | UNCLASSIFIED | gap reason | unclassified |

Equal trace + printed differences ⇒ **DIFFERENT `static-observed-differs`**, never
equivalent-by-execution.

**F7 (r2):** no new SSR rule-table entry. Comments are already OBSERVED (D-0196). r4
already maps equal-trace + comment drift to DIFFERENT via the existing SD-2a reason
`exec-static-comments-differ` through the **same** `decide` path — one table.

Decisions map / ratchet behave as in SD-1/SD-2a. Summary schema v2 `exec` object gains
`engine: "ssr-runtime"`, `ssr_mode`, same `seed` / `cases_run` / `runner_sha256` /
`oracle_commit` fields.

---

## 6. Determinism

1. **Seeds.** `--exec-seed` accepted; v1 SSR path does not draw from it. Recorded in
   the summary for parity with CSR.
2. **Jobs invariance.** `--exec-jobs N` vs `1` must yield bit-identical per-case
   verdicts and the same summary totals (P5).
3. **Sandbox.**
   - Pin `SSR_RUNTIME_ESM` to the oracle absolute dist (same instance the test file
     imports), matching harness `ssrRender.ts`.
   - No `Date.now` / `Math.random` in the generated SSR fixtures we own; if a fixture
     or runtime path touches them, the observer **traps** by stubbing
     `Date.now → 0`, `Math.random → 0` inside the runner vm context and records a
     `sandbox-trap` debug event. A trap that fires on one side only →
     `exec-differs` (observable).
   - No network, no filesystem writes from the runner.
   - `process.env` filtered to the allowlist SD-2a already uses, plus `SSR_MODE` when
     debugging.

---

## 7. Coverage claims

SSR v1 claims **only**:

- Every case ran `serverSideRenderComponent` to completion or throw on both sides.
- Final HTML event present (or throw event) on both sides.
- For `asyncYield`, chunk events concatenate to the HTML event.

**Branch coverage is not proven.** Incomplete coverage with equal traces →
`UNCLASSIFIED reason=exec-uncovered` with `branch_coverage: not-proven`. Do not
certify equivalence on an uncovered pair (and under r4 there is no exec-equivalent
anyway).

---

## 8. Test plan (positive / negative, mutation kills)

### 8.1 Unit / integration (to be added only after Arch is satisfied with r2)

Positive:

- P1: Identical oracle/oracle bundles → traces byte-equal, rewrite short-circuit or
  exec path stable.
- P2: Fixture with `props` → props hash in enter event; HTML reflects prop values.
- P3: Fixture with slotted child → slotted markup present in the HTML event.
- P4: All three modes on one tiny fixture → three case_keys, independent traces.
- P5: Jobs invariance 1 vs 4.

Negative:

- N1: Mutate one side’s HTML return (observer post-hook) → `exec-differs`.
- N2: Throw on rust side only → `exec-throw-one-side`.
- N3: Truncate trace JSON → exit 2.
- N4: Equal traces + force `printed-differs` in rewrite → `static-observed-differs`
  (r4 guard).
- N5: Comment-only banner drift → `exec-static-comments-differ` (same SD-2a decide path).
- N6: `Math.random` stub fires one side → `exec-differs` (sandbox).

(N7 / engine-server cross-check removed in r2 — F6.)

### 8.2 Mutation kills (each applied alone) — unchanged

Mirror SD-2a §10.8 / §11.6 style: flip one clause in `decide`, expect a named test
red. Minimum set for SSR:

1. Delete `static-observed-differs` branch → N4 goes green (kill).
2. Treat HTML via `formatHTML` before compare → introduce a whitespace-only HTML pair
   that wrongly goes non-DIFFERENT (kill). **Kept under F3.**
3. Drop throw-one-side → N2 misclassified (kill).
4. Accept malformed trace as UNCLASSIFIED → N3 exit≠2 (kill).

---

## 9. Cost and board row

| Item | Estimate |
|---|---|
| Fixture × mode cases | ≤ 387 × 3 ≈ **1161** (minus skips); F1 sync research used **359** compiled pairs |
| Not printer-equal (sync research) | **353** → exec cost under r4 (all end DIFFERENT / UNCLASSIFIED / CRASH) |
| Raw sync HTML byte-differ among those | **0** today (lock = ratchet) |
| Wall time / case | ~SSR render of one fixture; expect ≪ H-1 CSR exec |
| New code (post-design freeze) | `tools/semdiff/exec/ssr-*.mjs`, thin `src/exec.rs` mode switch, tests — **no** bundler |
| Board | Extends `semantic-diff` row; H-3 still renders from `--out` summary. SSR pairs from harness emitter only. |
| Scratch | `/workspace/scratch/semdiff/sd2b-*` only (D-0275) |

Fits the board as the D-0195 half of SD-2. CSR half (SD-2a) is on main; SD-2c dropped.

---

## 10. Forks (AB-D0007 rulings)

| Id | Question | Ruling |
|---|---|---|
| **F1** | Pair unit: per-module vs fixture bundle? | **APPROVE — fixture bundle.** |
| **F2** | Semdiff script emitter vs harness? | **REJECTED** as written. One emitter: harness `bundleFixture` / ssr-render path, extended like H-1. Semdiff consumes only. Ownership/sequencing → Migrate. |
| **F3** | HTML: byte-exact vs `formatHTML`? | **APPROVE — byte-exact.** `formatHTML` debug only; kill test 8.2 #2 kept. |
| **F4** | Modes: sync-only vs all three? | **APPROVE — sync, async, asyncYield** (`lwc-ssr` lib.rs:16). |
| **F5** | Observer depth? | **APPROVE — thin** (enter/exit + throw + HTML + asyncYield chunks). Observes less than D-0196’s call trace; gap may only retarget a DIFFERENT reason, never a clean verdict (§4.1). |
| **F6** | engine-server cross-check? | **Deleted.** §1.3, crosscheck flag, and N7 removed. Dead over 0 pairs. |
| **F7** | SSR comment rule-table entry? | **No new entry.** Reuse SD-2a `exec-static-comments-differ` via the same decide path; comments already OBSERVED (D-0196). |
| **F8** | Module-grain board cases? | **APPROVE — no.** |
| **F9** | Defer SD-2b? | **APPROVE — do not defer.** Value = triage + byte-exact HTML lock; not would_certify. |

---

## 11. Approval gate

AB-D0007 approved the design **with** the r2 amendments in this file. No
`tools/semdiff` SSR exec code and no second bundler until Migrate sequences the
harness emitter and Arch is pointed at this r2 text. After that: implement behind
`--exec` with `engine=ssr-runtime`, keep CSR path untouched, land via normal Arch →
land.sh.

Premise / F1 artifacts (not committed to notes):  
`/workspace/scratch/semdiff/sd2b-premise/` (module dump + rewrite) and
`/workspace/scratch/semdiff/sd2b-premise/f1-bundles/` (bundles, rewrite, raw HTML).
