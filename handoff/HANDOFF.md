# LWC → Rust migration: handoff (refreshed 2026-11-10 06:30 PST)

Work is on main or committed bot/* branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ c9e03e1 plus bot/* refs (bot/harness-s5 a82b713;).
- `notes.bundle`: notes repo main @ 16e9772.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- `compiler-rust/` = official board 6e5d1ca4f011 @ main c9e03e1 (2026-11-10 05:56 PST). See compiler-rust/OFFICIAL_BOARD.txt.
- Main @ c9e03e1 matches that board. Every bot/* tip is UNMEASURED until an official board matches it.
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
