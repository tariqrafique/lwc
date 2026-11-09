# state/semdiff.md
Updated: 2026-11-09 01:14 PST by semdiff. Read PLAN.md first; this file holds only this bot's working state.

## Current item
- SD-2a §11 (AB-L0064 rework 2, defect fix): bot/semdiff @ **ad6d47f** on main **34175ef**, pid **d2efcf1e81dbea5429d40a379e8f891ce37d190c**. Not landed. Do not run land.sh; resubmit to Arch (design §11). PENDING rules R-TRAILING-COMMA-1 and R-ASI-SEMI-1 load but do not apply.
- Applied-set identity is rt-0 while both rules are PENDING. Approving a rule flips it to rt-1 and then a ratchet against rt-0 needs `--reset-baseline`.
- Corpus (live-tree, NON-REPORTABLE): 16549 eq / 246 different_undecided / 0 unclassified / 0 crash. vs b87b3fb: exactly 140 UNCLASSIFIED `exec-equal-static-observed` → DIFFERENT `static-observed-differs` (all token-spelling-differs; C1 numeric + C2 arrow parens). EXEC_EQUAL is dead on `semdiff run --exec`.
- Question for Migrate (D-0196): should Function#toString be scoped to the banner comment `checkVersionMismatch` reads plus known engine reads? A hoist-only pair is DIFFERENT under the strict reading; that is what scoping would change.

## Owed REWORKs (ids)
- AB-L0064 (§11 premise cap + D-0258 rules): reworked at ad6d47f (fixes vs b87b3fb: static-observed is DIFFERENT; PENDING rules do not apply), awaiting re-review.

## Branch/worktree + last landed main commit
- Worktree: /workspace/wt/semdiff  branch bot/semdiff @ ad6d47f (1 ahead of main 34175ef).
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

## 2026-11-08 23:42 PST: re-cluster on 6016fa4 (live-tree, NON-REPORTABLE)
- bot/semdiff is at 19818d7 on main 6ddb2d3; someone else had already rebased it, as the same patch as babaf01. pid e52f32eb… is unchanged on 6ddb2d3 and on 6016fa4. I measured on 6016fa4 + 37cc88b in the detached worktree /workspace/wt/semdiff-6016fa4. Tests: 78 passed.
- Buckets: 16549 printer-equal, 0 exec-equal, 246 different_undecided, 0 unclassified, 0 crash. Summary 36446623… is byte-identical across jobs 6 and jobs 4; registry pinned at notes 4a2b05c. Pairs content 95b1ed4e… is the same as 9c96ee4. No cluster delta.
- Case list: arch-board/proposals/semdiff-cases-6016fa4.tsv (notes fd12ec5). Owner of all 5 clusters: LWC Template Rust.
- Next: AB-L0064 REWORK on §10 (pid e52f32eb). Rebase onto main first.

## 2026-11-09 00:20 PST: AB-L0064 rework
- Tip 27bd96c on dbbb940, pid 3bf36b4b06d6c50ce9a914b563b0929a7a9c62a7 (the prior pid e52f32eb is superseded). Tests: 82 passed (spelling 11, exec 22).
- (1) Unaligned sensitive tokens (number, paren, template, property key, shorthand) are paired as multisets by value. A forced mismatch is DIFFERENT; an unpaired token is NotProven, which becomes UNCLASSIFIED.
- (2) F9 forms are observed: keys are keyed by role, and shorthand comes from the AST flag.
- (3) M5 test (trailing comma).
- (4) The CLI probes were EXEC_EQUAL on 19818d7. Now the 2 paren probes are UNCLASSIFIED and the rest DIFFERENT; the controls stay EXEC_EQUAL.
- Mutations M6-M14: all killed.
- Budget 4000 justified from the corpus: max edit distance 50, largest module 8864 tokens.
- Corpus d46d6de+b54cce3: summary 3b3ee9a6… byte-identical (jobs 6 / jobs 4, registry pinned at notes 67a2b74). No verdict changes vs 6016fa4. H-1 pairs 95b1ed4e… are unchanged through dbbb940.
- Next: Arch re-review; land on APPROVE; SD-2b design.

## 2026-11-09 00:12 PST
- D-0244: the 5 clusters (246 cases; TSV proposals/semdiff-cases-6016fa4.tsv, notes fd12ec5) went directly to Template Rust.
- AB-L0064 rework resubmitted to Arch: bot/semdiff 27bd96c on dbbb940, pid 3bf36b4b06d6c50ce9a914b563b0929a7a9c62a7, 82/82. On APPROVE: rebase, check the pid, add the trailer, run land.sh.

## 2026-11-09 00:20 PST
- Arch sent AB-L0064 back for rework a second time (notes 57e9662): regex, cross-site cancellation, and unlisted token classes. That is the second failed fix on the same premise. New approach: treat equal traces on printed-differs (or spelling/AST-differs) as UNCLASSIFIED exec-equal-static-observed, and delete the pairing machinery. Open question for Migrate: should D-0196 limit what Function#toString observes?
- Template Rust accepted C1-C5. They land together in AB-D0004 S4, which is the exit gate. e9d5199 and d46d6de are codegen-identical, so no re-cluster yet. They will ping on S4.

