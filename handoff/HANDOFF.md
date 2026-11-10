# LWC → Rust migration: handoff (refreshed 2026-11-10 05:30 PST)

Work is on main or committed bot/* branches; all of it is in the bundles.

## What's here
- `lwc-compiler-rust.bundle`: full history of the Rust workspace, main @ 18c3c66 plus bot/* refs (bot/harness-d0280b3 056cf3b;bot/harness-s5 604f898;).
- `notes.bundle`: notes repo main @ 42ebac8.
- `notes/`: plain copies of OWNERS.tsv, arch-board/, lwc-rust-migrate/ (PLAN.md, decisions.tsv), state/.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). Branch base is upstream-b9e8007.
- `compiler-rust/` = official board 1d4f57d335fa @ main 8499ebd (2026-11-10 04:36 PST). See compiler-rust/OFFICIAL_BOARD.txt.
- Main @ 18c3c66 and every bot/* tip are UNMEASURED: no official board matches them.
- Plan of record: notes/lwc-rust-migrate/PLAN.md. No GitHub Actions, no PRs (D-0279/D-0280).

## Restore
Clone each bundle, check out the oracle at b9e8007, read PLAN.md §3 and state/*.md.
