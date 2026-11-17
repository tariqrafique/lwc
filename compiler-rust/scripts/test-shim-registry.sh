#!/usr/bin/env bash
# Hermetic self-test for harness/scripts/shim-gate.mjs SHIM registry (AB-L0072).
# Verdicts (rc 1): tracked/untracked/stale/malformed/tag-mismatch/TODO(parity/owner-not-in-lanes.
# Internal errors (rc 2): missing registry, empty SHIM_REGISTRY, directory/unreadable registry,
# bad CRATES_ROOT, missing/empty/malformed OWNERS. Also: repo-wide scan; board.sh plant.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GATE="$ROOT/harness/scripts/shim-gate.mjs"
T="$(mktemp -d /tmp/shimreg-test.XXXXXX)"; trap 'rm -rf "$T"' EXIT
fail=0
check() {
  local name="$1" want="$2"; shift 2
  local rc=0; "$@" >"$T/out" 2>&1 || rc=$?
  if [ "$rc" = "$want" ]; then
    echo "ok   $name -> rc $rc"
  else
    echo "FAIL $name: want rc $want got $rc"; sed 's/^/  | /' "$T/out"; fail=1
  fi
}

# Exclusion stubs required by MAIN's closed list (must exist under the scanned repo).
plant_exclusions() {
  local r="$1"
  mkdir -p "$r/harness/scripts" "$r/scripts" "$r/harness/ratchets" \
    "$r/crates/lwc-ssr/src/compile_js" "$r/crates/foo/src" "$r/tools"
  : > "$r/harness/scripts/shim-gate.mjs"
  : > "$r/scripts/test-shim-registry.sh"
  : > "$r/harness/README.md"
  : > "$r/harness/decisions.snapshot.tsv"
  # board.sh is NOT excluded — leave it empty (no tag-open sequence) unless a test plants one.
  : > "$r/harness/board.sh"
  echo 'fn x() {}' > "$r/crates/lwc-ssr/src/compile_js/mod.rs"
  echo 0 > "$r/harness/ratchets/ssr-compile-js-scanners.txt"
  printf '# path\tword\treason\n' > "$r/harness/shim-allowlist.tsv"
  printf '# path(repo-relative)\ttag\towner\texpiry\treason\n' > "$r/harness/shim-registry.tsv"
}

# Minimal MAIN OWNERS (closed lanes). Branch OWNERS must not affect the gate.
main_owners_ok() {
  printf '%s\n' \
    '# test OWNERS' \
    $'=js\tjs' \
    $'crates/foo/*\ttemplate' \
    $'harness/*\tharness' \
    $'*\t*' \
    > "$T/main/OWNERS"
}

setup() {
  rm -rf "$T/tree" "$T/main"
  mkdir -p "$T/main" "$T/tree"
  plant_exclusions "$T/tree"
  main_owners_ok
  # Branch OWNERS with a fake lane — must be ignored.
  printf '%s\n' $'crates/foo/*\tbranch-only-lane\n*\t*\n' > "$T/tree/OWNERS"
  ( cd "$T/tree" && git init -q -b main && git config user.email t@t && git config user.name t
    git add -A && git commit -qm init )
}

hdr='# path(repo-relative)	tag(exact text inside SHIM(...))	owner	expiry	reason'

write_reg() {
  printf '%s\n' "$hdr" "$@" > "$T/tree/harness/shim-registry.tsv"
  ( cd "$T/tree" && git add -A && git commit -qm reg >/dev/null )
}

run_gate() {
  CRATES_ROOT="$T/tree/crates" \
    SHIM_ALLOWLIST="$T/tree/harness/shim-allowlist.tsv" \
    SHIM_REGISTRY="$T/tree/harness/shim-registry.tsv" \
    SHIM_OWNERS="$T/main/OWNERS" \
    SHIM_REPO_ROOT="$T/tree" \
    node "$GATE"
}

# ---- verdicts (rc 0 / 1) ----

setup
printf '%s\n' '/// SHIM(AB-D0004 S2, expires S6): tracked' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\tAB-D0004 S2, expires S6\ttemplate\tS6\tacorn re-lexer'
check "tracked passes" 0 run_gate
grep -q 'shim-registry: tags=1 rows=1 untracked=0 stale=0 (GATING)' "$T/out" \
  || { echo "FAIL tracked summary"; sed 's/^/  | /' "$T/out"; fail=1; }

setup
printf '%s\n' '/// SHIM(AB-D0004 S2, expires S6): no row' > "$T/tree/crates/foo/src/lib.rs"
write_reg
check "untracked fails" 1 run_gate
grep -q 'UNTRACKED crates/foo/src/lib.rs:1 AB-D0004 S2, expires S6' "$T/out" \
  || { echo "FAIL untracked message"; sed 's/^/  | /' "$T/out"; fail=1; }