## 2026-11-09 01:05 PST: AB-L0064 rework 2 (premise) + D-0258
- Tip b87b3fb on 2e03887, pid b1fcd017f66f1506e1b66706a74931827ac01112. Tests: 78 passed (`cargo test -p semdiff --locked`).
- Premise: pairing cannot prove spelling equality next to a real difference. printed-differs / token-spelling-differs / printer-equal-ast-differs + equal traces → UNCLASSIFIED exec-equal-static-observed. Deleted spelling.rs (516 lines). vs main the commit is +551 / −77 (main never had spelling.rs).
- D-0258 rules R-TRAILING-COMMA-1 and R-ASI-SEMI-1 in rules/rt-1.toml, arch_review PENDING, applied because D-0258 is locked. Tests in tests/rules.rs (positives + required negatives).
- Mutations: cap off → arch_probes fail; trailing-comma skip off → trailing_comma_is_unobservable fails; ASI skip off → asi_semicolon… fails; elision treated as trailing → trailing_comma_predicate fails. The built-in test `without_the_static_cap_printed_differs_probes_become_exec_equal` documents the false pass.
- Corpus: summary f53ca75c… byte-identical (jobs 6 / jobs 4). 16549 / 106 / 140 / 0 / 0. 140 class changes, all exec-static-spelling-differs → exec-equal-static-observed.
- Next: Arch re-review of b87b3fb and of the two pending rules; Migrate on the D-0196 toString scope question; land on APPROVE (needs `--reset-baseline` for the rt-0 → rt-1 ratchet).

## 2026-11-09 01:00 PST: AB-L0064 rework 2 defect fix (resubmit)
- Two defects in b87b3fb fixed before resubmit. Tip **ad6d47f** on main **34175ef**, pid **d2efcf1e81dbea5429d40a379e8f891ce37d190c**. Tests: 79 passed (`cargo test -p semdiff --locked`: 2 lib, 21 cli, 6 decisions, 22 exec, 10 ratchet, 12 rewrite, 6 rules).
- (1) Equal traces + rewrite printed-differs / token-spelling-differs / printer-equal-ast-differs are DIFFERENT `static-observed-differs` (rewrite first_diff + `trace: equal`), not UNCLASSIFIED. NotProven / exec-uncovered stay UNCLASSIFIED. A hoist-only pair is DIFFERENT (strict D-0196; the pending Migrate toString-scope question would change that).
- (2) PENDING rules load but do not apply (AB-D0003). Applied-set identity is rt-0. Rule positives/negatives run through `compare_forcing_rules`. A trailing-comma-only pair stays token-spelling-differs.
- Mutations, all killed: printed-differs-to-UNCLASSIFIED; PENDING-applies (unit + CLI); cap-off; comma skip off; `is_semicolon` always false; elision treated as trailing.
- Corpus: H-1 pairs 95b1ed4e…, registry pin sha 7199549d…. Summaries jobs 6 and jobs 4 byte-identical, sha256 05ed7cd6…. Buckets 16549 / 246 / 0 / 0 / 0 (by_reason: printer-equal 16549, exec-differs 71, exec-throw-one-side 35, static-observed-differs 140). vs b87b3fb (f53ca75c…): exactly 140 class changes, all UNCLASSIFIED exec-equal-static-observed → DIFFERENT static-observed-differs.
- Did not run land.sh; messaged no agents.

## 2026-11-09 01:00 PST
- AB-L0064 rework-2 resubmitted to Arch: bot/semdiff ad6d47f on 34175ef, pid d2efcf1e81dbea5429d40a379e8f891ce37d190c, 79/79. Asked Arch for per-rule verdicts on R-TRAILING-COMMA-1 and R-ASI-SEMI-1. Live corpus: 16549 / 246 DIFFERENT / 0 unclassified / 0 crash.
- Asked Migrate: should D-0196 limit the toString channel? Until it answers, exec-equal can't be reached.
- Template S2 7b8c7c5 is codegen-identical, so no re-cluster. Waiting on S4.

## 2026-11-09 01:15 PST
- AB-L0064 r3: REWORK (notes b9c3fbb). The premise was accepted. Still to do: delete the dead EXEC_EQUAL path; R-ASI-SEMI-1 is APPROVE; R-TRAILING-COMMA-1 needs restricting by AST parent, because it must not skip destructuring patterns; fix the rt-1.toml comment; define the rule ids in one place. Rework in progress.
- D-0263 (Migrate) narrows toString to banner, engine reads, stringified-into-DOM and in-output coercion. The design doc SD-2c comes first and goes to Arch before any code.
- Template S3 2a5cff8 is codegen-identical. Lock: cargo --locked is missing the parse5 git source on main; Arch is routing that to Migrate.
