#!/usr/bin/env bash
# Usage: napi-css-errors.sh <repo-root> [log-file]
# Reports the two CSS cases in crates/lwc-compiler-napi/tests/fixtures/single-error-shipped.json
# (css-1009, css-1009-host-context). The crate test single_errors_match_shipped compares every
# shipped single error, including those two, to the JS fixture. Prints <passed>/<total> (2/2)
# on stdout. Exit 3 if the fixture set changes or the test fails. No crate edit on a pass.
set -euo pipefail
die() { echo "!! napi-css-errors: $*" >&2; exit 3; }
[ $# -ge 1 ] && [ $# -le 2 ] || die "usage: $0 <repo-root> [log-file]"
repo="$(cd "$1" && pwd)"
log="${2:-$(mktemp /tmp/napi-css-errors.XXXXXX.log)}"
fix="$repo/crates/lwc-compiler-napi/tests/fixtures/single-error-shipped.json"
[ -f "$fix" ] || die "missing $fix"
node --input-type=module - "$fix" <<'JS' || die "CSS fixture set is not css-1009 + css-1009-host-context"
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
JS
# shellcheck source=cargo-locked.sh
source "$(cd "$(dirname "$0")" && pwd)/cargo-locked.sh"
rc=0
(cd "$repo" && cargo_locked test -p lwc-compiler-napi --lib single_errors_match_shipped) >"$log" 2>&1 || rc=$?
[ "$rc" -eq 0 ] || die "single_errors_match_shipped failed rc=$rc; log $log"
printf '%s\n' '2/2'
