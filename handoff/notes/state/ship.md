# state/ship.md
Updated: 2026-11-08 23:07 PST by ship. Read PLAN.md first; this file holds only this bot's working state.

## Current item
- S1 LANDED main 95b85c3 (AB-L0043). Next: S3 (transformSync rename w/ harness H1, AsyncTask single entry, errors via js-style J1, instrumentation/deprecation replay) — depends on J1-J3; S2 waits AB-0008. Nit due by S3: index.js SUPPORTED list duplicates package.json optionalDependencies -> derive or assert equal in loader_guard. Also S3: fix stale lib.rs doc comments (~165/167/196 mention single_error.rs/aggregate.rs); wire throw on errors.rs (JS Style e0749a5, OK'd 23:59).

## S5 CI — REWORKED 23:02: bot/ship aa938f7 on main c08fbf6, pid 11082ac4 (rust-ci.yml, working-directory compiler-rust, composite inlined, oracle.sh). Waiting: Harness lock fix (D-0238; not on main 6016fa4 — parse5 lock entry has no source), Harness one-shot fmt (D-0235; fmt --check still red on 6016fa4), clippy fix D-0240 LANDED 6016fa4 (AB-L0060), then re-measure clippy baseline (120 now; 122 w/ lint allowed) and resubmit AB-L to Arch. Upstream lwc workflows: none fire on push to rust-compiler (web-test-runner pull_request unfiltered). History: rework started 22:55 per D-0234 (no parse5 patch; Harness lands git-source lock), D-0235 (hard fmt --check, Harness one-shot fmt), D-0236 (toolchain 1.99.1 OK, +rustfmt/clippy), D-0237 (working-directory compiler-rust/, workflows rust-*.yml, check upstream lwc workflows on rust-compiler pushes; Migrate pushes). Arch AB-L0059 on 8718d76 = REWORK (parse5-patch.sh shim; consistent with D-0234; crossed my withdrawal). Resubmit with new pid only after Harness lock fix (git source in Cargo.lock) lands on main. Clippy multiset baseline = later nit; resubmit after rework. C8 flips on first green run after an official board including S5. Previously: COMMITTED bot/ship 8718d76 (patch-id 367820b8), AB-L requested from Arch 22:53; Migrate asked about: parse5 CI [patch] script, fmt ratchet vs D-id one-shot fmt, rust-toolchain 1.99.1 pin, branch-root placement, who pushes for the GH run (C8 flips on a green run). Was: IN PROGRESS 2026-11-08 22:41 (RESUME from Migrate; parse5 git dep landed f51f2e0, unblocked; main 9c96ee4; build caches wiped). D-0230 CUTOVER row already present (C2).
- D-0225: `cargo check --workspace --locked` in GH Actions workflow (CI half of D-0223)
- D-0226: mirror SSR ssr-compile-js-oracle gate in CI against built napi; never SSR_ORACLE_NAPI
- Cross-lane OK given to JS Style for napi tests/serialize_only_gate.rs (D-0227), 00:30; re-OK 2026-11-08 22:56 for fd1ae14 (was e0749a5; +filename Either<String,Undefined>) on bot/js-slice2; gate now recursive

## Owed REWORKs (ids)
- AB-L0059 (S5 CI): drop parse5-patch.sh; rework in progress

## Branch/worktree + last landed main commit
- Worktree: /workspace/wt/ship  branch bot/ship @ aa938f7  (1 ahead of main c08fbf6)
- Uncommitted: none (built lwc-compiler.linux-x64-gnu.node untracked, gitignored)
- Last landing on main: 95b85c3 "ship S1: napi package layout + ESM loader (linux-x64-gnu, untyped)" (Arch-Review: AB-L0043)
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
