# js-J3: lwc_shared CompilerMetric + DeprecationId from the 9.5.3 oracle (D-0205 / D-0212 as amended by D-0250)

**Resubmission for AB-L0065 (REWORK on 98e8335, notes 4dd4825).**
Branch `bot/js-j3` @ `c2626c5`, merge-base `d46d6de` (2 commits). Worktree /workspace/wt/js-j3.
Patch-id (scripts/review-patch-id.sh): `a62242fd11d9f7443f394b1c5a9daee8e6ee42f0`.
While the gates ran, main moved to `0e8b07a`, which touches lwc-ssr and tools/ssr-compile-js-oracle. `git merge-tree` against it is clean, and no new main file contains a needle. I have not rebased onto it; gates are measured at c2626c5 vs d46d6de. land.sh will rebase. If that changes the pid, it has to be re-sent.

## AB-L0065 rework items
1. **Repo-root scan.** `defined_only_in_lwc_shared` scans every `.rs/.js/.mjs/.cjs/.ts/.mts/.cts` from the repo root.
   - Skipped directory names: `target`, `node_modules`, `.git`, nothing else. The `npm` skip is gone.
   - Allow-entries, named and asserted to exist by `allow_entries_exist`:
     - `crates/lwc-shared/src/instrumentation/generated.rs`: the one definition.
     - `tools/gen-instrumentation/gen.mjs`: the generator, whose warning-shape regex contains `property is deprecated`.
   - No entry needed for `instrumentation/mod.rs`, which contains no needle.
   - No entry needed for the gate test itself: its needles come from the enums and its fragments are written with `concat!`. A doc line that first tripped the gate was reworded rather than allowed.
   - **Other copies in the repo:** none. `rg -uu` over every file type in the worktree (all 4763 `.json` harness goldens and fixtures, `.css`, `.md`, `.sh`, …, excluding only target, node_modules and .git) finds needles only in the two allow-entries. So no harness or fixture entry is needed. The oracle /workspace/lwc is outside the repo.
2. **Unreadable files fail.** `read_dir`, a directory entry, or `read_to_string` of a scanned file panics with the path; nothing is skipped. Pinned by `unreadable_file_panics_with_its_path`, a non-UTF-8 `bad.js` that must panic naming `bad.js`.
3. **Planted negatives.** `gate_catches_planted_copies` builds a scratch repo in the temp dir; a `Drop` guard removes it, even on panic. It plants 7 copies, and the scan must report exactly those 7 paths:
   - Arch's `crates/lwc-compiler-napi/npm/linux-x64-gnu/plant.js` (minify message) and `tools/semdiff/zz/p.mjs` (`lwc-dynamic-directive`);
   - `crates/lwc-planted/src/{lib,w}.rs`, `harness/src/x.cjs`, `scripts/y.mts`, `npm/z.ts`.
   - Not reported: an enum use, both allow-entries holding a message, `node_modules/` and `target/` copies, and a `.json` golden.
   - **Arch's mutations on the real tree.** Both files were planted for real, then removed by an EXIT trap; `git status` was clean afterwards and neither path exists.
     - New gate: FAILED, reporting `crates/lwc-compiler-napi/npm/linux-x64-gnu/plant.js:1` and `tools/semdiff/zz/p.mjs:1`.
     - The 98e8335 gate on the same plants: passed, which reproduces Arch's finding.
4. **The generator aborts on any unrecognized guard shape** (gen.mjs `guardOf`, and the comment now says exactly what it accepts). A warn must sit in the then-branch of exactly one `if` (up to its function), with only Blocks or ExpressionStatements in between. The condition must be an `&&` of `!isUndefined(<a>.<b>)` presence terms (at least one), plus at most one `!alreadyWarned*` latch that the same branch sets `= true`. Anything else exits 2 instead of defaulting to every call.
   - 9.5.3 still generates: `--check` fresh, and the identity mutant exits 0.
   - Mutants on a scratch copy of options.ts (the oracle was not touched), each exiting 2 with "unknown guard shape":
     - an extra `&& someFlag` term;
     - `|| x`;
     - the warn in the else branch;
     - a nested `if`;
     - the latch tested but not set;
     - the warn inside a `for`;
     - a latch with no presence term.
5. **D-0250** is cited in both commit messages and here.

| commit | what |
|---|---|
| 01405d1 | `tools/gen-instrumentation/gen.mjs` generates `lwc-shared/src/instrumentation/generated.rs`, with the strict guard reading (item 4). The hand-written `CompilerMetric` / `Instrumentation` / `InstrumentationHandle` move from lib.rs to `lwc_shared::instrumentation` and are re-exported at the crate root, so no caller changes. New: `DeprecationId`, `InstrumentationEvent`, `InstrumentationRecorder`. |
| c2626c5 | `tests/instrumentation_gate.rs`: freshness, live oracle, repo-wide single definition plus existence of the allow-entries, planted negatives, unreadable-file panic. |

