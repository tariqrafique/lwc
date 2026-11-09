# LWC → Rust migration: handoff (refreshed 2026-11-09 01:56 PST)

Fleet resumed 2026-11-08 22:40 PST; branches below are in flight and move often. Every bot's work is either on main or on a committed bot/* branch, and all of it is in the bundle below.

## What's here
- `lwc-compiler-rust.bundle`: full git history of the Rust workspace, with main @ ebfad54 plus every bot/* branch. Restore it with `git clone lwc-compiler-rust.bundle lwc-compiler-rust && cd lwc-compiler-rust && git branch -r`.
- `notes.bundle`: full notes repo @ b22a8a4 (PLAN.md, decisions.tsv, state/*.md, arch-board reviews, bin/notes-commit, bin/d-id).
- `notes/`: plain copies of the notes, for reading on GitHub.
- `semdiff-uncommitted.patch`: Semdiff's uncommitted WIP from wt/semdiff (exec.rs, lib.rs, tests/exec.rs, new spelling.rs). Apply it on bot/semdiff.

## Ground truth
- Oracle: salesforce/lwc 9.5.3 @ b9e8007 (clean). This branch's base is upstream-b9e8007.
- `compiler-rust/` on this branch = the last **official** board's source: board ac1b61fb239a @ c731fb601f40 (2026-11-01 21:58 PST). Main (ebfad54) is newer and unmeasured. Boards are provisional until AB-0006 lands.
- Plan of record: notes/lwc-rust-migrate/PLAN.md. Decisions: notes/lwc-rust-migrate/decisions.tsv (D-0001..D-0265).

## Unlanded branches (rebase onto main and re-gate before landing)
- bot/hygiene a28dd50 (Harness): OWNERS + lane enforcement. Landing order is AB-0006 → D-0219 → hygiene → D-0223 → D-0231.
- bot/harness-gate 18da4d2 (Harness): gate WIP. Resume point is notes state/harness.md.
- bot/template 56d1d2c (Template): AB-X0017 rework #2, then resubmit to Arch.
- bot/template-expr 922fc45 (Template): AB-D0004 S1, after X0017. D-0230 drops TemplateCompileResult.root.
- bot/template-clusters b22df87, bot/diagnostics-directives 3bc4435 (Template).
- bot/js 45e0859, bot/js-slice2 b80a4bd, bot/js-sourcemap fff6018 (JS Style). Slice 2 has AB-L0056 gaps: level_url for Origin::Ssr, and seam_x0013 pins LWC1121 at napi. bot/js-pre-reorder 1230144 can be deleted.
- bot/semdiff-sd1b d5c4a58 + semdiff-uncommitted.patch (Semdiff).
- bot/ship 95b85c3 (Ship): queued CI for D-0225 and D-0226. No npm publish.

## Remaining gaps on the last official board (ac1b61fb239a)
csr-render 355/390, csr-render-template 545/570, ssr-render 384/389 with 5 skips. style, js, template-options, and template-api are full.

## How to resume
1. Restore both bundles under /workspace (lwc-compiler-rust, notes). Check out the oracle at b9e8007.
2. Read PLAN.md §3 (ownership) and §5 (rules), then each state/<bot>.md.
3. Land in this order: hygiene chain → Template X0017 → S1 → JS slice2 → Semdiff SD-1B → SSR skips → Ship CI. Then publish a fresh official board from main.


## Refresh note
Main @ e75ce42 is UNMEASURED: no official board matches it; the latest official board is still ac1b61fb239a @ c731fb601f40 (compiler-rust/ stays there). Notes @ af16d79 (refreshed 2026-11-09 03:21 PST). The 'Unlanded branches' list above is stale; see state/*.md and these current bot/* tips in the bundle:
- bot/diagnostics-directives 3bc4435
- bot/harness 5739df7
- bot/harness-d0219 82ecba7
- bot/harness-obfix a051a35
- bot/hygiene a28dd50
- bot/js-d0265 2c0fc83
- bot/js-slice2 c08fbf6
- bot/js-sourcemap fff6018
- bot/semdiff c3bab62
- bot/semdiff-sd1b 9c96ee4
- bot/ship 8327f60
- bot/ssr 4e1a48a
- bot/template e9d5199
- bot/template-clusters b22df87
- bot/template-expr d46d6de
- bot/template-s2 7b8c7c5
- bot/template-s3 2a5cff8
- bot/template-s4 e75ce42
