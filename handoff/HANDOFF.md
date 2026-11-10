# LWC → Rust migration: handoff (refreshed 2026-11-10 12:26 PST)

Work is on main or committed branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ 2610976 (all refs, including bot/* branches).
- `notes.bundle`: notes repo HEAD @ 8ebe134.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- Main @ 2610976 is UNMEASURED: it differs from the last official board's commit 7a64c20.
- `compiler-rust/` = official board 4c3ed0a43fd3 @ 7a64c20 (see compiler-rust/OFFICIAL_BOARD.txt).
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
