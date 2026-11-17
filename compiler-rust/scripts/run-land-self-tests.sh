#!/usr/bin/env bash
# Path-mapped land self-tests. Selects names from scripts/land-self-tests.tsv
# whose path column matches a changed file (exact, or a directory prefix
# ending in /). Independent names run in parallel (LAND_SELF_TEST_JOBS, default 4).
# Only gate files select (rootcause fix 5): changed paths under scripts/ or harness/scripts/.
# Self-tests test the tree's gate code, and a branch's gate code has no effect on its own land, so
# an agent change outside the gate files (harness/test, harness/board.sh, goldens, crates) runs none.
# Main-side gate commits run --all before they commit.
# Usage:
#   run-land-self-tests.sh --from-stdin          # changed paths on stdin
#   run-land-self-tests.sh --changed-file FILE
#   run-land-self-tests.sh --list [--from-stdin|--changed-file FILE]
#   run-land-self-tests.sh --all                 # every name (none is skipped)
# test-board-ratchet and test-shared-cargo-cache run against the PINNED napi: the board's
# $CARGO_TARGET_DIR/release/liblwc_compiler_napi.so, with CARGO_TARGET_DIR keyed by Cargo.lock +
# rust-toolchain.toml exactly as official-board.sh / land.sh compute it. Missing, not the expected
# LWC_SELF_TEST_NAPI_SHA (sha256 first 12, optional), or changed during the run -> those tests FAIL.
# LWC_SELF_TEST_NAPI_TARGET overrides the target dir (plants only).
#   run-land-self-tests.sh --check-map
set -euo pipefail
SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SELF_DIR/.." && pwd)"
MAP="${LAND_SELF_TESTS_TSV:-$SELF_DIR/land-self-tests.tsv}"
JOBS="${LAND_SELF_TEST_JOBS:-4}"
LIST_ONLY=0
ALL=0
CHECK_MAP=0
FROM_STDIN=0
CHANGED_FILE=""

usage() { echo "usage: run-land-self-tests.sh --from-stdin|--changed-file FILE|--all|--check-map [--list]" >&2; exit 2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --from-stdin) FROM_STDIN=1; shift ;;
    --changed-file) CHANGED_FILE="${2:-}"; shift 2 ;;
    --list) LIST_ONLY=1; shift ;;
    --all) ALL=1; shift ;;
    --check-map) CHECK_MAP=1; shift ;;
    -h|--help) usage ;;
    *) usage ;;
  esac
done

[ -f "$MAP" ] || { echo "run-land-self-tests: missing $MAP" >&2; exit 2; }

# name -> argv (first wins)
declare -A ARGV=()
# name -> 1 if any path equals its implementing script
declare -A HAS_SELF=()
# ordered unique names
NAMES=()

while IFS=$'\t' read -r name argv path || [ -n "${name:-}" ]; do
  [ -n "${name:-}" ] || continue
  case "$name" in '#'*) continue ;; esac
  [ -n "${argv:-}" ] && [ -n "${path:-}" ] || { echo "run-land-self-tests: malformed row: $name" >&2; exit 2; }
  if [ -z "${ARGV[$name]+x}" ]; then
    ARGV[$name]="$argv"
    NAMES+=("$name")
  elif [ "${ARGV[$name]}" != "$argv" ]; then
    echo "run-land-self-tests: name $name has conflicting argv" >&2
    exit 2
  fi
  impl="${argv%% *}"
  if [ "$argv" = "@check-land-main-refs" ]; then impl="scripts/check-land-main-refs.sh"
  elif [ "$argv" = "@lint-no-stripped-compare" ]; then impl="scripts/lint-no-stripped-compare.sh"
  fi
  if [ "$path" = "$impl" ]; then HAS_SELF[$name]=1; fi
done < "$MAP"

needs_napi() { [ "$1" = "test-board-ratchet" ] || [ "$1" = "test-shared-cargo-cache" ]; }

