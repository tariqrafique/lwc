# LWC → Rust compiler: plan of record

Owner: LWC Migrate (only Migrate edits this file). Written 2026-11-01 21:55 PST. Updated 2026-11-11 04:42 PST from official board f3cd89e5c936.
Source of truth order: this file → `state/<bot>.md` → cited files. Chat summaries are never authoritative.
Repos: oracle `/workspace/lwc` @ b9e8007 (9.5.3, clean); Rust main `/workspace/lwc-compiler-rust` (@ 505777693b50, official board f3cd89e5c936);
notes `/workspace/notes`. Workflow: cloud agents port on `agent/*`; LWC Worker lands with `scripts/land-agent-branch.sh` and publishes the official board (`docs/WORKFLOW.md`, D-0276/D-0279/D-0281/D-0287).

## 1. Fixed direction (do not relitigate; changes need a new decisions.tsv row by Migrate)
- Full-Rust breaking exit; JS compiler packages deleted per `lwc-rust-migrate/CUTOVER.md` ("Remove / stop publishing").
- Goldens lock runtime semantics; new goldens allowed; no Babel byte-identity (PROGRAM.md "Gates", "JS path locks").
- ESM only. AMD is the consumer's job (CUTOVER.md header). No JS shim, long-lived or interim (PROGRAM.md seam; shim gate).
- Oracle = clean `/workspace/lwc` @ b9e8007; board refuses a dirty oracle (decisions.tsv D-0010).
- A parity gate counts as CLOSED only when all three pass on the same official board: clean oracle,
  option coverage (`option-gate`: all fields mapped), unknown-option rejection (backend.ts `assertKnownKeys` + napi `deny_unknown_fields`).
- Render rows gate; diagnostic rows (marked "diagnostic, non-gating" in board.txt) never gate.
- Fix root causes at the owning layer (`arch-board/ARCHITECTURE.md` target modules + invariants). Arch Board reviews milestones only (a gate flipping, ownership or direction change, cutover); no per-landing Arch-Review trailer (D-0276).
- Divergences from upstream exist only as logged rows in `lwc-rust-migrate/decisions.tsv`. Column 1 is a stable `id` (`D-NNNN`, never renumbered or reused); add a row only with `bin/d-id <bot> <phase> <decision> <why> <evidence> <result>` (allocates the id and ts under a lock); cite rows by id, never by line. Coverage cells cite `dec:D-NNNN`.
- D-0282–D-0287: 30-minute land+official-board target, change-scoped rows, hash reuse, full cold every 5 lands or 6h (D-0282); Style item 9 was cloud work (D-0283); SD-2b asyncYield chunk boundaries and `this.hostElement` throw match the oracle (D-0284); CSR interaction steps were item 7b (D-0285); one-pass lands, registries = main ∪ incoming, removals/loosening need a D-id (D-0286); fixes to already-landed work go onto box main as plain Worker commits and iterate until the official board is VALID (D-0287). The GitHub `rust-compiler` push still waits for a VALID official board unless Tariq says otherwise.

## 2. Current official board
Newest by `published=`: `/cursor/stores/store-4280618e-f4d8-4614-b1b6-bf88ea27f849/official/f3cd89e5c936/` (strict key=value manifest).
status=VALID · producer=scripts/official-board.sh · commit=505777693b50 · src=f3cd89e5c936 · napi=2960afb98645 · ext_inputs=5104ccf5e2bb · notes_commit=d83d1fac508d · oracle b9e8007f9810 clean · published=2026-11-11T04:37:11-08:00 (America/Los_Angeles). Item 10b pass 2 landed (agent/coverage-green 5340bb6 + D-0289 cite swap 854710a). Then agent/js-transform-diffs f2c1d23 landed: upstream-cmp 1433/1436 (3 compile-error-both, LWC1007), js-transform-corpus 1958 with a diff baseline of 0 (was 1939; named semdiff rules with planted negatives; residual ac19ae2), unported-specs 24, napi-css-errors 5/5. Coverage report-only: 96 covered, 2 gap, 76 na, 18 dec, 0 invalid.
New in this board: the gating `lbc` row (`Tests  523 passed (523)`; `discovered=522 included=522 excluded=0`) and the `coverage-gate:` line (report-only: 192 cells, 26 covered, 11 diagnostic-only, 77 gap, 76 na, 2 dec, 0 invalid). Indented GAP/DIAG-ONLY cell lines are omitted here; they are in the official `board.txt`.
Row lines copied verbatim from `board.txt`:

