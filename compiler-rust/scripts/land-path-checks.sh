#!/usr/bin/env bash
# D-0282: shared in-repo-input checks for land.sh and c8-checks.sh.
# Usage:
#   land-path-checks.sh --root <tree> [--git <repo>] [--base <sha>] [--head <sha>]
#                       [--lints-only] [--list FILE] [--from-c8|--from-land]
# Env: LAND_PATH_CHECKS_LIST overrides the TSV (tests plant extra rows).
#      LAND_PATH_SKIP_GOLDEN_GATE=1 (C8: run-board owns golden-gate).
#      DEC_REPO / DEC_PATH / DEC_SHA used when baseline reports a case-set change (C8).
set -euo pipefail
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
root="" gitrepo="" base="" head="" lints_only=0 from="land"
list="${LAND_PATH_CHECKS_LIST:-$here/land-path-checks.tsv}"
while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="${2:-}"; shift 2 ;;
    --git) gitrepo="${2:-}"; shift 2 ;;
    --base) base="${2:-}"; shift 2 ;;
    --head) head="${2:-}"; shift 2 ;;
    --list) list="${2:-}"; shift 2 ;;
    --lints-only) lints_only=1; shift ;;
    --from-c8) from=c8; shift ;;
    --from-land) from=land; shift ;;
    -*) echo "usage: land-path-checks.sh --root TREE [--git REPO --base SHA --head SHA] [--lints-only] [--list FILE]" >&2; exit 2 ;;
    *) echo "usage: land-path-checks.sh --root TREE ..." >&2; exit 2 ;;
  esac
done
[ -n "$root" ] && [ -d "$root" ] || { echo "land-path-checks: --root is required" >&2; exit 2; }
root="$(cd "$root" && pwd)"
[ -f "$list" ] || { echo "land-path-checks: missing list $list" >&2; exit 2; }
ce_tmp="" csfile=""
cleanup_lpc() { rm -rf "${ce_tmp:-}"; rm -f "${csfile:-}"; }
trap cleanup_lpc EXIT

run_rel() {
  local rel="$1"; shift
  local p="$here/../$rel"
  [ -e "$p" ] || { echo "land-path-checks: missing $rel" >&2; return 2; }
  case "$rel" in
    *.mjs|*.js) node "$p" "$@" ;;
    *) "$p" "$@" ;;
  esac
}

while IFS=$'\t' read -r name path when; do
  [ -n "${name:-}" ] || continue
  case "$name" in '#'*|name) continue ;; esac
  [ "${when:-}" = always ] || { echo "land-path-checks: unknown when '$when' for $name" >&2; exit 2; }
  export LAND_PATH_FROM="$from"
  case "$name" in
    ratchet-ceiling)
      [ "$lints_only" -eq 1 ] && continue
      br="$root/harness/ratchets"
      ce="$here/../harness/ratchets"
      if [ -n "$gitrepo" ] && [ -n "$base" ]; then
        ce_tmp="$(mktemp -d /tmp/land-path-ratchets.XXXXXX)"
        _rat="$(git -C "$gitrepo" ls-tree -r --name-only "$base" -- harness/ratchets || true)"
        if [ -n "$_rat" ]; then
          git -C "$gitrepo" archive "$base" -- harness/ratchets | tar -x -C "$ce_tmp"
          ce="$ce_tmp/harness/ratchets"
        fi
      fi
      echo "land-path-checks: ratchet-ceiling ($from)"
      run_rel scripts/ratchet-ceiling.sh "$br" "$ce" || { echo "!! land-path-checks: ratchet-ceiling failed" >&2; exit 1; }
      ;;
    baseline-ceiling)
      [ "$lints_only" -eq 1 ] && continue
      [ "$from" = land ] && continue
      [ -n "$gitrepo" ] && [ -n "$base" ] && [ -n "$head" ] || { echo "land-path-checks: baseline-ceiling needs --git --base --head" >&2; exit 2; }
      csfile="$(mktemp /tmp/land-path-caseset.XXXXXX)"
      echo "land-path-checks: baseline-ceiling ($from) $base..$head"
      blrc=0
      run_rel scripts/baseline-ceiling.sh "$gitrepo" "$base" "$head" "$csfile" || blrc=$?
      if [ "$blrc" -ne 0 ]; then
        echo "!! land-path-checks: baseline-ceiling rc=$blrc" >&2
        # C8 has no Baseline-Raise review path; land.sh (--from-land) maps rc 1 to 3.
        if [ "$from" = land ] && [ "$blrc" -eq 1 ]; then exit 3; fi
        exit "$blrc"
      fi
      if [ -s "$csfile" ]; then
        echo "land-path-checks: case-set changes reported ($(wc -l < "$csfile") row-lines)"
        if [ "$from" = land ]; then
          echo "land-path-checks: leaving Case-Set judgement to land.sh"
        else
          echo "!! land-path-checks: case-set change on C8 path requires land.sh Case-Set (fail closed)" >&2
          exit 2
        fi
      fi
      ;;
    shim-gate)
      [ "$lints_only" -eq 1 ] && continue
      [ "$from" = land ] && continue
      echo "land-path-checks: shim-gate ($from)"
      CRATES_ROOT="$root/crates" \
        SHIM_ALLOWLIST="$root/harness/shim-allowlist.tsv" \
        SHIM_REGISTRY="$root/harness/shim-registry.tsv" \
        SHIM_RATCHET_DIR="$root/harness/ratchets" \
        SHIM_OWNERS="$here/../OWNERS" \
        SHIM_REPO_ROOT="$root" \
        run_rel harness/scripts/shim-gate.mjs || { echo "!! land-path-checks: shim-gate failed" >&2; exit 1; }
      ;;
    *)
      echo "land-path-checks: $name ($from)"
      run_rel "$path" "$root" || { echo "!! land-path-checks: $name failed" >&2; exit 1; }
      ;;
  esac
done < "$list"
echo "land-path-checks: ok ($from)"
