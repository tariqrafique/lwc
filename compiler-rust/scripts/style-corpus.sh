#!/usr/bin/env bash
# Usage: style-corpus.sh <repo-root> [log-file]
# Uncapped style-diff over the live style-coverage sources. Prints the style-corpus footer
# payload on stdout (board-footers.sh adds the row name). Exit 3 on a short count, a missing
# tree, or any JS/Rust mismatch. Caps in the tool do not apply: STYLE_DIFF_UNCAPPED=1 is set
# and a caller STYLE_DIFF_CAP / STYLE_DIFF_SLDS_CAP is unset.
#
# File counts are the full sets (no silent cap). Hash-dedup is reported, not used to drop a file
# from the count: bySource.files is every CSS file visited and bySource.unique + deduped == files.
# integration-* matches tools/style-diff walk() (skip node_modules and dist). Corpus leaves match
# walkAll (skip node_modules and .git).
set -euo pipefail
die() { echo "!! style-corpus: $*" >&2; exit 3; }
[ $# -ge 1 ] && [ $# -le 2 ] || die "usage: $0 <repo-root> [log-file]"
repo="$1"
log="${2:-$(mktemp /tmp/style-corpus.XXXXXX.log)}"
root="$(cd "$repo" && pwd)"
# shellcheck source=../harness/scripts/oracle-root.sh
source "$root/harness/scripts/oracle-root.sh"
LWC_ORACLE="$(oracle_root)" || die "oracle-root failed"
export LWC_ORACLE
export LWC_ROOT="$LWC_ORACLE"
STYLE_CORPUS="$(style_corpus_root)" || die "style-corpus-root failed"
export STYLE_CORPUS

css_count() {
  # $1 dir, $2 integration|corpus
  local dir="$1" mode="$2"
  if [ "$mode" = integration ]; then
    find "$dir" -type f -name '*.css' ! -path '*/node_modules/*' ! -path '*/dist/*' ! -path '*/.git/*' | wc -l
  else
    find "$dir" -type f -name '*.css' ! -path '*/node_modules/*' ! -path '*/.git/*' | wc -l
  fi
}

check_count() {
  # $1 source $2 dir $3 mode $4 want
  local name="$1" dir="$2" mode="$3" want="$4" n
  [ -d "$dir" ] || die "missing $dir ($name)"
  n="$(css_count "$dir" "$mode")"
  n="${n//[[:space:]]/}"
  [ "$n" = "$want" ] || die "$name css files=$n want $want ($dir)"
}

check_count integration-wtr "$LWC_ORACLE/packages/@lwc/integration-wtr" integration 206
check_count integration-wdio "$LWC_ORACLE/packages/@lwc/integration-wdio" integration 7
check_count integration-types "$LWC_ORACLE/packages/@lwc/integration-types" integration 1
check_count lbc "$STYLE_CORPUS/lbc" corpus 584
check_count lbc-recipes "$STYLE_CORPUS/lbc-recipes" corpus 32
check_count slds "$STYLE_CORPUS/slds" corpus 223
# style-diff fixture-smoke and parse-error walks are one-level actual.css, not every .css
# (the two import-target foo.css files under the fixtures stay on the `style` row).
actual_count() { find "$1" -mindepth 2 -maxdepth 2 -type f -name actual.css | wc -l; }
n="$(actual_count "$LWC_ORACLE/packages/@lwc/style-compiler/src/__tests__/fixtures")"
n="${n//[[:space:]]/}"
[ "$n" = 69 ] || die "fixture-smoke actual.css=$n want 69"
n="$(actual_count "$root/crates/lwc-style/tests/parse-errors")"
n="${n//[[:space:]]/}"
[ "$n" = 67 ] || die "parse-error actual.css=$n want 67"

out="${STYLE_CORPUS_OUT:-$root/target/style-corpus}"
mkdir -p "$(dirname "$log")" "$out"
unset STYLE_DIFF_CAP STYLE_DIFF_SLDS_CAP STYLE_DIFF_NO_FIXTURES STYLE_DIFF_NO_PARSE_ERRORS
export STYLE_DIFF_UNCAPPED=1
export STYLE_DIFF_OUT="$out"
rc=0
node "$root/tools/style-diff/run.mjs" >"$log" 2>&1 || rc=$?
report="$out/report.json"
[ -f "$report" ] || die "style-diff produced no report (rc=$rc); log $log"
expect="$(mktemp)"
cat > "$expect" <<'JSON'
{"integration-wtr":206,"integration-wdio":7,"integration-types":1,"lbc":584,"lbc-recipes":32,"slds":223,"fixture-smoke":69,"parse-error":67}
JSON
line=""
lrc=0
line="$(node "$root/scripts/style-corpus-line.mjs" "$report" "$expect")" || lrc=$?
rm -f "$expect"
if [ "$rc" -ne 0 ] || [ "$lrc" -ne 0 ]; then
  die "style-diff rc=$rc line rc=$lrc; log $log report $report"
fi
printf '%s\n' "$line"
