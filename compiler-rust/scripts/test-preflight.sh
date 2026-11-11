#!/usr/bin/env bash
# Plants for scripts/preflight.sh (rootcause fix 1). Uses a detached worktree at this checkout's HEAD
# as <base> and commits plants on top of it. Removes the worktree (no --force) on exit.
#   1. a harmless change passes (rc 0, every check ok/skip);
#   2. two independent failures (a literal /workspace path and a vitest error mute) are both reported in one run;
#   3. gate code from the incoming tree is ignored: the tree neuters workspace-path-scan.sh, base's runs;
#   4. an unformatted .rs file (D-0235) fails row path:cargo-fmt and the board's C8 pre-check (c8-checks.sh).
set -uo pipefail
# Under land.sh this runs inside the outer preflight, which exports PREFLIGHT_GATE_DIR (main's gate
# archive). Inherited, it makes every inner preflight below skip its own archive and run MAIN's gate
# code instead of this checkout's: drop it so <base> = this HEAD really judges the plants.
unset PREFLIGHT_GATE_DIR
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d /tmp/test-preflight.XXXXXX)"
W="$T/wt"
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }
git -C "$ROOT" worktree add --detach "$W" HEAD >/dev/null 2>&1 || { echo "FAIL cannot add worktree"; exit 1; }
cleanup() { git -C "$W" reset -q --hard; git -C "$W" clean -q -fd; git -C "$ROOT" worktree remove "$W"; rm -rf "$T"; }
trap cleanup EXIT
base="$(git -C "$W" rev-parse HEAD)"
commit() { git -C "$W" -c user.name=t -c user.email=t@t commit -q -m "$1"; }
run() { # name -> rc in $T/<name>.rc, output in $T/<name>.out
  local rc=0
  "$W/scripts/preflight.sh" --base "$base" --tree "$W" > "$T/$1.out" 2>&1 || rc=$?
  echo "$rc" > "$T/$1.rc"
}
row() { awk -v c="$2" '$1 == c { print $2 }' "$T/$1.out" | tail -1; }

# 1. harmless change
printf '\npreflight plant\n' >> "$W/harness/README.md"
git -C "$W" add harness/README.md; commit 'plant: harmless'
run clean
if [ "$(cat "$T/clean.rc")" = 0 ] && ! grep -qE '^[a-z:-]+ +FAIL' "$T/clean.out"; then ok "harmless change passes (rc 0)"
else bad "harmless change rc=$(cat "$T/clean.rc")"; sed -n '/^preflight: .* vs base/,$p' "$T/clean.out" | sed 's/^/       /'; fi
git -C "$W" reset -q --hard "$base"

# 2. two independent failures in one run
printf 'p="%s"\n' "/work""space/preflight-plant" > "$W/scripts/preflight-plant.sh"
printf 'export default { test: { %s: true } };\n' "dangerously""IgnoreUnhandledErrors" > "$W/harness/preflight-plant.ts"
git -C "$W" add scripts/preflight-plant.sh harness/preflight-plant.ts; commit 'plant: two failures'
run two
if [ "$(cat "$T/two.rc")" != 0 ] && [[ "$(row two path:workspace-path-scan)" == FAIL* ]] && [[ "$(row two path:lint-no-error-mute)" == FAIL* ]]; then
  ok "both failures reported in one run (path:workspace-path-scan and path:lint-no-error-mute FAIL, rc $(cat "$T/two.rc"))"
  sed -n '/^preflight: .* vs base/,$p' "$T/two.out" | sed 's/^/       /'
else bad "two failures: rc=$(cat "$T/two.rc")"; sed -n '/^preflight: .* vs base/,$p' "$T/two.out" | sed 's/^/       /'; fi
git -C "$W" reset -q --hard "$base"; git -C "$W" clean -q -fd

# 3. incoming gate code is ignored
sed -i '0,/^set -euo pipefail$/s//set -euo pipefail\necho "workspace-path-scan: clean"; exit 0  # plant: neutered/' "$W/scripts/workspace-path-scan.sh"
printf 'p="%s"\n' "/work""space/preflight-plant" > "$W/scripts/preflight-plant.sh"
git -C "$W" add scripts/workspace-path-scan.sh scripts/preflight-plant.sh; commit 'plant: neuter the scanner'
"$W/scripts/workspace-path-scan.sh" "$W" > "$T/neutered.out" 2>&1 && grep -q 'clean' "$T/neutered.out" || bad "plant did not neuter the tree's scanner"
run neuter
if [[ "$(row neuter path:workspace-path-scan)" == FAIL* ]] && grep -q 'preflight-plant.sh' "$T/neuter.out"; then
  ok "tree's neutered workspace-path-scan.sh is ignored; base's scanner still fails the plant"
else bad "neutered scanner: rc=$(cat "$T/neuter.rc")"; sed -n '/^preflight: .* vs base/,$p' "$T/neuter.out" | sed 's/^/       /'; fi

git -C "$W" reset -q --hard "$base"; git -C "$W" clean -q -fd

# 4. unformatted Rust (D-0235): preflight row and the board's C8 pre-check both fail
printf 'fn  preflight_fmt_plant( ){}\n' >> "$W/crates/lwc-shared/src/lib.rs"
git -C "$W" add crates/lwc-shared/src/lib.rs; commit 'plant: unformatted rust'
run fmt
if [ "$(cat "$T/fmt.rc")" != 0 ] && [[ "$(row fmt path:cargo-fmt)" == FAIL* ]] && grep -qE '^path:cargo-fmt +FAIL.*unformatted Rust in 1 file.*crates/lwc-shared/src/lib.rs' "$T/fmt.out"; then
  ok "unformatted .rs fails preflight: $(grep -E '^path:cargo-fmt ' "$T/fmt.out" | tr -s ' ')"
else bad "unformatted .rs: rc=$(cat "$T/fmt.rc")"; sed -n '/^preflight: .* vs base/,$p' "$T/fmt.out" | sed 's/^/       /'; fi
rc=0; "$W/scripts/c8-checks.sh" --skip-pack "$W" > "$T/c8.out" 2>&1 || rc=$?
if [ "$rc" -ne 0 ] && grep -q 'cargo-fmt-check: unformatted Rust in 1 file' "$T/c8.out" && grep -q '!! C8 FAILED: land-path-checks' "$T/c8.out"; then
  ok "unformatted .rs fails the board's C8 pre-check (c8-checks.sh rc $rc, before the workspace check)"
else bad "c8 fmt plant rc=$rc"; tail -15 "$T/c8.out" | sed 's/^/       /'; fi

[ "$fail" -eq 0 ] && { echo "test-preflight: ok"; exit 0; }
echo "test-preflight: FAILED"; exit 1
