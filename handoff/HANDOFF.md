# LWC → Rust migration: handoff (refreshed 2026-11-10 06:56 PST)

Work is on main or committed bot/* branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ 5e2230f (no bot/* branches).
- `notes.bundle`: notes repo main @ 5a38154.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- `compiler-rust/` = official board 8a00cf3f860c @ main 5e2230f (2026-11-10 06:37 PST). See compiler-rust/OFFICIAL_BOARD.txt.
- Main @ 5e2230f matches that board.
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
