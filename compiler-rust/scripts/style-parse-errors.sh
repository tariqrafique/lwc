#!/usr/bin/env bash
# Usage: style-parse-errors.sh <repo-root> [log-file]
# Runs `cargo test -p lwc-style --test fixtures` with LWC_STYLE_STRICT=1 (so any fixture mismatch exits non-zero) and
# prints the crate-local parse-error set as <passed>/<total> on stdout (the official manifest's style_parse_errors).
# The fixtures test is harness=false: it prints no libtest `test result:` line, but one custom summary line
#   upstream fixtures: U/UT; parse-errors: P/T
# covering the upstream @lwc/style-compiler fixture dir then crates/lwc-style/tests/parse-errors. We report P/T (parse-errors
# only). Exit 3: cargo/test failure, summary line missing or not exactly once, or T == 0. LWC_FIXTURE/LWC_FIXTURES are
# unset so a caller's env cannot filter or redirect the set.
set -euo pipefail
die3() { echo "!! style-parse-errors: $*" >&2; exit 3; }
[ $# -ge 1 ] && [ $# -le 2 ] || die3 "usage: $0 <repo-root> [log-file]"
repo="$1"; log="${2:-$(mktemp /tmp/style-fixtures.XXXXXX.log)}"
[ -d "$repo/crates/lwc-style/tests/parse-errors" ] || die3 "no $repo/crates/lwc-style/tests/parse-errors"
# shellcheck source=cargo-locked.sh
source "$(cd "$(dirname "$0")" && pwd)/cargo-locked.sh"
rc=0; (cd "$repo" && unset LWC_FIXTURE LWC_FIXTURES LWC_STYLE_DIFF && export LWC_STYLE_STRICT=1 && cargo_locked test -p lwc-style --test fixtures) > "$log" 2>&1 || rc=$?
[ $rc -eq 0 ] || die3 "lwc-style fixtures (LWC_STYLE_STRICT=1) failed rc=$rc; log $log"
re='^upstream fixtures: ([0-9]+)/([0-9]+); parse-errors: ([0-9]+)/([0-9]+)$'
n=0; out=""
while IFS= read -r line; do
  if [[ "$line" =~ $re ]]; then n=$((n+1)); out="${BASH_REMATCH[3]}/${BASH_REMATCH[4]}"; tot="${BASH_REMATCH[4]}"; fi
done < "$log"
[ $n -eq 1 ] || die3 "expected exactly 1 'upstream fixtures: ..; parse-errors: P/T' line, got $n; log $log"
[ "$tot" -gt 0 ] || die3 "parse-errors total is 0; log $log"
echo "$out"
