# semdiff SD-0: deterministic semantic-equivalence checker (design + v0 skeleton)

Owner: LWC Semantic Diff (bot/semdiff). Status: **design APPROVED with conditions (AB-D0003 @ dc885d0)**; the conditions are folded in below and marked `[AB-D0003 (n)]`. v0 code: AB-L0023 REWORK, resubmitted (tip and patch-id in `state/semdiff.md`). Decisions cited: D-0195 (SSR reference), D-0196 (observation model). Board row: `semantic-diff`.
Skeleton: `tools/semdiff` on bot/semdiff (sha in `state/semdiff.md`). Location: `arch-board/` had no `proposals/`
convention; the only precedent is the flat `land-gate-proposal.md`. This file creates `proposals/` as the brief asked.

## 0. Mandate (restated)
For each corpus case, every byte difference between the JS-oracle output and the Rust output gets exactly ONE class.
No LLM sits anywhere in the verdict path, and every verdict is byte-reproducible.
- **EQUIVALENT-BY-REWRITE**: both outputs parse with oxc, and the ASTs are equal after a *versioned, Arch-approved* rule table runs.
- **EQUIVALENT-BY-EXECUTION**: both modules run on the real oracle engines over a *recorded*, seeded input and update set. Their traces and DOM/HTML are identical. The verdict holds for that input set only.
- **DIFFERENT**: the case maps to an owning-bot bug, or to a `decisions.tsv` id (`different_decided`) through the semdiff case_key map (§2), and through nothing else.
- **UNCLASSIFIED / CRASH**: the tool could not decide, or it panicked. These are first-class buckets and are never folded into the others.

Gate: `different_undecided = unclassified = crash = 0`. It starts as a ratchet (§8) and later becomes strict.

## 1. Survey (facts, with sources)
**Numbers.** The only reportable source is official board `ac1b61fb239a` (store `official/ac1b61fb239a/`, commit c731fb6, napi acfc2e0095b1, oracle b9e8007 clean, published 2026-11-01T21:58:37-08:00, newest `published=` as of 22:22 PST). It is PROVISIONAL per PLAN until a post-AB-0007 board is published. Every number below comes from `template-diag.json` in that dir (schema template-diag-diff/v1, engine crate, driver 5b84a39feceb). That report covers 63,224 cases: pass 45,041, diagPass 62,588, diagMismatch 636, **codegenOnly 17,547**, oracleError 0. The napi engine report (`template-diag-napi.json`) has codegenOnly 17,079 and seam-reject 1,120.

| source (template-diag) | codegenOnly / cases |
|---|---|
| template-fixtures | 292 / 567 |
| integration-wtr | 564 / 895 |
| integration-wdio | 73 / 117 |
| ssr-compiler-fixtures | 233 / 348 |
| lwc-other | 23 / 39 |
| lightning-base-components (LBC) | 408 / 2,257 |
| lbc-recipes | 122 / 1,218 |
| mut:apiVersion | 2,404 / 5,033 |
| mut:apiVersion-edge | 1,952 / 3,882 |
| mut:options | 5,865 / 11,402 |
| mut:insert-root | 840 / 6,119 |
| mut:insert-elem | 3,958 / 21,821 |
| mut:attr-dup | 0 / 879 |
| mut:attr-value | 590 / 7,825 |
| mut:attr-drop | 223 / 822 |

**Not reportable (live tree / A/B):**
- `state/template.md` A/B says template's stack takes 15,563 → 2,021 (svg:true 631, slot-parent key 582, tmpl.slots order 342, sanitizedHtml parens 180, other 286).
- main has since moved to 48c44ae (it landed stcN hoisting, quoted keys, and $ctx handler memo after c731fb6), so 17,547 is stale for main.
- semdiff measures nothing until it has its own official row.

**How codegen-only is derived** (`harness/scripts/template-diag-diff.mjs`, owned by Template Diagnostics):
- The corpus is deterministic: fixtures plus real sources plus mutations (cap 40), deduped by sha256 of the template. The case id is the array index.
- The crate engine compares `@lwc/template-compiler` `compile()` against `lwc_template::compile` (driver `tools/template-diag-diff/driver`).
- Each side reports `fnv1a(code minus /*LWC compiler v…*/, with ALL [ \t\n\r\f\v]+ deleted)`.
- `codegen-only` means no diag buckets, both sides ok, and the hashes differ (compare() ~L314).
- **Only hashes cross the worker boundary (L209). No code is persisted, and the official report has no per-case codegen records** (`mismatches[]` holds only the 636 diag cases). So semdiff cannot consume the 17,547 today. Pairs have to be re-emitted (§7, H-1).
- **Soundness hole (filed as AB-0017, owner Template Diagnostics; measured by corpus `template-diag-pass-ws`, §6):** whitespace is stripped inside string and template literals too. `"a b"` and `"ab"` hash equal, so "pass" can hide real text and attribute differences. semdiff's v0 test `negative_whitespace_inside_string_literal_not_equivalent` pins the fix. `COVERAGE.tsv` v2 already demotes template-diag from template_codegen for this reason.

