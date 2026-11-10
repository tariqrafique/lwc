# LWC → Rust migration: handoff (refreshed 2026-11-10 04:00 PST)

Work is on main or committed bot/* branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ 1017bce plus bot/* refs (bot/harness-d0280b-table cde1e63;).
- `notes.bundle`: notes repo main @ d67669b.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- `compiler-rust/` = official board 2ead84925858 @ main 1017bce (published 2026-11-10 03:51 PST). See compiler-rust/OFFICIAL_BOARD.txt.
- Main @ 1017bce is the board's commit; any later main or bot/* tip is UNMEASURED until an official board matches it.
- Plan of record: notes/lwc-rust-migrate/PLAN.md.
- No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle (`git clone lwc-compiler-rust.bundle lwc-compiler-rust`), check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
