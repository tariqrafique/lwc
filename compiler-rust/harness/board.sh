#!/usr/bin/env bash
# Official board: pin ONE immutable napi binary for the whole run, log its hash.
# Usage: ./board.sh [suite ...]   (default: all rows below)
# Guards: (1) oracle must be pristine ORACLE_SHA, clean tree -> else abort before running anything.
#         (3) untagged-shim gate: untagged shim/compat/workaround/hack/mask/rescan line in crates/ (scripts/shim-gate.mjs, allowlist shim-allowlist.tsv w/ reason) => exit 1.
#         (2) option-inventory gate (scripts/option-gate.mjs) -> any unmapped/pending field => exit 1.
# template-diag row (diagnostic, non-gating): template-diag-diff corpus (scripts/template-diag-diff.mjs, owned by the
#   Template Diagnostics bot), engine=crate. Mismatches never gate; crash/panic > 0 or a missing/invalid report => exit 1.
#   Driver is built into ${TDD_DRIVER_TARGET:-${CARGO_TARGET_DIR:-<repo>/target}/template-diag-driver} (never main's target).
#   Report JSON: ${TEMPLATE_DIAG_OUT_DIR:-<row log dir>}/template-diag-<engine>.json (official-board.sh publishes it).
#   Repo root (tools/, crates/) = $BOARD_REPO_ROOT, else dirname $CRATES_ROOT, else this harness's parent.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"  # suites resolve fixture/virtual paths relative to harness/
ORACLE=/workspace/lwc ORACLE_SHA=b9e8007
ROWS=(style js js-eval template ssr ssr-async ssr-asyncYield ssr-render ssr-render-async ssr-render-asyncYield csr-render csr-render-template template-renderer js-options js-api template-options ssr_dynamic_imports style-api template-api ssr_render_api template-diag template-diag-napi)
# Diagnostic-only rows (Migrate 2026-11-01): never gate. SSR gates on ssr-render* only.
DIAG=" ssr ssr-async ssr-asyncYield template template-diag template-diag-napi "  # template = compile-observation; gates via csr-render-template
head_sha="$(git -C "$ORACLE" rev-parse HEAD)"
[[ "$head_sha" == "$ORACLE_SHA"* ]] || { echo "!! ORACLE NOT PINNED: $ORACLE HEAD=$head_sha, want $ORACLE_SHA" >&2; exit 2; }
dirty="$(git -C "$ORACLE" status --porcelain --untracked-files=all)"
[ -z "$dirty" ] || { echo "!! ORACLE DIRTY ($ORACLE @ $ORACLE_SHA):" >&2; echo "$dirty" | head -20 >&2; exit 2; }
"$here/scripts/ensure-deps.sh" || exit 2
VITEST="$ORACLE/node_modules/.bin/vitest"   # never npx: it silently fetches an unrelated vitest when links are missing
norun=(); rowfail=()
repo="${BOARD_REPO_ROOT:-${CRATES_ROOT:+$(dirname "$CRATES_ROOT")}}"; repo="$(cd "${repo:-$here/..}" && pwd)"
logdir="${BOARD_LOG_DIR:-$(mktemp -d /tmp/board-run.$(date +%Y%m%dT%H%M%S).$$.XXXX)}"; mkdir -p "$logdir"; echo "row logs: $logdir"
src="$here/lwc-compiler-napi.node"
pins="$here/.pins"; mkdir -p "$pins"
hash="$(sha256sum "$src" | cut -c1-12)"
pinned="$pins/lwc-compiler-napi.$hash.node"
[ -f "$pinned" ] || { tmp="$(mktemp "$pins/.tmp.XXXXXX")"; cp "$src" "$tmp"; chmod 0555 "$tmp"; mv -f "$tmp" "$pinned"; }
# prune pins older than 1 day
find "$pins" -name 'lwc-compiler-napi.*.node' -mtime +1 -delete 2>/dev/null || true
export LWC_COMPILER_BACKEND=rust LWC_COMPILER_NAPI_PATH="$pinned"
export PATH="/workspace/lwc/node_modules/.bin:$PATH"
# Template diagnostics corpus row: $1=row $2=engine. Prints one board line; any failure -> rowfail (=> !! BOARD FAILED).
template_diag_row() {
  local row="$1" engine="$2" t0=$SECONDS why="" line
  local out="$logdir/board-$row-$hash.out" dir="${TEMPLATE_DIAG_OUT_DIR:-$logdir}"
  local json="$dir/template-diag-$engine.json"
  mkdir -p "$dir"; rm -f "$json"   # never read a stale report
  local tenv=(TDD_ENGINE="$engine" TDD_OUT="$json")
  case "$engine" in
    crate)
      local manifest="$repo/tools/template-diag-diff/driver/Cargo.toml"
      local tgt="${TDD_DRIVER_TARGET:-${CARGO_TARGET_DIR:-$repo/target}/template-diag-driver}"
      if [ ! -f "$manifest" ]; then why="driver manifest missing: $manifest"
      elif ! cargo build --release --locked -q --manifest-path "$manifest" --target-dir "$tgt" >"$out" 2>&1; then why="driver build failed"
      else tenv+=(TDD_DRIVER="$tgt/release/template-diag-driver"); fi;;
    # napi = the transformSync seam through this board's pinned binary. NON-AUTHORITATIVE (Migrate 2026-11-01): ~1120
    #   seam-rejects ("invalid compile options") under root-cause by Template Diagnostics; only crash/oracle-error gate it.
    napi) tenv+=(TDD_NAPI="$pinned");;
    *) why="engine '$engine' not wired into board.sh";;
  esac
  if [ -z "$why" ]; then
    env "${tenv[@]}" node "$here/scripts/template-diag-diff.mjs" >>"$out" 2>&1 || why="template-diag-diff.mjs exit $?"
  fi
  echo "row elapsed $((SECONDS - t0))s  report $json" >>"$out"
  line="$(node "$here/scripts/template-diag-summary.mjs" "$json" "$engine" "$row" 2>>"$out")" || why="${why:-$(tail -1 "$out")}"
  printf '%s\n' "${line:-$(printf '%-22s (no summary)' "$row")}"
  [ -z "$why" ] || rowfail+=("$row: $why (see $out)")
}
suites=("$@"); [ $# -eq 0 ] && suites=("${ROWS[@]}")
echo "napi sha256:$hash  pinned=$pinned  oracle=$head_sha(clean)  oracle_repo=$ORACLE  $(date -Iseconds)"   # official-board.sh parses oracle=/oracle_repo= (verified values)
for s in "${suites[@]}"; do
  case "$s" in template-diag|template-diag-*) template_diag_row "$s" "$( [ "$s" = template-diag ] && echo crate || echo "${s#template-diag-}")"; continue;; esac
  out="$logdir/board-$s-$hash.out"
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
if [ ${#norun[@]} -ne 0 ]; then echo "!! BOARD FAILED: row(s) produced no test summary: ${norun[*]} (see $logdir/board-<row>-$hash.out)"; fi
for f in "${rowfail[@]}"; do echo "!! BOARD FAILED: $f"; done
[ $gate -eq 0 ] && [ $shimgate -eq 0 ] && [ ${#norun[@]} -eq 0 ] && [ ${#rowfail[@]} -eq 0 ] || exit 1
