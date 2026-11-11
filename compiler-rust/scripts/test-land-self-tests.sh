#!/usr/bin/env bash
# Plants for the land self-test path map: a dropped coverage row no longer
# selects that test, and every name must list its own implementing script.
set -euo pipefail
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
repo="$(cd "$here/.." && pwd)"
fail=0
say() { printf '%s\n' "$*"; }
ok() { say "ok   $*"; }
bad() { say "FAIL $*"; fail=1; }

runner="$here/run-land-self-tests.sh"
map="$here/land-self-tests.tsv"
[ -x "$runner" ] || bad "missing runner $runner"
[ -f "$map" ] || bad "missing map $map"

if "$runner" --check-map; then
  ok "map lists every test's own script"
else
  bad "map completeness failed"
fi

tmp="$(mktemp -d /tmp/test-land-self-tests.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

# Live map: board-select.mjs must select test-board-select
printf '%s\n' 'scripts/board-select.mjs' > "$tmp/changed"
got="$("$runner" --list --changed-file "$tmp/changed")"
grep -qx 'test-board-select' <<< "$got" || bad "board-select.mjs did not select test-board-select (got: $got)"
ok "board-select.mjs selects test-board-select"

# Plant: delete coverage rows for board-select.mjs; selector must drop the test
# unless the changed file is the test script itself.
awk -F'\t' '$1=="#" || $1=="" || $3 != "scripts/board-select.mjs"' "$map" > "$tmp/plant.tsv"
LAND_SELF_TESTS_TSV="$tmp/plant.tsv" "$runner" --list --changed-file "$tmp/changed" > "$tmp/planted.list" || true
if grep -qx 'test-board-select' "$tmp/planted.list"; then
  bad "planted map still selected test-board-select for board-select.mjs"
else
  ok "omitting the board-select.mjs mapping row drops that path"
fi

# The test's own script still selects it after the plant
printf '%s\n' 'scripts/test-board-select.sh' > "$tmp/own"
LAND_SELF_TESTS_TSV="$tmp/plant.tsv" "$runner" --list --changed-file "$tmp/own" > "$tmp/own.list"
grep -qx 'test-board-select' "$tmp/own.list" || bad "own script should still select test-board-select"
ok "own script still selects test-board-select after dropping the extra path"

# Plant: drop the self-path row for test-board-select → --check-map fails
awk -F'\t' '!( $1=="test-board-select" && $3=="scripts/test-board-select.sh" )' "$map" > "$tmp/noself.tsv"
if LAND_SELF_TESTS_TSV="$tmp/noself.tsv" "$runner" --check-map >"$tmp/noself.out" 2>&1; then
  bad "map missing a self-path was accepted"
  sed 's/^/       /' "$tmp/noself.out"
else
  grep -q 'test-board-select' "$tmp/noself.out" || bad "check-map failure should name test-board-select"
  ok "omitting a test's own script fails --check-map"
fi

# rootcause fix 5: an agent-style harness-only change outside the gate files selects nothing,
# even when the map lists those paths (harness/board.sh, harness/board-rows.sh, vitest configs).
printf '%s\n' 'harness/board.sh' 'harness/board-rows.sh' 'harness/vitest.config.ts' 'harness/test/lbc.behavior.test.ts' \
  'harness/goldens/lbc/x.json' 'crates/lwc-js/src/lib.rs' > "$tmp/agent"
got="$("$runner" --list --changed-file "$tmp/agent")"
if [ -z "$got" ]; then ok "agent-style harness-only change runs no self-tests"; else bad "agent-style change selected: $(echo $got)"; fi
# A gate file selects its mapped tests.
printf '%s\n' 'harness/board.sh' 'scripts/baseline-ceiling.sh' > "$tmp/gate"
got="$("$runner" --list --changed-file "$tmp/gate")"
want="$(awk -F'\t' '$3 == "scripts/baseline-ceiling.sh" { print $1 }' "$map" | sort -u)"
if [ -n "$want" ] && [ "$(sort -u <<< "$got")" = "$want" ]; then ok "scripts/baseline-ceiling.sh runs its mapped tests ($(echo $got))"
else bad "gate file selected '$(echo $got)', want '$(echo $want)'"; fi

# land.sh invokes the runner
grep -q 'run-land-self-tests.sh' "$repo/scripts/preflight.sh" || bad "preflight.sh does not invoke run-land-self-tests.sh"
ok "preflight.sh invokes the mapped runner"

# --all never skips the napi tests (Migrate 2026-11-11 00:14).
got="$("$runner" --list --all)"
for n in test-board-ratchet test-shared-cargo-cache; do
  if grep -qx "$n" <<< "$got"; then ok "--all lists $n"; else bad "--all does not list $n"; fi
done
# Pinned napi unavailable -> the napi tests FAIL naming the missing napi (no skip, nothing run).
printf '%s\n' 'scripts/test-board-ratchet.sh' > "$tmp/changed"
rc=0; LWC_SELF_TEST_NAPI_TARGET="$tmp/no-napi-target" "$runner" --changed-file "$tmp/changed" > "$tmp/nonapi.out" 2>&1 || rc=$?
if [ "$rc" -ne 0 ] && grep -qF "pinned napi MISSING: $tmp/no-napi-target/release/liblwc_compiler_napi.so" "$tmp/nonapi.out" \
   && grep -qx 'FAIL test-board-ratchet' "$tmp/nonapi.out"; then
  ok "missing pinned napi fails test-board-ratchet loudly"
else
  bad "missing pinned napi rc=$rc"; sed 's/^/       /' "$tmp/nonapi.out"
fi

# preflight.sh sets MAIN to a git-archive of the base (no .git): check-land-main-refs still reads base's table.
mkdir -p "$tmp/archive/scripts"; cp "$repo/scripts/land-main-refs.tsv" "$tmp/archive/scripts/"
printf '%s\n' 'scripts/land-main-refs.tsv' > "$tmp/changed"
rc=0; MAIN="$tmp/archive" "$runner" --changed-file "$tmp/changed" > "$tmp/archive.out" 2>&1 || rc=$?
if grep -qx 'ok   check-land-main-refs' "$tmp/archive.out"; then
  ok "check-land-main-refs runs with MAIN = a base archive (preflight)"
else
  bad "check-land-main-refs with MAIN = archive (rc=$rc)"; grep -E 'FAIL|cannot' "$tmp/archive.out" | sed 's/^/       /'
fi

if [ "$fail" -ne 0 ]; then
  say "test-land-self-tests: FAILED"
  exit 1
fi
say "test-land-self-tests: ok"
