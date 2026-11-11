#!/usr/bin/env bash
# D-0282: a lint added to the shared list is observed on both the land and C8 entry points.
# Also plants that the land.sh self-test trigger regex actually matches (the extra ')' never did).
set -euo pipefail
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
repo="$(cd "$here/.." && pwd)"
fail=0
say() { printf '%s\n' "$*"; }
ok() { say "ok   $*"; }
bad() { say "FAIL $*"; fail=1; }

tmp="$(mktemp -d /tmp/test-land-path-checks.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

# 1. Shared list exists and names the in-repo-input checks.
list="$here/land-path-checks.tsv"
[ -f "$list" ] || bad "missing $list"
for n in lint-shell-pipes workspace-path-scan lint-no-stripped-compare ratchet-ceiling baseline-ceiling shim-gate; do
  grep -qE "^${n}	" "$list" || bad "list missing $n"
done
ok "shared list names the in-repo-input checks"

# 2. Plant: append a lint to a copy of the list; both --from-land and --from-c8 run it.
printf '%s\n' '#!/bin/bash' 'echo "plant-lint ran as ${LAND_PATH_FROM:-?}" >> "$PLANT_LOG"' 'exit 0' > "$tmp/plant.sh"
chmod +x "$tmp/plant.sh"
# The driver resolves path relative to repo root (here/../path). Use a path under scripts/ via symlink.
ln -s "$tmp/plant.sh" "$here/../scripts/_plant-land-path-lint.sh"
cleanup_plant() { rm -f "$here/../scripts/_plant-land-path-lint.sh"; rm -rf "$tmp"; }
trap cleanup_plant EXIT
cp "$list" "$tmp/list.tsv"
printf '%s\t%s\t%s\n' plant-lint scripts/_plant-land-path-lint.sh always >> "$tmp/list.tsv"
: > "$tmp/plant.log"
export PLANT_LOG="$tmp/plant.log"
"$here/land-path-checks.sh" --root "$repo" --lints-only --list "$tmp/list.tsv" --from-land || bad "driver --from-land rc=$?"
"$here/land-path-checks.sh" --root "$repo" --lints-only --list "$tmp/list.tsv" --from-c8 || bad "driver --from-c8 rc=$?"
pc="$(grep -c 'plant-lint ran' "$tmp/plant.log" || true)"
[ "$pc" -eq 2 ] || bad "plant-lint ran $pc times, want 2 (land + c8)"
grep -q 'plant-lint ran as land' "$tmp/plant.log" || bad "land entry did not run planted lint"
grep -q 'plant-lint ran as c8' "$tmp/plant.log" || bad "c8 entry did not run planted lint"
ok "a lint added to the list runs on both land and c8 entry points"
rm -f "$here/../scripts/_plant-land-path-lint.sh"

# 3. land.sh selects self-tests from scripts/land-self-tests.tsv (not a single
#    catch-all regex). The mapping plant lives in test-land-self-tests.sh; here
#    we only check that this shared-list test is still on the map and that
#    land.sh invokes the runner.
grep -q 'run-land-self-tests.sh' "$repo/scripts/land.sh" || bad "land.sh does not invoke run-land-self-tests.sh"
grep -qE '^test-land-path-checks	' "$here/land-self-tests.tsv" || bad "map missing test-land-path-checks"
grep -q $'\tscripts/land-path-checks.sh$' "$here/land-self-tests.tsv" || bad "map missing scripts/land-path-checks.sh"
grep -q $'\tscripts/test-land-path-checks.sh$' "$here/land-self-tests.tsv" || bad "map missing scripts/test-land-path-checks.sh"
grep -q $'\tscripts/land.sh$' "$here/land-self-tests.tsv" || bad "map missing scripts/land.sh coverage"
ok "land.sh uses the mapped runner and the map covers land-path-checks"

# 4. both entry points actually call the shared driver.
grep -q 'land-path-checks.sh' "$here/c8-checks.sh" || bad "c8-checks.sh does not invoke land-path-checks.sh"
grep -q 'main_run scripts/land-path-checks.sh' "$repo/scripts/land.sh" || bad "land.sh does not main_run land-path-checks.sh"
ok "land.sh and c8-checks.sh both read the shared list via the driver"

if [ "$fail" -ne 0 ]; then
  say "test-land-path-checks: FAILED"
  exit 1
fi
say "test-land-path-checks: ok"
