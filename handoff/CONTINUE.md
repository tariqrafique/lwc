# CONTINUE: LWC compiler → Rust migration (start here)

Written 2026-11-16 09:30 PST, updated 09:40 PST (D-0294, D-0295) for an LLM or engineer who has **only a clone of
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
   - Consumers on the Rust API (CUTOVER.md §D, D1–D8). The started port is branch **`agent/consumers`** (D-0295), also
     in `handoff/consumers.bundle`. It is based on upstream `b9e8007`, not on `rust-compiler`, with two commits on top: `879dd7c`
     (bundler consumers on the napi transform API) and `fc970ae` (D-0291 rename to `@lwc/rust-compiler`). It adds
     `packages/@lwc/rust-compiler` (private, v0.1.0, re-exports the napi loader; `transformSync` is the same function as `transform`
     until S3), moves `@lwc/rollup-plugin` (src/index.ts) and the ssr-runtime CSS test (`styles.spec.ts`) onto it, and drops the
     non-upstream `dynamicImports.strict` key from two integration serve configs. The JS compiler packages are **not** deleted.
     The box run (Worker-reported, not on a board) had `yarn install --frozen-lockfile` and `yarn build` passing.
     Still left: `packages/@lwc/rust-compiler/index.js` imports the loader through a **box-relative path**
     (`../../../../../lwc-compiler-rust/crates/lwc-compiler-napi/index.js`), so it only resolves when this repo sits next to
     a `lwc-compiler-rust` checkout. It has to resolve the packed napi package instead. Also left: rollup-plugin still lists `@lwc/compiler` as a
     dependency; CUTOVER D2/D3/D6–D8 (ESM-only output, engine-server fixtures, the other consumers, the `lwc` barrel, docs); and the rollup-plugin spec suite.
     *Done when:* the consumers compile and test against the Rust `transform`/`transformSync` API with no JS compiler package imported.
     Never merge it into `master`; no PR (D-0295).
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
- **Land gate (D-0294):** while the box fleet runs, the box official board is the gate. Off the box, a fresh-clone
  `scripts/run-portable-board.sh` exit 0 plus `scripts/preflight.sh` exit 0 (with the land-path lints) IS the gate to fast-forward `rust-compiler`.
- Commit identity **`tariqrafique <tariqrafique@users.noreply.github.com>`** as both author and committer. Never a Salesforce
  corporate email address, and never a `Co-authored-by:` trailer. Check with `git log --format='%an <%ae> | %cn <%ce>%n%b'` before pushing.
- Oracle = salesforce/lwc 9.5.3 @ b9e8007 (`compiler-rust/harness/oracle.pin`). Never modify it; the board refuses a dirty oracle (D-0010).
- Never contradict a decisions.tsv row. Rows are append-only, and ids are never reused. Add a row only for a real compiler or cutover choice,
  and only with `notes/bin/d-id migrate <phase> <decision> <why> <evidence> <result>` (never hand-edit).
- Do not publish `@lwc/rust-compiler` to npm until Tariq says so (D-0202, D-0291).
- Pushes to `tariqrafique/parse5-rust` (the Rust HTML parser, pinned by Cargo.lock rev `370629a`) need Tariq's explicit OK each time.
- Rust toolchain is pinned in `compiler-rust/rust-toolchain.toml` (1.99.1). `cargo fmt --all --check` is a hard gate (D-0235). The clippy
  ratchet (`scripts/clippy-baseline.txt`, now **133**) may only go down.

## (e) Working without the box fleet (official procedure, D-0294)
**Two layouts.** On `rust-compiler`, `compiler-rust/` holds the Rust workspace tree plus 8 board artifacts at its top
(`OFFICIAL_BOARD.txt`, `board.txt`, `csr-ssr.tsv`, `csr-template.tsv`, `template-diag{,-napi}{.json,.soundness.json}`).
`handoff/lwc-compiler-rust.bundle` holds the same workspace with its full git history in **root layout** (main @ 3e5765a9c046).
On 2026-11-16 the box confirmed `git diff --stat <bundle main> rust-compiler:compiler-rust` shows exactly those 8 board files and nothing else.
**Run the gates in the root-layout repo, not in `compiler-rust/`.** The gate scripts read blobs as `git show <base>:scripts/...`
(root-relative). Run from the `compiler-rust/` subdirectory of an lwc clone, `preflight.sh` fails `land-main-refs` and
`baseline-ceiling` ("path 'compiler-rust/scripts/land-main-refs.tsv' exists, but not 'scripts/land-main-refs.tsv'"). `shim-gate` also fails there,
because `compiler-rust/board.txt` contains a shim tag. In a bundle clone, `scripts/preflight.sh --base main --tree .` exited 0 on 2026-11-16 (all rows ok/skip).