<!-- previous submission: 98e8335 on a051a35, pid ec3074e6 (AB-L0065 REWORK) -->

| commit (previous submission) | what |
|---|---|
| 2770df7 | `tools/gen-instrumentation/gen.mjs` generates `lwc-shared/src/instrumentation/generated.rs`. The hand-written `CompilerMetric` / `Instrumentation` / `InstrumentationHandle` move from lib.rs to `lwc_shared::instrumentation` and are re-exported at the crate root, so no caller changes. New: `DeprecationId`, `InstrumentationEvent`, `InstrumentationRecorder`. |
| 98e8335 | `tests/instrumentation_gate.rs`: freshness, live oracle, single definition, planted-copy negative test. |

## Scope vs the brief (D-0205 / D-0212 are the spec)
- **Naming.** Upstream has no `InstrumentationEvent` type. The metric enum keeps its existing, upstream-matching name `CompilerMetric` (`@lwc/errors` `CompilerMetrics`). Renaming it would mean editing lwc-template and lwc-compiler or adding an alias. `InstrumentationEvent` is the D-0205 event record (`Log(CompilerDiagnostic)` / `IncrementCounter(CompilerMetric)`), collected in call order by `InstrumentationRecorder` so napi can return the events for the loader to replay.
- **D-0250 (amends D-0212): stylesheetConfig latches once per process, minify warns on every call.** Background: options.ts:186-195 latches `alreadyWarnedOnStylesheetConfig`. `OutputConfig.minify` (options.ts:213-218) warns on every call, and upstream's own test pins it (options.spec.ts:41-53, "warns every time outputConfig.minify is passed"). `DeprecationId::once_per_process()` carries the per-id answer, read from the AST latch and confirmed at runtime.
- **LWC_VERSION.** No metric value or deprecation text contains a version, so nothing reads `meta::LWC_VERSION`. The generated header names the pin sha, not a version, and `exactly_one_lwc_version_const` / `_comment` still pass.
- **Not in scope (other lanes):** emitting the warnings and replaying events over napi. See consumer sites.

## Upstream sources (oracle /workspace/lwc @ b9e8007, LWC 9.5.3)
- `packages/@lwc/errors/src/compiler/instrumentation.ts:13-18` `CompilerMetrics` (4 entries, :14-17); `:22-25` `InstrumentationObject { log, incrementCounter }`.
- `packages/@lwc/compiler/src/options.ts:22` latch `alreadyWarnedOnStylesheetConfig`; `:186-195` stylesheetConfig warning (text :193); `:213-218` minify warning (text :216).
- `packages/@lwc/compiler/src/__tests__/options.spec.ts:31-53` (once vs every-time behaviour).
- Callers of `incrementCounter`: template-compiler `parser/index.ts:898` (RenderMode), `:1106` (Dynamic), `codegen/index.ts:605` (Spread); babel-plugin-component `dynamic-imports.ts:74` via `utils.ts:154-155`. No upstream package calls `instrumentation.log`.

## Generator
`node tools/gen-instrumentation/gen.mjs [--check|--stdout|--oracle]`
- Pin: `tools/gen-shared-tables/PIN`, the same single pin file gen-shared-tables uses. A HEAD mismatch or missing checkout exits 3.
- Reads the TS AST with the oracle's `typescript`:
  - the `CompilerMetrics` object literal;
  - every `console.warn(<literal>)` in options.ts. "Once" means an enclosing `if` tests a module `let alreadyWarned*`. The id and option path come from the quoted property name in the text.
  - Anything non-literal, or a different number of warn calls than were extracted, exits 2.
- Self-check before writing, by running the upstream runtime through tsx:
  - `errors/src/index.ts` `CompilerMetrics` must equal the AST table.
  - Calling `validateTransformOptions` twice per option path, each time on a fresh module instance, must print exactly 1× (latched) or 2× (unlatched) the AST text.
- `--oracle` prints the runtime facts as TSV for the Rust test.

