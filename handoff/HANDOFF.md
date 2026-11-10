# LWC → Rust migration: handoff (refreshed 2026-11-10 03:21 PST)

Fleet resumed 2026-11-08 22:40 PST. Work is on main or on committed bot/* branches, and all of it is in the bundle below.

## What's here
- `lwc-compiler-rust.bundle`: full git history of the Rust workspace, with main @ f7cc643 plus all current refs (bot/harness-semdiff-row 8cf3c64). Restore it with `git clone lwc-compiler-rust.bundle lwc-compiler-rust && cd lwc-compiler-rust && git branch -r`.
- `notes.bundle`: notes repo main @ 7a4917a (PLAN.md, decisions.tsv, state/*.md, arch-board reviews, bin/ helpers).
- `notes/`: plain copies of the notes (OWNERS.tsv, arch-board/, lwc-rust-migrate/, state/), for reading on GitHub.
- `semdiff-uncommitted.patch`: older Semdiff WIP from 2026-11-02 (not refreshed). Check whether bot/semdiff already supersedes it before applying.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). This branch's base is upstream-b9e8007.
- `compiler-rust/` on this branch = the latest **official** board 78a3500dd541 @ 3ecff19 (published 2026-11-09 22:03 PST). See compiler-rust/board.txt.
- Main @ f7cc643 is **UNMEASURED**: the latest official board 78a3500dd541 was built from 3ecff19 (now an ancestor of main). Only compiler-rust/ is measured. bot/harness-semdiff-row is also unmeasured.
- Plan of record: notes/lwc-rust-migrate/PLAN.md. Decisions: notes/lwc-rust-migrate/decisions.tsv (D-0001..D-0276).

## Branches
D-0276 (2026-11-09 12:46 PST) consolidated the fleet to one serial worker (LWC Harness, which becomes LWC Worker); Arch Board reviews at milestones only. The per-bot branches were pruned locally. Only bot/harness-d0276 (dd99d0a, now equal to main) and bot/harness-d0276-step2 (3cbadda, unlanded and unmeasured) remain besides main.
Unlanded tips that were not in main (bot/diagnostics-directives 3bc4435, bot/hygiene a28dd50) are no longer in this bundle; they remain in the earlier handoff bundle in this branch's git history. Read notes/state/*.md for the final per-area handoffs.

## How to resume
1. Restore both bundles under /workspace (lwc-compiler-rust, notes). Check out the oracle at b9e8007.
2. Read PLAN.md §3 (ownership) and §5 (rules), then each state/<bot>.md.
3. Follow the landing order in the state files, then publish a fresh official board from main.

## 2026-11-09 22:30 PST note
Official board 78a3500dd541 was built from main 3ecff19 (published 2026-11-09 22:03 PST) and compiler-rust/ holds it. Main has since advanced to f7cc643 and is unmeasured.