```
style                  Tests  70 passed (70)
js                     Tests  174 passed (174)
js-eval                Tests  2 passed (2)
template               Tests  568 passed (568)  (diagnostic row: not option coverage; case set still ratcheted)
ssr                    Tests  388 passed (388)  (diagnostic row: not option coverage; case set still ratcheted)
ssr-async              Tests  388 passed (388)  (diagnostic row: not option coverage; case set still ratcheted)
ssr-asyncYield         Tests  388 passed (388)  (diagnostic row: not option coverage; case set still ratcheted)
ssr-render             Tests  389 passed (389)
ssr-render-async       Tests  389 passed (389)
known-limitation: ssr-render-async scope-token (D-0242) both backends render [object Promise]
ssr-render-asyncYield  Tests  389 passed (389)
csr-render             Tests  395 passed (395)
csr-render-template    Tests  571 passed (571)
template-renderer      Tests  568 passed (568)
js-options             Tests  302 passed | 1239 skipped (1541)
js-api                 Tests  1239 passed | 302 skipped (1541)
template-options       Tests  67 passed (67)
ssr_dynamic_imports    Tests  16 passed (16)
style-api              Tests  829 passed (829)
template-api           Tests  5671 passed (5671)
ssr_render_api         Tests  2724 passed (2724)
template-diag          cases 63224  clean 63116  mismatches 108 (code 2, message 21, level 0, location 0, order 4, missing-diag 50, extra-diag 31)  codegen-unclean 0 (STATIC_EQUAL 46321, REWRITE_EQUAL 16795, exact-bytes+semdiff)  crash 0  oracle-error 0  (diagnostic, non-gating)  SOUND
template-diag-napi     cases 63224  clean 62027  mismatches 1197 (code 2, message 0, level 0, location 0, order 4, missing-diag 42, extra-diag 29, seam-reject 1120)  codegen-unclean 0 (STATIC_EQUAL 45675, REWRITE_EQUAL 16352, exact-bytes+semdiff)  crash 0  oracle-error 0  (diagnostic, non-gating)  SOUND
style-upstream         Tests  12 passed (12)
lbc                    Tests  523 passed (523)
lbc: discovered=522 included=522 excluded=0 compile-error-both=0
semantic-diff  eq=16795 diff_u=0 diff_d=0 unc=0 crash=0  gate=ok  rt=rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1
semantic-diff-ssr  printer-equal=18  static-observed-differs=3  exec-static-comments-differ=1071  exec-differs=0  exec-throw-one-side=0  exec-uncovered=0  crash=0  exclusions=69  exclusion-reasons=compile-error-both:69  sync:pe=6,sod=1,esc=357,ed=0,et=0,eu=0,ex=23  async:pe=6,sod=1,esc=357,ed=0,et=0,eu=0,ex=23  asyncYield:pe=6,sod=1,esc=357,ed=0,et=0,eu=0,ex=23  gate=ok
shim-check: SHIM(template-gap:=0  SHIM(|TODO(parity=0  (cutover blockers until 0)  untagged=0 (GATING)
shim-registry: tags=0 rows=0 untracked=0 stale=0 (GATING)
ssr-compile-js-scanners: count=9 baseline=9 (GATING, must be equal)
option-gate: 61 fields, 61 mapped, 0 pending, 0 invalid  [decisions /tmp/lwc-board-snap.NtgUmz/notes/decisions.tsv]
coverage-gate: notes 06afd5e9c148 matrix 121647686387 report-only: 24 sources x 8 stages = 192 cells, 26 covered (declared gating), 11 diagnostic-only, 77 gap, 76 na, 2 dec, 0 invalid
golden-io-guard: ok (golden writes + UPDATE_GOLDENS only in src/optionRows.ts)
style-parse-errors     67/67 (lwc-style fixture tests, LWC_STYLE_STRICT=1)
csr-clusters[ssr]: pass 392 fail 0
csr-clusters[template]: pass 568 fail 0
style-corpus     integration-wtr=206/206 integration-wdio=7/7 integration-types=1/1 lbc=584/584 lbc-recipes=32/32 slds=223/223 fixture-smoke=69/69 parse-error=67/67 dedup=319 jobs=5136 fail=0 (uncapped style-diff, hash-dedup, direct oracle)
napi-css-errors     2/2 (lwc-compiler-napi single_errors_match_shipped, css-1009 css-1009-host-context)
```

