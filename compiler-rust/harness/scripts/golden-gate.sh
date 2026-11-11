#!/usr/bin/env bash
# Golden-integrity gate: goldens must equal a fresh JS-oracle recording (oracle /workspace/lwc @ pinned sha).
# Re-records every row with LWC_COMPILER_BACKEND=js UPDATE_GOLDENS=1 into a scratch COPY of <harness> (depth 3,
# so cwd-relative paths match) whose goldens/ starts EMPTY (directory skeleton only, no files), then diffs goldens/.
# Fails (exit 1) on: any recording run exiting non-zero (vitest load/collect error, test failure), a differing golden,
# or a different golden FILE SET (a committed golden the recording did not produce, or vice versa). Committed goldens are
# never copied into the scratch tree, so they can never be compared against themselves (AB-0003). Never touches <harness>/goldens.
# Usage: scripts/golden-gate.sh [harness-dir]   (default: this harness). Slow (JS backend, all rows): run in background.
set -euo pipefail
# shellcheck source=oracle-root.sh
source "$(cd "$(dirname "$0")" && pwd)/oracle-root.sh"
ORACLE="$(oracle_root)" || exit 2
export LWC_ORACLE="$ORACLE" LWC_ROOT="$ORACLE" PATH="$ORACLE/node_modules/.bin:$PATH"
"$(dirname "$0")/ensure-deps.sh"
h="$(cd "${1:-$(dirname "$0")/..}" && pwd)"
tmp="$(mktemp -d /tmp/lwc-golden-gate.XXXXXX)"; trap 'rm -rf "$tmp"' EXIT
prog="${GOLDEN_GATE_PROGRESS:-$tmp/progress}"
mkdir -p "$tmp/harness"
(cd "$h" && find . -type f ! -path '*/node_modules/*' ! -path './goldens/*' ! -path './.pins/*' ! -path './.vitest/*' \
  ! -path './.csr-render-out/*' ! -path './.ssr-render-out/*' ! -name '*.node' -print0 | tar --null -T - -cf -) | tar -xf - -C "$tmp/harness"
(cd "$h" && find goldens -type d -print0) | (cd "$tmp/harness" && xargs -0 mkdir -p)   # skeleton only: zero golden files
[ -z "$(find "$tmp/harness/goldens" -type f -print -quit)" ] || { echo "!! GOLDEN GATE FAILED: scratch goldens/ not empty before recording"; exit 1; }
ln -s "$(readlink -f "$h/node_modules")" "$tmp/harness/node_modules"
cd "$tmp/harness"
export LWC_COMPILER_BACKEND=js UPDATE_GOLDENS=1 PATH="$ORACLE/node_modules/.bin:$PATH"
unset LWC_COMPILER_NAPI_PATH
recfail=()
run() { local mode="$1" rc=0; shift
  # Absent test files are a no-op so a new loop entry (style-upstream) can land before its test.
  local a
  for a in "$@"; do
    case "$a" in
      test/*)
        if [ ! -f "$a" ]; then
          echo "== $(date +%T) $mode $* skipped (absent $a)" >> "$prog"
          return 0
        fi
        ;;
    esac
  done
  echo "== $(date +%T) $mode $*" >> "$prog"
  SSR_MODE="$mode" "$ORACLE/node_modules/.bin/vitest" run "$@" --pool=threads --testTimeout=30000 --reporter=dot >>"$tmp/rec.log" 2>&1 || rc=$?
  [ "$rc" -eq 0 ] || recfail+=("${CORPUS:+CORPUS=$CORPUS }${SSR_RUNTIME:+SSR_RUNTIME=$SSR_RUNTIME }SSR_MODE=$mode $* (rc=$rc)"); }
for t in style js js-eval template js-options template-options ssr_dynamic_imports style-api template-api ssr_render_api style-upstream; do
  run sync "test/$t.behavior.test.ts"; done
for m in sync async asyncYield; do run "$m" test/ssr.behavior.test.ts; run "$m" test/ssr.render.test.ts; done
# ssr-render/<mode>.src.json goldens (SSR_RUNTIME=src, the ssr-runtime TS source) are committed too: record them, so no golden is ever un-gated.
for m in sync async asyncYield; do SSR_RUNTIME=src run "$m" test/ssr.render.test.ts; done
CORPUS=ssr run sync test/csr.render.test.ts
CORPUS=template run sync test/csr.render.test.ts
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
