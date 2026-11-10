# LWC → Rust migration: handoff (refreshed 2026-11-10 08:57 PST)

Work is on main or committed bot/* branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ 7c7ae1b plus bot/* branches (bot/snap-d0282 @ 1b562b0).
- `notes.bundle`: notes repo main @ 168a603.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- **Main @ 7c7ae1b is UNMEASURED.** No official board exists for it.
- The only measured tree is `compiler-rust/` = official board 811ad3f956d2 @ main 8e338f9 (see compiler-rust/OFFICIAL_BOARD.txt). Not touched by this refresh.
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
