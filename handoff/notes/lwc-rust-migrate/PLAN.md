# LWC → Rust compiler: plan of record

Owner: LWC Migrate (only Migrate edits this file). Written 2026-11-01 21:55 PST.
Source of truth order: this file → `state/<bot>.md` → cited files. Chat summaries are never authoritative.
Repos: oracle `/workspace/lwc` @ b9e8007 (9.5.3, clean); Rust main `/workspace/lwc-compiler-rust` (@ a9089c0 at writing);
notes `/workspace/notes`. Workflow: `/workspace/lwc-compiler-rust/docs/WORKFLOW.md` (worktree per bot, `scripts/land.sh` only path to main).

## 1. Fixed direction (do not relitigate; changes need a new decisions.tsv row by Migrate)
- Full-Rust breaking exit; JS compiler packages deleted per `lwc-rust-migrate/CUTOVER.md` ("Remove / stop publishing").
- Goldens lock runtime semantics; new goldens allowed; no Babel byte-identity (PROGRAM.md "Gates", "JS path locks").
- ESM only. AMD is the consumer's job (CUTOVER.md header). No JS shim, long-lived or interim (PROGRAM.md seam; shim gate).
- Oracle = clean `/workspace/lwc` @ b9e8007; board refuses a dirty oracle (decisions.tsv D-0010).
- A parity gate counts as CLOSED only when all three pass on the same official board: clean oracle,
  option coverage (`option-gate`: all fields mapped), unknown-option rejection (backend.ts `assertKnownKeys` + napi `deny_unknown_fields`).
- Render rows gate; diagnostic rows (marked "diagnostic, non-gating" in board.txt) never gate.
- Fix root causes at the owning layer (`arch-board/ARCHITECTURE.md` target modules + invariants). Non-trivial landings need
  an `Arch-Review: <id>` trailer whose reviews.tsv row is APPROVE with matching patch_id (WORKFLOW.md; trivial = `arch-board/trivial-paths.txt`, today `docs/*` only).
- Divergences from upstream exist only as logged rows in `lwc-rust-migrate/decisions.tsv`. Column 1 is a stable `id` (`D-NNNN`, never renumbered or reused); add a row only with `bin/d-id <bot> <phase> <decision> <why> <evidence> <result>` (allocates the id and ts under a lock); cite rows by id, never by line. Coverage cells cite `dec:D-NNNN`.

## 2. Current official board
Newest by `published=`: `/cursor/stores/store-4280618e-f4d8-4614-b1b6-bf88ea27f849/official/ac1b61fb239a/` (strict key=value manifest, AB-L0012).
commit=c731fb601f40 · src=ac1b61fb239a · napi=acfc2e0095b1 · ext_inputs=cac475305ab2 · oracle b9e8007 clean. Gating rows identical to dfa221a8867a.
Non-gating: template-diag crate 62588 clean / 636 mismatch; template-diag-napi (non-authoritative) 61594 / 1630 incl. 1120 seam-reject (api_version float, AB-0008).
**PROVISIONAL until AB-0007 lands and a fresh board is published** (AB-0003 approved). Applies to every official board so far
(ac1b61fb239a, dfa221a8867a, ae6f360e4294, 4aa80d2cad56, d366b22bed97).

| gating row | pass/total | note |
|---|---|---|
| style / style-api | 70/70 · 829/829 | |
| js / js-eval | 174/174 · 2/2 | |
| js-options / js-api | 302/1541 · 1239/1541 | complementary skip split (302+1239) |
| template-options / template-api | 67/67 · 5671/5671 | |
| template-renderer | 14/568 | not skips: a vitest -t filter (D-0140 subset); full file runs from the next official board (D-0217) |
| csr-render | 355/390 | 35 fail: slot-name 16, other 14, attr-values 4, attr-order 1 |
| csr-render-template | 545/570 | 25 fail: slot-name 11, attr-order 7, attr-values 4, other 3 |
| ssr-render / -async / -asyncYield | 384/389 each | 5 skipped; not progress until AB-0002 |
| ssr_render_api | 2710/2724 | 14 skipped; not progress until AB-0002 |
| ssr_dynamic_imports | 16/16 | not progress until AB-0002 |
| shim-check untagged | 0 | GATING |
| option-gate | 61/61 mapped, 0 pending, 0 invalid | |

Non-gating: template 568/568; ssr, ssr-async, ssr-asyncYield 388/388 (not progress until AB-0002);
template-diag 61400/63224 clean, 1824 mismatches, 0 crash.

## 3. Ownership and serial queue (D-0276, 2026-11-09)
One worker, one queue, one item at a time. **LWC Worker (28881e7c, formerly LWC Harness)** owns the whole workspace: every crate, `harness/**`, `scripts/**`, `tools/semdiff`, napi packaging, CI, the official board, and `CUTOVER.md`. Migrate (only contact with Tariq) owns this file, decisions.tsv rulings and OWNERS contents. Arch Board (af0d1105) reviews **milestones only**: a gate flipping, an ownership or direction change, and cutover. No per-landing Arch-Review trailer.
Parked (no work, routines deleted; their state/<bot>.md is the handoff): Template Rust 766f1a0b, SSR Rust 37aea75f, JS Style e79e5456, Semantic Diff 45d9257f, Ship 2d877d6d. Retired earlier: postcss, diagnostics, debugger-view.
Rules that still stand: land.sh is the only path to main; numbers only from official boards; D-0268 disk hygiene (remove slice worktrees on landing, one worktree, >=15G free before a board); D-0275 store-first audit logs; no build unlocked.