**Setup (Linux; the gates hard-code `/workspace/notes`).**
```
git clone -b rust-compiler https://github.com/tariqrafique/lwc lwc && cd lwc     # the GitHub branch (LWC)
sudo mkdir -p /workspace && sudo chown "$USER" /workspace
git clone handoff/notes.bundle /workspace/notes && git -C /workspace/notes switch -c main
git clone -b main handoff/lwc-compiler-rust.bundle /workspace/lwc-compiler-rust # root-layout work repo (W)
git fetch /workspace/lwc-compiler-rust main && git diff --stat FETCH_HEAD HEAD:compiler-rust   # must list only the 8 board files
```
`scripts/preflight.sh` reads decisions from `/workspace/notes` (`DEC_REPO`). `harness/scripts/oracle-root.mjs` prefers
`/workspace/notes/lwc-rust-migrate/decisions.tsv` and falls back to `harness/decisions.snapshot.tsv`.
Notes commits: `cd /workspace/notes && bin/notes-commit migrate -m '<msg>' <file>...`. New D-ids: `bin/d-id migrate ...`.
The box's notes commit-msg hook is not in the bundle, so follow its rules by hand.

**Build (in W).** Needs rustup (picks up 1.99.1), node, yarn, git.
```
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
scripts/install-napi.sh            # release napi -> harness/lwc-compiler-napi.node
```

**Portable board (in W).**
```
export LWC_ORACLE=$PWD/.oracle/lwc STYLE_CORPUS=$PWD/.oracle/style-corpus
scripts/fetch-oracle.sh --build    # clones salesforce/lwc @ harness/oracle.pin and runs yarn build; never /workspace/lwc
scripts/fetch-style-corpus.sh
PORTABLE_BOARD_TXT=/tmp/portable-board.txt scripts/run-portable-board.sh   # also fetches the pinned sample corpora + Prettier 3.9.9
```
Exit codes: 0 = every gating row passed, 4 = the snapshot mutated (invalid), 2 = an input fetch or setup failed. No row may be lower than
`handoff/official-board/65006817bd11/board.txt` or the newest portable board under `handoff/portable-board/`.
The ratchets are `harness/ratchets` and `harness/board-baseline.tsv`.

**Preflight + land-path lints (in W).**
```
scripts/preflight.sh --base main --tree .     # prints a table; exit 0 required. base = W's main = the current rust-compiler tree
```
Preflight runs the **base** commit's gate code (`git archive <base>`), so a branch cannot weaken its own gate. It already runs
land-main-refs, lint-shell-pipes, workspace-path-scan, lint-no-stripped-compare, lint-no-error-mute, ratchet-ceiling, baseline-ceiling,
shim-gate, cargo-fmt, option-gate, golden-io-guard and golden-gate (the last three only when the change set triggers them). To run one by hand:
```
scripts/lint-no-stripped-compare.sh .
scripts/workspace-path-scan.sh .
scripts/lint-no-error-mute.sh .
scripts/cargo-fmt-check.sh .
# check-land-main-refs exactly as preflight.sh (lines 79-86) calls it: base's table vs the tree's table
git show main:scripts/land-main-refs.tsv > /tmp/main-refs.tsv
scripts/check-land-main-refs.sh --check "$PWD/scripts/land.sh" "$PWD" --main-table /tmp/main-refs.tsv --incoming-table "$PWD/scripts/land-main-refs.tsv"
harness/scripts/golden-gate.sh harness        # slow: re-records every row on the JS oracle and diffs goldens/
scripts/clippy-ratchet.sh .                   # rustc must equal rust-toolchain.toml; count above baseline fails; count below fails until you lower scripts/clippy-baseline.txt in the same commit
```
On a 3e5765a bundle clone, the four lints, `cargo-fmt-check` and that `check-land-main-refs` call all exited 0, and so did `preflight.sh --base main --tree .`.
Not re-run for this note: the portable board, golden-gate and clippy.
Gate-code changes must also pass `scripts/run-land-self-tests.sh --all` (PLAN §3, 2026-11-10 23:20 ruling).

**Land procedure (replaces the LWC Worker + official board while off the box).**
1. In W: `git switch -c agent/<item> main`, fix the root cause, and commit as tariqrafique (author and committer).
2. In W: preflight exit 0 and the portable board exit 0 with no row lower than the last board. That's the gate (D-0294). If a gate fails, fix it; never loosen it.
3. In W: `git switch main && git merge --ff-only agent/<item>`, then refresh the bundle:
   `git bundle create <lwc>/handoff/lwc-compiler-rust.bundle main`.
