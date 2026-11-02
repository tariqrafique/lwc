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

## 3. Ownership
Path ownership is `OWNERS` at the repo root on main (D-0227), enforced by pre-commit and land.sh. The table below describes roles; where it names paths and disagrees with OWNERS, OWNERS wins. Only Migrate decides OWNERS contents.
Open = reviews.tsv verdict REWORK with no later APPROVE (last row per id wins). Closed and not owed:
AB-L0002/L0003/L0005 (→AB-L0006, same branch/files, inferred), AB-L0004 (→AB-L0009, landed b2e23d5),
AB-X0001 (→L0010), AB-X0002 (→L0008), AB-X0004 + AB-L0012 REWORK (→L0012 APPROVE, landed c731fb6),
AB-R005 follow-up (→AB-L0013, landed e7c35bf), AB-X0003 (→AB-L0015, landed a9089c0), AB-0004 (notes 8139fdc, CLOSED row written), AB-R004 + use_static + L0009 follow-up (→AB-L0016, landed 4f68532).

Fleet per D-0198 (restructure) and D-0199 (table-copy deletes, napi split). Retired: postcss, diagnostics, debugger-view (handoffs in their state files; no work to them).

| bot | owns | current item | owes, in order |
|---|---|---|---|
| harness (28881e7c) | `harness/**` (incl. template-diag scripts), `scripts/*.sh`, board/official/publish, coverage-gate, perf + source-map rows, `crates/lwc-compiler` options + dispatch | AB-0006 (+X0007, X0008, AB-0016, AB-X0010, template-diag UNSOUND label) | order per D-0218: fresh official board; coverage-gate report-only; AB-0008 (+d.ts schema input, api_version f64, undefined-per-field row D-0208, unknown-option row); AB-D0005 .pins snapshot + parse5 rev fail-closed; semdiff H-2/H-3; lbc row (Arch design first); AB-0004; AB-0009; `-p lwc-style` gates + 67 parse-errors with the fresh board |
| template (766f1a0b) | all of `crates/lwc-template` (parser, IR, codegen, validation/diagnostics), `lwc-shared/src/template_ir.rs` | CSR render parity, slot-name cluster first; LWC version banner comment (D-0196 observable) | renderer skips; AB-0011 a1 + generator (AB-D0002); AB-0013 after design review; inherited from diagnostics: ElementKind::LwcComponent, AB-X0009 + AB-0012, 1052 decision/test, 3 template_diagnostics coverage GAPs; 11 template_codegen GAPs |
| js-style (e79e5456) | `crates/lwc-js`, `crates/lwc-style`, shared error model in lwc-shared, napi lib.rs error surfacing | AB-R001 rebase onto main, Arch, land (notify template + ssr first) | slice 2 (delete napi reshaping, facades, lwc-js migration); AB-0010; own copies deleted on repoint (AB-0011 d, D-0199); 10 js + 13 style GAPs |
| ssr (37aea75f) | `crates/lwc-ssr` | AB-0002: AB-L0024 land; AB-L0025 rework (non-null class name by type; LWC_VERSION_COMMENT via one lwc_shared def); AB-L0026 rebase | S2..S5 on the shared error table; own copies deleted on repoint; 6 ssr_render GAPs |
| semdiff (45d9257f) | `tools/semdiff`, `semantic-diff` row, verdict schema (D-0195 reference, D-0196 observation model) | SD-1B classify codegen-only from H-1 pairs + cluster | rt-1 rules (each reviewed); SD-2 exec leg; 24 semantic_diff GAPs |
| ship (2d877d6d) | napi packaging files, platform binaries, publish, CI, consumer moves, JS-package deletes + no-duplicate-table lint at cutover, `CUTOVER.md`, section-4 preconditions | SHIP-0 read-only survey -> `lwc-rust-migrate/ship-survey.md` | CUTOVER.md draft from survey; packaging plan to Arch |
| arch-board (af0d1105) | `notes/arch-board/**`; sole reviewer | reviews, drift checks, COVERAGE.tsv | proposes ownership/scope changes to Migrate, never makes them |

Live worktree: `/workspace/wt/harness-gate` (branch `bot/harness-gate`, Harness's AB-0006 work). Not stale.

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
