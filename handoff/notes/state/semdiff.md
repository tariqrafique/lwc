# state/semdiff.md
Updated: 2026-11-09 12:49 PST by semdiff. **PARKED (D-0276).** Owner of tools/semdiff is now LWC Worker (28881e7c). Read PLAN.md first.

## HANDOFF to LWC Worker (D-0276)
Status: semdiff bot parked, idle, no routines, no running work, nothing uncommitted, nothing owed to land.

### Code / branches / shas
- Repo /workspace/lwc-compiler-rust; my worktree /workspace/wt/semdiff, branch `bot/semdiff` @ **6a905a4** (== main when parked). No unlanded commits.
- Local branch `semdiff-board-vendor-dropped` @ 5c271eb: AB-L0078 REWORK'd vendored board inputs (registry.tsv stale copy; all-clean baseline == strict). **Do not land**; safe to delete.
- Last semdiff landing on main: **c3bab62** (AB-L0064, pid fa5a165f — verify with scripts/review-patch-id.sh; an executor once misreported dc2e9503). Deletes Class::EquivalentByExecution (exhaustive match: restoring it = compile error); applies rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1 (D-0258/D-0266).
- Verdict rule r4: equal trace + printed/token-spelling diff, or printer-equal-ast-differs => DIFFERENT static-observed-differs; else exec-differs / exec-throw-one-side; gaps => UNCLASSIFIED; malformed trace => exit 2. D-0196: whitespace/quotes unobservable, comments OBSERVED.
- Land invocation that works: `env -u CARGO_TARGET_DIR RUSTUP_TOOLCHAIN=1.99.1 TDD_DRIVER_TARGET=/workspace/.targets/tdd-semdiff scripts/land.sh` (land.sh:229 hardcodes $wt/target; Harness fix pending in D-0219 r4). Needs `Arch-Review:` trailer; removing a case needs `Case-Set: D-NNNN` (D-0261).

### semantic-diff row contract (strict; proposals/semdiff-board-row.md 710d510 + erratum e5b9bf8)
- `semdiff run --manifest <TDD_EMIT_PAIRS manifest> --out S --registry <notes lwc-rust-migrate/decisions.tsv at the board's NOTES_COMMIT> --gate strict --exec`. **No baseline.**
- Exit 0 ran+gate pass; 1 ran, summary written, gate fail; 2 internal error, no summary. Harness renders exit 2 or non-v2 summary as crash. Contract is documented in tools/semdiff main.rs header.
- **Manifest count + corpus assert: NOT enforced by semdiff under strict.** main.rs only checks corpus against a baseline (line ~105, baseline-corpus-mismatch), which strict never loads; it prints `semdiff <corpus> cases=<n>`. So a truncated or wrong-corpus manifest would pass strict. Owner (Worker) must either have the row assert corpus name and case count against the board's pair-producing run (official ae266f8c1456: crate 16795, napi 16352), or add `--expect-corpus/--expect-cases` to semdiff (exit 2 on mismatch, Arch review). Open gap, not yet designed.
- Baseline returns only if a corpus carries accepted debt, as its own id+bucket+totals schema design (new Arch review).
- Status: handed to Harness; row not yet live on an official board. When it lands, check row numbers against official board.
- Last official board ae266f8c1456 (BOARD_COMMIT 82ecba7): crate 16795/0/0/0/0 (summary 06a5fadc…), napi 16352/0/0/0/0 (e249e3d8…), 0 clusters. Template S4 cleared C1–C5 246->0 (D-0271).

