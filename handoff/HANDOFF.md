# LWC → Rust migration: handoff (refreshed 2026-11-10 11:57 PST)

Work is on main or committed branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ 7a64c20 (all refs).
- `notes.bundle`: notes repo HEAD @ 9f20e20.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- Main @ 7a64c20 is UNMEASURED: it differs from the last official board's commit bb64d8d.
- `compiler-rust/` = official board 0d4392d66b82 @ bb64d8d (see compiler-rust/OFFICIAL_BOARD.txt).
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