## 3. Ownership and serial queue (D-0276, D-0281, D-0287)
One worker, one queue, one item at a time. **LWC Worker (28881e7c)** is the box gatekeeper: it fetches a finished `agent/*` branch, lands it (`scripts/land-agent-branch.sh` / `land.sh`), publishes the official board, and (per D-0287) applies fixes to already-landed work as plain commits on box main until that board is VALID. It owns the workspace trees (every crate, `harness/**`, `scripts/**`, `tools/semdiff`, napi packaging, the official board, `CUTOVER.md`). It does not port the next queue item. Migrate (only contact with Tariq) owns this file, decisions.tsv rulings and OWNERS contents. Arch Board (af0d1105) reviews **milestones only**: a gate flipping, an ownership or direction change, and cutover. No per-landing Arch-Review trailer.
Parked (no work; their state/<bot>.md is the handoff): Template Rust 766f1a0b, SSR Rust 37aea75f, JS Style e79e5456, Semantic Diff 45d9257f, Ship 2d877d6d. Retired earlier: postcss, diagnostics, debugger-view.
Rules that still stand: land.sh is the only path from an `agent/*` branch onto main (D-0287 is the exception for post-land fixes); numbers only from official boards; D-0268 disk hygiene; D-0275 store-first audit logs; no build unlocked; no PRs, no GitHub Actions, no force-push, `agent/*` only, delete after landing (D-0279/D-0280/D-0281). After D-0286, do not change land/board machinery unless the current item cannot land.

Finished items (land sha = the commit on `main` that carried the work; board id = the VALID official board published for that tip; D-0286 lands as main ∪ incoming so the board commit may be a later snapshot refresh on the same land):

1. **DONE** OWNERS = single owner; land.sh drops the per-land Arch-Review requirement. Land `dd99d0a` (2026-11-09 13:37 PST). No dedicated official board of its own; first subsequent VALID board is item 2 (`78a3500dd541`).
2. **DONE** Fresh official board on then-current main. Land / board commit `3ecff19`; official board `78a3500dd541` (VALID, published 2026-11-09 22:03 PST).
3. **DONE** (D-0281, 2026-11-10): (a) portable harness landed `c9e03e1`; (b) cloud fresh-clone board MATCH vs official `6e5d1ca4f011` (VALID, commit `c9e03e1`, published 2026-11-10 05:56 PST); (c) in force — cloud agents port on `agent/*`, Worker gatekeeps. D-0277/D-0278 superseded by D-0279+D-0280 (no Actions, no PRs).
4. **DONE** semantic-diff board row (`--gate strict`, no baseline). Feature land `8cf3c64`; official board `2ead84925858` (VALID, commit `1017bce`, published 2026-11-10 03:51 PST).
5. **DONE** Template S5 (SSR consumes `.ast`): land `a82b713` (AB-D0004 S5); official board `8a00cf3f860c` (VALID, commit `5e2230f`, published 2026-11-10 06:37 PST). S6 (first cloud task, `agent/s6`): land `8e338f9`; official board `811ad3f956d2` (VALID, published 2026-11-10 07:49 PST).
6. **DONE** ssr-render: clear the 5 skips (×3 modes). Feature land `d9e0e35` (reseed `33cbac3`, D-0242 byte-compare `7c7ae1b`); official board `ecfcb59de9ad` (VALID, commit `1b562b0`, published 2026-11-10 09:08 PST). Known-limitation D-0242 remains on the board (both backends render `[object Promise]` for ssr-render-async scope-token).
7. **DONE** SSR pair emitter + SD-2b exec leg. Feature land `f73cc9d` (yield merge `6e64d5c`, oracle path `a5206e7`); official board `5e80e0ff4f72` (VALID, commit `6026c08`, published 2026-11-10 15:41 PST).
7b. **DONE** (D-0285) CSR interaction steps after first mount. Feature land `4ee0bd7`; official board `5aafe4b7f269` (VALID, commit `bcabc2e`, published 2026-11-10 17:11 PST). csr-render 395/395, csr-render-template 571/571.
8. **DONE** JS: D-0265 phase 2 residuals (3 shim rows) and CompileError raw position as one typed variant. Agent tip `60fab49` (not on main; parent `1b562b0`); landed on main as `bb64d8d`; official board `0d4392d66b82` (VALID, commit `bb64d8d`, published 2026-11-10 10:58 PST). D-0243 was deferred and is now item 14.
9. **DONE** Style: close the live coverage GAPs (D-0283, `agent/style-gaps`). Feature land `8f9bb1b`; official board `edcfde44d1b8` (VALID, commit `3d3d152`, published 2026-11-10 20:21 PST). style-upstream 12/12, style-corpus 5136 jobs fail=0, napi-css-errors 2/2.