**oxc access.**
- Rust: `oxc = "0.156"` in `[workspace.dependencies]` (Cargo.lock 0.156.0). lwc-js uses features ast_visit, codegen, transformer, semantic. `ContentEq` (span-insensitive AST equality, generated for all nodes; f64 compared by bits) is in `oxc::span`.
- JS side: there is no oxc napi or parser in the harness. All oxc work stays in Rust.

**Execution machinery to reuse** (Harness-owned; no fork):
- `harness/src/backend.ts`: `CompilerBackend {id, transform(src, filename, opts)}`, `JsCompilerBackend`, `RustCompilerBackend`, `getBackend()`.
- `harness/src/ssrRender.ts`: `lwcBackendPlugin(backend, opts)` (the @lwc/rollup-plugin port), `bundleLwc(backend, {entrySource, pluginOptions, external, paths, extraPlugins, onWarning})`, `bundleFixture`, `fixturePluginOptions`, `listFixtures`, `SSR_RUNTIME_ESM`.
- `harness/src/csrRender.ts`: `ENGINE_DOM_ESM`, `bundleSsrFixtureCsr`, `bundleTemplateFixtureCsr(backend, fx, onWarning)` (virtual `x/<name>` component plus the template plus child stubs), `trackShadowRoots`, `serializeNode` (deterministic: inlines shadow roots and adoptedStyleSheets), `describeValue`.
- `harness/test/csr.render.test.ts`: `render(code, rel, props)` imports, `createElement('fixture-test')`, sets props, appends, flushes 3×`setTimeout(0)`, serializes body and head, captures console and errors, then normalizes with `norm()` (scratch paths and the `vm (N)` counter). **It is file-local, not exported.**
- `harness/test/ssr.render.test.ts`: `runtime.serverSideRenderComponent('fixture-test', C, props, styleDedupe, SSR_MODE)` → `formatHTML`.
- No board row renders through `@lwc/engine-server`. COVERAGE row `engine-server/fixtures.spec` has ssr_render=`dec:D-0028` ("delete engine-server"). Resolved by **D-0195** (§5): the SSR reference is ssr-runtime on both sides, and engine-server is a test-time HTML cross-check only.

**Board wiring.** `harness/board.sh` has ROWS, the DIAG list, and the `template_diag_row` pattern: build a driver into its own target dir, `rm -f` the stale JSON, run the tool, then a summary script prints one board line, and a missing or invalid report puts the row in `rowfail` and makes it `!! BOARD FAILED`. `semantic-diff` copies that pattern (§7).

## 2. Architecture
```
corpus adapter (JS, Harness/Diagnostics-owned hooks)        semdiff (Rust, tools/semdiff)            exec leg (JS, SD-2)
 template-diag-diff.mjs TDD_EMIT_PAIRS ─┐                     manifest.json ─► load ─► per case:
 lbc whole-component compile (Harness) ─┼─► pairs/*.js + ──►   1 rewrite leg: parse(oxc) both → rule table rt-N → printer+ContentEq+tokens+comments
 csr/ssr fixture bundles (later)       ─┘   manifest.json      2 if not equal and exec-eligible: exec request ──► exec driver (vitest, jsdom)
                                                               3 decisions map (case-key → D-id)            ◄── trace.json (hash) per side
                                                             ─► summary.json (one file, atomic) ─► board.sh semantic-diff row
```
- **Rewrite leg** (Rust, pure): `compare(oracle, rust) → Verdict`, deterministic and side-effect free. Classes are assigned only by fixed code and the versioned rule table.
- **Execution leg** (SD-2, JS): one driver that *imports* the harness modules above. Rust stays the single writer of verdicts and summaries. The exec driver only emits trace files with hashes, and Rust compares the hashes. That gives one authority for class assignment.
- **Decisions leg [AB-D0003 (3)]**: ONE source, the semdiff map `tools/semdiff/decisions.tsv` (`case_key  decision_id  why`), which lands in SD-1. A DIFFERENT case whose key is in the map becomes `different_decided`; every other DIFFERENT is `different_undecided`.
  - The map is validated with the coverage-gate's id rules. Any violation is an internal error (exit 2, no summary), never a silent pass:
    - every id matches `^D-[0-9]{4}$`;
    - no duplicate `case_key` rows;
    - every id resolves to a row in `lwc-rust-migrate/decisions.tsv`;
    - that row's result is `locked`.
  - **The manifest carries no `decision_id`.** Adapters (template-diag, lbc, fixture bundlers) produce cases, not judgements, so they must not author decisions. v0 rejects a manifest case with `decision_id`, or any other unknown key, with exit 2 (AB-L0023).
  - `case_key` = the manifest case `id`, which adapters must make stable: `corpus/source/sha256(template)/sha256(opts)`. Index ids change when the corpus is edited, so they are not valid keys.
