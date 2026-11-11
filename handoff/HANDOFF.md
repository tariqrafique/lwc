# LWC → Rust migration: handoff (refreshed 2026-11-10 20:05 PST)

Work is on main or committed branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: Rust workspace history, main @ f3e244c (no bot/* branches exist locally; agent/style-gaps is a remote-only branch, not bundled).
- `notes.bundle`: notes repo HEAD @ 560a601.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- Main @ f3e244c is UNMEASURED unless it equals the official board commit bcabc2e.
- `compiler-rust/` = official board 5aafe4b7f269 @ bcabc2e (see compiler-rust/OFFICIAL_BOARD.txt).
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
