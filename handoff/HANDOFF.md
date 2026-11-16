# Handoff

Updated 2026-11-16 11:12 PST. main bf0f176608d6, official board d0c1ca60cb18 (VALID), notes e25fb74. Plan of record: notes/PLAN.md. New LLM? Start with CONTINUE.md.

# LWC → Rust migration: handoff index

## What's here (all under handoff/)
- `CONTINUE.md`: self-contained guide for continuing without the box (goal, status, queue, rules, commands, risks).
- `lwc-compiler-rust.bundle`: full git history of the Rust workspace, main @ bf0f176608d6a629f3f533e87836dbbb22917fd0 (the commit of official board d0c1ca60cb18). Restore: `git clone -b main lwc-compiler-rust.bundle lwc-compiler-rust`.
- `notes.bundle`: notes repo @ e25fb74ebd664a601732961358d34297cf3a5375 (PLAN.md from board d0c1ca60cb18; 18b DONE, item 19 dispatched; decisions through D-0295). Restore: `git clone notes.bundle notes && git -C notes switch -c main`.
- `notes/`: plain copies for reading on GitHub, current as of this refresh: `notes/lwc-rust-migrate/` (PLAN.md, decisions.tsv, CUTOVER.md and the other plan docs), `notes/state/*.md`, `notes/arch-board/`, `notes/OWNERS.tsv`. Flat files directly under `notes/` are legacy copies of the same PLAN, decisions and state files, refreshed too.
- `consumers.bundle`: branch `agent/consumers` (D-0295), the consumer port on upstream b9e8007 (thin bundle; needs b9e8007, branch `upstream-b9e8007`). Restore inside an lwc clone: `git fetch consumers.bundle agent/consumers:agent/consumers`.
- `official-board/d0c1ca60cb18/`: OFFICIAL_BOARD.txt + board.txt of the newest official board (`official-board/65006817bd11/` is the previous one).
- `semdiff-uncommitted.patch`: old Semdiff WIP from 2026-11-02, not refreshed. Probably superseded; check before applying.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean).
- Newest official board: **d0c1ca60cb18** VALID, commit bf0f176608d6, notes_commit f596765, published 2026-11-16 11:03:25 PST. Item 18b landed: pretty-equal 1560/1560, allowlist 0 (was 1555/1560, allowlist 5). The D-0292 precondition for the JS package deletes is met.
- `compiler-rust/` on this branch is box main at the board commit plus the board files.
- Next item: 19 (F8 semdiff from source; in flight on `agent/semdiff-source`, cloud, dispatched 11:11 PST), then 20, then the item 15 remainder (JS deletes now unblocked). See notes/lwc-rust-migrate/PLAN.md §3.

- Off-box land gate (D-0294): fresh-clone portable board exit 0 + preflight exit 0 with land-path lints; procedure in CONTINUE.md (e).

## How to resume
Read CONTINUE.md. Then PLAN.md §2–§5, then notes/state/harness.md.
Rules in brief: no PRs, no GitHub Actions, no force-push, never touch master, commit as tariqrafique <tariqrafique@users.noreply.github.com>.