- **Why a workspace member** (this deviates from the `tools/template-diag-diff/driver` standalone precedent):
  - semdiff's judge must use the *same* oxc the compiler crates use, and `[workspace.dependencies]` pins exactly that.
  - land.sh's `cargo check --workspace --all-targets` then compiles it on every landing, so it cannot rot.
  - It depends on no compiler crate, so it cannot pick up compiler bugs.
  - Accepted [AB-D0003 (1)]. The old H-4 concern is moot: since AB-0005 (main 672ac09), land.sh runs `cargo test --workspace`, so semdiff's tests gate every landing.

## 3. Verdict schema (`semdiff-verdict/v0`, emitted by v0)
```json
{ "schema": "semdiff-verdict/v0",
  "class": "EQUIVALENT-BY-REWRITE | EQUIVALENT-BY-EXECUTION | DIFFERENT | UNCLASSIFIED | CRASH",
  "reason": "printer-equal | parse-error-oracle | parse-error-rust | printed-differs | printer-equal-ast-differs | token-spelling-differs | comments-differ | panic | exec-equal | exec-differs | exec-uncovered | decided",
  "observation_model": "D-0196",                  // the decisions.tsv id that defines "observable" [AB-D0003 (2)]
  "rule_table_version": "rt-0",
  "rules_applied": ["R-ID@rev", "..."],           // sorted, unique; only rules that changed either AST
  "input_set_sha256": null | "<hex>",              // exec leg: sha256 of the stable-JSON input set
  "trace_sha256": null | {"oracle": "<hex>", "rust": "<hex>"},
  "oracle": {"sha256": "<hex>", "parse_errors": 0, "printed_sha256": "<hex>|null"},
  "rust":   {"sha256": "<hex>", "parse_errors": 0, "printed_sha256": "<hex>|null"},
  "first_diff": null
              | {"coords": "printed", "line": <1-based line in the oxc-PRINTED text>, "oracle": "<≤200 chars>", "rust": "<≤200 chars>"}
              | {"coords": "tokens",  "index": <0-based token index>, "oracle": "<token spelling>|null", "rust": "<token spelling>|null"} }
```
- Stable JSON: serde_json pretty with a trailing `\n`, keys in fixed construction order, and no timestamps, paths, or pids, so it is byte-reproducible (tested).
- The schema version bumps on any field change.
- The rule-table version bumps on any rule addition, removal, or revision. A rule-table bump re-baselines the ratchet.
- v0 equality requires all five:
  1. both parse as ESM with zero oxc diagnostics;
  2. `Codegen` output with `CommentOptions::disabled()` is byte-identical;
  3. `Program::content_eq` holds (this guards against printer collisions);
  4. the token streams are identical: same kind and exact source spelling for every token, except string-literal tokens, which compare by decoded value (canonical WTF-8, so lone surrogates are kept);
  5. the comment texts in source order are identical.

  Check (4) exists because checks (2) and (3) erase more than D-0196 allows. Both the printer and `ContentEq` drop trailing commas, `;` vs ASI, numeric spelling (`0x10`/`16`, `1.0`/`1`), and identifier escapes, and v0 before AB-L0023 called those pairs EQUIVALENT. All of them are visible through `Function#toString` and none is listed in D-0196, so they are observed. With (4), the only spelling v0 can ignore is exactly D-0196's list: whitespace, which is never a token, and string quote/escape spelling. Each class has a test (`negative_unlisted_spelling_is_observed`, `negative_template_raw_text_is_observed`, `positive_string_quote_and_escape_spelling_is_unobservable`).

  Check (5) exists because comments are runtime-observable through `Function#toString`, and engine-core's version-mismatch check regexes the `/*LWC compiler vX*/` banner out of function source. oxc codegen also preserves "only some leading comments", so the printer alone would drop comment diffs silently.
- **Observation model: D-0196 (Migrate, locked; accepted in AB-D0003 (2)).** Every verdict and summary cites it as `observation_model: "D-0196"`.
  - UNOBSERVABLE: whitespace, and string-literal quote/escape spelling (`'a'` vs `"a"`, `"\x41"` vs `"A"` with the same value). This holds **even through `Function#toString`**: the port is a breaking cut with no byte identity of function source in either direction.
  - OBSERVED statically: comments in order (engine-core reads the LWC version banner through `Function#toString`), and template-literal raw text exactly (`String.raw` and tagged templates read it).
  - OBSERVED by execution: the runtime-API call trace with args, and the serialized DOM/HTML after every step (§5).
  - **Default is OBSERVED.** Any difference D-0196 does not list is observable until a new D-id says otherwise. In code this means a check may only *remove* a difference that is on D-0196's list. A rule (§4) that normalizes anything else needs its own D-id in addition to its AB review. v0 has no code path that calls an unlisted difference equivalent; check (4) above is the enforcement.

