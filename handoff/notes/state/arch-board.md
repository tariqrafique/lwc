# state/arch-board.md
Updated: 2026-11-01 22:42 PST by LWC Arch Board. Read PLAN.md first; this file holds only this bot's working state.

## Current item
- Review queue (on the critical path): turn requests around the same wake. AB-D0001 (SSR oxc design) APPROVED with 5 conditions; enforce them on S0-S5 slice reviews. Expected next, Harness AB-0003 golden-gate, Diagnostics AB-X0005, JS AB-R003+AB-0001+AB-0010.
- Drift checks: on every landing review, compare the bot's state/<bot>.md (owed ids, branch, last landed) with reviews.tsv + main; REWORK on mismatch. Every ~3 reviews, report PLAN.md drift to Migrate.

## Owed REWORKs (ids)
- None owed by arch-board. Open REWORK rows (owned by others): AB-0001..0003, 0005..0015, AB-R001..R003, AB-X0005.

## Branch/worktree + last landed main commit
- No worktree (reviewer; writes notes only). Notes repo /workspace/notes.
- Last landing reviewed: 4f68532 (AB-L0016, template codegen; pid 44a83ab7 matched).

## Last trusted board
- Official src=dfa221a8867a commit=7dcfe44 published 2026-11-01 21:48 PST. PROVISIONAL until AB-0003/AB-0007 land.

## Standing rules
- See arch-board/ARCHITECTURE.md "Board standing rules": reject SSR JS parity on the line-scanner; no diagnostics error-info edits until AB-0001; trivial = docs/* only.
- Verdict vocabulary: APPROVE | REWORK | REJECT; CLOSED = a non-landing finding resolved by a cited landing/commit (never satisfies land.sh).

## Dead ends
- none.

## Open questions for Migrate
- none.

## Since 22:00
- AB-L0017 APPROVE (ssr c6a7792, nit AB-X0006); AB-0003 APPROVE (harness e8ab0c6, nit AB-X0007).
- Told Migrate: SSR S2-S5 blocked on AB-0001; PLAN §3 ssr/arch-board/harness rows drifted. Next expected: harness AB-0007, SSR S0.
- AB-L0018 REWORK (diag: throw model, push-only diags); AB-L0019 REWORK (style: positioned token type). Awaiting resubmits.
- Migrate (PLAN faaa5e7): JS AB-R003 slice 1 (lwc-shared error-info structure) is critical path, review promptly; Template AB-0011 needs a design to the board first.
- AB-0007 APPROVE (harness a451a65); golden-I/O structural gate owed in AB-0005. Diagnostics folding AB-X0005 into the AB-L0018 resubmit (one pid closes both).
- AB-L0020 REWORK (JS R003 slice 1: single oracle /workspace/lwc, no SKIP tests, shrink-only allowlist count). Critical path: turn the resubmit around immediately.
- COVERAGE.tsv + coverage-gate spec in progress (Migrate/Tariq ask); then: send the GAP list to Migrate, the gate spec to Harness, and review the lbc row design.
- AB-L0020 APPROVE (JS slice 1), AB-0001 CLOSED. AB-L0021 APPROVE (template 1-3, closes AB-0014). AB-L0022 REWORK (style text + js_trim into lwc-shared).
- Expect: Semantic Diff design (rules: proof + negative tests per canonicalization; no effectful reordering), Template AB-0011 design, the lbc row design, resubmits L0018/L0019/L0022. Add the semantic_diff column to COVERAGE.tsv when the executor run lands.

## 2026-11-01 22:30
- AB-L0020 re-recorded APPROVE pid 8ada1ed0 (shim-allowlist compat row). AB-D0002 APPROVE w/ conditions (+amendment c560c9a; Migrate ruled quirk not replicated). AB-0016 REWORK (official-board notes `git add -A`). AB-0005 APPROVE e98cc31 pid 2f3f18ba; AB-X0008 pipe-grep lint -> AB-0006.
- COVERAGE.tsv v2 @5203dc1: 20 covered / 11 diag-only / 85 GAP / 76 NA. Spec to Harness; GAP owners + decisions.tsv id column ask to Migrate.
- Waiting: AB-L0018/0019/0022 resubmits; AB-0006 (+X0007,X0008,0016); coverage-gate; lbc row design; Semantic Diff design; SSR S0/S1.
