#!/usr/bin/env bash
# Usage: napi-css-errors.sh <repo-root> [log-file]
# Reports the two CSS cases in single-error-shipped.json (css-1009, css-1009-host-context)
# and the three aggregate-shipped.json JS cases. single_errors_match_shipped and
# aggregates_match_shipped compare every shipped case to the JS fixture. Prints
# css=<passed>/<total> aggregate=<passed>/<total> on stdout. Exit 3 if a fixture
# set changes or a test fails. No crate edit on a pass.
set -euo pipefail
die() { echo "!! napi-css-errors: $*" >&2; exit 3; }
[ $# -ge 1 ] && [ $# -le 2 ] || die "usage: $0 <repo-root> [log-file]"
repo="$(cd "$1" && pwd)"
log="${2:-$(mktemp /tmp/napi-css-errors.XXXXXX.log)}"
fix="$repo/crates/lwc-compiler-napi/tests/fixtures/single-error-shipped.json"
agg="$repo/crates/lwc-compiler-napi/tests/fixtures/aggregate-shipped.json"
[ -f "$fix" ] || die "missing $fix"
[ -f "$agg" ] || die "missing $agg"
node --input-type=module - "$fix" "$agg" <<'JS' || die "napi fixture set changed"
import fs from 'node:fs';
const cases = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const css = cases.filter((c) => String(c.filename || '').endsWith('.css'));
const got = css.map((c) => `${c.id}\t${c.filename}\t${c.src}`).sort();
const want = [
  'css-1009\tfoo.css\t:host{',
  'css-1009-host-context\tfoo.css\t:host-context(.a){}',
];
if (got.join('\n') !== want.join('\n')) {
  console.error(`napi-css-errors: css cases:\n${got.join('\n')}`);
  process.exit(1);
}
const aggregates = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'));
const ids = aggregates.map((c) => c.fixture).sort();
const wantAgg = [
  'api/reserved-property-name-class',
  'extra/decorator-used-as-value',
  'extra/import-before-and-between-classes',
];
if (ids.join('\n') !== wantAgg.join('\n')) {
  console.error(`napi-css-errors: aggregate cases:\n${ids.join('\n')}`);
  process.exit(1);
}
JS
# shellcheck source=cargo-locked.sh
source "$(cd "$(dirname "$0")" && pwd)/cargo-locked.sh"
rc=0
(cd "$repo" && cargo_locked test -p lwc-compiler-napi --lib match_shipped) >"$log" 2>&1 || rc=$?
[ "$rc" -eq 0 ] || die "match_shipped failed rc=$rc; log $log"
# Both tests must have run. A filter that matches neither is not a pass.
n=$(grep -c 'test errors::tests::.*match_shipped ... ok' "$log" || true)
[ "$n" -eq 2 ] || die "expected 2 match_shipped tests ok, got $n; log $log"
# 2 css cases + 3 aggregate cases. board-footers.sh requires <passed>/<total>.
printf '%s\n' '5/5'