# Resolve and pin the board's napi (see header). Prints why on failure.
napi_target() {
  local lk tk ck
  lk="$(git -C "$ROOT" show HEAD:Cargo.lock | sha256sum | awk '{print $1}')" || return 1
  tk="$(git -C "$ROOT" show HEAD:rust-toolchain.toml | sha256sum | awk '{print $1}')" || return 1
  ck="$(printf '%s%s' "$lk" "$tk" | sha256sum | cut -c1-16)"
  printf '%s\n' "${LWC_SELF_TEST_NAPI_TARGET:-/workspace/cache/lwc-target/$ck}"
}
pin_napi() {
  local dir pins tmpf
  dir="$(napi_target)" || { echo "!! run-land-self-tests: cannot resolve the board's CARGO_TARGET_DIR (Cargo.lock / rust-toolchain.toml at HEAD)"; return 1; }
  NAPI_SO="$dir/release/liblwc_compiler_napi.so"
  if [ ! -f "$NAPI_SO" ]; then
    echo "!! run-land-self-tests: pinned napi MISSING: $NAPI_SO (the board's napi for this Cargo.lock+toolchain; build it with official-board.sh or a crate land). Not skipped."
    return 1
  fi
  NAPI_SHA="$(sha256sum "$NAPI_SO" | cut -c1-12)"
  if [ -n "${LWC_SELF_TEST_NAPI_SHA:-}" ] && [ "$NAPI_SHA" != "$LWC_SELF_TEST_NAPI_SHA" ]; then
    echo "!! run-land-self-tests: pinned napi MISMATCH: $NAPI_SO is sha256:$NAPI_SHA, want $LWC_SELF_TEST_NAPI_SHA"
    return 1
  fi
  pins="$1/.pins"; mkdir -p "$pins"
  LWC_NAPI_PIN="$pins/lwc-compiler-napi.$NAPI_SHA.node"
  tmpf="$(mktemp "$pins/.tmp.XXXXXX")"; cp "$NAPI_SO" "$tmpf"; chmod 0555 "$tmpf"; mv -f "$tmpf" "$LWC_NAPI_PIN"
  [ "$(sha256sum "$LWC_NAPI_PIN" | cut -c1-12)" = "$NAPI_SHA" ] || { echo "!! run-land-self-tests: pinned napi MISMATCH: copy of $NAPI_SO does not hash to $NAPI_SHA"; return 1; }
  export LWC_NAPI_PIN LWC_NAPI_SHA="$NAPI_SHA"
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$dir}"
  echo "land-self-tests: napi sha256:$NAPI_SHA pinned=$LWC_NAPI_PIN (from $NAPI_SO)"
}

check_map() {
  local fail=0 name impl
  for name in "${NAMES[@]}"; do
    if [ -z "${HAS_SELF[$name]+x}" ]; then
      echo "FAIL $name does not list its own implementing script as a path"
      fail=1
    fi
    impl="${ARGV[$name]}"
    case "$impl" in
      @check-land-main-refs|@lint-no-stripped-compare) ;;
      *)
        impl="${impl%% *}"
        [ -f "$ROOT/$impl" ] || { echo "FAIL $name argv script missing: $impl"; fail=1; }
        ;;
    esac
  done
  return "$fail"
}

if [ "$CHECK_MAP" -eq 1 ]; then
  if check_map; then
    echo "ok   land-self-tests map ($((${#NAMES[@]})) names)"
    exit 0
  fi
  echo "run-land-self-tests: map check failed"
  exit 1
fi

select_from_changed() {
  local changed name argv path
  declare -A sel=()
  changed="$(grep -E '^(scripts|harness/scripts)/' <<< "$1" || true)"
  echo "run-land-self-tests: $(grep -c . <<< "$1" || true) changed path(s), $(grep -c . <<< "$changed" || true) gate file(s)" >&2
  while IFS= read -r file || [ -n "${file:-}" ]; do
    [ -n "${file:-}" ] || continue
    if [ "$file" = "scripts/test-land-classify.sh" ]; then
      for name in "${NAMES[@]}"; do
        sel[$name]=1
      done
    fi
  done <<< "$changed"
  while IFS=$'\t' read -r name argv path || [ -n "${name:-}" ]; do
    [ -n "${name:-}" ] || continue
    case "$name" in '#'*) continue ;; esac
    while IFS= read -r file || [ -n "${file:-}" ]; do
      [ -n "${file:-}" ] || continue
      if [ "$path" = "$file" ]; then sel[$name]=1; break; fi
      if [[ "$path" == */ ]] && [[ "$file" == "$path"* ]]; then sel[$name]=1; break; fi
    done <<< "$changed"
  done < "$MAP"
  for name in "${NAMES[@]}"; do
    [ -n "${sel[$name]+x}" ] && printf '%s\n' "$name"
  done
  return 0
}

CHANGED=""
if [ "$ALL" -eq 1 ]; then
  for name in "${NAMES[@]}"; do
    CHANGED+="$name"$'\n'
  done
elif [ "$FROM_STDIN" -eq 1 ]; then
  files="$(cat)"
  CHANGED="$(select_from_changed "$files")"
elif [ -n "$CHANGED_FILE" ]; then
  files="$(cat "$CHANGED_FILE")"
  CHANGED="$(select_from_changed "$files")"
else
  usage
fi

if [ "$LIST_ONLY" -eq 1 ]; then
  printf '%s' "$CHANGED"
  [ -z "$CHANGED" ] || [[ "$CHANGED" == *$'\n' ]] || printf '\n'
  exit 0
