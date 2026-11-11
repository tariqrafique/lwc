# LWC → Rust migration: handoff (refreshed 2026-11-10 21:00 PST)

Work is on main or committed branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: Rust workspace history, main @ ee92647 (main only; no bot/* branches exist).
- `notes.bundle`: notes repo HEAD @ 2f273b5.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- Main @ ee92647 equals the official board 37036c11d866 commit (ee92647a2767e0936c1ef2d6a01896125d2ba6e1), so it is measured.
- `compiler-rust/` = official board 37036c11d866 @ ee92647a (see compiler-rust/OFFICIAL_BOARD.txt).
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
