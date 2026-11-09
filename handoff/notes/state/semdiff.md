# state/semdiff.md
Updated: 2026-11-08 23:18 PST by semdiff. Read PLAN.md first; this file holds only this bot's working state.

## Current item
- SD-2a §10 (exec must not upgrade D-0196-observed spelling): bot/semdiff @ babaf01, one commit on main 9c96ee4, pid e52f32eb8183f951427236f4f2eb580835b79ef0. Not landed; needs Arch review (design §10 + fork F9). main has since moved to 6ddb2d3 (no tools/semdiff changes): rebase before land.sh, and resubmit if the pid changes.
- Re-cluster on 9c96ee4+babaf01 with exec: 16549 eq / 246 different_undecided / 0 unclassified / 0 crash (live-tree, NON-REPORTABLE). Open clusters are all Template's: numeric 105, arrow parens 36, complex->undefined 3x35.
- Builds: RUSTUP_TOOLCHAIN=1.99.1 (no default toolchain on the box), CARGO_TARGET_DIR=/workspace/.targets/semdiff, RUSTC_WRAPPER=, TDD_DRIVER_TARGET=/workspace/.targets/tdd-semdiff. `--locked` works.

## Owed REWORKs (ids)
- none.

## Branch/worktree + last landed main commit
- Worktree: /workspace/wt/semdiff  branch bot/semdiff @ babaf01 (1 ahead of 9c96ee4). bot/semdiff-sd1b was fast-forwarded to 9c96ee4 (d5c4a58 was already an ancestor, so there was nothing to rebase).
- Uncommitted: none
- Last landing on main by this bot: 4497308 (SD-2a, AB-L0051)

## Last trusted board
- Official src=ac1b61fb239a commit=c731fb601f40 (published 2026-11-01T21:58:37-08:00); rows this bot moves: semantic-diff (not yet on board)
- Provisional? yes: PLAN section 2. codegenOnly 17,547 is stale vs main.

## Dead ends
- Shared sccache wrapper hung 22:24-22:33 PST. I built with RUSTC_WRAPPER= instead.
- In the notes repo, commit with a pathspec (git commit -- <paths>). Other bots' commit -a/add -A sweeps up uncommitted edits.

## Open questions for Migrate
- New keys under the ratchet: RESOLVED. AB-L0030 confirmed the strict rule.
- Reset equivalent -> different_undecided: RESOLVED by 3bc1f50. It fails, implemented in d8c9566.
- Reset crash -> unclassified: RESOLVED by 3bc1f50. It fails (STRICT_BAD != baseline bucket).
- coverage-gate.mjs, when written, must call `semdiff check-registry` (spec updated in notes 59dedbe).
- H-3 must pass --registry from the same notes commit the board records.
- Ratify the case_key scheme (corpus/source/sha256(template)/sha256(opts)) that the decisions map and the per-key ratchet both key on.

## Landed 2026-11-01 22:46 PST
- main 891ff88 "semdiff AB-L0023: drop manifest decision_id; enforce D-0196 default-observed" (+ a2c4cb2 SD-0 skeleton), Arch-Review: AB-L0023 APPROVE (notes f24d9f3), pid 30e4be071cf11d6ba07ee94822fb577c96d46c05. land log /tmp/semdiff-land-L0023.log.
- Next: SD-1 = case_key->D-id map (tools/semdiff/decisions.tsv, coverage-gate validation) + per-case_key ratchet + classify codegen-only once Harness H-1 emits pairs. Then rt-1 rules (each separately reviewed), SD-2 exec leg (D-0195).

## Landed 2026-11-01 23:05 PST
- main ee6ebb1 (fd02afd SD-1A + ee6ebb1 AB-L0030 rework), Arch-Review: AB-L0030 APPROVE, pid 6f7cf6ed80959d689ba0fa7935484d2b6e91b44d. Log /tmp/semdiff-land-L0030.log.
- Arch non-blocking: reset crash->unclassified fails (fail-closed, accepted); change only as a rank check if it blocks a real rule-table bump. My (a) question (equivalent->different_undecided waived on reset) answered implicitly by APPROVE as-is.
- Next: SD-1B classify codegen-only from H-1 manifest (main 7bbc8f9 TDD_EMIT_PAIRS), cluster unclassified -> rt-1 rule candidates + owning-crate routes. Live-tree numbers not reportable until official board.