Also landed after item 9, not a leftover port item: **D-0286** one-pass lands. Commits `4bd18a3` / `123512b` / `ee92647`; official board `37036c11d866` (VALID, commit `ee92647`, published 2026-11-10 20:52 PST). Every scoreboard row identical to `edcfde44d1b8`.

Remaining queue (Worker or a cloud agent pulls the top item; Migrate reorders; finish or park an item with a state note before starting the next). From D-0281 on, porting is a cloud-agent task on `agent/<item>` unless listed as box-only Worker work:

10. **DONE** LBC whole-component row. Branch tip `179a794` landed in main `b2bb4be`; official board `66479d02aafa` (VALID, commit `b2bb4be`, published 2026-11-10 23:53 PST). `lbc` Tests 523 passed (523), discovered=522 included=522 excluded=0. The coverage-gate line is on the board and is still report-only.
10b. **DONE** (except the 2 template-diag-mutations cells, moved to item 12 together with the COVGATE_STRICT flip): js-transform baseline 0, upstream-cmp 1433/1436, coverage 96 covered / 2 gap / 18 dec / 0 invalid on official board f3cd89e5c936 (main 5057776). Coverage gate green and gating. Cloud-agent work (`agent/coverage-green`). Gap cells whose owner is a parked bot get no owner; work goes to cloud agents. **Done when:** every GAP cell is either covered by a gating board row or reclassified na/dec citing a D-id, then the coverage gate flips from report-only to gating. **Progress (00:53 PST 11-11):** pass 1 at agent tip 9dc5ff6 closed 44 cells (agent-reported, not yet on an official board: 70 covered / 44 gap). D-0288 scopes semantic_diff and crate-internal fixture cells to dec, and rejects the waivers for js_transform, render on the integration/rollup/remainder trees, and the unported compiler specs, which get real rows (pass 2 on the same branch). Mutations stay GAP until item 12, and strict flips after item 12. **Pass 2 (02:15 PST):** tip 5340bb6, agent-reported 96 covered / 2 gap / 18 dec / 0 invalid. The Worker lands it after item 13 with the dec:D-0288→D-0289 swap (D-0289 = D-0288 locked). D-0289 also says js-transform-corpus (baseline 1939/1957 diffs) doesn't count toward DONE until it's 0 or classified. Diff root-cause is on agent/js-transform-diffs (same cloud agent). **Landed 03:09 PST** at main 2fe71f2, board 55f1d0c2885f; branch deleted. js-transform-diffs landed at main 3d2e472, board fd035de9d515. Residual landed at main 5057776, board f3cd89e5c936. Follow-up on main (Worker): over-broadening negatives for R-VAR-LITERAL-IMPORT-1 and R-OBJECT-SHORTHAND-1, plus unit tests for the uid and directive fixes. Remaining for DONE: js-transform-corpus baseline to 0 or classified; the 22 upstream-cmp compile-error-both exclusions checked against each test's own compile config; the 2 mutation cells (item 12); then strict. **03:35 PST:** agent tip f2c1d23 (agent-reported) cuts the js-transform baseline 1939→36 with 6 named rules and planted negatives, and upstream-cmp exclusions 22→3 (the 3 left are LWC1007 TypeScript sources). It's being landed. Pass 3 on agent/js-transform-residual takes the last 36 to 0: var-init-order and duplicate-key first, then comment-dropped. **04:20 PST:** pass 3 tip 98ed2f6 (agent-reported) takes the ratchet 36→0, and the Worker is landing it. After that board, 10b is DONE except the 2 mutation cells, so the strict flip moves into item 12's done-when.
11. AB-0008 (one option schema, including the 1120 template-diag-napi `api_version` float seam-rejects) and AB-0009 (dispatch `.mts` + LWC1005). Cloud-agent work (`agent/ab-0008-0009`). **Running** since 04:22 PST, bc-a98e147f, brief /workspace/item11/BRIEF.md. **Done when:** a VALID official board records one option schema (unknown-option rejection asserted; napi seam-reject count no longer 1120 for `api_version` float) and dispatch `.mts` + LWC1005 NO_AVAILABLE_TRANSFORMER.
12. (Also owns closing corpus:template-diag-mutations × template_codegen/template_diagnostics, then setting COVGATE_STRICT=1 so the coverage gate gates.) template-diag mismatches: official board `37036c11d866` `template-diag.json` has `totals.diagMismatch=108` and a `mismatches` array of length 108 (board.txt: 108 = code 2, message 21, order 4, missing-diag 50, extra-diag 31). Cloud-agent work (`agent/template-diag`). **Done when:** a VALID official board shows template-diag mismatches=0, or each remaining mismatch is cited by a `D-NNNN` (no undocumented leftovers).
13. **DONE** D-0235 fmt gating green: `cargo fmt --all` 40e71c2, `cargo fmt --all --check` hard check 6255cc8, main b5fa58b, official board fbdb6da03d8a VALID. Box-only Worker work (D-0235 is a one-shot `cargo fmt` on main, then `cargo fmt --all --check` as a hard land/official-board gate; D-0286 says gate-code changes go main-first by the Worker; D-0280 dropped the Actions half). **Done when:** `cargo fmt --all --check` is green on main and a VALID official board / land path fails if it is not.
14. The D-0243 wave: one `lwc_shared::js::string_literal` and delete the crate-local copies (lwc-ssr, lwc-template, lwc-style). Cloud-agent work (`agent/d0243`). AB-D0004 has settled (S5/S6 landed), so the original “after AB-D0004” timing no longer blocks it. **Done when:** a VALID official board is green on the same rows as today and the extra quoters are gone.
15. Ship: linux-x64 napi (already packing on the board’s napi sha), a written and tested plan for building macOS and Windows napi **without** GitHub Actions (D-0280), CUTOVER.md brought current from the latest official board, consumers (`@lwc/rollup-plugin` and the other bundler integrations) on the Rust `transform` / `transformSync` API, JS compiler package deletes, CUTOVER verify. Box-only Worker work: consumer ports live in the box-local tree `/workspace/wt/lwc-consumers` (D-0202: never pushed, never read by the board) and CUTOVER verify/rollback touch local layout; other triples cannot be produced on this Linux box or a typical cloud Linux VM, so the plan is written and tested here. **Done when:** CUTOVER.md preconditions and verify steps are checked from a VALID official board plus the written macOS/Windows napi plan (tested as far as a Linux host can test it), consumers compile against the Rust API, and the JS packages slated for delete are gone from the consumer/cutover tree (oracle `/workspace/lwc` stays untouched).

