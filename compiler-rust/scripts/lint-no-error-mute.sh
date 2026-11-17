#!/usr/bin/env bash
# No global error mute in the test/board harness (rootcause report fix 3, Migrate 2026-11-10 23:20).
# Bans, in every tracked file of the tree, with NO allowlist:
#   M1  dangerouslyIgnoreUnhandledErrors          (vitest: ignore every unhandled error in the run)
#   M2  LWC_VITEST_IGNORE_UNHANDLED               (the env switch that turned M1 on)
#   M3  onUnhandledError that returns false       (vitest: same effect as M1)
# A component callback throw that a test expects must be caught where it happens (e.g. a window
# 'error' listener that records it), so an unexpected throw still fails the run.
# Only this script is excluded (it names the patterns; its plants are built in a temp dir).
# Usage: scripts/lint-no-error-mute.sh [<repo-root>] | --self-test
# Exit: 0 clean; 1 banned pattern found; 2 cannot run.
set -euo pipefail
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
SELF_REL="scripts/lint-no-error-mute.sh"
command -v rg >/dev/null || { echo "lint-no-error-mute: rg (ripgrep) is required" >&2; exit 2; }

RULES=$(cat <<'R'
M1	dangerouslyIgnoreUnhandledErrors	dangerouslyIgnoreUnhandledErrors
M2	LWC_VITEST_IGNORE_UNHANDLED	LWC_VITEST_IGNORE_UNHANDLED
M3	onUnhandledError returning false	onUnhandledError\s*(?::\s*(?:async\s+)?(?:function\b[^{]*|\([^)]*\)\s*=>\s*|\w+\s*=>\s*)|\([^)]*\)\s*)(?:false\b|\{[^}]*\breturn\s+false\b)
R
)

# scan <root>: prints "rule<TAB>file:line: text" per hit; returns 2 if it cannot run.
scan() {
  local root="$1" list id what re out rc
  list="$(mktemp /tmp/lint-no-error-mute.XXXXXX)"
  if ! "$here/tracked-files.sh" "$root" > "$list.all"; then
    rm -f "$list" "$list.all"; echo "lint-no-error-mute: cannot list tracked files under $root" >&2; return 2
  fi
  tr '\0' '\n' < "$list.all" | grep -vxF -- "$SELF_REL" | tr '\n' '\0' > "$list" || true
  rm -f "$list.all"
  while IFS=$'\t' read -r id what re; do
    [ -n "$id" ] || continue
    rc=0
    # rg exit 1 = no hit in that batch; anything >= 2 is an error -> 255 stops xargs (exit 124).
    out="$(cd "$root" && xargs -0 -r sh -c 'rg -n -U --no-heading --color never --no-config -e "$0" -- "$@"; r=$?; [ "$r" -le 1 ] || exit 255' "$re" < "$list")" || rc=$?
    case "$rc" in 0|123) ;; *) rm -f "$list"; echo "lint-no-error-mute: rg failed (rc=$rc) on rule $id" >&2; return 2 ;; esac
    [ -z "$out" ] || while IFS= read -r hit; do printf '%s\t%s\t%s\n' "$id" "$what" "$hit"; done <<< "$out"
  done <<< "$RULES"
  rm -f "$list"
}

lint() {
  local root="$1" hits src=0
  hits="$(scan "$root")" || src=$?
  [ "$src" -eq 0 ] || return 2
  if [ -n "$hits" ]; then
    echo "!! lint-no-error-mute: global error mute is banned (no allowlist). Catch the expected throw where it happens and let anything else fail the run:" >&2
    awk -F'\t' '{ print "  " $3 "  [" $1 " " $2 "]" }' <<< "$hits" >&2
    return 1
  fi
  echo "lint-no-error-mute: clean"
}

self_test() {
  local t fail=0 rc name
  t="$(mktemp -d /tmp/lint-no-error-mute-st.XXXXXX)"
  trap 'rm -rf "$t"' RETURN
  mkdir -p "$t/scripts" "$t/harness/test"
  cp "$here/lint-no-error-mute.sh" "$here/tracked-files.sh" "$t/scripts/"
  # Near-misses that must stay clean: a narrow handler that records and rethrows.
  cat > "$t/harness/test/ok.test.ts" << 'F'
window.addEventListener('error', (e) => { e.preventDefault(); note(e.error); });
export default { test: { onUnhandledError(err) { if (expected(err)) return true; throw err; } } };
const unhandled = 'IgnoreUnhandled';
F
  git -C "$t" init -q; git -C "$t" add scripts harness
  rc=0; "$t/scripts/lint-no-error-mute.sh" "$t" > "$t/out" 2>&1 || rc=$?
  if [ "$rc" -eq 0 ]; then echo "ok   clean tree (and this script naming the patterns) passes"; else echo "FAIL clean tree rc=$rc"; sed 's/^/       /' "$t/out"; fail=1; fi
  for name in M1 M2 M3a M3b M3c; do
    case "$name" in
      M1) printf '%s\n' "export default { test: { dangerouslyIgnoreUnhandledErrors: true } };" > "$t/harness/plant.ts" ;;
      M2) printf '%s\n' 'export LWC_VITEST_IGNORE_UNHANDLED=1' > "$t/harness/plant.ts" ;;
      M3a) printf '%s\n' "export default { test: { onUnhandledError: () => false } };" > "$t/harness/plant.ts" ;;
      M3b) printf '%s\n' "export default { test: {" "  onUnhandledError(err) {" "    log(err);" "    return false;" "  } } };" > "$t/harness/plant.ts" ;;
      M3c) printf '%s\n' "export default { test: { onUnhandledError: function (e) { return false; } } };" > "$t/harness/plant.ts" ;;
    esac
    git -C "$t" add harness/plant.ts
    rc=0; "$t/scripts/lint-no-error-mute.sh" "$t" > "$t/out" 2>&1 || rc=$?
    if [ "$rc" -eq 1 ] && grep -q "harness/plant.ts" "$t/out" && grep -q "\[${name:0:2} " "$t/out"; then echo "ok   $name plant fails"; else echo "FAIL $name plant rc=$rc (want 1)"; sed 's/^/       /' "$t/out"; fail=1; fi
  done
  return "$fail"
}

case "${1:-}" in
  --self-test) self_test ;;
  -h|--help) sed -n '2,11p' "$0" ;;
  -*) echo "usage: lint-no-error-mute.sh [<repo-root>] | --self-test" >&2; exit 2 ;;
  *) lint "$(cd "${1:-$here/..}" && pwd)" ;;
esac
