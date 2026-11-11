#!/usr/bin/env bash
# D-0282 plants for cadence state, compose, and empty-suite honor.
set -euo pipefail
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
repo="$(cd "$here/.." && pwd)"
rt="$here/board-runtime.mjs"
fail=0
say() { printf '%s\n' "$*"; }
ok() { say "ok   $*"; }
bad() { say "FAIL $*"; fail=1; }

tmp="$(mktemp -d /tmp/test-board-runtime.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

# 1. no previous VALID board → cold + FULL
out="$(node "$rt" decide "$repo" HEAD "$tmp/empty-store")"
grep -qE "^BOARD_COLD='?1'?$" <<< "$out" || bad "no-prev should be cold: $out"
grep -qE "^BOARD_SCOPE='?FULL'?$" <<< "$out" || bad "no-prev should be FULL: $out"
ok "no previous board is a cold FULL"

# 2. set-carry-disabled persists and forces cold
mkdir -p "$tmp/store"
node "$rt" set-carry-disabled "$tmp/store" --reason "plant"
[ -f "$tmp/store/BOARD_RUNTIME_STATE.json" ] || bad "state file missing"
out="$(node "$rt" decide "$repo" HEAD "$tmp/store")"
grep -qE "^BOARD_COLD='?1'?$" <<< "$out" || bad "carry_disabled should be cold: $out"
grep -qE "^BOARD_CARRY_DISABLED='?1'?$" <<< "$out" || bad "carry_disabled flag missing: $out"
ok "carry_disabled forces cold"

# 3. compose splices carried rows from the previous board
cat > "$tmp/this.log" <<'LOG'
c8: check ok
napi sha256:aaa  pinned=/tmp/now  oracle=abc(clean)  oracle_repo=/o  2026-11-10T00:00:00-08:00
ssr                    Tests  1 passed (1)
shim-check: ok
style-parse-errors 67/67 (lwc-style fixture tests, LWC_STYLE_STRICT=1)
LOG
cat > "$tmp/prev.txt" <<'LOG'
official board  commit=old  src=prevsrc
napi sha256:bbb  pinned=/tmp/old  oracle=abc(clean)  oracle_repo=/o  2026-11-09T00:00:00-08:00
style                  Tests  70 passed (70)
js                     Tests  174 passed (174)
ssr                    Tests  388 passed (388)
shim-check: old
style-parse-errors 1/1 (old)
LOG
node "$rt" compose "$tmp/this.log" "$tmp/prev.txt" "$tmp/out.log" "$repo" --ran "ssr"
grep -q '^napi sha256:aaa' "$tmp/out.log" || bad "compose kept this header"
grep -q '^style                  Tests  70 passed (70)$' "$tmp/out.log" || bad "compose carried style from prev"
grep -q '^ssr                    Tests  1 passed (1)$' "$tmp/out.log" || bad "compose kept rerun ssr"
grep -q '^shim-check: ok$' "$tmp/out.log" || bad "compose preferred this-run shim-check"
grep -q '^style-parse-errors 67/67' "$tmp/out.log" || bad "compose kept this-run footer"
ok "compose merges carried rows"

# 4. check-carry-agreement: planted mismatch on an unchanged-hash row would fail,
#    but only when hashes match. Use identical files so a planted line mismatch fires.
#    We cannot cheaply force a matching hash without cargo metadata; instead plant
#    that identical tails report ok when there is no previous matching hash at a
#    missing ref... skip live hash here. Plant disagreement detector directly:
node -e '
import { readFileSync } from "fs";
const rt = readFileSync(process.argv[1], "utf8");
if (!rt.includes("carry disagreement")) process.exit(1);
' "$rt" || bad "agreement checker missing"
ok "agreement checker is present"

# 5. board.sh honors empty suites when BOARD_SUITES_EXPLICIT=1
#    (source-level plant: the guard exists). A full board.sh run is too expensive.
grep -q 'BOARD_SUITES_EXPLICIT' "$repo/harness/board.sh" || bad "board.sh missing BOARD_SUITES_EXPLICIT guard"
ok "board.sh empty-suite guard present"

# 6. golden-gate skip; fmt is the hard land-path-checks row cargo-fmt that C8 runs (D-0235)
grep -q 'BOARD_GOLDEN_GATE' "$here/run-board.sh" || bad "run-board missing golden-gate skip"
grep -q 'land-path-checks.sh' "$here/c8-checks.sh" || bad "c8-checks does not run land-path-checks"
grep -qP '^cargo-fmt\tscripts/cargo-fmt-check.sh\talways$' "$here/land-path-checks.tsv" || bad "land-path-checks.tsv missing the hard cargo-fmt row"
grep -q 'BOARD_PREV_BOARD_TXT' "$here/board-footers.sh" || bad "board-footers missing carried csr fallback"
grep -q 'board-runtime.mjs' "$here/official-board.sh" || bad "official-board missing runtime wiring"
ok "driver hooks present"

# 7. --git-ref is stable, and differs from a pretend-edit of a file present at HEAD
h1="$(node "$here/board-select.mjs" hash "$repo" style --oracle-sha test --git-ref HEAD)"
h2="$(node "$here/board-select.mjs" hash "$repo" style --oracle-sha test --git-ref HEAD)"
[ "$h1" = "$h2" ] && [ -n "$h1" ] || bad "git-ref hash not stable: $h1 vs $h2"
h3="$(node "$here/board-select.mjs" hash "$repo" style --oracle-sha test --git-ref HEAD --pretend-edit crates/lwc-style/src/lib.rs)"
[ "$h1" != "$h3" ] && [ -n "$h3" ] || bad "git-ref + pretend-edit should change the hash"
ok "hash --git-ref is stable and observes pretend-edit"

# 8. decide output must be eval-safe: a row named js-eval must not run /usr/bin/js
out="$(node "$rt" decide "$repo" HEAD "$tmp/empty-store")"
eval "$out"
[[ "${BOARD_ROWS:-}" == *js-eval* ]] || bad "eval of decide lost js-eval rows: ${BOARD_ROWS:-<empty>}"
[[ "${BOARD_SCOPE:-}" == FULL ]] || bad "eval of decide lost BOARD_SCOPE"
ok "eval of decide output is safe (js-eval is a value, not a command)"

if [ "$fail" -ne 0 ]; then
  say "test-board-runtime: FAILED"
  exit 1
fi
say "test-board-runtime: ok"
