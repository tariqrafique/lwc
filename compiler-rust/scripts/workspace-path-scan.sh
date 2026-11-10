#!/usr/bin/env bash
# D-0280(b): fail on a literal `/workspace/` path in crates/, tools/, harness/, and
# scripts/, outside the path-exact allowlist next to this file.
# Exit 0 = clean; 1 = violation(s); 2 = scanner could not run.
# Usage: scripts/workspace-path-scan.sh [repo-root]   |   scripts/workspace-path-scan.sh --self-test
set -euo pipefail
runner="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
ALLOW_NAME="workspace-path-allowlist.tsv"

load_allow() {
  local f="$1" line path
  ALLOWED=()
  [ -r "$f" ] || { echo "!! workspace-path-scan: cannot read allowlist $f" >&2; return 2; }
  while IFS= read -r line || [ -n "$line" ]; do
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    [[ "$line" =~ ^[[:space:]]*$ ]] && continue
    path="${line%%$'\t'*}"
    [ -n "$path" ] || { echo "!! workspace-path-scan: empty path in $f" >&2; return 2; }
    case "$path" in
      */*|.*|*) ;;
    esac
    ALLOWED+=("$path")
  done < "$f"
}

is_allowed() {
  local rel="$1" a
  for a in "${ALLOWED[@]}"; do
    [ "$rel" = "$a" ] && return 0
  done
  return 1
}

scan() {
  local root="$1" allowfile="$2"
  load_allow "$allowfile" || return 2
  local hits="" rel f rc=0
  local list
  list="$(mktemp)"
  if ! "$runner/tracked-files.sh" "$root" crates tools harness scripts >"$list"; then
    rm -f "$list"
    echo "!! workspace-path-scan: cannot list tracked files under $root" >&2
    return 2
  fi
  while IFS= read -r -d '' rel; do
    f="$root/$rel"
    if is_allowed "$rel"; then
      continue
    fi
    if grep -n -F '/workspace/' "$f" >/dev/null 2>&1; then
      hits+="$rel"$'\n'
      hits+="$(grep -n -F '/workspace/' "$f" | sed "s|^|$rel:|")"$'\n'
    fi
  done < "$list"
  rm -f "$list"
  if [ -n "$hits" ]; then
    echo "!! workspace-path-scan: literal /workspace/ path outside the allowlist (resolve through harness/scripts/oracle-root.mjs / LWC_ORACLE, or add a path-exact allowlist row with a reason if the file is genuinely box-only):" >&2
    printf '%s' "$hits" >&2
    return 1
  fi
  echo "workspace-path-scan: clean"
  return 0
}

self_test() {
  local t fail=0 rc
  t="$(mktemp -d /tmp/workspace-path-scan.XXXXXX)"
  trap 'rm -rf "$t"' RETURN
  mkdir -p "$t/crates" "$t/tools" "$t/harness" "$t/scripts"
  cp "$runner/workspace-path-scan.sh" "$t/scripts/"
  cp "$runner/tracked-files.sh" "$t/scripts/"
  printf '%s\n' '.oracle/' > "$t/.gitignore"
  printf '%s\n' '# path	reason' \
    'scripts/allowed.sh	planted allowlisted box-only path' \
    'scripts/workspace-path-scan.sh	the scanner names the forbidden prefix' \
    > "$t/scripts/workspace-path-allowlist.tsv"
  printf '%s\n' 'const box = "/workspace/allowed";' > "$t/scripts/allowed.sh"
  printf '%s\n' 'fn ok() {}' > "$t/crates/ok.rs"
  git -C "$t" init -q
  git -C "$t" add crates tools harness scripts .gitignore
  rc=0
  "$t/scripts/workspace-path-scan.sh" "$t" >/dev/null 2>&1 || rc=$?
  if [ "$rc" -eq 0 ]; then echo "ok   allowlisted path passes"; else echo "FAIL allowlisted path passes -> rc $rc"; fail=1; fi
  printf '%s\n' 'const p = "/workspace/evil";' > "$t/crates/evil.rs"
  git -C "$t" add crates/evil.rs
  rc=0
  "$t/scripts/workspace-path-scan.sh" "$t" >/dev/null 2>&1 || rc=$?
  if [ "$rc" -eq 1 ]; then echo "ok   new literal in a crate fails"; else echo "FAIL new literal in a crate fails -> rc $rc"; fail=1; fi
  git -C "$t" rm -q --cached crates/evil.rs
  rm -f "$t/crates/evil.rs"
  mkdir -p "$t/.oracle/lwc"
  printf '%s\n' 'const p = "/workspace/evil";' > "$t/.oracle/lwc/plant.js"
  rc=0
  "$t/scripts/workspace-path-scan.sh" "$t" >/dev/null 2>&1 || rc=$?
  if [ "$rc" -eq 0 ]; then echo "ok   untracked .oracle plant is ignored"; else echo "FAIL untracked .oracle plant is ignored -> rc $rc"; fail=1; fi
  return $fail
}

case "${1:-}" in
  --self-test) self_test;;
  *)
    root="$(cd "${1:-$runner/..}" && pwd)"
    for d in crates tools harness scripts; do
      [ -d "$root/$d" ] || { echo "!! workspace-path-scan: missing $root/$d" >&2; exit 2; }
    done
    scan "$root" "$runner/$ALLOW_NAME"
    ;;
esac
