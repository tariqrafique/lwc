# CONTINUE: LWC compiler → Rust migration (start here)

Written 2026-11-16 09:30 PST for an LLM or engineer who has **only a clone of
`github.com/tariqrafique/lwc`, branch `rust-compiler`**, and no access to the box, its bots or
Cursor cloud agents. Every number below comes from official board **65006817bd11**. Every rule
comes from `handoff/notes/lwc-rust-migrate/PLAN.md`, `decisions.tsv` (rows are cited as D-NNNN),
`CUTOVER.md` or the scripts in `compiler-rust/scripts/`. If this file disagrees with those files,
they win.

## (a) Goal and definition of done
- **Goal:** a full-Rust replacement for `@lwc/compiler` 9.5.3 (salesforce/lwc @ `b9e8007f9810edb59596536b596f80ca6c0d3fab`,
  the *oracle*). Rust crates `lwc-compiler` (facade), `lwc-style`, `lwc-template`, `lwc-ssr`, `lwc-js`, `lwc-shared`,
  plus the napi package `crates/lwc-compiler-napi`, published (later, see rules) as `@lwc/rust-compiler` (D-0291).
- **Fixed direction (PLAN §1):** a breaking exit. The JS compiler packages get deleted (`@lwc/compiler`, `@lwc/style-compiler`,
  `@lwc/template-compiler`, `@lwc/babel-plugin-component`, `@lwc/ssr-compiler`, CUTOVER.md §E). ESM only, no AMD, no JS shim.
  Goldens lock runtime semantics; no Babel byte-identity.
- **Done** = every unchecked line in PLAN §4 "Remaining exit criteria" and CUTOVER.md §A–§E is met on a VALID board:
  pretty-equal at N/N with an empty allowlist (D-0292, gates the JS deletes), real macOS/Windows napi load tests
  (`compiler-rust/docs/napi-cross-build.md` §4), consumers on the Rust API, JS packages deleted in one revertable commit,
  and CUTOVER.md "Verify" green. A parity gate counts as CLOSED only with clean oracle + option-gate all mapped +
  unknown-option rejection on the same board (PLAN §1).

## (b) Current status: official board 65006817bd11 (VALID)
Files: `handoff/official-board/65006817bd11/{OFFICIAL_BOARD.txt,board.txt}`.
`commit=3e5765a9c046` (box `lwc-compiler-rust@main`, = `handoff/lwc-compiler-rust.bundle` main), `notes_commit=ed6a5ab`,
`parse5_rust_commit=370629a079da`, oracle b9e8007 clean, published 2026-11-16T09:17:56-08:00.
- **pretty-equal 1555/1560 identical, allowlist 5** (was 1418/1560, allowlist 142 on 738958401bb4).
  Classes: blank-line 6, other 4, comment-added 2, comment-placement 2, comment-dropped 1. object-shorthand and duplicate-key are 0.
  The 5 cases (`compiler-rust/harness/pretty-equal-allowlist.txt`): LightningFlowComponents `fsc_flowButtonBar`,
  `fsc_flowButtonBarCPE` (blank-line), `ibanComponent` (blank-line, comment-dropped), `datatable`
  (comment-added, comment-placement, other); lwc-utils `fuseBasic` (other).
- semantic-diff eq=16809 diff 0 unc 0 crash 0; semantic-diff-ssr printer-equal 852, static-observed-differs 240,
  exec-static-comments-differ 0, exec-differs 0, gate=ok.
- template-diag and template-diag-napi: 63224 cases, 0 mismatches, SOUND. lbc 523/523. upstream-cmp 1433 passed / 3 skipped (1436).
- js-transform-corpus 1958/1958; template-options 444/444; js-options 469 passed + js-api 1239 passed (1708); option-gate 61/61 mapped.
- coverage-gate strict: 192 cells, 98 covered, 0 gap, 76 na, 18 dec, 0 invalid. shim-check 0, shim-registry 0.
- Render rows all green: csr-render 395/395, csr-render-template 571/571, ssr-render ×3 389/389, ssr_render_api 2724/2724.
  Known limitation D-0242 (ssr-render-async scope-token renders `[object Promise]` on both backends).

