#!/usr/bin/env bash
# D-0280(b): every source-scanning gate ignores untracked files under .oracle/
# and still catches the same trigger when that file is tracked.
# Usage: scripts/test-source-scan-tracked.sh [repo-root]
set -euo pipefail
runner="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
root="$(cd "${1:-$runner/..}" && pwd)"
fail=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fail=1; }
# Split so this file itself is not a scanner hit.
ws='/work''space/evil'
shim_tag='SHIM''(''scan-plant)'
pipe_grep='echo x | gre''p -q y'
pipe_head='echo x | hea''d -1'

# --- Part A: plant trigger literals in the real repo's gitignored .oracle/ ---
plant_dir="$root/.oracle/scan-plant"
cleanup() { rm -rf "$plant_dir"; }
trap cleanup EXIT
mkdir -p "$plant_dir"
printf '%s\n' "const p = \"$ws\";" "$shim_tag" 'fn js_ws() {}' > "$plant_dir/plant.rs"
printf '%s\n' "const p = \"$ws\";" "$shim_tag" > "$plant_dir/plant.js"
printf '%s\n' "$pipe_grep" "$pipe_head" > "$plant_dir/plant.sh"
printf '%s\n' "writeFileSync('goldens/x.json', '');" 'process.env.UPDATE_GOLDENS' > "$plant_dir/plant.ts"
# control: these files must not appear in git ls-files
if "$runner/tracked-files.sh" "$root" | tr '\0' '\n' | grep -F '.oracle/scan-plant'; then
  bad "tracked-files listed .oracle/scan-plant"
else
  ok "tracked-files omits untracked .oracle/scan-plant"
fi

rc=0
"$runner/workspace-path-scan.sh" "$root" >/tmp/scan-plant-wps.out 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then ok "workspace-path-scan ignores .oracle plant"; else bad "workspace-path-scan rc=$rc on .oracle plant"; sed 's/^/       /' /tmp/scan-plant-wps.out; fi

rc=0
"$runner/lint-shell-pipes.sh" "$root" >/tmp/scan-plant-lsp.out 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then ok "lint-shell-pipes ignores .oracle plant"; else bad "lint-shell-pipes rc=$rc on .oracle plant"; sed 's/^/       /' /tmp/scan-plant-lsp.out; fi

rc=0
"$root/harness/scripts/golden-io-guard.sh" "$root/harness" >/tmp/scan-plant-gio.out 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then ok "golden-io-guard ignores .oracle plant"; else bad "golden-io-guard rc=$rc on .oracle plant"; sed 's/^/       /' /tmp/scan-plant-gio.out; fi

# --- Part B: scratch git repo proves a tracked copy of the same literal is caught ---
t="$(mktemp -d /tmp/source-scan-tracked.XXXXXX)"
trap 'rm -rf "$plant_dir" "$t"' EXIT
mkdir -p "$t/crates" "$t/tools" "$t/harness" "$t/scripts"
cp "$runner/workspace-path-scan.sh" "$runner/tracked-files.sh" "$runner/workspace-path-allowlist.tsv" "$t/scripts/"
# allowlist mentions the scanner itself; copy it so the self path matches
printf '%s\n' '.oracle/' > "$t/.gitignore"
printf '%s\n' 'fn ok() {}' > "$t/crates/ok.rs"
git -C "$t" init -q
git -C "$t" add crates tools harness scripts .gitignore
mkdir -p "$t/.oracle/lwc"
printf '%s\n' "const p = \"$ws\";" > "$t/.oracle/lwc/plant.js"
rc=0
"$t/scripts/workspace-path-scan.sh" "$t" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then ok "control repo: only .oracle plant is ignored"; else bad "control repo ignored-oracle rc=$rc (want 0)"; fi
printf '%s\n' "const p = \"$ws\";" > "$t/crates/evil.rs"
git -C "$t" add crates/evil.rs
rc=0
"$t/scripts/workspace-path-scan.sh" "$t" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 1 ]; then ok "control: same literal in a tracked file is caught"; else bad "control tracked-literal rc=$rc (want 1)"; fi

if [ "$fail" -eq 0 ]; then
  echo "test-source-scan-tracked: ok"
  exit 0
fi
echo "test-source-scan-tracked: FAILED" >&2
exit 1