## Landed 2026-11-01 23:09 PST
- main 273d936 "semdiff: reset fails iff now in STRICT_BAD != baseline bucket" Arch-Review: AB-L0030 (rework-2 APPROVE, pid 24da96fbe90a06845f62f8096774fcd75306e64d). Log /tmp/semdiff-land-L0030b.log. Owed REWORKs: none.
- Fleet split (dr eggbot 23:08): semdiff owns tools/semdiff, verdict logic, result file; LWC Harness only renders the semantic-diff row from it. template-diag-diff manifest (H-1) now owned by LWC Template Rust (Diagnostics retired).
- SD-1B on bot/semdiff-sd1b: rebased, then fast-forwarded to main 6ad019e; see Current item.

## 2026-11-01 23:23 PST routing
- SD-1B clusterer bot/semdiff-sd1b @ 042412b pid b8cdc6a9e541c9ec838dcdbd45d87a3c2f9d953a -> Arch review.
- Bugs routed to LWC Template Rust (banner, slotAssignment, classMap, slot name, svg, @slot keys, slots sort, styleDecls, complex-expr value loss, printer parens/numeric). Recommended printer fix over rt-1 rules (asked Arch).
- Harness: cargo config parse5 [patch] unused -> `cargo --locked` fails fleet-wide.
- case_key scheme ratified by Arch + Migrate, no D-id needed.

## Landed 2026-11-01 23:25 PST
- main d5c4a58 cluster.mjs (SD-1B), Arch-Review: AB-L0041 APPROVE pid b8cdc6a9e541c9ec838dcdbd45d87a3c2f9d953a (notes df0f61a). Log /tmp/semdiff-land-L0041.log.
- Arch: no rt-1 rules for parens/number spelling (observed under D-0196); Template fixes via printer.rs (AB-L0034) + AB-D0004. AB-0017 residual (in-string whitespace strip) = false-pass path, Arch routing owner via Migrate; semdiff must not patch it.
- Template stack (AB-L0035 banner, L0032 svg, L0033 slots sort, L0034, L0036; L0022 databag in rework): their stack head measures 660 codegen-only. Re-cluster on each Template landing ping.
- Harness disabled parse5 [patch]; --locked works again (rebase needed when Template lands git dep).
- Next: SD-2 exec leg (engine-dom first, then ssr-runtime per D-0195) on bot/semdiff.

