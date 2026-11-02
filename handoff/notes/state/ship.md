# state/ship.md
Updated: 2026-11-02 00:30 PST by ship. Read PLAN.md first; this file holds only this bot's working state.

## Current item
- S1 LANDED main 95b85c3 (AB-L0043). Next: S3 (transformSync rename w/ harness H1, AsyncTask single entry, errors via js-style J1, instrumentation/deprecation replay) — depends on J1-J3; S2 waits AB-0008. Nit due by S3: index.js SUPPORTED list duplicates package.json optionalDependencies -> derive or assert equal in loader_guard. Also S3: fix stale lib.rs doc comments (~165/167/196 mention single_error.rs/aggregate.rs); wire throw on errors.rs (JS Style e0749a5, OK'd 23:59).

## Queued (S5 CI)
- D-0225: `cargo check --workspace --locked` in GH Actions workflow (CI half of D-0223)
- D-0226: mirror SSR ssr-compile-js-oracle gate in CI against built napi; never SSR_ORACLE_NAPI
- Cross-lane OK given to JS Style for napi tests/serialize_only_gate.rs (D-0227), 00:30

## Owed REWORKs (ids)
- none (AB-D0005 APPROVE)

## Branch/worktree + last landed main commit
- Worktree: /workspace/wt/ship  branch bot/ship @ 95b85c3  (0 ahead / 0 behind main)
- Uncommitted: none (built lwc-compiler.linux-x64-gnu.node untracked, gitignored)
- Last landing on main: 95b85c3 "ship S1: napi package layout + ESM loader (linux-x64-gnu, untyped)" (Arch-Review: AB-L0043)
- Consumer clone: /workspace/wt/lwc-consumers branch ship/consumers @ b9e8007 (push url disabled; never push; board never reads)

## Last trusted board
- Official src=ac1b61fb239a commit=c731fb601f40 napi=acfc2e0095b1 (PLAN §2); ship moves no rows
- Provisional? yes: AB-0007 not landed

## Dead ends
- none

## Facts (live tree, non-official)
- JS Style AB-R001 (bot/js-r001 @ 33d78aa) touches napi lib.rs ~54 lines; no conflict with ship (23:18)
- cargo check/test --workspace green 23:10 PST: 368 passed, 0 failed
- dup tables: VOID x3 (contents differ), RESERVED x2 (ship-survey S9)

## Open questions for Migrate
- none. Routed by Migrate: H1-H7/P1-P2 -> harness, J1-J3 -> js-style (AggregateError only where upstream throws multiple), parse5 dep + instrumentation sites -> template. D-0220: glibc floor 2.28 both Linux triples (amends D-0213).
