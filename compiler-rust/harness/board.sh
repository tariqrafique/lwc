#!/usr/bin/env bash
# Official board: pin ONE immutable napi binary for the whole run, log its hash.
# Usage: ./board.sh [suite ...]   (default: all rows below)
# Guards: (1) oracle must be pristine ORACLE_SHA, clean tree -> else abort before running anything.
#         (3) untagged-shim gate: untagged shim/compat/workaround/hack/mask/rescan line in crates/ (scripts/shim-gate.mjs, allowlist shim-allowlist.tsv w/ reason) => exit 1.
#         (2) option-inventory gate (scripts/option-gate.mjs) -> any unmapped/pending field => exit 1.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"  # suites resolve fixture/virtual paths relative to harness/
ORACLE=/workspace/lwc ORACLE_SHA=b9e8007
ROWS=(style js js-eval template ssr ssr-async ssr-asyncYield ssr-render ssr-render-async ssr-render-asyncYield csr-render csr-render-template template-renderer js-options js-api template-options ssr_dynamic_imports style-api template-api ssr_render_api)
# Diagnostic-only rows (Migrate 2026-11-01): never gate. SSR gates on ssr-render* only.
DIAG=" ssr ssr-async ssr-asyncYield template "  # template = compile-observation; gates via csr-render-template
head_sha="$(git -C "$ORACLE" rev-parse HEAD)"
[[ "$head_sha" == "$ORACLE_SHA"* ]] || { echo "!! ORACLE NOT PINNED: $ORACLE HEAD=$head_sha, want $ORACLE_SHA" >&2; exit 2; }
dirty="$(git -C "$ORACLE" status --porcelain --untracked-files=all)"
[ -z "$dirty" ] || { echo "!! ORACLE DIRTY ($ORACLE @ $ORACLE_SHA):" >&2; echo "$dirty" | head -20 >&2; exit 2; }
"$here/scripts/ensure-deps.sh" || exit 2
VITEST="$ORACLE/node_modules/.bin/vitest"   # never npx: it silently fetches an unrelated vitest when links are missing
norun=()
src="$here/lwc-compiler-napi.node"
pins="$here/.pins"; mkdir -p "$pins"
hash="$(sha256sum "$src" | cut -c1-12)"
pinned="$pins/lwc-compiler-napi.$hash.node"
[ -f "$pinned" ] || { tmp="$(mktemp "$pins/.tmp.XXXXXX")"; cp "$src" "$tmp"; chmod 0555 "$tmp"; mv -f "$tmp" "$pinned"; }
# prune pins older than 1 day
find "$pins" -name 'lwc-compiler-napi.*.node' -mtime +1 -delete 2>/dev/null || true
export LWC_COMPILER_BACKEND=rust LWC_COMPILER_NAPI_PATH="$pinned"
export PATH="/workspace/lwc/node_modules/.bin:$PATH"
suites=("$@"); [ $# -eq 0 ] && suites=("${ROWS[@]}")
echo "napi sha256:$hash  pinned=$pinned  oracle=$head_sha(clean)  $(date -Iseconds)"
for s in "${suites[@]}"; do
  out="/tmp/board-$s-$hash.out"
  test="test/$s.behavior.test.ts"; mode=sync; extra=()
  case "$s" in
    js-api) test=test/js-options.behavior.test.ts; extra=(-t '^js-options: api ');;
    js-options) extra=(-t '^js-options: (lws|explicit|sourcemap|epm|dyn) ');;
    template-renderer) test=test/template.behavior.test.ts; extra=(-t 'renderer-hooks/|directive-spread/no-renderer-hook');;
    csr-render) test=test/csr.render.test.ts; export CORPUS=ssr;;
    csr-render-template) test=test/csr.render.test.ts; export CORPUS=template;;
    ssr-render) test=test/ssr.render.test.ts;;
    ssr-render-*) test=test/ssr.render.test.ts; mode="${s#ssr-render-}";;
    ssr-*) test=test/ssr.behavior.test.ts; mode="${s#ssr-}";;
  esac
  SSR_MODE="$mode" "$VITEST" run "$test" "${extra[@]}" --pool=threads --testTimeout=5000 --reporter=dot >"$out" 2>&1 || true
  line="$(grep -E '^\s+Tests ' "$out" | tail -1 | sed 's/^ *//')"
  err="$(grep -cE 'SIGBUS|SIGSEGV|Worker exited unexpectedly' "$out" || true)"
  tag=""; [[ "$DIAG" == *" $s "* ]] && tag="  (diagnostic, non-gating)"
  printf '%-22s %s%s%s\n' "$s" "$line" "$tag" "$([ "$err" != 0 ] && echo '  !! WORKER CRASH')"
  [ -n "$line" ] || norun+=("$s")
done
shimgate=0; node "$here/scripts/shim-gate.mjs" || shimgate=1
gate=0; OPTGATE_DIAG="$DIAG" node "$here/scripts/option-gate.mjs" "${ROWS[*]}" || gate=1
if [ $shimgate -ne 0 ]; then echo "!! BOARD FAILED: untagged shim-ish line(s) in crates/ — tag as SHIM(<kind>: ...), remove, or allowlist with reason"; fi
if [ $gate -ne 0 ]; then echo "!! BOARD FAILED: option-inventory gate"; fi
if [ ${#norun[@]} -ne 0 ]; then echo "!! BOARD FAILED: row(s) produced no test summary: ${norun[*]} (see /tmp/board-<row>-$hash.out)"; fi
[ $gate -eq 0 ] && [ $shimgate -eq 0 ] && [ ${#norun[@]} -eq 0 ] || exit 1
