#!/usr/bin/env bash
# Print WARN lines for BOX_CONFIG_DEPENDENT allowlist rows whose script is
# absent from the tree. Always exits 0 so a board can surface orphans
# without changing its status (D-0286: independent of the land checker).
# Usage: print-orphan-allowlist-warns.sh <tree>
set -euo pipefail
tree="${1:-}"
[ -n "$tree" ] && [ -d "$tree" ] || { echo "usage: print-orphan-allowlist-warns.sh <tree>" >&2; exit 2; }
table="$tree/scripts/land-main-refs.tsv"
[ -f "$table" ] || exit 0
orphans=""
while IFS=$'\t' read -r path class dep policy || [ -n "${path:-}" ]; do
  [ -n "${path:-}" ] || continue
  case "$path" in '#'*) continue ;; esac
  [ "${class:-}" = BOX_CONFIG_DEPENDENT ] || continue
  [ -n "$path" ] || continue
  if [ ! -f "$tree/$path" ]; then
    orphans+="$path	$dep"$'\n'
  fi
done < "$table"
if [ -n "$orphans" ]; then
  echo "WARN orphan allowlist BOX_CONFIG_DEPENDENT row(s) (script absent):"
  printf '%s' "$orphans" | sed 's/^/  /'
fi
exit 0
