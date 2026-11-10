#!/usr/bin/env bash
# The one footer writer (AB-M0002). Prints the trailing rows of board.txt:
#   style-parse-errors     P/T (lwc-style fixture tests, LWC_STYLE_STRICT=1)
#   csr-clusters[ssr]: ...
#   csr-clusters[template]: ...
# Usage: board-footers.sh <harness-dir> <style-parse-errors passed/total>
# A missing CSR report fails (does not continue).
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: board-footers.sh <harness-dir> <passed/total>" >&2; exit 2; }
hs="$1"
spe="$2"
[[ "$spe" =~ ^[0-9]+/[0-9]+$ ]] || { echo "board-footers: spe '$spe' is not <passed/total>" >&2; exit 2; }
echo "style-parse-errors     $spe (lwc-style fixture tests, LWC_STYLE_STRICT=1)"
for c in ssr template; do
  rep="$hs/.csr-render-out/rust-$c/report-$c.json"
  if [ ! -f "$rep" ]; then
    echo "!! csr clusters: missing $rep" >&2
    exit 3
  fi
  echo "csr-clusters[$c]: $(node "$hs/scripts/csr-render-clusters.mjs" "$rep" | grep -E '^(pass |\[)' | sed -E 's/^\[([0-9]+)\] ([a-z-]+):.*/\2=\1/' | paste -sd' ')"
done
