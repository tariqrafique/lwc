# Sourced (not executed) by harness/board.sh and scripts/land.sh: the ONE board row list and option-gate invocation (AB-0005).
# Gate code still comes from main (D-0286). Row *contents* are main UNION the branch's additions.
# A branch may add a row in the same land. To remove a row or move one into DIAG (loosening),
# cite it in this file as `# D-NNNN: remove <row>` or `# D-NNNN: loosen <row>`.
# template-diag-semdiff, template-diag-ssr, template-diag-style, template-diag-realgate,
# template-diag-napierrors: gating coverage rows. The template-diag- prefix is the
# existing non-vitest row shape (baseline counts only; golden-gate does not record
# them). They are not in DIAG. Each one runs a JS-oracle vs Rust compare and fails
# the board on a mismatch. See board.sh.
ROWS=(style js js-eval template ssr ssr-async ssr-asyncYield ssr-render ssr-render-async ssr-render-asyncYield csr-render csr-render-template template-renderer js-options js-api template-options ssr_dynamic_imports style-api template-api ssr_render_api template-diag template-diag-napi style-upstream lbc template-diag-semdiff template-diag-ssr template-diag-style template-diag-realgate template-diag-napierrors upstream-cmp js-transform-corpus unported-specs)
# Diagnostic-only rows (Migrate 2026-11-01): never gate. SSR gates on ssr-render* only.
DIAG=" ssr ssr-async ssr-asyncYield template template-diag template-diag-napi "  # template = compile-observation; gates via csr-render-template
BOARD_ROWS_HARNESS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Option-inventory gate (scripts/option-gate.mjs next to this file). Inputs via env: CRATES_ROOT, OPTGATE_INVENTORY.
option_gate() { OPTGATE_DIAG="$DIAG" node "$BOARD_ROWS_HARNESS/scripts/option-gate.mjs" "${ROWS[*]}"; }