## Tests (crates/lwc-shared/tests/instrumentation_gate.rs). All fail closed, none skip.
- `generated_is_fresh`: `gen.mjs --check`.
- `matches_live_oracle`: `gen.mjs --oracle`, which reports what the runtime does, not what the AST says. Metric values equal `CompilerMetrics` in order. (option_path, 1 or 2 by `once_per_process`, message) equals what 9.5.3 prints over two `validateTransformOptions` calls (D-0250).
- `allow_entries_exist`, `defined_only_in_lwc_shared`, `gate_catches_planted_copies`, `unreadable_file_panics_with_its_path`: see items 1-3.
- Unit tests in instrumentation/mod.rs: `js_value_round_trips`, `recorder_keeps_call_order_through_the_handle`.
- Still holds from the first submission: a literal appended to lwc-js fails the gate, an edited generated.rs fails fresh and oracle, and `LWC_ROOT=/nonexistent` fails with a pin mismatch.

## lwc-js / lwc-style
- lwc-js `transforms/dynamic_imports.rs:23` already uses `lwc_shared::CompilerMetric::DynamicImportTransform`, so there is no local copy.
- lwc-style has no instrumentation or deprecation use; upstream style-compiler has none either.
- `rg` across crates found no ad-hoc metric strings or deprecation texts outside lwc-shared.

## Consumer sites in other bots' crates (not edited; listed for the owners)
1. **lwc-template (Template), parity gap:** `CompilerMetric::LwcSpreadDirective` is never counted. Upstream counts it at codegen/index.ts:605, when `lwc:spread` is emitted. The Rust counterpart is `crates/lwc-template/src/codegen/mod.rs:383` (`if let Some(e) = spread { entries.push(...) }`). Confirmed with 9.5.3: `<template><div lwc:spread={x}></div></template>` → `['lwc-spread-directive']`. Other counters are already on lwc_shared: parser/mod.rs:110/980, parser/element_check.rs:160, parser/validate.rs:230.
2. **lwc-template lib.rs:680-689 and lwc-compiler lib.rs:418-439 (tests):** a local `Rec` Instrumentation could be replaced by `lwc_shared::InstrumentationRecorder`. Optional; no strings are duplicated.
3. **napi (Ship) for D-0212:** `crates/lwc-compiler-napi/src/lib.rs:69-75` (`outputConfig.minify`, parsed, no warning) and `:39-42` / `:283` (`stylesheetConfig`, no warning). The loader (`index.js`, AB-D0005: no option inspection) can only print what native returns. So native has to report which `DeprecationId`s fired, and the loader prints `message()`, latching only when `once_per_process()`. Text must come from lwc_shared, not the loader; the gate scans index.js.
4. **napi (Ship) for D-0205:** napi takes no `instrumentation` today. `InstrumentationRecorder::take()` gives the ordered `InstrumentationEvent`s to return for loader replay. lwc-compiler lib.rs:147 and lwc-ssr lib.rs:87 already forward the handle (upstream ssr-compiler compile-template/index.ts:95 does the same).

## Gates: merge-base d46d6de vs branch c2626c5 (2026-11-08 23:45–23:55 PST, local, PROVISIONAL)
d46d6de was measured in a detached worktree /workspace/wt/js-j3-main (napi f58e174dad0b); the branch in /workspace/wt/js-j3 (napi 100a41d7e6cf). Both napi builds came from install-napi.sh. Oracle b9e8007 clean. Every board row is identical, diffed line by line.

| gate | d46d6de | c2626c5 |
|---|---|---|
| cargo check --workspace --all-targets | green | green |
| cargo clippy --workspace --all-targets | 0 errors (146 warning lines) | 0 errors (146 warning lines; none new) |
| cargo test --workspace | 453 pass / 0 fail | 461 pass / 0 fail (+6 gate, +2 unit) |
| lwc-style fixtures | 136/136 | 136/136 |
| style-diff | 3150 jobs, 0 fail, 100% MATCH | same |
| js | 174/174 | 174/174 |
| js-options | 302 | 302 |
| js-api | 1239 | 1239 |
| template (diag) | 568/568 | 568/568 |
| style | 70/70 | 70/70 |
| style-api | 829/829 | 829/829 |
| ssr (diag) ×3 modes | 388/388 | 388/388 |
| ssr-render ×3 modes | 384 + 5 skip | same |
| csr-render | 390/390 | 390/390 |
| csr-render-template | 567/570 | 567/570 (same 3) |
| template-api / ssr_render_api / template-options / ssr_dynamic_imports | 5671 / 2710+14 skip / 67 / 16 | same |
| template-diag / -napi | clean 63115 / 62026 | same |
| shim gate | untagged 0, scanners 9=9 | same |
| option gate | 61/61 mapped, 0 pending | same |
| golden-io-guard | ok | ok |
| lint-shared-tables | ok 23/23 | ok 23/23 |

## Blockers / asks
- None for this change. It touches only lwc-shared and tools/, so no Cross-Lane trailer is needed.
- For Template: the LwcSpreadDirective counter (site 1). For Ship: D-0212 and D-0205 wiring (sites 3-4).