2026-11-10 23:20 PST root-cause ruling (`/workspace/rootcause/report.md`): allowlists are judged main ∪ incoming (Worker fix1 `8585774`); a no-error-mute lint; one `scripts/preflight.sh` shared by land.sh and cloud agents; land self-tests run only when a branch touches gate files; gate-code commits must pass `run-land-self-tests.sh --all` (D-0287 does not cover gate code). Worker in flight: `scripts/preflight.sh` and the land self-test scoping.

## 4. Remaining exit criteria (full cutover)
- [x] Golden-gate hole closed (AB-0003 `5cc367c`, AB-0007 `cdd2a74`, 2026-11-01); current official board `f3cd89e5c936` is VALID, not provisional.
- [x] Board fails on row regressions (AB-0006 `ebfad54`); land.sh runs option gate + behaviour ratchet + dependents (AB-0005 `672ac09`).
- [x] csr-render 395/395 and csr-render-template 571/571 on official board `f3cd89e5c936` (original targets were 390/390 and 570/570; item 7b added cases).
- [x] ssr-render ×3 389/389, ssr_render_api 2724/2724, ssr_dynamic_imports 16/16 on `f3cd89e5c936` (AB-0002 work is landed; SSR numbers count).
- [x] Previously listed skips cleared: ssr-render 5 (D-0242 known-limitation remains, both backends), ssr_render_api 14, template-renderer full file 568/568 (D-0217). Any new skip still needs a decisions.tsv id.
- [ ] Unknown-option rejection asserted by an explicit board check, alongside option-gate and clean oracle (AB-0008, item 11).
- [x] semantic-diff gate green on `f3cd89e5c936`: eq=16795, diff=0, unc=0, crash=0, gate=ok.
- [ ] Coverage gate green: every input-source × stage cell in arch-board/COVERAGE.tsv backed by a board row or a decisions.tsv id (item 10b; report-only on `f3cd89e5c936` until remaining GAP cells are a row or a `dec:D-NNNN`).
- [x] LBC whole-component parity row closed (compile + SSR/CSR render on both engines), gating (item 10, official board `66479d02aafa`: 523/523, discovered=522 included=522 excluded=0).
- [ ] One option schema (AB-0008); dispatch `.mts` + LWC1005 (AB-0009) (item 11).
- [ ] One error model (AB-R003, AB-0001, AB-R001); one LWC_VERSION (AB-0010); shared tables in lwc-shared (AB-0011).
- [ ] Zero open REWORK rows in `arch-board/reviews.tsv`.
- [ ] CUTOVER.md preconditions: shims 0 (tagged + untagged) already hold on `66479d02aafa`; crate-surface review; `cargo check && cargo test` on main; fmt gate (item 13).
- [ ] Move engine-server fixtures out of @lwc/ssr-compiler; consumers on the Rust API (CUTOVER.md) (item 15).
- [ ] Delete JS packages and run CUTOVER.md "Verify"; rollback path per CUTOVER.md "Rollback" (item 15).
- [ ] template-diag 108 mismatches each fixed or cited by a D-id (item 12); D-0243 wave (item 14).

