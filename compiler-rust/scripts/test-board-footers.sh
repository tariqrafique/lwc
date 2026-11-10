#!/usr/bin/env bash
# Footer writer + single-driver wiring (AB-M0002).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
S="$ROOT/scripts/board-footers.sh"
OB="$ROOT/scripts/official-board.sh"
PB="$ROOT/scripts/run-portable-board.sh"
RB="$ROOT/scripts/run-board.sh"
T="$(mktemp -d /tmp/board-footers-test.XXXXXX)"; trap 'rm -rf "$T"' EXIT
fail=0

grep -q 'run-board.sh' "$OB" || { echo "FAIL official-board.sh does not call run-board.sh"; fail=1; }
grep -q 'run-board.sh' "$PB" || { echo "FAIL run-portable-board.sh does not call run-board.sh"; fail=1; }
# Neither driver writes rows itself: no direct board.sh / c8-checks / board-footers invocation,
# no inlined style-parse-errors or csr-clusters echoes.
for f in "$OB" "$PB"; do
  name="$(basename "$f")"
  if grep -E '(^|[[:space:]$])hs/board\.sh|(^|[[:space:]])\$root/harness/board\.sh|"\$hs/board\.sh"' "$f"; then
    echo "FAIL $name invokes board.sh directly"; fail=1
  fi
  if grep -nE 'c8-checks\.sh|board-footers\.sh' "$f" | grep -vE 'run-board|#'; then
    echo "FAIL $name invokes c8-checks.sh or board-footers.sh itself"; fail=1
  fi
  if grep -nE 'echo "style-parse-errors|echo "csr-clusters' "$f"; then
    echo "FAIL $name still echoes footer rows"; fail=1
  fi
done
# Grep gate: only board-footers.sh writes those footer row strings among scripts/.
# (tests and this file mention them; run-board.sh only calls board-footers.sh.)
writers="$(grep -Rln --include='*.sh' -e 'style-parse-errors     ' -e 'csr-clusters\[' "$ROOT/scripts" || true)"
# allow board-footers.sh itself
only="$(printf '%s\n' "$writers" | grep -vE 'test-board-footers\.sh$' || true)"
if [ "$(grep -c . <<< "$only" || true)" -eq 1 ] && grep -q 'scripts/board-footers.sh$' <<< "$only"; then
  echo "ok   only board-footers.sh writes footer row text"
else
  echo "FAIL footer writers:"; printf '%s\n' "$writers" | sed 's/^/       /'
  fail=1
fi
# Shared driver is the one that calls board.sh, c8-checks, and board-footers.
grep -q 'board.sh' "$RB" && grep -q 'c8-checks.sh' "$RB" && grep -q 'board-footers.sh' "$RB" \
  && echo "ok   run-board.sh calls board.sh, C8, and board-footers" \
  || { echo "FAIL run-board.sh is missing board.sh/C8/footers"; fail=1; }

mkdir -p "$T/hs/scripts" \
  "$T/hs/.csr-render-out/rust-ssr" \
  "$T/hs/.csr-render-out/rust-template"
cp "$ROOT/harness/scripts/csr-render-clusters.mjs" "$T/hs/scripts/"

printf '%s\n' '{"rows":[{"fixture":"a","pass":true,"status":"ok","goldenStatus":"ok"},{"fixture":"b","pass":true,"status":"ok","goldenStatus":"ok"},{"fixture":"c","pass":true,"status":"ok","goldenStatus":"ok"}]}' \
  > "$T/hs/.csr-render-out/rust-ssr/report-ssr.json"
printf '%s\n' '{"rows":[{"fixture":"ok","pass":true,"status":"ok","goldenStatus":"ok"},{"fixture":"bad","pass":false,"status":"fail","goldenStatus":"ok","got":{"html":"","error":"Assert Violation: [object Object], is not a vnode"},"want":{"html":"x"}}]}' \
  > "$T/hs/.csr-render-out/rust-template/report-template.json"

got="$("$S" "$T/hs" "67/67")"
want=$'style-parse-errors     67/67 (lwc-style fixture tests, LWC_STYLE_STRICT=1)\ncsr-clusters[ssr]: pass 3 fail 0\ncsr-clusters[template]: pass 1 fail 1 vnode=1'
if [ "$got" = "$want" ]; then
  echo "ok   planted reports match expected footer rows"
else
  echo "FAIL footer rows:"
  printf ' got:\n%s\n want:\n%s\n' "$got" "$want"
  fail=1
fi

rc=0; "$S" "$T/hs" "not-a-ratio" >"$T/out" 2>&1 || rc=$?
if [ "$rc" -eq 2 ]; then echo "ok   bad spe is rc 2"; else echo "FAIL bad spe rc=$rc"; fail=1; fi

rm -rf "$T/hs/.csr-render-out"
rc=0; "$S" "$T/hs" "0/1" >"$T/out" 2>&1 || rc=$?
if [ "$rc" -eq 3 ] && grep -q '^!! csr clusters: missing' "$T/out"; then
  echo "ok   missing CSR report fails (rc 3)"
else
  echo "FAIL missing CSR report rc=$rc: $(cat "$T/out")"
  fail=1
fi

exit $fail
