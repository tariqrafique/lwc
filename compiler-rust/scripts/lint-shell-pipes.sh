#!/usr/bin/env bash
# AB-X0008 lint: no pipe into a reader that exits early, in scripts/*.sh, harness/*.sh, harness/scripts/*.sh:
#   - a quiet grep (grep -q / -qE / -Eq / -E -q / --quiet / --silent), or
#   - head (any options).
# Under pipefail the reader exits at its first match / line count, the writer dies of SIGPIPE (rc 141) and the pipeline
# reports failure on large inputs: an 'if' around it then fails open/closed at random.
# Write   grep -q PATTERN <<< "$x"   /   head -n5 <<< "$x"   (or read a FILE) instead.
# Logical lines are checked: backslash-continued lines are joined, and so is a line ending in '|' with the next line
# (both forms continue a pipeline in bash). Comment lines (first non-blank char '#') are skipped. A hit is reported at the
# logical line's first physical line.
# Exit 0 = clean; 1 = violation(s) listed; 2 = lint could not run (no files, unreadable file, awk/grep error). Never a silent pass.
# Usage: scripts/lint-shell-pipes.sh [repo-root]   (default: this repo)   |   scripts/lint-shell-pipes.sh --self-test
set -euo pipefail
# '|' not preceded by '|' (so 'a || grep -q' is fine), then grep, any option words, then a quiet option; or then head.
QGREP='(^|[^|])\|[[:space:]]*grep([[:space:]]+-[A-Za-z]+)*[[:space:]]+(-[A-Za-z]*q[A-Za-z]*|--quiet|--silent)([[:space:]]|$)'
HEAD='(^|[^|])\|[[:space:]]*head([[:space:]]|$)'
# logical <files...>: prints '<file>:<first line>:<joined logical line>' per logical line; rc 2 if a file can't be read.
logical() {
  local f
  for f in "$@"; do [ -r "$f" ] || { echo "!! lint-shell-pipes: cannot read $f" >&2; return 2; }; done
  awk 'function flush() { if (buf != "") print FILENAME_ ":" start ":" buf; buf = ""; cont = 0 }
    FNR == 1 { if (NR > 1) flush(); FILENAME_ = FILENAME }
    {
      line = $0
      if (!cont) { if (line ~ /^[[:space:]]*#/) next; start = FNR; buf = "" }
      if (line ~ /\\$/) { buf = buf substr(line, 1, length(line) - 1) " "; cont = 1; next }
      if (line ~ /\|[[:space:]]*$/ && line !~ /\|\|[[:space:]]*$/) { buf = buf line " "; cont = 1; next }
      buf = buf line; cont = 1; flush()
    }
    END { flush() }' "$@"
}
tracked_list() {
  local root="$1" helper rel
  helper="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)/tracked-files.sh"
  [ -x "$helper" ] || { echo "!! lint-shell-pipes: missing $helper" >&2; return 2; }
  while IFS= read -r -d '' rel; do
    printf '%s\0' "$root/$rel"
  done < <("$helper" "$root" 'scripts/*.sh' 'harness/*.sh' 'harness/scripts/*.sh')
}
lint() { # root -> rc 0/1/2
  local root="$1" files=() rc=0 arc=0 hits all rel
  while IFS= read -r -d '' rel; do
    files+=("$rel")
  done < <(tracked_list "$root") || return 2
  [ ${#files[@]} -gt 0 ] || { echo "!! lint-shell-pipes: no *.sh under $root/{scripts,harness,harness/scripts}" >&2; return 2; }
  all="$(logical "${files[@]}")" || arc=$?
  [ $arc -eq 0 ] || { echo "!! lint-shell-pipes: could not read/join lines (rc=$arc)" >&2; return 2; }
  hits="$(grep -E -e "$QGREP" -e "$HEAD" <<< "$all")" || rc=$?
  case $rc in
    0) echo "!! lint-shell-pipes: pipe into quiet grep / head (SIGPIPE fail-open under pipefail; use 'grep -q P <<< \"\$x\"' / 'head -n N <<< \"\$x\"'):" >&2
       echo "$hits" >&2; return 1;;
    1) echo "lint-shell-pipes: clean (${#files[@]} files under $root)"; return 0;;
    *) echo "!! lint-shell-pipes: grep failed (rc=$rc)" >&2; return 2;;
  esac
}
self_test() {
  local t fail=0 rc q='-q' P='|'; t="$(mktemp -d /tmp/lint-shell-pipes.XXXXXX)"; trap 'chmod -R u+rwx "$t"; rm -rf "$t"' RETURN
  expect() { # name want root
    rc=0; lint "$3" >/dev/null 2>&1 || rc=$?
    if [ "$rc" -eq "$2" ]; then echo "ok   $1 -> rc $rc"; else echo "FAIL $1 -> rc $rc, want $2"; fail=1; fi; }
  mkdir -p "$t/clean/scripts" "$t/clean/harness/scripts"
  helper="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
  cp "$helper/tracked-files.sh" "$t/clean/scripts/"
  cp "$helper/lint-shell-pipes.sh" "$t/clean/scripts/"
  printf '%s\n' '#!/usr/bin/env bash' "if grep $q x <<< \"\$y\"; then :; fi" "a || grep $q b f" "grep -c x f | tail -1" \
    'head -n5 <<< "$x"' "# a comment may say: x $P head -1 or x $P grep $q y" 'a ||' "  grep $q b f" 'x | \' '  tail -1' \
    'x | header-tool' 'x | sort -u |' '  tr a b' > "$t/clean/scripts/ok.sh"
  git -C "$t/clean" init -q
  git -C "$t/clean" add scripts harness
  expect "clean file passes" 0 "$t/clean"
  local v; for v in "$q" "${q}E" "-E${q#-}" "-E $q" "--quiet" "-v -c $q"; do
    rm -rf "$t/bad"; cp -a "$t/clean" "$t/bad"; printf 'if echo "$x" | grep %s y; then :; fi\n' "$v" > "$t/bad/harness/scripts/bad.sh"
    git -C "$t/bad" add harness/scripts/bad.sh
    expect "planted violation 'grep $v' fails" 1 "$t/bad"
  done
  plant() { rm -rf "$t/bad"; cp -a "$t/clean" "$t/bad"; printf '%s\n' "$@" > "$t/bad/scripts/bad.sh"; git -C "$t/bad" add scripts/bad.sh; }
  plant "x ${P}grep $q y";                       mv "$t/bad/scripts/bad.sh" "$t/bad/harness/b.sh"; git -C "$t/bad" add harness/b.sh; git -C "$t/bad" rm -q --cached scripts/bad.sh 2>/dev/null || true; expect "planted violation in harness/*.sh fails" 1 "$t/bad"
  plant "echo \"\$x\" $P head -20";              expect "pipe into head fails" 1 "$t/bad"
  plant "git status $P head -n5 $P tr a b";        expect "pipe into head mid-pipeline fails" 1 "$t/bad"
  plant 'echo "$x" \' "  | grep $q y";           expect "backslash-continued '| grep -q' fails" 1 "$t/bad"
  plant 'echo "$x" | \' "  grep $q y";           expect "backslash after '|' then grep -q fails" 1 "$t/bad"
  plant 'echo "$x" |' "  grep $q y";             expect "trailing '|' then grep -q fails" 1 "$t/bad"
  plant 'find . \' '  -type f \' "  $P head -1";  expect "multi-line backslash pipeline into head fails" 1 "$t/bad"
  plant 'echo "$x" | sort |' '  head -3';        expect "trailing '|' then head fails" 1 "$t/bad"
  mkdir -p "$t/none"; expect "no files = rc 2" 2 "$t/none"
  if [ "$(id -u)" -ne 0 ]; then cp -r "$t/clean" "$t/unread"; chmod 000 "$t/unread/scripts/ok.sh"; expect "unreadable file = rc 2" 2 "$t/unread"; fi
  return $fail
}
case "${1:-}" in
  --self-test) self_test;;
  *) lint "$(cd "${1:-$(dirname "$0")/..}" && pwd)";;
esac