## (c) Ordered work queue (PLAN §3; do one item at a time, top first)
1. **18b, pretty-equal residuals.** Fix the 5 allowlisted cases above at the root (printer/comment handling in
   `crates/lwc-js` and `vendor/oxc_codegen`, as item 18 did), and delete each allowlist line as it goes to 0.
   *Done when:* pretty-equal 1560/1560 with an empty allowlist on a VALID board. When handed off, a Cursor cloud agent was working on
   `agent/pretty-residuals` (dispatched 09:21 PST 2026-11-16). Check `git ls-remote origin 'agent/*'` first; if that branch
   exists, continue it rather than starting over.
2. **19, F8:** semdiff compares comments and literal spelling from the source, not via the oxc reprint.
   *Done when:* no printer-equal verdict hides a hunk.
3. **20, F9:** option-sweep census (inline maps, LWS on, old apiVersion) plus the new external corpus.
   *Done when:* each variant is at N/N or allowlisted under a D-id, and pretty-equal is at N/N with an empty allowlist.
4. **Item 15 remainder (ship):**
   - Real macOS and Windows napi load tests. *Done when:* `docs/napi-cross-build.md` §4 steps 1–3 pass on darwin-arm64, darwin-x64,
     win32-x64 and win32-arm64 with Node 20 and 22. linux-arm64-gnu has never been loaded either (§4 last paragraph).
     Cross builds come from `scripts/build-napi-cross.sh` (zig 0.13, cargo-zigbuild, cargo-xwin; D-0290 six triples, D-0291).
   - Consumers on the Rust API (CUTOVER.md §D, D1–D8). *Done when:* the consumers compile and test against the Rust
     `transform`/`transformSync` API. **Caveat:** that port (commits `2dcd877`, `e5c7433` on `ship/consumers`) exists only in a
     box-local clone that D-0202 says is never pushed. See (g).
   - Delete the JS compiler packages (CUTOVER.md §E) in ONE revertable commit. *Allowed only when* pretty-equal is N/N (D-0292).
   - CUTOVER.md "Verify" and "Rollback". *Done when:* the four Verify commands pass (cargo check/test, harness with
     `LWC_COMPILER_BACKEND=rust`, the grep finds no JS compiler imports, consumers `yarn build && yarn test`).
5. **Residuals:**
   - **semdiff `--exec` on the board (D-0256(b), D-0285).** The exec leg stays off the official board until Semdiff signs off. D-0285
     says it gets revisited after item 7b, which is DONE. *Done when:* a D-id rules it on or off, and if on, the row is gating.
   - **pre-commit `--no-verify`.** The `scripts/git-hooks/pre-commit` hook refuses commits on `main` (bypass `LWC_BOOTSTRAP`).
     D-0287 fixes went onto box main with `--no-verify` (`notes/state/harness.md` lines ~676 and ~706).
     *Done when:* post-land fixes have a sanctioned path that does not skip hooks.
   - PLAN §4 still lists open boxes. Some are already met on this board (template-diag 0, coverage strict) and only need ticking.
     Others still need checking: one error model / LWC_VERSION / shared tables (AB-R003, AB-0001, AB-R001, AB-0010, AB-0011),
     crate-surface review, zero open REWORK in `notes/arch-board/reviews.tsv`, and the duplicate-table lint (CUTOVER B6).

## (d) Hard rules (do not break; from PLAN §1/§5, decisions.tsv, and Tariq)
- ESM only. No AMD, no CJS loader, no JS shim (CUTOVER header, PLAN §1).
- No Babel byte-identity. Goldens lock runtime semantics; recording new goldens is allowed (PLAN §1). Goldens are written only through
  the JS-oracle recording path (`harness/scripts/golden-gate.sh`; golden-io-guard allows `UPDATE_GOLDENS` only in `src/optionRows.ts`).
- Root-cause fixes only, at the owning layer (`notes/arch-board/ARCHITECTURE.md`). No masking, no stripped compares
  (`lint-no-stripped-compare`), no error mutes (`lint-no-error-mute`). Allowlists only shrink (D-0286).
