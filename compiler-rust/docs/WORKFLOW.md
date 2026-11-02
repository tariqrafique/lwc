# Working on lwc-compiler-rust (git, one worktree per bot)

- `/workspace/lwc-compiler-rust` is **main**. Never edit it. It moves only via `scripts/land.sh` (fast-forward).
- Your checkout: `/workspace/wt/<bot>` on branch `bot/<bot>` (bots: template, diagnostics, ssr, js, postcss, harness).
  Own `target/` inside it; dependency builds are shared through sccache (`/workspace/wt/.cargo/config.toml`).
  `/workspace/wt/parse5-rust` and `/workspace/wt/notes` are symlinks so the `../parse5-rust` path dep resolves.
- Loop: edit in your worktree -> `cargo check` / tests / `scripts/install-napi.sh` (installs into YOUR worktree's
  `harness/lwc-compiler-napi.node`) -> `harness/board.sh <rows>` from your worktree -> `git commit` on `bot/<bot>` ->
  `scripts/land.sh`.
- `scripts/land.sh` (global flock): rebase onto main -> `cargo check --workspace --all-targets` -> `cargo test` for every
  crate you changed -> shim gate -> golden-gate if `harness/{goldens,test,src,fixtures}` changed -> fast-forward main ->
  rebuild + install the live napi in main's harness if crates changed. Any failure: nothing lands.
  Rebase conflicts come back to you: `git rebase main`, resolve, commit, re-run.
- Direct commits on `main` are refused by the pre-commit hook. `decisions.tsv` merges as a union (append-only rows).
- To pick up others' landed work: `git rebase main` in your worktree.
- Official boards: `scripts/official-board.sh` builds only from a commit on main (`BOARD_COMMIT=<ref>`, default main)
  via `git archive`, and records the commit in OFFICIAL_BOARD.txt. Never from a working tree.
- Notes (`/workspace/notes`) are a separate git repo, edited in place; each official board commits a notes snapshot.