## 4. Rule table (format + v1 candidates; v0 ships ZERO rules)
**Format.** `tools/semdiff/rules/rt-N.toml`, one `[[rule]]` per rule:
- `id`
- `rev`
- `kind`: an AST-pass name implemented in Rust under `src/rules/<id>.rs`
- `justification`
- `preconditions`
- `eval_order_argument`
- `positive_tests` and `negative_tests`: paths under `tests/rules/<id>/{pos,neg}/*.js`, each pair being an `a.js`/`b.js`
- `arch_review`: an `AB-…` id with APPROVE

**Loading is fail-closed and APPROVE-gated [AB-D0003 (6)].** At startup semdiff loads `rt-N.toml` and checks every rule's `arch_review` against `arch-board/reviews.tsv`: the row must exist, have verdict `APPROVE`, and match the rule's `id@rev`. Any rule without such a row makes the whole run an internal error (exit 2, no summary). semdiff never skips that rule and runs on. Rules are reviewed one at a time, never en bloc. Each review needs the rule's own positive and negative tests and, when the rule normalizes something D-0196 does not list, the D-id that makes it unobservable. Rules are AST→AST passes applied to BOTH sides to a fixpoint, in table order, and the `rules_applied` list is recorded. A rule may only *normalize* (map both spellings to one canonical form). It may never delete a node with a possible side effect.

**Hard constraint for every rule:** it must not reorder, duplicate, or drop the evaluation of anything that is not proven pure. Purity here is syntactic: literals, identifiers bound to `const` module-scope literals or imports (whose read cannot run code under ESM live bindings), and object or array literals of pure values. Getters, Proxies, and `$cmp.x` reads are never pure.

| id | rewrite | justification (cluster it targets) | preconditions | why evaluation order cannot change |
|---|---|---|---|---|
| R-PAREN-1 | drop `ParenthesizedExpression` wrappers (parse with `preserve_parens: false`) | sanitizedHtml parens (180 A/B) and generic printer noise | none. **Required negatives for its review [AB-D0003 (6)]:** `(a?.b).c` vs `a?.b.c` (the parens end the optional chain: the first throws when `a` is nullish, the second short-circuits); `delete (x)`: in module code both `delete x` and `delete (x)` are early errors, so the rule must run only after a clean parse and must never turn a parse error into a verdict. `delete (a.b)` vs `delete (0, a.b)`: the comma form deletes a value, not a Reference, so it must stay distinct, and the rule never removes the comma operator; a parenthesized directive prologue `("use strict"); f()` vs `"use strict"; f()` (the parens make it an expression statement, not a directive, so strictness changes). Each must stay non-equivalent. | parens are grouping only. The AST without them has the same tree shape, and oxc reinserts parens where precedence needs them. `(a.b)()` keeps `this` binding per spec (ParenthesizedExpression preserves the Reference). |
| R-STC-INLINE-1 | inline `const stcN = <pure literal>` module-scope bindings at each use, then drop the binding | template's `optimizeStaticExpressions` hoisting divergence (Rust vs oracle hoist set) | binding is module-scope `const`; initializer pure per above; binding never reassigned, exported, or used in identity-sensitive position (`===`, WeakMap key, `Object.is`, cache key, passed to `api_*` as the `stc` data bag whose identity the engine may cache) | evaluating a pure literal has no effects. **Identity is the risk**: one hoisted object is shared across renders, but inlined copies are fresh. So the rule applies only where the binding is read in non-identity positions, and otherwise the case is sent to the exec leg. |
| R-IMPORT-SPEC-ORDER-1 | sort specifiers inside ONE `import {a, b} from "m"` | helper-import ordering | same declaration; no duplicate local names | specifiers bind live names, and their order has no evaluation. The order of import *declarations* (module evaluation order) is NOT touched. |
| R-API-DESTRUCT-ORDER-1 | sort properties of `const {t: api_text, h: api_element} = $api;` | `$api` helper first-use order | the destructured source is the `$api` parameter of a `registerTemplate` template function; all properties are plain identifiers with no defaults or computed keys; the source is the engine's frozen `api` object (data properties, no getters in engine-core `api`) | property reads of frozen data properties have no effects. With no defaults, no code runs, so order is unobservable. Needs an engine-core citation (`api` object literal, `freeze(api)`) recorded in the rule. |
| R-STATIC-PROP-KEY-QUOTE-1 | `{a: 1}` ≡ `{"a": 1}` ≡ `{'a': 1}` | quoted-keys churn | non-computed keys whose StringValue is equal; excludes `__proto__` outright: `{__proto__: x}` and `{"__proto__": x}` both set [[Prototype]], but `{__proto__}` (shorthand) and `{["__proto__"]: x}` (computed) define an own property, so the rule must never cross the computed/shorthand boundary and v1 stays conservative by not touching `__proto__` at all | same key, same position, so there is no reorder. **May already be subsumed by v0** (printer normalizes key spelling); a positive test decides whether the rule is needed at all. |