- **No PRs** (never open one; parse5-rust PR #1 is never merged). **No GitHub Actions** and no `.github/workflows` changes (D-0280).
- **No force-push.** Never touch `master`. Push only to `rust-compiler` and `agent/*` on tariqrafique/lwc.
- Commit identity **`tariqrafique <tariqrafique@users.noreply.github.com>`** as both author and committer. Never a Salesforce
  corporate email address, and never a `Co-authored-by:` trailer. Check with `git log --format='%an <%ae> | %cn <%ce>%n%b'` before pushing.
- Oracle = salesforce/lwc 9.5.3 @ b9e8007 (`compiler-rust/harness/oracle.pin`). Never modify it; the board refuses a dirty oracle (D-0010).
- Never contradict a decisions.tsv row. Rows are append-only, and ids are never reused. Add a row only for a real compiler or cutover choice,
  and only with `notes/bin/d-id migrate <phase> <decision> <why> <evidence> <result>` (never hand-edit).
- Do not publish `@lwc/rust-compiler` to npm until Tariq says so (D-0202, D-0291).
- Pushes to `tariqrafique/parse5-rust` (the Rust HTML parser, pinned by Cargo.lock rev `370629a`) need Tariq's explicit OK each time.
- Rust toolchain is pinned in `compiler-rust/rust-toolchain.toml` (1.99.1). `cargo fmt --all --check` is a hard gate (D-0235). The clippy
  ratchet (`scripts/clippy-baseline.txt`, now **133**) may only go down.

## (e) Working without the box fleet
**Layout.** On `rust-compiler`, `compiler-rust/` is the Rust workspace (same tree as box main at the board commit, plus
board files), and `handoff/` holds this file, notes, the official-board copy and two git bundles. Recent agent branches
(e.g. `agent/object-shorthand`) branch from `rust-compiler` and edit `compiler-rust/`. Merge new `rust-compiler` tips into them; don't rebase.

**Restore notes (needed by the gates).** `scripts/preflight.sh` reads decisions from the hard-coded `/workspace/notes`
(`DEC_REPO`), and `harness/scripts/oracle-root.mjs` prefers `/workspace/notes/lwc-rust-migrate/decisions.tsv`, falling back to
`harness/decisions.snapshot.tsv`. So on a Linux host:
```
sudo mkdir -p /workspace && sudo chown "$USER" /workspace
git clone handoff/notes.bundle /workspace/notes && git -C /workspace/notes switch -c main   # notes HEAD = PLAN after board 65006817bd11
# optional, full box history of the Rust workspace (root layout, main @ 3e5765a9c046):
git clone -b main handoff/lwc-compiler-rust.bundle /workspace/lwc-compiler-rust
```
Notes commits: `cd /workspace/notes && bin/notes-commit migrate -m '<msg>' <file>...` (explicit files only; ownership in `OWNERS.tsv`).
The box's commit-msg hook is not in the bundle. Follow the same rules by hand.

**Build (from `compiler-rust/`).** Needs rustup (picks up 1.99.1), node, yarn, git.
```
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
scripts/install-napi.sh            # release napi -> harness/lwc-compiler-napi.node
```

**Portable board (the parity gate you can run anywhere, D-0280).**
```
cd compiler-rust
export LWC_ORACLE=$PWD/.oracle/lwc STYLE_CORPUS=$PWD/.oracle/style-corpus
scripts/fetch-oracle.sh --build    # clones salesforce/lwc @ oracle.pin, yarn build; never /workspace/lwc
scripts/fetch-style-corpus.sh
PORTABLE_BOARD_TXT=$PWD/../portable-board.txt scripts/run-portable-board.sh   # also fetches the pretty-equal sample corpora + Prettier 3.9.9
```
Verified on the box (2026-11-16, bundle main 3e5765a): `lint-no-stripped-compare`, `workspace-path-scan`, `lint-no-error-mute`, `check-land-main-refs` and `cargo-fmt-check` below all exit 0. The board, preflight, golden-gate and clippy commands were not re-run for this note.
Exit 0 = every gating row passed; 4 = the snapshot mutated (invalid); 2 = an input fetch or setup failed. Compare the rows with `handoff/official-board/65006817bd11/board.txt`.
A row may only go up (ratchets in `harness/ratchets`, `harness/board-baseline.tsv`).

**Preflight + lints (the land checks).**
```
scripts/preflight.sh --base <base-commit> --tree .        # prints a table; exit 0 required. <base> = the rust-compiler tip you branched from
scripts/lint-no-stripped-compare.sh .
scripts/check-land-main-refs.sh --check scripts/land.sh . --main-table scripts/land-main-refs.tsv --incoming-table scripts/land-main-refs.tsv
harness/scripts/golden-gate.sh harness                     # slow: re-records goldens on the JS oracle and diffs
scripts/workspace-path-scan.sh .
scripts/lint-no-error-mute.sh .
scripts/cargo-fmt-check.sh .
scripts/clippy-ratchet.sh .                                # needs rustc == rust-toolchain.toml; count > baseline fails, count < baseline fails until you lower scripts/clippy-baseline.txt in the same commit
```
Preflight runs the base commit's gate code (`git archive <base>`), so a branch cannot weaken its own gate. Gate-code changes must
also pass `scripts/run-land-self-tests.sh --all` (PLAN §3 note, 2026-11-10 23:20 ruling).

**Official board vs. what you can do.** On the box, `scripts/official-board.sh` builds from a commit via `git archive`, pins box
paths (`/workspace/lwc`, `/workspace/notes`, `/workspace/parse5-rust`), and publishes a VALID manifest
(`OFFICIAL_BOARD.txt`). The *LWC Worker* bot then landed agent branches with `scripts/land-agent-branch.sh` → `scripts/land.sh`.
Both are box-bound. A single LLM replaces that gate like this:
1. Branch `agent/<item>` from the current `rust-compiler` tip. Work in `compiler-rust/`. Commit as tariqrafique.
2. Run preflight (exit 0), the lints, and the portable board (exit 0, no row lower than the last official board).
3. `git fetch origin && git merge-base --is-ancestor origin/rust-compiler HEAD` must succeed. If it fails, merge `origin/rust-compiler` in (no rebase) and re-run 2.
4. Fast-forward only: `git push origin agent/<item>` then `git push origin HEAD:rust-compiler` (no `-f`). Delete `agent/<item>` after it is on rust-compiler.
5. Update `handoff/notes/lwc-rust-migrate/PLAN.md` (§2 board, §3 item DONE with sha) in the same or a following commit, citing the
   portable board you ran. Say clearly that it is a portable board, not an official one. Whether a portable board may stand in for
   the official gate is Tariq's call; D-0280/D-0281 currently treat portable runs as advisory.

## (f) Where things live (branch `rust-compiler`)
- `handoff/CONTINUE.md` is this file. `handoff/HANDOFF.md` is a short index with the sha of each bundle.
- `handoff/notes/lwc-rust-migrate/PLAN.md` is the plan of record: §1 direction, §2 board, §3 queue, §4 exit criteria, §5 protocol.
- `handoff/notes/lwc-rust-migrate/decisions.tsv` holds every decision, D-0001…D-0293. `CUTOVER.md` is the cutover checklist; its status column dates from 2026-11-01, so re-check it against the board.
  `PROGRAM.md`, `ship-packaging-plan.md` and `ship-survey.md` are background.
- `handoff/notes/state/*.md` is each bot's working log. `harness.md` (the Worker) has every land with shas and durations.
- `handoff/notes/arch-board/` holds the architecture, `COVERAGE.tsv` and `reviews.tsv`. `handoff/notes/OWNERS.tsv` holds ownership.
- `handoff/official-board/65006817bd11/` has the newest official board. `compiler-rust/board.txt` + `OFFICIAL_BOARD.txt` hold the board pushed with that tree.
- `handoff/lwc-compiler-rust.bundle` is the box main history at 3e5765a9c046. `handoff/notes.bundle` is the notes repo history.
- Flat files directly under `handoff/notes/` (e.g. `notes/PLAN.md`, `notes/harness.md`) are legacy copies of the same files.

## (g) Open risks
- **Consumer ports are not on GitHub.** `ship/consumers` (`2dcd877`, `e5c7433`) lives only in a box-local clone (`/workspace/wt/lwc-consumers`,
  "never pushed" per D-0202). Without the box, it must be redone or Tariq must allow exporting it.
- **No official board off the box.** `official-board.sh` is box-pinned, so off-box gating relies on the portable board (advisory under D-0280).
- **The gate scripts assume `/workspace/notes`.** If the notes aren't there, the decisions snapshot is used. The case-set checks in preflight need the notes git repo.
- **macOS/Windows/linux-arm64 napi binaries have never been loaded** on real hardware. The darwin size gap (zig linker) is open (`docs/napi-cross-build.md` §5).
- `CUTOVER.md` status and PLAN §4 checkboxes are stale against this board. Re-derive them before claiming cutover.
- After a land, the box tree and the `rust-compiler` tree were kept in sync by transplanting the `compiler-rust/` diff. Once you work only on GitHub,
  `rust-compiler` becomes the source of truth, and `lwc-compiler-rust.bundle` stops moving.
- The pretty-equal and corpus rows need network access to fetch the pinned sample corpora (`scripts/fetch-sample-corpus.sh`, lock `harness/external-inputs.lock`).
