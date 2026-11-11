#!/usr/bin/env bash
# Golden-integrity gate: goldens must equal a fresh JS-oracle recording (oracle /workspace/lwc @ pinned sha).
# Re-records every row with LWC_COMPILER_BACKEND=js UPDATE_GOLDENS=1 into a scratch COPY of <harness> (depth 3,
# so cwd-relative paths match) whose goldens/ starts EMPTY (directory skeleton only, no files), then diffs goldens/.
# Fails (exit 1) on: any recording run exiting non-zero (vitest load/collect error, test failure), a differing golden,
# or a different golden FILE SET (a committed golden the recording did not produce, or vice versa). Committed goldens are
# never copied into the scratch tree, so they can never be compared against themselves (AB-0003). Never touches <harness>/goldens.
# Usage: scripts/golden-gate.sh [--plan] [harness-dir]   (default: this harness).
# Recording set is every vitest row in board-rows.sh (D-0286): main UNION the
# harness copy's additions. --plan prints the set and checks removals/loosening
# without recording. Slow full run (JS backend, all rows): run in background.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=../../scripts/registry-union.sh
source "$here/../../scripts/registry-union.sh"
plan_only=0
if [ "${1:-}" = --plan ]; then plan_only=1; shift; fi
# shellcheck source=oracle-root.sh
source "$here/oracle-root.sh"
ORACLE="$(oracle_root)" || exit 2
export LWC_ORACLE="$ORACLE" LWC_ROOT="$ORACLE" PATH="$ORACLE/node_modules/.bin:$PATH"
"$(dirname "$0")/ensure-deps.sh"
h="$(cd "${1:-$here/..}" && pwd)"
main_rows_file="$here/../board-rows.sh"
branch_rows_file="$h/board-rows.sh"
recording_rows="$(union_board_rows "$main_rows_file" "$branch_rows_file")" || { echo "!! GOLDEN GATE FAILED: board-rows registry union rejected (uncited removal or DIAG loosening)"; exit 1; }
recording_plan="$(while IFS= read -r _r; do [ -n "$_r" ] || continue; board_row_golden_records "$_r"; done <<< "$recording_rows" | sort -u)"
if [ "$plan_only" -eq 1 ]; then
  printf '%s\n' "$recording_plan"
  echo "golden-gate: plan $(printf '%s\n' "$recording_plan" | grep -c . || true) unique recording(s) from $(printf '%s\n' "$recording_rows" | grep -c . || true) board-rows"
  exit 0
fi
tmp="$(mktemp -d /tmp/lwc-golden-gate.XXXXXX)"; trap 'rm -rf "$tmp"' EXIT
prog="${GOLDEN_GATE_PROGRESS:-$tmp/progress}"
mkdir -p "$tmp/harness"
(cd "$h" && find . -type f ! -path '*/node_modules/*' ! -path './goldens/*' ! -path './.pins/*' ! -path './.vitest/*' \
  ! -path './.csr-render-out/*' ! -path './.ssr-render-out/*' ! -path './.lbc-out/*' ! -name '*.node' -print0 | tar --null -T - -cf -) | tar -xf - -C "$tmp/harness"
(cd "$h" && find goldens -type d -print0) | (cd "$tmp/harness" && xargs -0 mkdir -p)   # skeleton only: zero golden files
[ -z "$(find "$tmp/harness/goldens" -type f -print -quit)" ] || { echo "!! GOLDEN GATE FAILED: scratch goldens/ not empty before recording"; exit 1; }
ln -s "$(readlink -f "$h/node_modules")" "$tmp/harness/node_modules"
cd "$tmp/harness"
export LWC_COMPILER_BACKEND=js UPDATE_GOLDENS=1 PATH="$ORACLE/node_modules/.bin:$PATH"
unset LWC_COMPILER_NAPI_PATH
recfail=()
run() { local mode="$1" rc=0; shift
  echo "== $(date +%T) $mode $*" >> "$prog"
  SSR_MODE="$mode" "$ORACLE/node_modules/.bin/vitest" run "$@" --pool=threads --testTimeout=30000 --reporter=dot >>"$tmp/rec.log" 2>&1 || rc=$?
  [ "$rc" -eq 0 ] || recfail+=("${CORPUS:+CORPUS=$CORPUS }${SSR_RUNTIME:+SSR_RUNTIME=$SSR_RUNTIME }SSR_MODE=$mode $* (rc=$rc)"); }