Explicitly rejected (never rules): reordering object keys (Object.keys and engine attr/prop patch order are observable; v0 negative test), reordering statements (v0 negative test), constant folding, and dead-code elimination. Each rule's `rev` bump needs a new review.

## 5. Execution trace design (SD-2)
**What runs.** We pick the pair module's harness bundling for its corpus. Template corpus: `bundleTemplateFixtureCsr`. SSR fixtures: `bundleSsrFixtureCsr` / `bundleFixture`. Each side's code is injected through a `FixedOutputBackend implements CompilerBackend`, which returns the given code for the target file and delegates everything else (component JS, child stubs) to `JsCompilerBackend`. **Only the module under test varies**, so the component JS is identical on both sides.

**SSR reference: D-0195.** For SSR, the oracle `@lwc/ssr-compiler` output and the Rust SSR output BOTH run on the same oracle `@lwc/ssr-runtime` @ b9e8007 (`SSR_RUNTIME_ESM`, `serverSideRenderComponent` in each `SSR_MODE`). Because the runtime is the same, any difference comes from the compilers. `@lwc/engine-server` @ b9e8007, loaded from the oracle checkout, is allowed only as a test-time cross-check of the rendered HTML. It is never a dependency of anything shipped and never the sole source of a verdict: an engine-server mismatch with an ssr-runtime match is reported as `UNCLASSIFIED reason=engine-server-crosscheck` for diagnosis, and never upgrades or downgrades a verdict on its own.

**Instrumentation: the observer [AB-D0003 (7)]** (engine untouched, oracle untouched). The bundle's `lwc` import resolves to the semdiff **observer**, an interposer that only records and never changes behaviour or return values. It lives in `tools/semdiff/exec/` and stays out of `crates/` and shipped code. It re-exports the oracle `engine-dom` (`ENGINE_DOM_ESM`, which must be the same instance as the test file, per csr.render.test.ts) or `ssr-runtime`, and overrides exactly:
1. `registerTemplate(tmpl)` registers `new Proxy(tmpl, {apply})`. Engine identity checks see the proxy consistently, and post-registration writes (`tmpl.stylesheets = …`) reach the target through the default get and set traps. On each call, `apply` wraps `$api` in a recording proxy: for each `api_*` call it records `(helper, args)`, and for VNodes it records shape (`sel`, `key`, `data` minus functions, child count), never `elm` or `owner`. Return values are not modified.
2. Functions in args (handlers, `api_bind` results, `$ctx` memo) are recorded as `fn#k`, where k is first-seen order within the step. This makes memoization (identity reuse across re-renders) visible without addresses.
3. `freezeTemplate`, `registerComponent`, `registerDecorators`: the call is recorded with stable args.
4. SSR (D-0195): `ssr-runtime` `serverSideRenderComponent` in each `SSR_MODE`, recording the HTML string per step. Optionally, `engine-server` `renderComponent(tag, Ctor, props)` runs as the test-time cross-check only.
5. console and `window` error capture exactly as `render()` in csr.render.test.ts does.
- DOM after each step: `serializeNode` over `document.body` and new `head` nodes, then normalized with csr.render's `norm()`.
- **Harness item H-2**: extract `render()` and `norm()` from `csr.render.test.ts` into `csrRender.ts` (as `mountAndObserve`) so semdiff imports them instead of copying them.

**Determinism.**
- `vi.useFakeTimers({toFake: ['setTimeout','setInterval','Date']})`. Microtasks stay real, since engine rehydration is microtask-driven, and they are flushed with a fixed `await Promise.resolve()` ×8 plus `vi.runAllTimers()`.
- `Math.random` is seeded (splitmix64). `Date.now` is fixed at 0.
- The VM counter is normalized. Each side runs in its own fresh worker with an identical case order, so engine global counters evolve identically.
- No network. Unordered collections are stable-sorted only where the engine's own order is unspecified (none known; if one is found it is a rule, reviewed).

**Inputs and update sequences** (seeded, recorded):
- **[AB-D0003 (5)]** The generator reads BOTH modules' ASTs (Rust, oxc) and takes the **UNION** of what each side reads: `$cmp.<id>` member chains, listener keys in `data.on`, `api_iterator` sources, and `if`/`lwc:if` tests. Reading only the oracle would never drive a slot that only the Rust side reads, which is exactly where a Rust-only bug hides. The union is sorted, so the slot order is deterministic.
- It synthesizes a fixed component class (compiled by the JS oracle on both sides) with declared reactive fields for each `$cmp` root, handlers that record `(name, event.type)` and bump a counter field, and an `@api __sd_set(path, value)`.
- Value pool per slot, chosen by the seed: `undefined, null, "", "a", "a b", 0, 1, -1, true, false, [], [{key:"k0",…}], [{key:"k0"},{key:"k1"}], {}`.
- Sequence: mount → K steps (K=6) of `__sd_set` or a dispatched event on a traced listener element, each followed by a flush and an observation → unmount.
- The input set is `{seed, K, slots, values[], steps[]}`. It is serialized as stable JSON, and `input_set_sha256` goes into the verdict, so the verdict is reproducible from the record alone.

