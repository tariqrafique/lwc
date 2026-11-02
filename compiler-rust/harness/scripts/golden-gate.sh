#!/usr/bin/env bash
# Golden-integrity gate: goldens must equal a fresh JS-oracle recording (oracle /workspace/lwc @ pinned sha).
# Re-records every row with LWC_COMPILER_BACKEND=js UPDATE_GOLDENS=1 into a scratch COPY of <harness> (depth 3,
# so cwd-relative paths match) and diffs goldens/. Any diff => exit 1. Never touches <harness>/goldens.
# Usage: scripts/golden-gate.sh [harness-dir]   (default: this harness). Slow (JS backend, all rows): run in background.
set -euo pipefail
"$(dirname "$0")/ensure-deps.sh"
h="$(cd "${1:-$(dirname "$0")/..}" && pwd)"
tmp="$(mktemp -d /tmp/lwc-golden-gate.XXXXXX)"; trap 'rm -rf "$tmp"' EXIT
prog="${GOLDEN_GATE_PROGRESS:-/tmp/golden-gate.progress}"
mkdir -p "$tmp/harness"
(cd "$h" && find . -type f ! -path '*/node_modules/*' ! -path './.pins/*' ! -path './.vitest/*' \
  ! -path './.csr-render-out/*' ! -path './.ssr-render-out/*' ! -name '*.node' -print0 | tar --null -T - -cf -) | tar -xf - -C "$tmp/harness"
ln -s "$(readlink -f "$h/node_modules")" "$tmp/harness/node_modules"
cd "$tmp/harness"
export LWC_COMPILER_BACKEND=js UPDATE_GOLDENS=1 PATH="/workspace/lwc/node_modules/.bin:$PATH"
unset LWC_COMPILER_NAPI_PATH
run() { local mode="$1"; shift; echo "== $(date +%T) $mode $*" >> "$prog"
  SSR_MODE="$mode" /workspace/lwc/node_modules/.bin/vitest run "$@" --pool=threads --testTimeout=30000 --reporter=dot >>"$tmp/rec.log" 2>&1 || true; }
for t in style js js-eval template js-options template-options ssr_dynamic_imports style-api template-api ssr_render_api; do
  run sync "test/$t.behavior.test.ts"; done
for m in sync async asyncYield; do run "$m" test/ssr.behavior.test.ts; run "$m" test/ssr.render.test.ts; done
CORPUS=ssr run sync test/csr.render.test.ts
CORPUS=template run sync test/csr.render.test.ts
[ -n "${GOLDEN_GATE_KEEP:-}" ] && { rm -rf "$GOLDEN_GATE_KEEP"; cp -r "$tmp/harness/goldens" "$GOLDEN_GATE_KEEP"; }  # fresh JS recording, for reviewed re-records
echo "== $(date +%T) diff" >> "$prog"
if diff -rq "$h/goldens" "$tmp/harness/goldens" > "$tmp/diff.txt"; then
  echo "golden-gate: $(find "$h/goldens" -type f | wc -l) goldens == fresh JS-oracle recording"
else
  cp "$tmp/diff.txt" /tmp/golden-gate.last-diff.txt; cp "$tmp/rec.log" /tmp/golden-gate.last-rec.log
  echo "!! GOLDEN GATE FAILED: $(wc -l < "$tmp/diff.txt") goldens differ from a fresh JS-oracle recording (/tmp/golden-gate.last-diff.txt)"
  head -20 "$tmp/diff.txt"; exit 1
fi
