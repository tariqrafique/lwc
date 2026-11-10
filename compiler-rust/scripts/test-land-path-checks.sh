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

# 3. land.sh self-test trigger regex: extra ')' made it match nothing; the fixed
#    pattern must match scripts/land.sh and the new helper/tests.
ext="$(python3 - <<'PY' "$repo/scripts/land.sh"
import re, pathlib, sys
text = pathlib.Path(sys.argv[1]).read_text()
m = re.search(r"grep -qE '([^']+)' <<< \"\$changed\"", text)
if not m:
    sys.exit(2)
print(m.group(1))
PY
)" || bad "could not extract land.sh trigger regex"
[ -n "${ext:-}" ] || bad "empty trigger regex"
grep -q 'land-path-checks' <<< "$ext" || bad "fixed regex should list land-path-checks"
# planted extra paren: the fixed regex must not end with ))$ 
grep -qE '\)\)$' <<< "$ext" && bad "trigger regex still ends with extra )  (never matches)"
grep -qE "$ext" <<< 'scripts/land.sh' || bad "fixed regex does not match scripts/land.sh"
grep -qE "$ext" <<< 'scripts/land-path-checks.sh' || bad "fixed regex does not match scripts/land-path-checks.sh"
grep -qE "$ext" <<< 'scripts/test-land-path-checks.sh' || bad "fixed regex does not match scripts/test-land-path-checks.sh"
ok "land.sh self-test trigger regex matches land.sh and the shared-list paths"

# 4. both entry points actually call the shared driver.
grep -q 'land-path-checks.sh' "$here/c8-checks.sh" || bad "c8-checks.sh does not invoke land-path-checks.sh"
grep -q 'main_run scripts/land-path-checks.sh' "$repo/scripts/land.sh" || bad "land.sh does not main_run land-path-checks.sh"
ok "land.sh and c8-checks.sh both read the shared list via the driver"

if [ "$fail" -ne 0 ]; then
  say "test-land-path-checks: FAILED"
  exit 1
fi
say "test-land-path-checks: ok"
