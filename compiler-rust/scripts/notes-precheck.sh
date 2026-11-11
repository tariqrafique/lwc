#!/usr/bin/env bash
# Notes read-path precheck for scripts/official-board.sh (AB-0016). Read-only: never stages, commits or writes in <notes-repo>.
# Exit 0 iff every <path> exists at <notes-repo> HEAD and has no staged, unstaged or untracked change
# (git status --porcelain --untracked-files=all -- <paths>). Exit 3 (with the dirty paths named) otherwise, including
# any git error: a failed check is never a pass. Paths outside <paths> are ignored (other bots' half-written notes).
# Usage: scripts/notes-precheck.sh <notes-repo> <path>...
set -euo pipefail
die3() { echo "!! BOARD ABORTED: $*" >&2; exit 3; }
[ $# -ge 2 ] || die3 "usage: notes-precheck.sh <notes-repo> <path>..."
repo="$1"; shift
rc=0; head="$(git -C "$repo" rev-parse --verify -q 'HEAD^{commit}')" || rc=$?
[ $rc -eq 0 ] || die3 "notes precheck: cannot resolve HEAD in $repo (git rc=$rc)"
for p in "$@"; do
  rc=0; git -C "$repo" cat-file -e "$head:$p" 2>/dev/null || rc=$?
  [ $rc -eq 0 ] || die3 "notes precheck: read path '$p' not in $repo HEAD ${head:0:12} (git rc=$rc)"
done
rc=0; dirty="$(git -C "$repo" status --porcelain --untracked-files=all -- "$@")" || rc=$?
[ $rc -eq 0 ] || die3 "notes precheck: git status failed in $repo (rc=$rc)"
[ -z "$dirty" ] || die3 "notes read path(s) dirty in $repo (commit them in $repo yourself; official-board never does):
$dirty"
echo "notes precheck: clean @ ${head:0:12} for: $*"
