# Handoff

Updated 2026-11-16 09:26 PST. main 3e5765a9c046, official board 65006817bd11 (VALID), notes fd1954d. Plan of record: notes/PLAN.md. New LLM? Start with CONTINUE.md.

# LWC → Rust migration: handoff index

## What's here (all under handoff/)
- `CONTINUE.md`: self-contained guide for continuing without the box (goal, status, queue, rules, commands, risks).
- `lwc-compiler-rust.bundle`: full git history of the Rust workspace, main @ 3e5765a9c046c743fdd01b2c71270a99cbf7f740 (the commit of official board 65006817bd11). Restore: `git clone -b main lwc-compiler-rust.bundle lwc-compiler-rust`.
- `notes.bundle`: notes repo @ fd1954d2fe647733d638997cf3108bd761dbb966 (PLAN.md rewritten from board 65006817bd11; item 18 DONE, 18b added). Restore: `git clone notes.bundle notes && git -C notes switch -c main`.
- `notes/`: plain copies for reading on GitHub, current as of this refresh: `notes/lwc-rust-migrate/` (PLAN.md, decisions.tsv, CUTOVER.md and the other plan docs), `notes/state/*.md`, `notes/arch-board/`, `notes/OWNERS.tsv`. Flat files directly under `notes/` are legacy copies of the same PLAN, decisions and state files, refreshed too.
- `official-board/65006817bd11/`: OFFICIAL_BOARD.txt + board.txt of the newest official board.
- `semdiff-uncommitted.patch`: old Semdiff WIP from 2026-11-02, not refreshed. Probably superseded; check before applying.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean).
- Newest official board: **65006817bd11** VALID, commit 3e5765a9c046, notes_commit ed6a5ab, published 2026-11-16 09:17:56 PST. Item 18 F5 landed: pretty-equal 1555/1560, allowlist 5 (was 1418, allowlist 142).
- `compiler-rust/` on this branch is box main at the board commit plus the board files.
- Next item: 18b (pretty-equal residuals to 1560/1560, empty allowlist), then 19, 20, then the item 15 remainder. See notes/lwc-rust-migrate/PLAN.md §3.

## How to resume
Read CONTINUE.md. Then PLAN.md §2–§5, then notes/state/harness.md.
Rules in brief: no PRs, no GitHub Actions, no force-push, never touch master, commit as tariqrafique <tariqrafique@users.noreply.github.com>.