4. In the lwc clone: `git switch -c agent/<item> origin/rust-compiler`, then copy W's tree into `compiler-rust/` and keep the board artifacts:
   ```
   git fetch /workspace/lwc-compiler-rust main
   git rm -rq compiler-rust && git read-tree --prefix=compiler-rust/ -u FETCH_HEAD
   git checkout HEAD -- compiler-rust/OFFICIAL_BOARD.txt compiler-rust/board.txt compiler-rust/csr-ssr.tsv compiler-rust/csr-template.tsv \
     compiler-rust/template-diag.json compiler-rust/template-diag-napi.json compiler-rust/template-diag.soundness.json compiler-rust/template-diag-napi.soundness.json
   mkdir -p handoff/portable-board/<W main sha12> && cp /tmp/portable-board.txt handoff/portable-board/<W main sha12>/board.txt
   ```
   (Checked on the box: with an unchanged W, this sequence leaves `git status` empty.) The 8 artifacts stay as the last *official* board; off-box boards go under `handoff/portable-board/`.
5. Update the notes: `/workspace/notes/lwc-rust-migrate/PLAN.md` §2 (the portable board, its W sha and preflight rc) and §3 (item DONE with sha).
   Commit with `bin/notes-commit`, rebuild `handoff/notes.bundle` (`git -C /workspace/notes bundle create <lwc>/handoff/notes.bundle HEAD`), and copy PLAN.md/decisions.tsv/state into `handoff/notes/`.
6. `git add` the explicit paths (`compiler-rust handoff/...`; never `-A` or `.`). Commit as tariqrafique with no Co-authored-by, and check with
   `git log -1 --format='%an <%ae> | %cn <%ce>%n%b'`.
7. Fast-forward only: `git fetch origin && git merge-base --is-ancestor origin/rust-compiler HEAD` must pass (if not, merge, never rebase, and redo 2–6).
   Then `git push origin HEAD:rust-compiler` (no `-f`). Delete a remote `agent/<item>` only after its work is on rust-compiler.
While the box fleet runs, the official procedure is still the box one: `scripts/land-agent-branch.sh` → `scripts/land.sh` → `scripts/official-board.sh`, all box-pinned.

## (f) Where things live
On branch `rust-compiler`:
- `handoff/CONTINUE.md` is this file. `handoff/HANDOFF.md` is the short index with the sha of each bundle.
- `handoff/notes/lwc-rust-migrate/PLAN.md` is the plan of record (§1 direction, §2 board, §3 queue, §4 exit criteria, §5 protocol).
- `handoff/notes/lwc-rust-migrate/decisions.tsv` holds every decision, D-0001…D-0295. `CUTOVER.md` is the cutover checklist; its status column dates from 2026-11-01, so re-check it.
  `PROGRAM.md`, `ship-packaging-plan.md` and `ship-survey.md` are background.
- `handoff/notes/state/*.md` is each bot's working log. `harness.md` (the Worker) has every land with shas and durations.
- `handoff/notes/arch-board/` holds the architecture, `COVERAGE.tsv` and `reviews.tsv`. `handoff/notes/OWNERS.tsv` holds ownership.
- `handoff/official-board/65006817bd11/` has the newest official board. `compiler-rust/{board.txt,OFFICIAL_BOARD.txt,...}` are that board's artifacts.
- Bundles: `handoff/lwc-compiler-rust.bundle` is the root-layout workspace history (main 3e5765a9c046), `handoff/notes.bundle` is the notes repo,
  and `handoff/consumers.bundle` is `agent/consumers` (a thin bundle; it needs `b9e8007`, which is branch `upstream-b9e8007` on GitHub).
- Flat files directly under `handoff/notes/` (e.g. `notes/PLAN.md`, `notes/harness.md`) are legacy copies of the same files.

Other branches on tariqrafique/lwc: `agent/consumers` is the consumer port (D-0295, based on `b9e8007`), and `upstream-b9e8007` is the oracle commit.
Check `git ls-remote origin 'agent/*'` for in-flight items such as `agent/pretty-residuals`.

## (g) Open risks
- **The consumer port imports napi through a box-relative path** (see the item 15 bullet), so it won't resolve in a plain clone until that's fixed.
- **The gates must run in the root layout.** `preflight.sh` breaks from the `compiler-rust/` subdirectory (see (e)). Keep W and
  `handoff/lwc-compiler-rust.bundle` in step with `rust-compiler` on every land, or the two diverge.
- **The gate scripts assume `/workspace/notes`.** Without it, the decisions snapshot is used, but preflight's case-set checks need the notes git repo.
- **The macOS, Windows and linux-arm64 napi binaries have never been loaded** on real hardware. The darwin size gap (zig linker) is still open (`docs/napi-cross-build.md` §5).
- `CUTOVER.md` status and the PLAN §4 checkboxes are stale against this board. Re-derive them before claiming cutover.
- The pretty-equal and corpus rows need network access to fetch the pinned sample corpora (`scripts/fetch-sample-corpus.sh`, lock `harness/external-inputs.lock`).
