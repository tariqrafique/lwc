#!/usr/bin/env bash
# Guards for scripts/fetch-oracle.sh: refuse /workspace/lwc (and anything under it) before any network.
set -euo pipefail
S="$(cd "$(dirname "$0")" && pwd)"
F="$S/fetch-oracle.sh"
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }
expect_refuse() {
  local name="$1" dest="$2" rc=0
  "$F" "$dest" >"$tmp/out" 2>&1 || rc=$?
  if [ "$rc" -eq 2 ] && grep -q 'must never touch /workspace/lwc' "$tmp/out"; then
    ok "$name -> rc 2"
  else
    bad "$name -> rc $rc: $(tr '\n' ' ' < "$tmp/out")"
  fi
}
tmp="$(mktemp -d /tmp/test-fetch-oracle.XXXXXX)"; trap 'rm -rf "$tmp"' EXIT
expect_refuse "/workspace/lwc" /workspace/lwc
expect_refuse "/workspace/lwc/subdir" /workspace/lwc/subdir
# A relative path that canonicalizes under /workspace/lwc
expect_refuse "realpath via .." /tmp/../workspace/lwc
# dest that is not /workspace/lwc must pass the guard (we stop before fetch by using a fake URL and checking the message).
# The script inits+fetches; do not run a real fetch here. Only the refuse cases are asserted.
[ "$fail" -eq 0 ] || exit 1
echo "test-fetch-oracle: ok"