**Verdict.** `EQUIVALENT-BY-EXECUTION` iff the rewrite leg did not prove equality, both sides' traces are byte-equal (sha256 of the stable JSON per step), and at least one step reached each traced `$api` helper the diff touches. If the diff region is never executed, the result is `UNCLASSIFIED reason=exec-uncovered`, never equivalent [AB-D0003 (8)].

**Trace stability check.** Each side runs twice. If a side's two hashes differ, the case is `CRASH reason=nondeterministic-trace`.

## 6. Corpus list
1. **slice 1 (primary): `template-diag-codegen-only`**, the 17,547 crate-engine codegen-only cases (official ac1b61fb239a; the count will shift with main). Needs H-1 pair emission. Rewrite leg first. The exec leg covers the template-fixture and real-source subsets (component synthesizable); mutations are rewrite-only until the generator covers them.
2. **`template-diag-pass-ws`**: cases the fnv1a hash calls *pass* but whose raw code differs (the whitespace hole, **AB-0017**, owner Template Diagnostics). Run the same pipeline. Any non-equivalent case here is a template_codegen false pass, and this corpus's count is the measurement of AB-0017.
3. **`csr-render-template` / `csr-render`**: the 25 + 35 failing render rows (board.txt). These go to the exec leg directly, as diagnosis only.
4. **`lbc` (future)**: Harness builds whole-component compiles of `/workspace/style-corpus/lbc` (pinned in `harness/external-inputs.lock`) and lbc-recipes through `bundleLwc` with each backend, emitting a manifest. semdiff consumes it unchanged (same schema, `corpus: "lbc"`).
5. Later: js-transform outputs (babel-plugin-component fixtures), ssr-compiler targetSSR outputs (exec via `serverSideRenderComponent`), style outputs (style-diff already has an eval-semantic fallback: fold it in as an exec leg).

## 7. Harness CLI contract (separable; LWC Harness Cutover wires the `semantic-diff` row)
**Entry.** `cargo build --release --locked -q -p semdiff --target-dir <tgt>/semdiff`, then:
```
semdiff run  --manifest <manifest.json> --out <summary.json> --registry <notes lwc-rust-migrate/decisions.tsv>
             [--gate none|ratchet|strict] [--baseline <summary.json>] [--reset-baseline]
semdiff pair <oracle.js> <rust.js>            # one verdict on stdout (debug; not a board entry)
semdiff check-registry --registry <decisions.tsv> [--require <D-id>]...   # THE D-id validator (AB-L0030)
```

