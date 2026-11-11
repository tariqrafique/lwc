#!/usr/bin/env bash
# The one footer writer (AB-M0002). Prints the trailing rows of board.txt:
#   style-parse-errors     P/T (lwc-style fixture tests, LWC_STYLE_STRICT=1)
#   csr-clusters[ssr]: ...
#   csr-clusters[template]: ...
#   style-corpus     <payload from scripts/style-corpus.sh>
#   napi-css-errors     P/T (lwc-compiler-napi single_errors_match_shipped, css-1009 css-1009-host-context)
# Usage: board-footers.sh <harness-dir> <style-parse-errors passed/total> <style-corpus payload> <napi-css passed/total>
# A missing CSR report fails (does not continue).
set -euo pipefail
[ $# -eq 4 ] || { echo "usage: board-footers.sh <harness-dir> <passed/total> <style-corpus-payload> <napi-css passed/total>" >&2; exit 2; }
hs="$1"
spe="$2"
corpus="$3"
napi="$4"
[[ "$spe" =~ ^[0-9]+/[0-9]+$ ]] || { echo "board-footers: spe '$spe' is not <passed/total>" >&2; exit 2; }
case "$corpus" in
  ''|*$'\n'*|*$'\r'*) echo "board-footers: style-corpus payload empty or multiline" >&2; exit 2 ;;
esac
[[ "$napi" =~ ^[0-9]+/[0-9]+$ ]] || { echo "board-footers: napi-css '$napi' is not <passed/total>" >&2; exit 2; }
echo "style-parse-errors     $spe (lwc-style fixture tests, LWC_STYLE_STRICT=1)"
for c in ssr template; do
  rep="$hs/.csr-render-out/rust-$c/report-$c.json"
  if [ ! -f "$rep" ]; then
    _prev="${BOARD_PREV_BOARD_TXT:-}"
    _carried=""
    if [ -n "$_prev" ] && [ -f "$_prev" ]; then
      _carried="$(grep -E "^csr-clusters\[$c\]:" "$_prev" || true)"
    fi
    if [ -n "$_carried" ]; then
      echo "$_carried"
      continue
    fi
    echo "!! csr clusters: missing $rep" >&2
    exit 3
  fi
  echo "csr-clusters[$c]: $(node "$hs/scripts/csr-render-clusters.mjs" "$rep" | grep -E '^(pass |\[)' | sed -E 's/^\[([0-9]+)\] ([a-z-]+):.*/\2=\1/' | paste -sd' ')"
done
echo "style-corpus     $corpus"
echo "napi-css-errors     $napi (lwc-compiler-napi single_errors_match_shipped, css-1009 css-1009-host-context)"
