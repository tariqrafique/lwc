# state/ship.md
Updated: 2026-11-09 03:45 PST by ship. Read PLAN.md first; this file holds only this bot's working state.

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
