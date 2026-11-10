# LWC → Rust migration: handoff (refreshed 2026-11-10 12:59 PST)

Work is on main or committed branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ a975bce (all refs, including bot/* branches).
- `notes.bundle`: notes repo HEAD @ 428ef07.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- Main @ a975bce is UNMEASURED unless it equals the official board commit cf41f69.
- `compiler-rust/` = official board c5cdfca7156d @ cf41f69 (see compiler-rust/OFFICIAL_BOARD.txt).
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
