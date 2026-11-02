# state/<bot>.md
Updated: <YYYY-MM-DD HH:MM PST> by <bot>. Read PLAN.md first; this file holds only this bot's working state.

## Current item
- <review/sweep id or task>: <one line goal>; status <not started | in progress | in review | blocked on X>

## Owed REWORKs (ids)
- <AB-xxxx>: <status, branch/commit if any>

## Branch/worktree + last landed main commit
- Worktree: /workspace/wt/<bot>  branch bot/<bot> @ <sha>  (<n> ahead / <n> behind main)
- Uncommitted: <none | files>
- Last landing on main: <sha> "<subject>" (Arch-Review: <id>)

## Last trusted board
- Official src=<12-hex> commit=<sha> published=<time PST>; rows this bot moves: <row pass/total>
- Provisional? <yes: reason | no>

## Dead ends (what was tried and why it failed, with evidence paths)
- <approach>: failed because <reason>. Evidence: <path or sha>

## Open questions for Migrate
- <question>; blocking? <yes/no>
