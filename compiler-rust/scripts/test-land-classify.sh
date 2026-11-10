#!/usr/bin/env bash
# Land-trigger self-test runner (parse5-pin, shim-registry, lint-shell-pipes,
# baseline-ceiling, ratchet-ceiling, lint-no-stripped-compare,
# check-land-main-refs). Exit 0 iff every sub-test passes.
set -euo pipefail
SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
fail=0
"$SELF_DIR/test-parse5-pin.sh" || fail=1   # AB-L0050 (3): parse5 pin gate negatives stay proven
"$SELF_DIR/test-shim-registry.sh" || fail=1  # AB-L0070: SHIM registry gating
"$SELF_DIR/test-oracle-root.sh" || fail=1     # AB-M0002: single resolver
"$SELF_DIR/test-board-footers.sh" || fail=1  # AB-M0002: one footer writer + one driver
"$SELF_DIR/test-semantic-diff-row.sh" || fail=1  # PLAN §3 item 4: semantic-diff row plants
# AB-X0008: the shell-pipe lint's own self-test (planted violation fails, clean file passes, grep rc>=2 fails).
if "$SELF_DIR/lint-shell-pipes.sh" --self-test; then echo "ok   lint-shell-pipes --self-test"; else echo "FAIL lint-shell-pipes --self-test"; fail=1; fi
if "$SELF_DIR/workspace-path-scan.sh" --self-test; then echo "ok   workspace-path-scan --self-test"; else echo "FAIL workspace-path-scan --self-test"; fail=1; fi
if "$SELF_DIR/test-source-scan-tracked.sh" "$(cd "$SELF_DIR/.." && pwd)"; then echo "ok   test-source-scan-tracked"; else echo "FAIL test-source-scan-tracked"; fail=1; fi
"$SELF_DIR/test-land-agent-branch.sh" || fail=1  # D-0281: agent/* land helper plants
# AB-0006 rework: board-baseline/v2 parser + comparator self-test, then the board.sh / land-gate end-to-end test.
if "$SELF_DIR/baseline-ceiling.sh" --self-test; then echo "ok   baseline-ceiling --self-test"; else echo "FAIL baseline-ceiling --self-test"; fail=1; fi
# test-board-ratchet needs a napi binary and is invoked from land.sh after the
# napi release build, not from this bundle.
if "$SELF_DIR/ratchet-ceiling.sh" --self-test; then echo "ok   ratchet-ceiling --self-test"; else echo "FAIL ratchet-ceiling --self-test"; fail=1; fi
MAIN="$(cd "$SELF_DIR/.." && pwd)" "$SELF_DIR/lint-no-stripped-compare.sh" --self-test || fail=1   # D-0219: the stripped-compare lint still catches every form
MAIN="$(cd "$SELF_DIR/.." && pwd)" "$SELF_DIR/lint-no-stripped-compare.sh" "$(cd "$SELF_DIR/.." && pwd)" || fail=1  # D-0219: no new whitespace-normalizing compare in board tooling
# D-0274(5)/D-0275: every $MAIN/ ref in land.sh is allowlisted; box-config deps are named.
main_root="${MAIN:-/workspace/lwc-compiler-rust}"
main_sha="$(git -C "$main_root" rev-parse HEAD)"
_main_refs="$(mktemp)"
incoming_root="$(cd "$SELF_DIR/.." && pwd)"
if ! git -C "$main_root" show "${main_sha}:scripts/land-main-refs.tsv" > "$_main_refs"; then
  echo "FAIL cannot git show ${main_sha}:scripts/land-main-refs.tsv from $main_root"
  fail=1
elif "$SELF_DIR/check-land-main-refs.sh" --self-test "$incoming_root" \
    --main-table "$_main_refs" --incoming-table "$incoming_root/scripts/land-main-refs.tsv"; then
  echo "ok   check-land-main-refs --self-test"
else
  echo "FAIL check-land-main-refs --self-test"; fail=1
fi
rm -f "$_main_refs"
exit $fail