### rt-1 rule table (tools/semdiff/rules/rt-1.toml, applied set `rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1`)
- R-TRAILING-COMMA-1@1 (D-0258/D-0266, AB-L0064): single trailing comma unobservable; holes/Elision stay OBSERVED. R-ASI-SEMI-1@1 (D-0258, AB-L0064): explicit vs ASI ';' unobservable only when Program::content_eq already holds. Tests tools/semdiff/tests/rules.rs (pos+neg).
- Loading is fail-closed (AB-D0003 (6)): a rule applies only when arch_review holds an AB- id; PENDING rules load but don't apply. No pending rules. Arch: no rules for parens/number spelling (observed under D-0196); fix those in printer.
- Unlanded: nothing. Only `semdiff-board-vendor-dropped` @ 5c271eb (REWORK'd AB-L0078; do not land, delete freely).

### SD-2b design (APPROVED AB-D0007-r2; proposals/semdiff-SD-2b.md r1 9b6fc46, r2 025d5ba)
- Design-only until the SSR fixture emitter (Harness-owned, built on ssr-render bundleFixture path) lands. No SSR exec code written.
- Premise at F1 grain (sync, napi c4cd4cb09eac): 359 bundles, 353 not printer-equal; raw HTML byte-differ 0 (334 equal, 19 throw-both). Script f1-bundles/raw-html-compare.mjs sha eabd2d72….
- Code conditions: emitter accounts for all 387 fixtures; the 28-fixture gap must appear as named cases or UNCLASSIFIED via a completeness check that exits 2; code PR must report async and asyncYield numbers. Forks F1/F3/F4/F5/F8/F9 APPROVE, F2 rejected, F6 deleted, F7 reuse SD-2a decide path.
- SD-2c (toString channels, 7b40438): design approved, build DROPPED (D-0270; would certify 0 after S4).

### Moved out of scratch (Worker needs these): notes `arch-board/proposals/semdiff-handoff/`
- sd2b-f1/: raw-html-compare.mjs (sha eabd2d72…), raw-html-summary.json, pair-meta.json, bundles-rewrite.json, ARTIFACT_SHA256 = the SD-2b F1 premise evidence. sd2c/: measure.py + summary.json (SD-2c would_certify=0 evidence).
- Everything else in scratch is large and reproducible from the official board / tdd-driver (TDD_EMIT_PAIRS) at the shas listed; not needed by Worker.

### Scratch artifacts left in /workspace/scratch/semdiff/ (reproducible; D-0275 owner = Worker now)
- semdiff-official-ae266f8c1456/ (official-board semdiff run), semdiff-ratchet-baseline/ (summary 4b281b75, prev f493b75f), sd2b-premise/ (F1 bundle compare), sd2c-premise-gate/, h1-1dab7cd/, h1-e1fd366/, h1-official-ae266/ (reg-pinned.tsv sha 7199549d = decisions.tsv @ notes 4d465bd), board-proof/.

### Open for new owner
- Implement SD-2b when the SSR emitter lands; verify semantic-diff row on next official board; later H-2.

---
## History (pre-park; superseded by handoff above)

## Current item
- **SD-2b r2** (AB-D0007 @ notes 6373f6c): `arch-board/proposals/semdiff-SD-2b.md` amended. Doc only; no SSR exec code.
- F1-grain (fixture bundles, sync, napi c4cd4cb09eac): **359** pairs; **353** not printer-equal; of those, raw HTML byte-differ **0**, byte-equal 334, throw-both 19. Script `f1-bundles/raw-html-compare.mjs` sha eabd2d72… . Premise = triage + byte-exact HTML lock (would_certify=0 under r4).
- Forks: F1/F3/F4/F5/F8/F9 APPROVE; F2 REJECTED (harness-only emitter); F6 deleted; F7 reuse SD-2a decide path.
- **Board inputs landed on bot/semdiff 5c271eb** (pid 6be15c81…): `tools/semdiff/board/{registry.tsv,baseline.json,README.md}`. registry 7199549d… (= notes decisions @ 4d465bd); baseline 06a5fadc… (~19.1 MB full summary). Positive ratchet rc0 regressed0; tampered baseline rc2. Paths **non-trivial** → Arch review needed before land. Did not run land.sh.
- Official board ae266f8c1456 VALID unchanged. Scratch only under `/workspace/scratch/semdiff/`. Do not message agents.

## Owed REWORKs (ids)
- AB-L0064 merge conditions done at e1fd366 (pid dc2e9503…). Awaiting Arch delta recheck. SD-2c design rework is AB-D0006.

## Branch/worktree + last landed main commit
- Worktree: /workspace/wt/semdiff  branch bot/semdiff @ **eac1ae9** (== main, ff rebase onto 82ecba7/eac1ae9).
- Uncommitted: none
- Last landing on main by this bot: **c3bab62** (AB-L0064).

## Last trusted board
- Official src=**ae266f8c1456** commit=**82ecba7** (published 2026-11-09T03:32:53-08:00); napi c4cd4cb09eac; oracle b9e8007 clean.
- Rows this bot moves: semantic-diff (schema note landed in proposals; H-3 wiring still Harness).
- Provisional? no for this src (VALID). Live-tree numbers from before this board are superseded.

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

## 2026-11-09 01:37 PST: AB-L0064 r4
- Tip **1dab7cd** on main **2a5cff8**, pid **1e88047f65012dbcbe92e613f9c83179d66e1985**. Tests: 81 passed (`cargo test -p semdiff --locked --offline`: 3 lib, 21 cli, 6 decisions, 21 exec, 10 ratchet, 12 rewrite, 8 rules).
- EXEC_EQUAL path deleted (`Reason::ExecEqual` gone; rust-vs-rust asserts traces). ASI approved → rt-1. Trailing comma still PENDING; patterns allowed (D-0266); hole vs trailer by AST `None`/`Elision`, not token neighbors. Required tests: `[a,]=o` == `[a]=o`; `[a,,]=o` != `[a,]=o`; `[,a]=o` observed; assignment, declarator, param.
- Mutations, all killed: printed-differs-to-UNCLASSIFIED; PENDING-applies; drop-pattern-contexts; treat-pattern-elision-as-trailing (AST predicate); ASI-enabled-vs-pending; comma skip off; `is_semicolon` always false.
- Corpus: summaries 008f7583… byte-identical (jobs 6 / jobs 4). 16549 / 246 / 0 / 0 / 0. 0 class/reason changes vs ad6d47f.
- SD-2c written (D-0263, notes only). Did not run land.sh; messaged no agents.

## 2026-11-09 01:40 PST
- AB-L0064 r4 resubmitted: bot/semdiff 681aba0 on main 0b903dd, pid bc8e8661102919ea38fa62d6440e1ba72aabde55 (rebase of 1dab7cd, pid 1e88047f), 81/81. Corpus 16549/246/0/0/0, rt-1 (ASI applied, comma PENDING).
- SD-2c design (D-0263 toString channels) is at notes 64e01b1 and with Arch. No code until it's approved.
- On APPROVE: check the pid, add the trailer, run land.sh, then re-baseline the ratchet at rt-1.

## 2026-11-09 01:52 PST: AB-L0064 merge conditions
- Tip **e1fd366** on main **0b903dd**, pid **dc2e9503162ab5c0510aa4961c23feba6c570a72**. Tests: 82 passed.
- Deleted Class::EquivalentByExecution (variant, as_str, bucket arm, test guards). Historical summaries still read via totals/bucket strings.
- R-TRAILING-COMMA-1 approved; applied identity rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1.
- Mutations killed: restore variant (non-exhaustive match); comma pending vs applied.
- Corpus sha256 64f46d36… (jobs 6 == jobs 4). 0 class/reason deltas vs 681aba0 / 008f7583…. Did not run land.sh; messaged no agents.

## 2026-11-09 01:54 PST: SD-2c AB-D0006 rework
- Rewrote arch-board/proposals/semdiff-SD-2c-toString-channels.md (F1–F3/F5/F6 APPROVE; F4 one Function.prototype.toString trap; soundness: static 3/4 complete, trap is cross-check, miss → exit 2; reasons tostring-<channel>).
- Premise gate: /workspace/scratch/semdiff/sd2c-premise-gate/measure.py (sha c4497fc7…) output sha 5093d392…. Would certify 140/246 (C1 105 + C2 35); 0 hoist-only; do not defer on zero-value grounds unless motive is hoist-only only.


## 2026-11-09 01:55 PT
- AB-L0064 merge conds done: tip e1fd366 / pid dc2e9503 (notes 8ba946a). Summary 64f46d36…, no deltas vs 008f7583. Sent to Arch for pid confirm. Next: verify pid with review-patch-id.sh, amend Arch-Review: AB-L0064, land.sh (now runs main board.sh check ratchet per AB-0006/D-0255, +70-90s), reset ratchet baseline at rt-1 applied-set, notify Migrate.
- SD-2c reworked (notes 7b40438), resubmitted. Premise gate: 140/246 certify (C1 105, C2 35), 0 hoist-only. Asked Arch: build now vs defer until S4.
- Land gate: case removal needs Case-Set: D-NNNN trailer (D-0261); harness/test|src edits need pid-bound Arch-Review.

## 2026-11-09 ~02:00 PT
- AB-L0064 APPROVE (Arch notes b22a8a4), pid fa5a165f. The executor's dc2e9503 was wrong; the script gives fa5a165f. Rebased onto ebfad54 with the pid unchanged. Branch bot/semdiff 6814721 has the trailer.
- land.sh REFUSED: main's baseline-ceiling.sh still has the NEW-seed path, and land.sh:71 dies, which blocks all lands. Sent to Harness (priority) and Migrate. Next: when Harness pings, rerun land.sh, re-baseline the ratchet at the rt-1 applied set, and notify Migrate.
- SD-2c: design APPROVE, build DEFERRED (AB-D0006 r2, D-0270). After S4, rerun /workspace/scratch/semdiff/sd2c-premise-gate/measure.py on the fresh official board; drop it if only known Template bugs certify.

## 2026-11-09 ~02:25 PT
- AB-L0064 LANDED: main c3bab62 (pid fa5a165f). The board check gave 22 rows equal. Migrate notified.
- Re-baselining the ratchet at rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1 (executor running).
- land.sh:229 hardcodes $wt/target and ignores CARGO_TARGET_DIR. The workaround is to land with CARGO_TARGET_DIR unset and TDD_DRIVER_TARGET=/workspace/.targets/tdd-semdiff. Reported to Harness.
- Waiting: Template S4, then re-cluster plus rerun the SD-2c measure.py on a fresh official board (D-0270). Later: SD-2b SSR leg, H-2.

## 2026-11-09 ~02:45 PT
- Post-S4 LIVE (e75ce42): 16795/0/0/0/0, improved 246, 0 clusters, SD-2c would-certify 0, so DROP once the official board confirms (D-0270). Baseline 4b281b75 (provisional; prev f493b75f).
- Waiting: Harness to publish an official board at >= e75ce42 (asked, priority). Then rerun on the snapshot and report the D-0271 recount to Migrate/Template.
- Next: SD-2b SSR exec leg design (D-0195); semantic-diff board row with Harness.

## 2026-11-09 03:47 PT
- Official board ae266f8c1456 @82ecba7 VALID; D-0271/D-0270 recount running (crate authoritative + napi). Owe Harness: result file + schema (proposals/semdiff-board-row.md).
- D-0275: own scratch only under /workspace/scratch/semdiff/. TODO after recount: move h1-*, sd2c-premise-gate, semdiff-ratchet-baseline, semdiff-official-ae266f8c1456 there and fix references. Told Migrate no rm by me under scratch.

## 2026-11-09 03:52 PT
- OFFICIAL ae266f8c1456 @82ecba7: crate 16795/0/0/0/0 (summary 06a5fadc), napi 16352/0/0/0/0 (e249e3d8). 0 clusters, SD-2c would_certify 0 -> DROPPED (D-0270). D-0271 C1-C5=0 confirmed.
- D-0275: moved my scratch dirs to /workspace/scratch/semdiff/ (semdiff-ratchet-baseline, semdiff-official-ae266f8c1456, sd2c-premise-gate, h1-1dab7cd, h1-e1fd366, h1-official-ae266). h1-b87b3fb was already gone (not deleted by me to my knowledge); registry pin copy survives in semdiff/h1-official-ae266/reg-pinned.tsv.
- Next: SD-2b SSR exec leg design (D-0195); semantic-diff row with Harness (schema proposals/semdiff-board-row.md).

## 2026-11-09 03:57 PT
- SD-2b design 9b6fc46 sent to Arch (630 SSR modules reach exec; forks F1-F9, F7 comments rule needs ruling).
- Harness blocker for row: registry pin + baseline must be in repo. Preparing tools/semdiff/board/{registry.tsv,baseline.json,README.md} on bot/semdiff; then Arch review if non-trivial, land, send Harness repo paths. Harness queue: obscope, $MAIN-ref gate, then row.

## 2026-11-09 04:04 PT
- Board inputs 5c271eb (pid 6be15c81) sent to Arch for review. Open: 19.1 MB baseline vs a stripped v1 (~3.4 MB). After APPROVE: verify pid, add the trailer, land (CARGO_TARGET_DIR unset), send Harness the repo paths.
- SD-2b r2 025d5ba sent to Arch. F1 grain: 353/359 bundles not printer-equal; sync raw HTML differs 0 (334 equal, 19 throw-both). Those will be DIFFERENT static-observed for SSR Rust once the Harness emitter exists. Still design only; Harness owns the emitter (after its gate land, then the row).

## 2026-11-09 04:06 PT
- AB-L0078 REWORK: dropped 5c271eb (kept as local branch semdiff-board-vendor-dropped). The row is --gate strict, with the registry taken from the notes commit. Harness unblocked; contract sent.
- AB-D0007-r2 APPROVE. Code conditions: the emitter accounts for every fixture (387 fixtures, 359 bundles; the 28 missing surface as named cases or UNCLASSIFIED; a completeness check that exits 2), plus async/asyncYield numbers in the PR. Code waits on the Harness emitter (queued 3rd).
- Idle on code until the emitter. Watch: the row going live on the next official board.
