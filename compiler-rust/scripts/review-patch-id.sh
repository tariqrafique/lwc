#!/usr/bin/env bash
# Patch-id that land.sh binds a Baseline-Raise (and any milestone Arch review) to:
# git patch-id --stable of the whole diff main...<ref>.
# Usage (reviewer or bot, from any worktree): scripts/review-patch-id.sh [ref=HEAD]
# Rebase onto main first; if main moves and the rebased diff changes, the id changes and the review must be redone.
set -euo pipefail
ref="${1:-HEAD}"; base="$(git merge-base main "$ref")"
git diff "$base" "$ref" | git patch-id --stable | cut -d' ' -f1
