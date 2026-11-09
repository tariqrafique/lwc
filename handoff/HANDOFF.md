# LWC → Rust migration: handoff (refreshed 2026-11-09 13:25 PST)

Fleet resumed 2026-11-08 22:40 PST. Work is on main or on committed bot/* branches, and all of it is in the bundle below.

## What's here
- `lwc-compiler-rust.bundle`: full git history of the Rust workspace, with main @ f679437 plus all current refs (bot/harness-d0276 4f0c530, bot/harness-d0276-step2 3322bb2). Restore it with `git clone lwc-compiler-rust.bundle lwc-compiler-rust && cd lwc-compiler-rust && git branch -r`.
- `notes.bundle`: notes repo main @ 4936e7f (PLAN.md, decisions.tsv, state/*.md, arch-board reviews, bin/ helpers).
- `notes/`: plain copies of the notes (OWNERS.tsv, arch-board/, lwc-rust-migrate/, state/), for reading on GitHub.
- `semdiff-uncommitted.patch`: older Semdiff WIP from 2026-11-02 (not refreshed). Check whether bot/semdiff already supersedes it before applying.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). This branch's base is upstream-b9e8007.
- `compiler-rust/` on this branch = the latest **official** board ae266f8c1456 @ 82ecba7af88f (published 2026-11-09 03:32 PST). See compiler-rust/board.txt.
- Main @ f679437 is **UNMEASURED**: no official board matches it. Only compiler-rust/ (board ae266f8c1456) is measured.
- Plan of record: notes/lwc-rust-migrate/PLAN.md. Decisions: notes/lwc-rust-migrate/decisions.tsv (D-0001..D-0276).

## Branches
D-0276 (2026-11-09 12:46 PST) consolidated the fleet to one serial worker (LWC Harness, which becomes LWC Worker); Arch Board reviews at milestones only. The per-bot branches were pruned locally. Only bot/harness-d0276 (4f0c530) and bot/harness-d0276-step2 (3322bb2), both unlanded and unmeasured, remain besides main.
Unlanded tips that were not in main (bot/diagnostics-directives 3bc4435, bot/hygiene a28dd50) are no longer in this bundle; they remain in the earlier handoff bundle in this branch's git history. Read notes/state/*.md for the final per-area handoffs.

## How to resume
1. Restore both bundles under /workspace (lwc-compiler-rust, notes). Check out the oracle at b9e8007.
2. Read PLAN.md §3 (ownership) and §5 (rules), then each state/<bot>.md.
3. Follow the landing order in the state files, then publish a fresh official board from main.