while IFS= read -r _pline || [ -n "${_pline:-}" ]; do
  [ -n "$_pline" ] || continue
  _mode="${_pline%%$'\t'*}"
  _rest="${_pline#*$'\t'}"
  _test="${_rest%%$'\t'*}"
  _env="${_rest#*$'\t'}"
  # Prefix-assign so the env is exported for `run` and its vitest child
  # (same as the old `SSR_RUNTIME=src run ...` / `CORPUS=ssr run ...`).
  case "$_env" in
    '') run "$_mode" "$_test" ;;
    CORPUS=ssr) CORPUS=ssr run "$_mode" "$_test" ;;
    CORPUS=template) CORPUS=template run "$_mode" "$_test" ;;
    SSR_RUNTIME=src) SSR_RUNTIME=src run "$_mode" "$_test" ;;
    *) echo "!! GOLDEN GATE FAILED: unexpected recording env '$_env'"; exit 1 ;;
  esac
done <<< "$recording_plan"
if [ ${#recfail[@]} -gt 0 ]; then
  keep="$(mktemp -d /tmp/golden-gate-fail.XXXXXX)"; cp "$tmp/rec.log" "$keep/"
  echo "!! GOLDEN GATE FAILED: ${#recfail[@]} JS-oracle recording run(s) failed (load error or test failure; log $keep/rec.log):"
  printf '   %s\n' "${recfail[@]}"; exit 1
fi
[ -n "${GOLDEN_GATE_KEEP:-}" ] && { rm -rf "$GOLDEN_GATE_KEEP"; cp -r "$tmp/harness/goldens" "$GOLDEN_GATE_KEEP"; }  # fresh JS recording, for reviewed re-records
# Path-leak class (Migrate 2026-11-01): a golden must never embed a recording/snapshot location. The oracle
# (/workspace/lwc/) is a fixed path and allowed; any temp dir, snapshot dir or the rust repo path is a leak.
# >>> leak-scan (exercised verbatim by the AB-X0007 rc>=2 check): grep rc 0 = leak, 1 = clean, >=2 (I/O error) = FAIL.
lrc=0; leaks="$(grep -rlE '/tmp/|lwc-board-(snap|h)\.|lwc-golden-gate\.|/workspace/lwc-compiler-rust|/workspace/notes' "$h/goldens" "$tmp/harness/goldens")" || lrc=$?
if [ $lrc -ge 2 ]; then echo "!! GOLDEN GATE FAILED: path-leak scan could not run (grep rc=$lrc)"; exit 1; fi
# <<< leak-scan
if [ -n "$leaks" ]; then echo "!! GOLDEN GATE FAILED: goldens embed a temp/snapshot/repo path (not cwd-independent):"; head -20 <<< "$leaks"; exit 1; fi
echo "== $(date +%T) diff" >> "$prog"
(cd "$h/goldens" && find . -type f | sort) > "$tmp/set.committed"; (cd "$tmp/harness/goldens" && find . -type f | sort) > "$tmp/set.fresh"
if ! cmp -s "$tmp/set.committed" "$tmp/set.fresh"; then
  keep="$(mktemp -d /tmp/golden-gate-fail.XXXXXX)"; drc=0; diff "$tmp/set.committed" "$tmp/set.fresh" > "$keep/set.diff" || drc=$?
  [ $drc -eq 1 ] || echo "!! GOLDEN GATE FAILED: diff of file sets exit $drc (want 1: sets differ per cmp)"; cp "$tmp/rec.log" "$keep/"
  echo "!! GOLDEN GATE FAILED: golden FILE SET differs ($(grep -c '^<' "$keep/set.diff") committed-only, $(grep -c '^>' "$keep/set.diff") fresh-only; $keep/set.diff)"
  head -20 "$keep/set.diff"; exit 1
fi
if diff -rq "$h/goldens" "$tmp/harness/goldens" > "$tmp/diff.txt"; then
  echo "golden-gate: $(wc -l < "$tmp/set.fresh") goldens == fresh JS-oracle recording (same file set, recorded from empty)"
else
  keep="$(mktemp -d /tmp/golden-gate-fail.XXXXXX)"; cp "$tmp/diff.txt" "$tmp/rec.log" "$keep/"
  echo "!! GOLDEN GATE FAILED: $(wc -l < "$tmp/diff.txt") goldens differ from a fresh JS-oracle recording ($keep/diff.txt)"
  head -20 "$tmp/diff.txt"; exit 1
fi
