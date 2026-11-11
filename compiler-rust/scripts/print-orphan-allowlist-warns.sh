#!/usr/bin/env bash
# Print WARN lines for BOX_CONFIG_DEPENDENT allowlist rows whose script is
# absent on both main and incoming. Always exits 0 so a board can surface
# orphans without changing its status.
# Usage: print-orphan-allowlist-warns.sh <tree>
set -euo pipefail
tree="${1:-}"
[ -n "$tree" ] && [ -d "$tree" ] || { echo "usage: print-orphan-allowlist-warns.sh <tree>" >&2; exit 2; }
runner="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
land="$tree/scripts/land.sh"
table="$tree/scripts/land-main-refs.tsv"
chk="$runner/check-land-main-refs.sh"
[ -x "$chk" ] && [ -f "$land" ] && [ -f "$table" ] || exit 0
out=0
out="$("$chk" --check "$land" "$tree" --main-table "$table" --incoming-table "$table" 2>&1)" || true
printf '%s\n' "$out" | awk '
  /^WARN orphan allowlist/ { print; p=1; next }
  p && /^  / { print; next }
  { p=0 }
'
exit 0