## 5. Protocol (anti context rot)
- Every wake: read this file. LWC Worker updates `state/harness.md` when its state changes (item, branch tip, landing, board, dead end, blocker) and commits it in `/workspace/notes`. Write facts with paths and shas, not narrative. Parked bots do no work.
- Only Migrate edits PLAN.md. The Worker puts disagreements under "Open questions for Migrate" in its state file.
- Numbers come only from an official board in the store (newest `published=`). Live-tree runs are not reportable.
- Shared-box hygiene: in `/workspace/notes` commit only with `bin/notes-commit <bot> -m <msg> <file>...` (the hook refuses anything else; ownership is `OWNERS.tsv`).
  Never kill by pattern (`pkill -f`, `killall`); kill only PIDs your own turn started. Remove a worktree only with `git worktree remove` (no `--force`)
  after confirming its branch has no unmerged commits.
- D-ids: allocate only with `bin/d-id` (locked; rejects duplicates and future timestamps). Never hand-edit new rows into decisions.tsv.
- D-0276 supersedes the old multi-writer protocol: one serial Worker; no Cross-Lane trailers; no per-bot worktrees; no writing into another bot's tree; Arch reviews milestones only.
- D-0279 / D-0280 / D-0281: no PRs, no GitHub Actions, no force-push. New work uses `agent/<item>` on tariqrafique/lwc only (not `bot/<bot>-*` or `wip/*`); each `agent/*` branch is deleted after it lands. The Worker's land of that branch, then the official board on this computer against `/workspace/lwc` @ b9e8007, is the gate.
- D-0268 disk hygiene and D-0275 store-first audit logs still stand.
- Migrate forwards decisions, never unverified scores or results.
