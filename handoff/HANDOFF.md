# Handoff

Updated 2026-11-11 04:08 PST. main 3d2e472f3394, official board fd035de9d515 (VALID), notes 8ebcbcd. Plan of record: notes/PLAN.md.

# LWC → Rust migration: handoff (refreshed 2026-11-10 23:58 PST)

Work is on main or on committed `agent/*` branches. Both bundles below are current as of this refresh.

## What's here
- `lwc-compiler-rust.bundle`: full git history of the Rust workspace, with main @ 9183a4c08c5552032dc72aef8b3490b7596a9cf4. Restore it with `git clone -b main lwc-compiler-rust.bundle lwc-compiler-rust`.
- `notes.bundle`: notes repo main @ ffd254a87f348dd2e38d396958d65fd6c33493ac (PLAN.md rewritten from official board 66479d02aafa). Restore with `git clone -b main notes.bundle notes`.
- `notes/`: plain copies of selected notes for reading on GitHub. `notes/lwc-rust-migrate/PLAN.md`, `notes/lwc-rust-migrate/decisions.tsv`, and `notes/state/*.md` are current as of this refresh. Other files under `notes/` may be older; prefer the notes bundle for history.
- `semdiff-uncommitted.patch`: older Semdiff WIP from 2026-11-02 (not refreshed). Check whether it is already superseded before applying.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean).
- Newest official board: **66479d02aafa** VALID, commit `b2bb4be10388c530a5b77069de8116c024be0b04`, notes at board time `06afd5e9c148`, published 2026-11-10 23:53 PST. See `compiler-rust/board.txt` once that board is copied onto the rust-compiler branch, or the official store. New rows vs 37036c11d866: `lbc` 523/523 (discovered=522 included=522 excluded=0) and `coverage-gate` report-only (192 cells, 26 covered, 11 diagnostic-only, 77 gap, 76 na, 2 dec, 0 invalid).
- Main @ 9183a4c is **UNMEASURED**: it is two commits past the official board (`1887e9f6c` ban global vitest error mutes, then `9183a4c08` decisions snapshot to notes@ffd254a). The bundle tip is 9183a4c; `b2bb4be` is an ancestor of that tip and is in the bundle.
- Plan of record: notes/lwc-rust-migrate/PLAN.md (notes @ ffd254a). Item 10 (LBC whole-component row) is DONE. Next port item is 10b (`agent/coverage-green`): coverage gate green and gating. Decisions: notes/lwc-rust-migrate/decisions.tsv.

## How to resume
1. Restore both bundles under /workspace (lwc-compiler-rust, notes). Check out the oracle at b9e8007.
2. Read PLAN.md §2 (current official board), §3 (ownership and queue), and §5 (rules), then `state/harness.md`.
3. Cloud agents port on `agent/<item>`. LWC Worker lands with `scripts/land-agent-branch.sh` and publishes the official board. No PRs, no GitHub Actions, no force-push.

## 2026-11-10 23:58 PST refresh
Bundled main 9183a4c and notes ffd254a. Official board 66479d02aafa remains the newest VALID board (commit b2bb4be).
