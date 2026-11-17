#!/usr/bin/env bash
# D-0280(b): enumerate repo source files. Source-scanning gates must use this
# helper (or scripts/tracked-files.mjs, which execs it).
#   * In a git work tree: `git ls-files` (untracked / gitignored inputs are omitted).
#   * In a git-archive snapshot (official-board; no .git): every file except
#     fetched/build directories (.oracle/, target/, node_modules/, .git/).
# Fetched inputs are never repo source.
# Usage: scripts/tracked-files.sh <repo-root> [pathspec...]
# Prints NUL-delimited repo-relative paths. Exit 2 if the tree cannot be listed.
set -euo pipefail
if [ $# -lt 1 ]; then
  echo "usage: tracked-files.sh <repo-root> [pathspec...]" >&2
  exit 2
fi
root="$1"
shift
[ -d "$root" ] || { echo "tracked-files: not a directory: $root" >&2; exit 2; }

is_git_work_tree() {
  git -C "$1" rev-parse --is-inside-work-tree >/dev/null 2>&1
}

# cwd must be "$root". Emits NUL-delimited repo-relative paths.
find_archived() {
  find "$@" -type f \
    ! -path '*/.oracle/*' \
    ! -path '*/target/*' \
    ! -path '*/node_modules/*' \
    ! -path '*/.git/*' \
    -print0 | sed -z 's|^\./||'
}

emit_archived() {
  local spec f
  if [ $# -eq 0 ]; then
    find_archived .
    return 0
  fi
  for spec in "$@"; do
    if [[ "$spec" == *[\*\?]* ]]; then
      shopt -s nullglob
      for f in $spec; do
        [ -f "$f" ] || continue
        case "$f" in
          */.oracle/*|*/target/*|*/node_modules/*|*/.git/*) continue ;;
        esac
        printf '%s\0' "$f"
      done
      shopt -u nullglob
    elif [ -d "$spec" ]; then
      find_archived "$spec"
    elif [ -f "$spec" ]; then
      printf '%s\0' "$spec"
    fi
  done
}

if is_git_work_tree "$root"; then
  if ! git -C "$root" ls-files -z -- "$@"; then
    echo "tracked-files: git ls-files failed under $root" >&2
    exit 2
  fi
else
  ( cd "$root" && emit_archived "$@" ) || {
    echo "tracked-files: archive listing failed under $root" >&2
    exit 2
  }
fi
