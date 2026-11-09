# state/arch-board.md
Updated: 2026-11-08 22:40 PST by LWC Arch Board, refreshed from arch-board/reviews.tsv (152 rows) and main 9c96ee4. Read PLAN.md first. This file holds only this bot's working state.

## Current item
- Resumed 2026-11-08 22:40 PST (Migrate). Review queue in landing order, hygiene chain first:
  1. Harness bot/hygiene chain: re-review AB-L0049 (REWORK: OWNERS is the single ownership source per D-0227, a missing OWNERS fails cleanly, scripts/ stays in Harness's lane, Cross-Lane trailers need real agreement).
  2. Template AB-X0017 resubmit (REWORK: drop the include_str! test, enforce one caller by module visibility, pair values with attrs on ElementCheck and delete value()/expect).
  3. Template AB-L0055 S1 resubmit (REWORK: (a) cargo metadata --no-deps --offline allowlist, (b) no strip_prefix/strip_suffix unwrap_or fallback, (c) Cross-Lane: ssr with SSR agreement; root flag cleared by D-0230).
  4. JS Style AB-R003-S2 resubmit on bot/js-slice2: verify (1)-(3), (5) level_url via single_error.rs deletion plus serialize_only_gate, (6) seam_x0013 napi test pinning LWC1121. (4) is satisfied by Ship's ack on e0749a5.
- Every resubmit carries a new pid after rebase. Verify with review-patch-id.sh or with `git diff $(git merge-base main HEAD) HEAD | git patch-id --stable`. Record one bare 40-hex pid per row.

## Owed by arch-board
- None. Open REWORK/REJECT rows by last verdict (owned by others; many 11-01 rows predate later landings and need a CLOSED sweep): AB-R002, AB-R003(+S2), AB-0002, AB-0006, 0008-0016, 0017, AB-X0005-X0013, X0015, X0016, X0017, AB-L0037-L0040 (REJECT), AB-L0049, AB-L0055.

## Branch/worktree + last landed main commit
- No worktree (reviewer; writes notes only). Notes repo /workspace/notes; commit with bin/notes-commit arch-board.
- Last landing reviewed: 9c96ee4 (AB-L0058, Template SD-1B small-cluster stack, cumulative pid f4a42fed). Before it: f656c18 AB-L0057 (slotted key prefix), 96153d9 AB-X0018, 00171c9 AB-L0056.

## Standing rules
- ARCHITECTURE.md "Board standing rules": no pass re-scans emitted JS; complex-mode codegen only through the AB-D0004 typed oxc AST; legacy mode moves onto oxc in S6, so don't extend the hand member_chain further; whitespace-only parity is not a goal (D-0196); D-0224 reproduces astring bugs; D-0227 OWNERS is the single ownership source.
- Invariant (AB-L0058): per-node codegen facts come from the node or a typed parameter, never a Gen field. Open nit: Gen.cur_cr should become a local.
- AB-L0057 forward condition: slot_parent stays a typed parameter when codegen moves to oxc.
- Verdicts: APPROVE | REWORK | REJECT; CLOSED = resolved by a cited landing (never satisfies land.sh).

## Open questions for Migrate
- Proposal pending: land.sh should validate that the patch_id field is one bare 40-hex value.
