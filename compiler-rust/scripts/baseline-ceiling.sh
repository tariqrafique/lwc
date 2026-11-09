#!/usr/bin/env bash
# Board baseline only gets better (AB-0006, ratchet-ceiling.sh's model applied to harness/board-baseline.tsv).
# Usage: baseline-ceiling.sh <repo> <pre-land-sha> <branch-sha> [<case-set-out> [<measured.tsv>]]
#          land.sh: "$wt" "$base" "$head" <cs>             (the branch's committed claim, fail fast)
#                   "$wt" "$base" "$head" <cs> <measured>  (board check: the MEASURED run vs main, D-0255)
#        baseline-ceiling.sh --self-test
# <case-set-out>: rows with a D-0255 case-set change (an id removed from the row's passed+failed+skipped universe or
#   newly skipped), one per line; land.sh then requires a 'Case-Set: <D-id>' trailer whose decision lists case-set:<row>.
# Both sides are read from GIT, never from a working tree: `git ls-tree` (mode) + `git show <sha>:harness/board-baseline.tsv` of
# harness/board-baseline.tsv at <pre-land-sha> (main before the land) and at <branch-sha>.
#   - Either side present but not a regular file (mode != 100644; 120000 = symlink, 160000 = gitlink): rc 2.
#   - Branch commit lacks the path (deleted or renamed): rc 2 (never raise-acceptable).
#   - Pre-land commit lacks the path: rc 2 (never raise-acceptable). The seed has landed; a missing
#     pre-land baseline is no longer treated as NEW.
#   - Otherwise harness/scripts/board-baseline.mjs ceiling: every main row (diagnostic rows included) must exist on the
#     branch and be no WORSE: no newly failing/skipped id (a swap counts), no fewer passed, no row total drop, passed must
#     rise by at least the ids no longer failing/skipped; template-diag*: no higher crash / oracle-error.
# Exit: 0 ok; 1 a row is WORSE or missing (land.sh accepts only under its own 'Baseline-Raise: <AB-id>' review);
#       2 anything else: usage, git error, non-regular or missing pre-land or branch blob, malformed baseline, node failure, or ANY
#       shell error in this script (EXIT trap: only the explicit verdict exits can return 0 or 1).
# The trap is the first statement (before set -euo pipefail) so a failure in the old window cannot leak out as rc 1.
done_rc=; T=
trap 'rc=$?; [ -z "$T" ] || rm -rf "$T"; if [ "$rc" != "${done_rc:-unset}" ]; then echo "!! baseline-ceiling: aborted (rc=$rc): treated as rc 2 (cannot run; never raise-acceptable)" >&2; exit 2; fi' EXIT
fin() { done_rc="$1"; exit "$1"; }
set -euo pipefail
P=harness/board-baseline.tsv
here="$(cd "$(dirname "$0")" && pwd)"
MJS="$here/../harness/scripts/board-baseline.mjs"
[ -f "$MJS" ] || { echo "!! baseline-ceiling: missing $MJS" >&2; fin 2; }
if [ "${1:-}" = --self-test ]; then
  [ $# -eq 1 ] || { echo "!! baseline-ceiling: --self-test takes no arguments" >&2; fin 2; }
  src=0; node "$MJS" --self-test || src=$?; [ $src -eq 0 ] && fin 0; fin 2
fi
[ $# -ge 3 ] && [ $# -le 5 ] || { echo "!! baseline-ceiling: usage: $0 <repo> <pre-land-sha> <branch-sha> [<case-set-out> [<measured.tsv>]] | --self-test" >&2; fin 2; }
repo="$1"; base="$2"; head="$3"; csout="${4:-}"; measured="${5:-}"
[ -z "$csout" ] || : > "$csout"
[ -z "$measured" ] || [ -f "$measured" ] || { echo "!! baseline-ceiling: measured baseline $measured missing" >&2; fin 2; }
for s in "$base" "$head"; do git -C "$repo" rev-parse -q --verify "$s^{commit}" > /dev/null || { echo "!! baseline-ceiling: not a commit in $repo: $s" >&2; fin 2; }; done
T="$(mktemp -d)"
# blob <label> <sha> <out>: sets have=1 and writes <out> for a regular file; have=0 when the commit lacks the path.
blob() {
  local ent mode rest; have=0
  ent="$(git -C "$repo" ls-tree "$2" -- "$P")"
  [ -n "$ent" ] || return 0
  [ "$(wc -l <<< "$ent")" -eq 1 ] || { echo "!! baseline-ceiling: $1 ($2): ambiguous ls-tree for $P" >&2; fin 2; }
  mode="${ent%% *}"; rest="${ent#* }"
  [ "$mode" = 100644 ] && [ "${rest%% *}" = blob ] || { echo "!! baseline-ceiling: $1 ($2): $P is mode $mode (${rest%%$'\t'*}), not a regular file (120000 = symlink): refused" >&2; fin 2; }
  git -C "$repo" show "$2:$P" > "$3"; have=1
}
blob pre-land "$base" "$T/main.tsv"; hm=$have
blob branch "$head" "$T/branch.tsv"; hb=$have
[ $hb -eq 1 ] || { echo "!! baseline-ceiling: branch commit ${head:0:12} has no $P (deleted or renamed): refused (rc 2, never raise-acceptable)" >&2; fin 2; }
[ $hm -eq 1 ] || { echo "!! baseline-ceiling: pre-land commit ${base:0:12} has no $P: refused (rc 2, never raise-acceptable)" >&2; fin 2; }
cand="$T/branch.tsv"; [ -z "$measured" ] || { cand="$measured"; echo "baseline-ceiling: judging the MEASURED run $measured against pre-land ${base:0:12}"; }
crc=0; node "$MJS" ceiling "$cand" "$T/main.tsv" ${csout:+"$csout"} || crc=$?
case $crc in 0) fin 0;; 3) fin 1;; *) echo "!! baseline-ceiling: board-baseline.mjs ceiling rc=$crc" >&2; fin 2;; esac
