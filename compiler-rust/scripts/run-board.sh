#!/usr/bin/env bash
# Single in-repo board driver (AB-M0002). Produces every board.txt data row:
# golden-gate, C8, board.sh, then footers (style-parse-errors + csr-clusters).
# official-board.sh pins/verifies box inputs, calls this, then publishes.
# run-portable-board.sh fetches inputs, then calls this.
#
# Usage: run-board.sh <repo-root> <board-log> [board.sh args...]
# Required env: PARSE5_DIR (official pins the box clone; portable sets remote).
# Optional env: C8_TARBALLS, SPE_LOG, BOARD_TXT, GOLDEN_GATE_PROGRESS,
#   CARGO_TARGET_DIR, SSR_ORACLE_*, CRATES_ROOT, OPTGATE_DECISIONS, SEMDIFF_REGISTRY,
#   TEMPLATE_DIAG_OUT_DIR, SHIM_FILE_LIST, SHIM_RATCHET_DIR, BOARD_REPO_ROOT.
# C8 helper scripts come from this file's tree (the runner).
set -euo pipefail
runner="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
[ $# -ge 2 ] || { echo "usage: run-board.sh <repo-root> <board-log> [board.sh args...]" >&2; exit 2; }
ws="$(cd "$1" && pwd)"; shift
board_log="$1"; shift
hs="$ws/harness"
[ -d "$hs" ] || { echo "!! run-board: $ws has no harness/" >&2; exit 2; }
[ -n "${PARSE5_DIR:-}" ] || { echo "!! run-board: PARSE5_DIR must be set" >&2; exit 2; }
export NO_COLOR="${NO_COLOR:-1}"
export PARSE5_DIR

PARSE5_DIR="$PARSE5_DIR" "$runner/parse5-pin-check.sh" "$ws" \
  || { echo "!! run-board: parse5 pin check failed" >&2; exit 2; }

"$hs/scripts/golden-gate.sh" "$hs" \
  || { echo "!! run-board: golden-gate failed" >&2; exit 3; }

# Snapshot archives have no .git: official-board sets SSR_ORACLE_GIT/MAIN.
# A fresh clone is often detached (no `main` ref); default --main=main then fails.
if [ -z "${SSR_ORACLE_MAIN:-}" ] && git -C "$ws" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  export SSR_ORACLE_MAIN="$(git -C "$ws" rev-parse HEAD)"
  export SSR_ORACLE_GIT="${SSR_ORACLE_GIT:-$ws}"
fi
c8_tout="${C8_TARBALLS:-$ws/.oracle/tarballs}"
"$runner/c8-checks.sh" "$ws" "$c8_tout" || { echo "!! run-board: C8 checks failed" >&2; exit 3; }

so="${CARGO_TARGET_DIR:-$ws/target}/release/liblwc_compiler_napi.so"
[ -f "$so" ] || { echo "!! run-board: missing $so after C8" >&2; exit 3; }
cp "$so" "$hs/lwc-compiler-napi.node"
"$hs/scripts/ensure-deps.sh" "$hs" || { echo "!! run-board: ensure-deps failed" >&2; exit 2; }

spe_log="${SPE_LOG:-$ws/.oracle/style-fixtures.log}"
mkdir -p "$(dirname "$spe_log")" "$(dirname "$board_log")"
spe="$("$runner/style-parse-errors.sh" "$ws" "$spe_log")" \
  || { echo "!! run-board: style-parse-errors failed (see $spe_log)" >&2; exit 3; }
[[ "$spe" =~ ^[0-9]+/[0-9]+$ ]] || { echo "!! run-board: style-parse-errors.sh printed '$spe', want <passed>/<total>" >&2; exit 3; }

set +e
env -u BOARD_RATCHET_UPDATE -u TDD_SEMDIFF_EXEC "$hs/board.sh" "$@" 2>&1 | tee "$board_log"
rc=${PIPESTATUS[0]}
set -e
if grep -q '^!! ' "$board_log" && [ "$rc" -eq 0 ]; then
  echo "!! rc=0 but board printed a failure; forcing rc=1" >&2
  rc=1
fi

set +e
foot="$("$runner/board-footers.sh" "$hs" "$spe")"
frc=$?
set -e
printf '%s\n' "$foot" | tee -a "$board_log"
if [ "$frc" -ne 0 ]; then
  echo "!! run-board: board-footers failed (rc=$frc)" >&2
  exit "$frc"
fi

if [ -n "${BOARD_TXT:-}" ]; then
  mkdir -p "$(dirname "$BOARD_TXT")"
  sed -n '/^napi sha256:/,$p' "$board_log" > "$BOARD_TXT"
fi
exit "$rc"