fi

run_named() {
  local name="$1" argv="${ARGV[$name]}"
  local main_root="${MAIN:-$ROOT}"
  case "$argv" in
    @check-land-main-refs)
      local _main_refs rc=0
      _main_refs="$(mktemp)"
      # Under preflight.sh MAIN is a git-archive of the base commit (not a repo): read its file.
      if git -C "$main_root" rev-parse --git-dir >/dev/null 2>&1; then
        git -C "$main_root" show HEAD:scripts/land-main-refs.tsv > "$_main_refs" || rc=1
      else
        cp "$main_root/scripts/land-main-refs.tsv" "$_main_refs" || rc=1
      fi
      if [ "$rc" -ne 0 ]; then
        echo "FAIL cannot read base scripts/land-main-refs.tsv from $main_root"
        rm -f "$_main_refs"
        return 1
      fi
      "$SELF_DIR/check-land-main-refs.sh" --self-test "$ROOT" \
        --main-table "$_main_refs" --incoming-table "$ROOT/scripts/land-main-refs.tsv" || rc=$?
      rm -f "$_main_refs"
      return "$rc"
      ;;
    @lint-no-stripped-compare)
      MAIN="$main_root" "$SELF_DIR/lint-no-stripped-compare.sh" --self-test || return $?
      MAIN="$main_root" "$SELF_DIR/lint-no-stripped-compare.sh" "$ROOT" || return $?
      ;;
    *)
      # word-split argv; append repo root for the tracked-files plant
      local -a cmd=()
      # shellcheck disable=SC2206
      cmd=($argv)
      if [ "${cmd[0]}" = "scripts/test-source-scan-tracked.sh" ] && [ "${#cmd[@]}" -eq 1 ]; then
        cmd+=("$ROOT")
      fi
      "${cmd[@]}"
      ;;
  esac
}

mapfile -t SELECTED < <(printf '%s' "$CHANGED" | sed '/^$/d')
if [ "${#SELECTED[@]}" -eq 0 ]; then
  echo "land-self-tests: none selected"
  exit 0
fi
echo "land-self-tests: selected ${SELECTED[*]} (jobs=$JOBS)"

if [ "$JOBS" -lt 1 ]; then JOBS=1; fi
tmp="$(mktemp -d /tmp/land-self-tests.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT
fail=0
running=0
declare -a pids=()
declare -a pid_names=()

# Simpler job control: launch all with a cap using a FIFO token pool
tokens="$tmp/tokens"
mkfifo "$tokens"
exec 3<>"$tokens"
rm -f "$tokens"
for _ in $(seq 1 "$JOBS"); do printf '\n' >&3; done

napi_pinned=0 napi_err=""
for name in "${SELECTED[@]}"; do
  needs_napi "$name" || continue
  if pin_napi "$tmp" > "$tmp/napi.msg"; then napi_pinned=1; else napi_err="$(cat "$tmp/napi.msg")"; fi
  cat "$tmp/napi.msg"
  break
done

for name in "${SELECTED[@]}"; do
  if needs_napi "$name" && [ "$napi_pinned" -eq 0 ]; then
    printf '%s\n' "$napi_err" > "$tmp/$name.out"; echo "2 0" > "$tmp/$name.rc"; continue
  fi
  read -r -u 3
  (
    rc=0
    start=$(date +%s)
    if run_named "$name" >"$tmp/$name.out" 2>&1; then
      rc=0
    else
      rc=$?
    fi
    end=$(date +%s)
    echo "$rc $((end-start))" >"$tmp/$name.rc"
    printf '\n' >&3
  ) &
  pids+=("$!")
  pid_names+=("$name")
done
for pid in "${pids[@]}"; do
  wait "$pid" || true
done
exec 3>&-

if [ "$napi_pinned" -eq 1 ] && [ "$(sha256sum "$NAPI_SO" | cut -c1-12)" != "$NAPI_SHA" ]; then
  echo "!! run-land-self-tests: pinned napi MISMATCH: $NAPI_SO changed during the run (pinned $NAPI_SHA)"
  fail=1
fi
for name in "${SELECTED[@]}"; do
  echo "----- $name -----"
  if [ -f "$tmp/$name.out" ]; then cat "$tmp/$name.out"; fi
  if [ -f "$tmp/$name.rc" ]; then
    read -r rc secs < "$tmp/$name.rc"
    echo "land-self-tests: $name ${secs}s rc=$rc"
    if [ "$rc" != 0 ]; then
      echo "FAIL $name"
      fail=1
    else
      echo "ok   $name"
    fi
  else
    echo "FAIL $name (no status)"
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "land-self-tests: FAILED"
  exit 1
fi
echo "land-self-tests: ok"
exit 0
