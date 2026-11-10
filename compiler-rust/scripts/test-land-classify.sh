#!/usr/bin/env bash
# Land-trigger self-test runner (parse5-pin, shim-registry, lint-shell-pipes,
# baseline-ceiling, test-board-ratchet, ratchet-ceiling, lint-no-stripped-compare,
# check-land-main-refs). Exit 0 iff every sub-test passes.
set -euo pipefail
SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
fail=0
"$SELF_DIR/test-parse5-pin.sh" || fail=1   # AB-L0050 (3): parse5 pin gate negatives stay proven
"$SELF_DIR/test-shim-registry.sh" || fail=1  # AB-L0070: SHIM registry gating
# AB-X0008: the shell-pipe lint's own self-test (planted violation fails, clean file passes, grep rc>=2 fails).
if "$SELF_DIR/lint-shell-pipes.sh" --self-test; then echo "ok   lint-shell-pipes --self-test"; else echo "FAIL lint-shell-pipes --self-test"; fail=1; fi
# AB-0006 rework: board-baseline/v2 parser + comparator self-test, then the board.sh / land-gate end-to-end test.
if "$SELF_DIR/baseline-ceiling.sh" --self-test; then echo "ok   baseline-ceiling --self-test"; else echo "FAIL baseline-ceiling --self-test"; fail=1; fi
if "$SELF_DIR/test-board-ratchet.sh"; then echo "ok   test-board-ratchet"; else echo "FAIL test-board-ratchet"; fail=1; fi
if "$SELF_DIR/ratchet-ceiling.sh" --self-test; then echo "ok   ratchet-ceiling --self-test"; else echo "FAIL ratchet-ceiling --self-test"; fail=1; fi
MAIN="$(cd "$SELF_DIR/.." && pwd)" "$SELF_DIR/lint-no-stripped-compare.sh" --self-test || fail=1   # D-0219: the stripped-compare lint still catches every form
MAIN="$(cd "$SELF_DIR/.." && pwd)" "$SELF_DIR/lint-no-stripped-compare.sh" "$(cd "$SELF_DIR/.." && pwd)" || fail=1  # D-0219: no new whitespace-normalizing compare in board tooling
# D-0274(5)/D-0275: every $MAIN/ ref in land.sh is allowlisted; box-config deps are named.
if "$SELF_DIR/check-land-main-refs.sh" --self-test "$(cd "$SELF_DIR/.." && pwd)"; then echo "ok   check-land-main-refs --self-test"; else echo "FAIL check-land-main-refs --self-test"; fail=1; fi
exit $fail
