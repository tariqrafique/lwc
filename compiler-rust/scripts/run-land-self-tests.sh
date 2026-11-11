#!/usr/bin/env bash
# Path-mapped land self-tests. Selects names from scripts/land-self-tests.tsv
# whose path column matches a changed file (exact, or a directory prefix
# ending in /). Independent names run in parallel (LAND_SELF_TEST_JOBS, default 4).
# Usage:
#   run-land-self-tests.sh --from-stdin          # changed paths on stdin
#   run-land-self-tests.sh --changed-file FILE
#   run-land-self-tests.sh --list [--from-stdin|--changed-file FILE]
#   run-land-self-tests.sh --all                 # every name except needs-napi
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
  local changed="$1" name argv path
  declare -A sel=()
  while IFS= read -r file || [ -n "${file:-}" ]; do
    [ -n "${file:-}" ] || continue
    if [ "$file" = "scripts/test-land-classify.sh" ]; then
      for name in "${NAMES[@]}"; do
        needs_napi "$name" && continue
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
    needs_napi "$name" && continue
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
      if ! git -C "$main_root" show HEAD:scripts/land-main-refs.tsv > "$_main_refs"; then
        echo "FAIL cannot git show HEAD:scripts/land-main-refs.tsv from $main_root"
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

for name in "${SELECTED[@]}"; do
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
