#!/usr/bin/env bash
# Prints every NON-trivial path of the diff base..head, one per line (empty output = trivial landing).
# Usage: land-classify.sh <base> <head> <trivial-paths.txt>   (run inside the repo/worktree)
# Policy (owned by LWC Arch Board): one glob per line, '#' comments; '*' crosses '/'; '+glob' = additions-only
# (path matches AND numstat shows added>0 and deleted==0; binary '-' and mode-only 0/0 never qualify). Missing policy file = nothing trivial.
# --no-renames everywhere: a rename is a delete of the old path + an add of the new one, so moving src/ into a
# trivial location still reports the deleted src path.
set -euo pipefail
base="$1"; git rev-parse -q --verify "$1^{commit}" >/dev/null && git rev-parse -q --verify "$2^{commit}" >/dev/null || { echo "land-classify: bad revs $1 $2" >&2; exit 2; }
head="$2"; policy="$3"
globs=()
if [ -f "$policy" ]; then
  while IFS= read -r l || [ -n "$l" ]; do l="${l%%#*}"; l="$(printf '%s' "$l" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"; [ -n "$l" ] && globs+=("$l"); done < "$policy"
fi
git diff --no-renames --name-only "$base" "$head" | while IFS= read -r f; do
  [ -n "$f" ] || continue; m=
  for g in "${globs[@]+"${globs[@]}"}"; do
    if [[ $g == +* ]]; then
      [[ $f == ${g#+} ]] && [ "$(git diff --no-renames --numstat "$base" "$head" -- "$f" | awk '$1 ~ /^[0-9]+$/ && $1 > 0 && $2 == "0" {print "add"}')" = add ] && { m=1; break; }
    else [[ $f == $g ]] && { m=1; break; }; fi
  done
  [ -n "$m" ] || printf '%s\n' "$f"
done
