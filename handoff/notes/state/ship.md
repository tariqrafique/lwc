# state/ship.md
Updated: 2026-11-09 12:48 PST by ship. PARKED per D-0276 — successor: LWC Worker (28881e7c) owns packaging, CI, CUTOVER.md. Read PLAN.md first; this file holds only this bot's working state.

## FINAL HANDOFF (D-0276, 2026-11-09 12:49 PST) — ship parked, lands nothing further
- Unlanded work: NONE. bot/ship @ eac1ae9 (= S5 land; main has moved on to f679437). Worktree /workspace/wt/ship clean (only gitignored built .node). No stashes, no in-flight processes, no pending AB-L reviews, no owed REWORKs.
- Consumer clone /workspace/wt/lwc-consumers: ship/consumers @ b9e8007, clean, zero commits. No consumer moved yet (rollup plugin + other bundler/platform integrations all still on JS @lwc/compiler). Push url disabled; never push.
- Routines: none owned by ship (nothing to delete).
- C8 status: S5 landed main eac1ae9 (AB-L0059). Flips on: (a) D-0235 one-shot fmt on main (fmt --check is hard, red until then); (b) Migrate's push of .github/workflows/rust-*.yml with first official board incl. eac1ae9 (D-0237); (c) first green GH run — fork Actions setting + upstream web-test-runner.yml unfiltered pull_request trigger pending Tariq. Then mark CUTOVER C8 [x].
- Packaging plan: notes/lwc-rust-migrate/ship-packaging-plan.md (AB-D0005 APPROVE, C1). Slices: S1 done (95b85c3 pkg+ESM loader, linux-x64-gnu); S5 done (eac1ae9 CI). Remaining:
  - S3: transformSync rename (w/ harness H1), AsyncTask single entry (D-0209), throw via js-style napi/src/errors.rs (J1), instrumentation replay (D-0205, InstrumentationRecorder::take()) + deprecation replay (D-0212: return DeprecationIds; loader prints message(), latches only once_per_process — stylesheetConfig latches, minify every call). Also: SUPPORTED list vs optionalDependencies dup nit; stale lib.rs doc comments ~:166/:168; seam test. Blocked on J1/J3 (bot/js-j3 pending Arch) + H1.
  - S2: typed index.d.ts after AB-0008. S4: `version` export after AB-0010. S6: other triples + glibc 2.28 floor smoke (D-0220).
  - Later: SHA-pin third-party actions; (file,lint) multiset clippy baseline (now count 142); duplicate-table lint (VOID x3, RESERVED x2); `lwc` barrel; AB-0010/AB-0011 JS package + table-copy deletes after consumers migrate; package name/scope pending Tariq (C2, D-0202). No npm publish.
- napi gotchas: install only via scripts/install-napi.sh; use `--cargo-flags=--locked` (= form); napi's unlocked `cargo metadata` can rewrite a stale lock — napi-pack.sh asserts Cargo.lock sha unchanged.
- After D-0253 lands: re-run .github/ci/ssr-oracle-gate.sh (last: 141 cases, 23 differ, 23 known, rc 0).
- CUTOVER.md state: C1 [x]; C2 [ ] name pending Tariq; C3 partial (S1); C4 [ ] only sync `transform`; C5 [ ] (S3); C6 [ ] (S3); C7 per D-0238/D-0274 parse5 git rev e3520ba landed 82ecba7; C8 [ ] annotated as above (notes b8d6165). Ship made no further CUTOVER edits.
- /tmp/ship-ci-* and /tmp/ship-corepack scratch + actionlint/shellcheck binaries left in place; disposable.

## Current item
- S1 LANDED main 95b85c3 (AB-L0043). Next: S3 (transformSync rename w/ harness H1, AsyncTask single entry, errors via js-style J1, instrumentation/deprecation replay) — depends on J1-J3; S2 waits AB-0008. Nit due by S3: index.js SUPPORTED list duplicates package.json optionalDependencies -> derive or assert equal in loader_guard. Also S3: fix stale lib.rs doc comments (~165/167/196 mention single_error.rs/aggregate.rs); wire throw on errors.rs (JS Style e0749a5, OK'd 23:59).

## S5 CI — LANDED main eac1ae9 03:35 (AB-L0059 APPROVE pid e6425927; land.sh board check ok 142s, 22 rows equal). C8 waits: D-0235 fmt + first green GH run after official board incl. S5 (Migrate pushes with first official board after eac1ae9; GH Actions on fork pending Tariq — don't plan C8 on a GH run until Migrate confirms). D-0275: never touch /workspace/scratch/<other bot>; answered Migrate 03:47 (ship ran no rm under scratch). Later nits: SHA-pin actions; (file,lint) multiset clippy baseline.
- D-0225: `cargo check --workspace --locked` in GH Actions workflow (CI half of D-0223)
- D-0226: mirror SSR ssr-compile-js-oracle gate in CI against built napi; never SSR_ORACLE_NAPI
- Cross-lane OK given to JS Style for napi tests/serialize_only_gate.rs (D-0227), 00:30; re-OK 2026-11-08 22:56 for fd1ae14 (was e0749a5; +filename Either<String,Undefined>) on bot/js-slice2; gate now recursive

## Owed REWORKs (ids)
- none (AB-L0059 APPROVE, landed)

## Branch/worktree + last landed main commit
- Worktree: /workspace/wt/ship  branch bot/ship @ eac1ae9  (= main)
- Uncommitted: none (built lwc-compiler.linux-x64-gnu.node untracked, gitignored)
- Last landing on main: eac1ae9 ship S5 CI (Arch-Review: AB-L0059); prior 95b85c3 "ship S1: napi package layout + ESM loader (linux-x64-gnu, untyped)" (Arch-Review: AB-L0043)
- Consumer clone: /workspace/wt/lwc-consumers branch ship/consumers @ b9e8007 (push url disabled; never push; board never reads)

## Last trusted board
- Official src=ac1b61fb239a commit=c731fb601f40 napi=acfc2e0095b1 (PLAN §2); ship moves no rows
- Provisional? yes: AB-0007 not landed

## Dead ends
- none

## Facts (live tree, non-official)
- 2026-11-08 22:52: cold check --locked green; test 447/0; ssr-compile-js oracle 141 cases, 23 differ, 23 known; clippy 120 and fmt 121 baselines. Scratch in /tmp/ship-ci-*
- JS Style AB-R001 (bot/js-r001 @ 33d78aa) touches napi lib.rs ~54 lines; no conflict with ship (23:18)
- cargo check/test --workspace green 23:10 PST: 368 passed, 0 failed
- dup tables: VOID x3 (contents differ), RESERVED x2 (ship-survey S9)

## Open questions for Migrate
- none. Routed by Migrate: H1-H7/P1-P2 -> harness, J1-J3 -> js-style (AggregateError only where upstream throws multiple), parse5 dep + instrumentation sites -> template. D-0220: glibc floor 2.28 both Linux triples (amends D-0213).
