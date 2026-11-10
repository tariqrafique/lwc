# LWC → Rust migration: handoff (refreshed 2026-11-10 15:00 PST)

Work is on main or committed branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ a9ee6d9 (all refs, including bot/* branches).
- `notes.bundle`: notes repo HEAD @ 2ead970.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- Main @ a9ee6d9 is UNMEASURED unless it equals the official board commit a9ee6d9.
- `compiler-rust/` = official board 9faa91fecea7 @ a9ee6d9 (see compiler-rust/OFFICIAL_BOARD.txt).
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
