# Sourced (not executed) by harness/board.sh and scripts/land.sh: the ONE board row list and option-gate invocation (AB-0005).
# land.sh sources $MAIN's copy, so the rows and gate script that judge a landing are main's, never the branch's.
ROWS=(style js js-eval template ssr ssr-async ssr-asyncYield ssr-render ssr-render-async ssr-render-asyncYield csr-render csr-render-template template-renderer js-options js-api template-options ssr_dynamic_imports style-api template-api ssr_render_api template-diag template-diag-napi style-upstream)
# Diagnostic-only rows (Migrate 2026-11-01): never gate. SSR gates on ssr-render* only.
DIAG=" ssr ssr-async ssr-asyncYield template template-diag template-diag-napi "  # template = compile-observation; gates via csr-render-template
BOARD_ROWS_HARNESS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Option-inventory gate (scripts/option-gate.mjs next to this file). Inputs via env: CRATES_ROOT, OPTGATE_INVENTORY.
option_gate() { OPTGATE_DIAG="$DIAG" node "$BOARD_ROWS_HARNESS/scripts/option-gate.mjs" "${ROWS[*]}"; }