setup
printf '%s\n' 'fn x() {}' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\tAB-D0004 S2, expires S6\ttemplate\tS6\tgone'
check "stale fails" 1 run_gate
grep -q 'STALE crates/foo/src/lib.rs AB-D0004 S2, expires S6' "$T/out" \
  || { echo "FAIL stale message"; sed 's/^/  | /' "$T/out"; fail=1; }

setup
printf '%s\n' 'fn x() {}' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\tsome-tag\ttemplate\tsoon\treason'
check "malformed expiry fails" 1 run_gate

setup
printf '%s\n' 'fn x() {}' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\tsome-tag\ttemplate\tAB-XX1234\treason'
check "malformed expiry AB-XX fails" 1 run_gate

setup
printf '%s\n' 'fn x() {}' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\tsome-tag\ttemplate\tS6\treason\textra'
check "extra tab fails" 1 run_gate

setup
printf '%s\n' 'fn x() {}' > "$T/tree/crates/foo/src/lib.rs"
printf '%s\r\n' "$hdr" $'crates/foo/src/lib.rs\tsome-tag\ttemplate\tS6\treason' \
  > "$T/tree/harness/shim-registry.tsv"
( cd "$T/tree" && git add -A && git commit -qm crlf >/dev/null )
check "CRLF fails" 1 run_gate

setup
printf '%s\n' 'fn x() {}' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\tsome-tag\ttemplate\tS6'
check "malformed missing field fails" 1 run_gate

setup
printf '%s\n' '/// SHIM(actual-tag): x' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\tother-tag\ttemplate\tS6\treason'
check "tag mismatch fails" 1 run_gate

setup
printf '%s\n' '// TODO(parity: still a gap)' > "$T/tree/crates/foo/src/lib.rs"
write_reg
check "TODO(parity count-only passes" 0 run_gate
grep -q 'shim-registry: tags=0 rows=0 untracked=0 stale=0 (GATING)' "$T/out" \
  || { echo "FAIL TODO(parity registry"; sed 's/^/  | /' "$T/out"; fail=1; }

# Owner not in MAIN lanes -> INVALID rc 1 (branch-only-lane is in branch OWNERS, not MAIN)
setup
printf '%s\n' '/// SHIM(owned): x' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\towned\tbranch-only-lane\tS6\treason'
check "owner not in MAIN lanes -> INVALID" 1 run_gate
grep -q 'REGISTRY-INVALID' "$T/out" || { echo "FAIL owner-not-in-lanes msg"; sed 's/^/  | /' "$T/out"; fail=1; }

setup
printf '%s\n' '/// SHIM(owned): x' > "$T/tree/crates/foo/src/lib.rs"
write_reg $'crates/foo/src/lib.rs\towned\ttemplate\tS6\treason'
check "MAIN OWNERS known owner passes" 0 run_gate

# repo-wide non-excluded
setup
printf '%s\n' '// SHIM(tools-tag): planted' > "$T/tree/tools/planted.mjs"
write_reg
check "repo-wide non-excluded SHIM( seen" 1 run_gate
grep -q 'UNTRACKED tools/planted.mjs:1 tools-tag' "$T/out" \
  || { echo "FAIL repo-wide untracked"; sed 's/^/  | /' "$T/out"; fail=1; }

setup
printf '%s\n' '// SHIM(docs-only): in README' > "$T/tree/harness/README.md"
write_reg
check "excluded file SHIM( ignored" 0 run_gate

# board.sh is NOT excluded: a planted SHIM( is UNTRACKED
setup
printf '%s\n' '# planted SHIM(board-plant): must be seen' > "$T/tree/harness/board.sh"
write_reg
check "board.sh planted SHIM( is UNTRACKED" 1 run_gate
grep -q 'UNTRACKED harness/board.sh:1 board-plant' "$T/out" \
  || { echo "FAIL board.sh plant"; sed 's/^/  | /' "$T/out"; fail=1; }

# ---- internal errors (rc 2) ----

setup
printf '%s\n' 'fn x() {}' > "$T/tree/crates/foo/src/lib.rs"
( cd "$T/tree" && git add -A && git commit -qm x >/dev/null )
check "missing registry" 2 \
  env CRATES_ROOT="$T/tree/crates" SHIM_ALLOWLIST="$T/tree/harness/shim-allowlist.tsv" \
      SHIM_REGISTRY="$T/tree/harness/no-such-registry.tsv" SHIM_OWNERS="$T/main/OWNERS" \
      SHIM_REPO_ROOT="$T/tree" node "$GATE"