**Manifest** (`semdiff-manifest/v0`, JSON; paths relative to the manifest's dir):
```json
{ "schema": "semdiff-manifest/v0", "corpus": "template-diag-codegen-only",
  "cases": [ { "id": "<unique non-empty>", "oracle": "pairs/123.oracle.js" | (exclusive) "oracle_code": "...",
               "rust": "pairs/123.rust.js"   | "rust_code": "..." } ] }
```
- Each side needs exactly one of path or inline code.
- The key sets are CLOSED: cases may carry only `{id, oracle, oracle_code, rust, rust_code}`, and the top level only `{schema, corpus, cases}`.
- These are internal errors (exit 2): `decision_id` or any other unknown key (AB-L0023: adapters do not author decisions; §2), duplicate ids, a wrong schema, an empty corpus, or an unreadable file.
- `id` is the case_key (§2), so adapters must keep it stable across corpus edits.
- **SD-1A (built, bot/semdiff 1a2cc4d):** `id` must have the shape `<corpus>/<source…>/<64 lowercase hex>/<64 lowercase hex>`. The `<corpus>` part must equal the manifest `corpus`. Source segments may not be empty, `.` or `..`, and whitespace is not allowed. Anything else is exit 2. Only the shape is checked; H-1 owns the hash values.
- **check-registry (AB-L0030, bot/semdiff 97e3b66).** This is the one registry validator. `run` and coverage-gate.mjs both use the same code (`Registry::check`, then `Registry::resolve`).
  - **Output:** `semdiff-registry-check/v1` stable JSON on stdout, with sorted keys:
    `{schema, registry_sha256, ids, locked, required, required_evaluated, exit, findings[{code, line, id, detail}]}`.
  - **Exit codes:**
    - 0: the registry is valid and every `--require` is `locked`.
    - 1: there is a `DEC-UNRESOLVED` or `DEC-NOT-LOCKED` finding.
    - 2: there is a `DEC-ID-INVALID` or `MALFORMED` finding (the JSON is still printed), or a usage error (stderr only, no JSON). Usage errors are missing `--registry`, a `--require` that doesn't match `^D-[0-9]{4}$`, or an unknown argument.
  - **Finding codes:**
    - `MALFORMED`: the file is unreadable, not UTF-8, empty, or a header names a column twice.
    - `DEC-ID-INVALID`: a row's id is missing, doesn't match the pattern, or repeats. Every one is reported with its line number.
    - `DEC-UNRESOLVED`: a required id is not an exact match for any row. Without an `id` column nothing resolves, per coverage-gate-spec.
    - `DEC-NOT-LOCKED`: `result` is not exactly `locked`. A missing `result` column or a short row counts as not locked (fail closed).
  - Requires are not evaluated when the registry has a defect (`required_evaluated: false`).
- **Registry (SD-1A):** `--registry` is required. The whole registry is id-validated (coverage-gate-spec §Checks), and D-0196 must be `locked`. The map `tools/semdiff/decisions.tsv` is compiled into the binary. Its rows must be 3 fields with a valid case_key, `^D-[0-9]{4}$`, an id that resolves to a `locked` registry row, a non-empty `why`, and no duplicate key. Any violation is exit 2.

**Summary** (`semdiff-summary/v1` since SD-1A; v0 shown below, v1 deltas listed after it; ONE file, written atomically via tmp + rename only after every case is classified):
```json
{ "schema": "semdiff-summary/v0", "tool": {"name": "semdiff", "version": "0.0.0"}, "observation_model": "D-0196", "rule_table_version": "rt-0",
  "corpus": "...", "manifest_sha256": "<hex>",
  "totals": {"cases": N, "equivalent": n, "different_decided": n, "different_undecided": n, "unclassified": n, "crash": n},
  "by_reason": {"printed-differs": n, ...},
  "cases": [ {"id": "<case_key>", "verdict": { /* §3 */ } } ] }
```
- **v1 (SD-1A):** each record is `{id, bucket, decision_id, verdict}`; `decision_id` is null unless the key is mapped and DIFFERENT. A top-level `decisions` block holds `{map, map_sha256, registry_sha256, entries, applied, mapped_not_different}`. Under `--gate ratchet` there is also a `ratchet` block: `{baseline_sha256, baseline_rule_table_version, reset, regressed, improved, new_keys, new_failing, dropped_keys}`. A ratchet baseline must be v1.
- v0 records carry no `decision_id`. SD-1 adds `decision_id` (filled ONLY from the case_key map), the `different_decided` producer, and a summary schema bump together in one change. v0 deliberately has no always-null placeholder that consumers could start depending on.
- REQUIRED: all five `totals` buckets, with `sum = cases`. A summary missing any of them is invalid, and consumers must treat it as a row failure. `semdiff::read_totals` is the validator, and the binary self-checks before writing.

**Exit codes:**
- 0: ran, summary written, gate passed or `none`.
- 1: ran, summary written, gate failed (the failure lines go to stdout).
- 2: internal or usage error, and **no summary exists**. A pre-existing `--out` is deleted before any work, so a stale file can never be read.

A per-case panic is contained as `crash` (exit by gate). It is not an internal error.

**Gate semantics:**
- `strict`: `different_undecided = unclassified = crash = 0`.
- `ratchet --baseline B` **[AB-D0003 (4)] is per case_key, not per total.** A totals-only check can hide a swap, where one case gets fixed while another regresses.
  - No case_key present in both runs may move from `equivalent` to anything worse.
  - Among the bad buckets the order is `different_decided` < `different_undecided` < `unclassified` < `crash`, and a key may not move up that order.
  - Keys new in this run are reported (`new_keys: n` plus the list) and judged by `strict` rules for themselves. Keys only in B are reported as `dropped_keys`.
  - So corpus growth needs **no** baseline reset. A **rule-table version bump does reset the baseline**: a B whose `rule_table_version` differs is exit 2 unless the run is flagged `--reset-baseline` on a board that records the reset.
  - B must itself be a valid summary (all five totals), or the run is exit 2.
  - **SD-1A status (bot/semdiff 1a2cc4d):** the per-key ratchet is built and replaces the v0 totals ratchet.
    - A baseline `corpus` mismatch is exit 2 (`baseline-corpus-mismatch`).
    - A rule-table mismatch is exit 2 (`baseline-rule-table-mismatch`).
    - `--reset-baseline` records `ratchet.reset=true`. It is refused when the rule tables match or outside `--gate ratchet`.
    - **A reset is not a free pass (AB-L0030; rule per ruling notes 3bc1f50, code d8c9566):**
      - A key fails iff it is now in a `STRICT_BAD` bucket (different_undecided, unclassified, crash) that differs from its baseline bucket. So equivalent -> different_undecided fails, and crash -> unclassified fails (fail-closed, though an improvement). This includes every X -> different_undecided move, even crash/unclassified -> different_undecided.
      - Reset and strict share one bad-bucket list (`STRICT_BAD`, lib.rs); there is no separate reset list.
      - New keys are judged by strict as usual.
      - Only equivalent <-> different_decided is waived (the upward move is listed in `ratchet.waived`).
      - Improvements are listed in `improved` in both modes.
- The row is non-gating (DIAG) until Migrate flips it.

**board.sh wiring (H-3).** Copy `template_diag_row`: build, `rm -f` the JSON, emit pairs (H-1) into `$logdir/semdiff-pairs/`, run semdiff, then a summary line from `totals`, and put any exit 2 or a missing or invalid summary in `rowfail`. Publish the summary in the official dir as `semantic-diff.json`, and use the previous official one as the ratchet baseline.

**Coordination items (Harness / Diagnostics; semdiff edits none of these files):**
- **H-1 (Diagnostics):** `TDD_EMIT_PAIRS=<dir>` in template-diag-diff.mjs. For codegen-only (and, under a flag, pass-but-raw-differs) cases, the worker writes `<id>.oracle.js` and `<id>.rust.js` plus `manifest.json` with stable ids (`source/sha256(template)/sha256(opts)`) and `meta {source, family, file, label, opts}`. This is about 17.5k × 2 small files per run, or one `pairs.jsonl` if preferred; semdiff accepts inline `*_code`.
- **H-2 (Harness):** export `render`/`norm` from csr.render.test.ts into `src/csrRender.ts`, and expose a `FixedOutputBackend` helper or a backend-override hook on `bundleTemplateFixtureCsr`.
- **H-3 (Harness):** the `semantic-diff` row in board.sh, plus its publish in official-board.sh.
- ~~H-4~~: moot. Since AB-0005, land.sh runs `cargo test --workspace`.

## 8. Gate and ratchet wiring
- **Phase R (ratchet)**: the row is DIAG. The baseline is the previous official `semantic-diff.json`, and regressions print `!! BOARD FAILED` only once Migrate flips the row out of DIAG.
- **Phase G (gating)**: strict. PLAN §4's exit criterion is "every oracle↔Rust diff on every corpus classified; DIFFERENT without decisions id = 0, unclassified = 0".
- Only a rule-table version bump resets the baseline. Corpus growth does not, because the ratchet is per case_key (§7). A reset needs a published board noting it, and it can't land in the same diff as a classifier change.

## 9. Non-goals
- No LLM or heuristic judgement. No fuzzy matching. No "close enough" DOM diff.
- No proof of general equivalence. EXECUTION verdicts hold for the recorded input set only.
- No editing of compiler crates, the oracle, harness, or template-diag-diff. Changes there are requests (H-1..H-3).
- No source maps or diagnostics equivalence (diag mismatches stay with template-diag).
- No new JS parser in the harness.

## 10. v0 skeleton (this proposal's code)
`tools/semdiff`: `src/lib.rs` (compare, manifest, summarize, gate, atomic write), `src/main.rs` (CLI), `src/sha256.rs` (inline FIPS 180-4, so no new Cargo.lock deps), `tests/rewrite.rs`, `tests/cli.rs`.
- Positive tests: a whitespace/formatting-only diff, and identical text.
- Positive: string quote/escape spelling only (D-0196).
- Negative tests: swapped object keys, swapped statements, a changed literal (string and number), whitespace inside a string, a changed version-banner comment, unlisted spelling (trailing commas, numeric spelling, ASI vs `;`, identifier escape), template raw text, and parse errors per side.
- AB-L0023 tests: a manifest carrying `decision_id` (null or set) or an unknown top-level key exits 2 with no summary; summary records carry no `decision_id`; verdicts and the summary cite `observation_model: "D-0196"`.
- CLI tests: required totals, byte-reproducible summary, a forced internal error mid-run (missing case file) that exits 2 and leaves no summary (a stale one is removed), malformed manifest / duplicate id / ratchet-without-baseline giving exit 2, strict and ratchet gates, and an invalid baseline giving exit 2.

## 11. Open questions
Resolved:
- engine-server vs D-0028 → D-0195.
- Ratchet on corpus growth → per case_key, no reset (AB-D0003 (4)).
- Observation model → D-0196.
- Workspace member → accepted (AB-D0003 (1)).
- H-4 → moot.

Still open:
- **Migrate:** ratify the case_key scheme (§2) that the decisions map and the ratchet both key on.
- **Arch:** the R-STC-INLINE-1 identity precondition, at that rule's own review.
- **Harness:** H-1..H-3, plus whether 35k pair files per board run is acceptable or a jsonl is wanted.
