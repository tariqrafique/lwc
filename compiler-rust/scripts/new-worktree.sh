#!/usr/bin/env bash
# Create (idempotently) a bot's worktree: /workspace/wt/<bot> on branch bot/<bot>, own target/ (gitignored).
# Layout keeps ../parse5-rust (crates' path dep) and ../notes resolving: /workspace/wt/{parse5-rust,notes} are symlinks.
set -euo pipefail
bot="${1:?usage: new-worktree.sh <bot>}"; MAIN=/workspace/lwc-compiler-rust; W=/workspace/wt
mkdir -p "$W"; ln -sfn /workspace/parse5-rust "$W/parse5-rust"; ln -sfn /workspace/notes "$W/notes"
if [ ! -d "$W/$bot" ]; then
  git -C "$MAIN" show-ref -q --verify "refs/heads/bot/$bot" \
    && git -C "$MAIN" worktree add -q "$W/$bot" "bot/$bot" \
    || git -C "$MAIN" worktree add -q -b "bot/$bot" "$W/$bot" main
fi
"$W/$bot/harness/scripts/ensure-deps.sh"
echo "worktree: $W/$bot  branch: bot/$bot  target: $W/$bot/target"
