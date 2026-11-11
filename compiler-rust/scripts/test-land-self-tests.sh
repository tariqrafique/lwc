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

# board.sh (not land.sh-only) selects ratchet, not the main-refs self-test
printf '%s\n' 'harness/board.sh' > "$tmp/board"
got="$("$runner" --list --changed-file "$tmp/board")"
grep -qx 'test-board-ratchet' <<< "$got" || bad "board.sh did not select test-board-ratchet (got: $got)"
grep -qx 'check-land-main-refs' <<< "$got" && bad "board.sh should not select check-land-main-refs"
ok "board.sh selects test-board-ratchet only among the heavy tests"

# land.sh invokes the runner
grep -q 'run-land-self-tests.sh' "$repo/scripts/land.sh" || bad "land.sh does not invoke run-land-self-tests.sh"
ok "land.sh invokes the mapped runner"

if [ "$fail" -ne 0 ]; then
  say "test-land-self-tests: FAILED"
  exit 1
fi
say "test-land-self-tests: ok"
