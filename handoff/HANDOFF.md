# LWC → Rust migration: handoff (refreshed 2026-11-10 20:40 PST)

Work is on main or committed branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: Rust workspace history, main @ 3d3d152 (all refs, `--all`).
- `notes.bundle`: notes repo HEAD @ 86b79e1.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- Main @ 3d3d152 equals the official board edcfde44d1b8 commit (3d3d1525d6a9656e95b2c6608d0e508eeb0a1b77), so it is measured.
- `compiler-rust/` = official board edcfde44d1b8 @ 3d3d1525 (see compiler-rust/OFFICIAL_BOARD.txt).
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