Serial queue (Worker pulls the top item; Migrate reorders; finish or park an item with a state note before starting the next):
1. OWNERS = single owner; land.sh drops the per-land Arch-Review requirement (milestone reviews only). Collapse parked bots' worktrees and branches: land approved-but-unlanded work (Template AB-L0058-FU f3f5fe5), archive the rest as tags, remove the worktrees.
2. Fresh official board on current main (first board with eac1ae9 CI, D-0239/D-0241, AB-L0079 gate).
3. semantic-diff board row: `--gate strict`, no baseline, `--registry` = decisions.tsv at NOTES_COMMIT, assert manifest case count and corpus (Arch 03112c1).
4. Template S5 (SSR consumes .ast), then S6.
5. ssr-render: clear the 5 skips (x3 modes).
6. SSR pair emitter on ssr-render's bundleFixture path, then SD-2b exec leg (AB-D0007-r2 approved).
7. JS: D-0265 phase 2 residuals (3 shim rows); CompileError raw position as one typed variant; then D-0243 as one wave after AB-D0004.
8. Style: 13 GAPs after D-0246; unlocked cargo in tools/style-diff/run.mjs and template-diag-diff.mjs.
9. Ship: D-0235 fmt green, C8 GitHub run (waits on Tariq enabling Actions), napi packaging, CUTOVER.md, consumer moves, JS-package deletes.

## 4. Remaining exit criteria (full cutover)
- [ ] Golden-gate hole closed (AB-0003, AB-0007); fresh official board published; all earlier boards stay provisional.
- [ ] Board fails on row regressions (AB-0006); land.sh runs option gate + behaviour ratchet + dependents (AB-0005).
- [ ] csr-render 390/390 and csr-render-template 570/570 on an official board.
- [ ] ssr-render ×3, ssr_render_api, ssr_dynamic_imports hold after AB-0002 lands (only then do SSR numbers count).
- [ ] Every skipped test (ssr-render 5, ssr_render_api 14, template-renderer 554 resolved by D-0217) justified in decisions.tsv or removed.
- [ ] Unknown-option rejection asserted by an explicit board check, alongside option-gate and clean oracle.
- [ ] semantic-diff gate green: every oracle↔Rust output diff on every corpus classified deterministically; DIFFERENT without decisions id = 0, unclassified = 0.
- [ ] Coverage gate green: every input-source × stage cell in arch-board/COVERAGE.tsv backed by a board row or a decisions.tsv id (Arch Board owns matrix; Harness builds gate).
- [ ] LBC whole-component parity row closed (compile + SSR/CSR render on both engines), gating.
- [ ] One option schema (AB-0008); dispatch `.mts` + LWC1005 (AB-0009).
- [ ] One error model (AB-R003, AB-0001, AB-R001); one LWC_VERSION (AB-0010); shared tables in lwc-shared (AB-0011).
- [ ] Zero open REWORK rows in `arch-board/reviews.tsv`.
- [ ] CUTOVER.md preconditions: shims 0 (tagged + untagged), crate-surface review, `cargo check && cargo test` on main.
- [ ] Move engine-server fixtures out of @lwc/ssr-compiler; consumers on the Rust API (CUTOVER.md).
- [ ] Delete JS packages and run CUTOVER.md "Verify"; rollback path per CUTOVER.md "Rollback".

## 5. Protocol (anti context rot)
- Every bot, at the start of every wake: read this file, then `/workspace/notes/state/<bot>.md`
  (bots: harness, template, js-style, ssr, semdiff, ship, arch-board). If missing, copy `state/TEMPLATE.md`.
- Before ending any turn in which its state changed (item, branch tip, landing, board, dead end, question),
  the bot updates `state/<bot>.md` and commits it in `/workspace/notes`. Write facts with paths and shas, not narrative.
- Only Migrate edits PLAN.md. Bots put disagreements under "Open questions for Migrate" in their state file.
- Arch Board flags drift between state files and actual landings (`git log main`, reviews.tsv, newest official board)
  as a reviews.tsv row or a note to Migrate.
- Numbers come only from an official board in the store (newest `published=`). Live-tree runs are not reportable.
- Shared-box hygiene: in `/workspace/notes` commit only with `bin/notes-commit <bot> -m <msg> <file>...` (the hook refuses anything else; ownership is `OWNERS.tsv`).
  Never kill by pattern (`pkill -f`, `killall`); kill only PIDs your own turn started. Remove a worktree only with `git worktree remove` (no `--force`)
  after confirming its branch has no unmerged commits; otherwise ask Migrate.
- D-ids: allocate only with `bin/d-id` (locked; rejects duplicates and future timestamps). Never hand-edit new rows into decisions.tsv.
- Judgment rules:
  1. Cross a lane only after the owner agrees. A `Cross-Lane` trailer records that agreement; it is not permission.
  2. Migrate forwards decisions, never unverified scores or results.
  3. Never write into another bot's worktree or into shared global config. Sole exception: Harness owns `/workspace/.cargo/config.toml` (D-0228).
  4. Kill only your own processes; never `git worktree remove --force`.
  5. A handoff lists every branch with commits not on main (`git branch --no-merged main`).
  6. Keep this file's worktree notes true; tell Migrate when one goes stale or live.
- Branch names: `bot/<bot>-*` only. `wip/*` must be renamed to its owner's `bot/` prefix before it lands (e.g. `wip/sourcemap` -> `bot/js-sourcemap`).