setup
write_reg
check "empty SHIM_REGISTRY" 2 \
  env CRATES_ROOT="$T/tree/crates" SHIM_ALLOWLIST="$T/tree/harness/shim-allowlist.tsv" \
      SHIM_REGISTRY= SHIM_OWNERS="$T/main/OWNERS" SHIM_REPO_ROOT="$T/tree" node "$GATE"

setup
rm -f "$T/tree/harness/shim-registry.tsv"
mkdir -p "$T/tree/harness/shim-registry.tsv"
# exclusion assert needs the path to exist as the documented file — but it's a dir now, which
# fails as registry-is-directory before exclusion? Order: registry check first. Good.
check "registry directory" 2 \
  env CRATES_ROOT="$T/tree/crates" SHIM_ALLOWLIST="$T/tree/harness/shim-allowlist.tsv" \
      SHIM_REGISTRY="$T/tree/harness/shim-registry.tsv" SHIM_OWNERS="$T/main/OWNERS" \
      SHIM_REPO_ROOT="$T/tree" node "$GATE"

setup
write_reg
check "bad CRATES_ROOT" 2 \
  env CRATES_ROOT="$T/tree/crates-MISSING" SHIM_ALLOWLIST="$T/tree/harness/shim-allowlist.tsv" \
      SHIM_REGISTRY="$T/tree/harness/shim-registry.tsv" SHIM_OWNERS="$T/main/OWNERS" \
      SHIM_REPO_ROOT="$T/tree" node "$GATE"

setup
write_reg
if [ "$(id -u)" != 0 ]; then
  chmod 000 "$T/tree/harness/shim-registry.tsv"
  check "unreadable registry" 2 \
    env CRATES_ROOT="$T/tree/crates" SHIM_ALLOWLIST="$T/tree/harness/shim-allowlist.tsv" \
        SHIM_REGISTRY="$T/tree/harness/shim-registry.tsv" SHIM_OWNERS="$T/main/OWNERS" \
        SHIM_REPO_ROOT="$T/tree" node "$GATE"
  chmod 644 "$T/tree/harness/shim-registry.tsv"
else
  echo "ok   unreadable registry -> rc 2 (skipped as root)"
fi

# missing OWNERS -> rc 2
setup
write_reg
rm -f "$T/main/OWNERS"
check "missing OWNERS" 2 run_gate
grep -q 'REGISTRY-ERROR' "$T/out" || { echo "FAIL missing OWNERS msg"; sed 's/^/  | /' "$T/out"; fail=1; }

# empty OWNERS -> rc 2
setup
write_reg
: > "$T/main/OWNERS"
check "empty OWNERS" 2 run_gate

# malformed OWNERS line (<2 columns) -> rc 2
setup
write_reg
printf '%s\n' '# ok' 'notabs-here' $'crates/foo/*\ttemplate' > "$T/main/OWNERS"
check "malformed OWNERS line" 2 run_gate
grep -q 'OWNERS malformed' "$T/out" || { echo "FAIL malformed OWNERS msg"; sed 's/^/  | /' "$T/out"; fail=1; }

# ---- SHIM_FILE_LIST (official-board git-archive trees, D-0273) ----
setup
printf '%s\n' '/// SHIM(AB-D0004 S2, expires S6): only in git' > "$T/tree/crates/foo/src/lib.rs"
printf '%s\n' '/// SHIM(AB-D0004 S2, expires S6): from list' > "$T/tree/crates/foo/src/fromlist.rs"
write_reg $'crates/foo/src/fromlist.rs\tAB-D0004 S2, expires S6\ttemplate\tS6\tlist-only'
printf 'crates/foo/src/fromlist.rs\0' > "$T/list"
check "SHIM_FILE_LIST overrides git ls-files" 0 \
  env CRATES_ROOT="$T/tree/crates" SHIM_ALLOWLIST="$T/tree/harness/shim-allowlist.tsv" \
      SHIM_REGISTRY="$T/tree/harness/shim-registry.tsv" SHIM_OWNERS="$T/main/OWNERS" \
      SHIM_REPO_ROOT="$T/tree" SHIM_FILE_LIST="$T/list" node "$GATE"
check "SHIM_FILE_LIST missing is rc 2" 2 \
  env CRATES_ROOT="$T/tree/crates" SHIM_ALLOWLIST="$T/tree/harness/shim-allowlist.tsv" \
      SHIM_REGISTRY="$T/tree/harness/shim-registry.tsv" SHIM_OWNERS="$T/main/OWNERS" \
      SHIM_REPO_ROOT="$T/tree" SHIM_FILE_LIST="$T/no-such-list" node "$GATE"

exit $fail