## 2026-11-01 23:59 PST SD-2a (exec leg, CSR/engine-dom)
- bot/semdiff ee72af9 (rebased onto main 9cc1c0a), pid bc49d4caadebed1d88c5ed8a20fdadce78142ffb. Design: arch-board/proposals/semdiff-SD-2a.md. Not landed; awaiting Arch review.
- New: tools/semdiff/src/exec.rs + exec/*.mjs (observer in tools/, oracle engine-dom @ b9e8007, jsdom), --exec/--exec-seed/--exec-jobs/--exec-keep, summary schema v2 (v1 still read), verdict v1, class EQUIVALENT-BY-EXECUTION.
- Tests 62 pass (12 new in tests/exec.rs). Smoke (live-tree, non-reportable) 200 of 16795: equiv 192 (190 printer-equal + 2 exec-equal), different_undecided 8 (7 @slot-key namespacing, 1 $api.d(undefined)).
- Forks F1-F8 in design §7 for Arch. H-2 not landed (built on csrRender.ts exports).

## 2026-11-02 00:05 PST
- SD-2a (CSR exec leg) on bot/semdiff @ ee72af9, pid bc49d4caadebed1d88c5ed8a20fdadce78142ffb, sent to Arch for review. Design: proposals/semdiff-SD-2a.md (notes 4f82cd4). Forks F1-F8 (F7 comments, F8 oracle path pinned).
- AB-0017 residual is owned by D-0219 (Harness template-diag-diff). Nothing for semdiff to do.
- Template AB-L0048 is on main 9cc1c0a. Re-cluster in progress. codegen-only is still 16795 at main 9cc1c0a, so root-causing why.

## 2026-11-02 00:10 PST
- Re-cluster on 9cc1c0a (notes b1ddbc9). Live tree: 16795 codegen-only = 15959 equivalent + 836 unclassified, 0 crash. Template got the open clusters (582 @slot key, 105 num, 36 arrow parens, 3x35 complex undefined, 2 flatten, 2 computed index, 2 NEW svg leak, 1+1 key). Migrate got the summary.
- SD-2a AB-L0051 REWORK in progress: typed trace parse, ORACLE_DIR single-source, branch_coverage not-proven, malformed-trace crash tests. Resubmit with a new pid.

## 2026-11-02 00:13 PST
- SD-2a rework resubmitted to Arch: bot/semdiff @ 65b0e76 (main 5739df7), pid 208b48a3615b021f7b221e795e939b754031ad14, 69/69 tests. Doc §9 is at notes 7917b28. Arch needs to confirm that the check is load then mount(A), not step 0 = mount(A). On APPROVE: add the trailer, check the pid, then land.sh.

## Landed 2026-11-02 00:16 PST
- main 4497308 SD-2a (CSR exec leg), Arch-Review: AB-L0051 APPROVE pid 208b48a3615b (notes 5ae5db0; load then mount(A) order confirmed). Log at /tmp/semdiff-land-L0051.log.
- Next: SD-2b SSR exec leg (oracle ssr-compiler vs Rust SSR, both on @lwc/ssr-runtime @ b9e8007, engine-server cross-check only; D-0195). Move the exec seam onto H-2 once it lands. Run exec on the full codegen-only unclassified set (836) for a breakdown.

## 2026-11-08 23:20 PST: resume after the fleet pause
- Paused WIP 028344b reviewed and reworked, then squashed into **babaf01** on main 9c96ee4. pid **e52f32eb8183f951427236f4f2eb580835b79ef0**. Tests: `cargo test -p semdiff --locked` 78 passed (spelling unit 7, exec 21).
- Review fixes over the WIP, each with a test:
  - a lexer failure returned Same and now returns NotProven;
  - template raw text next to a real difference was missed and is now `template-raw`;
  - an unkeyable number (an integer past u128) silently fell through and is now NotProven;
  - Myers backtrace now stores i32;
  - `divergence_within` exposes the budget for tests.
- False pass proven on the pre-fix 9c96ee4 binary: `75`/`0b1001011` alone or masked, and template `h\x69`/`hi` masked, were EXEC_EQUAL. Design: SD-2a §10 (notes 60dd548). Fork F9 for Arch: property-key quoting and shorthand spelling.
- Re-cluster (notes 6850f4d), live-tree NON-REPORTABLE:
  - H-1 pairs content sha 95b1ed4e… (the manifest sha de43daea… covers ids and paths only);
  - summary fa3c58c6… byte-identical (jobs 6 / jobs 4) with the registry pinned at notes 3d19c38;
  - 16549 printer-equal / 0 exec-equal / 246 different_undecided / 0 unclassified / 0 crash.
  - Collapsed: @slot key 582, flatten 2, computed index 2, svg leak 2, complex key 1+1.
  - Open (Template): numeric 105, arrow parens 36 (1 of them exec-differs through the lwc:external toString), tagged-template throw 35, object-expr undefined 35, template-literal undefined 35.
- Hazard: decisions.tsv changes mid-run, which changes `registry_sha256`. Pin a copy for reproducibility runs.
- Next steps:
  - Arch review of babaf01: rebase onto current main, check the pid, land.sh with the trailer;
  - route the 246 to Template (through Migrate);
  - SD-2b SSR exec leg design;
  - gate DIFFERENT=0 needs Template fixes or D-ids for these 246.

## 2026-11-08 23:20 PST
- bot/semdiff rebased to 19818d7 on main 6ddb2d3, pid e52f32eb8183f951427236f4f2eb580835b79ef0 (unchanged). Sent to Arch with fork F9. On APPROVE: add the trailer, check the pid, run land.sh.
- Migrate has the 246 DIFFERENT (Template-owned) and was asked to route them or let me send them directly. Next is the SD-2b design.
