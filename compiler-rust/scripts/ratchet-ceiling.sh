#!/usr/bin/env bash
# Ratchets only go down (AB-X0010). Usage:
#   ratchet-ceiling.sh <branch-ratchets-dir> <ceiling-dir>     land.sh: <wt>/harness/ratchets vs $MAIN/harness/ratchets
#   ratchet-ceiling.sh --self-test
# Every <ceiling-dir>/*.txt must exist in <branch-ratchets-dir> with a value <= the ceiling's. A file new on the branch is
# allowed (printed as NEW). Each file must hold exactly one integer line (digits, optional single trailing newline).
# A RAISE IS ALWAYS REFUSED here: raising a ratchet needs its own reviewed Arch id and a dedicated landing that edits
# main's harness/ratchets/<name>.txt by hand (outside land.sh); it never rides a feature landing.
# Exit: 0 ok; 1 a value above the ceiling or a ceiling file missing on the branch; 2 usage, unreadable dir/file,
# or a value that is not one integer line (fail closed).
set -euo pipefail
die2() { echo "!! ratchet-ceiling: $*" >&2; exit 2; }
# readval <file>: prints the integer, or exits 2.
readval() {
  local f="$1" c re=$'^([0-9]{1,18})\n?$'
  [ -f "$f" ] && [ -r "$f" ] || die2 "cannot read $f"
  c="$(cat -- "$f"; printf x)" || die2 "cannot read $f"; c="${c%x}"
  [[ "$c" =~ $re ]] || die2 "$f: must hold exactly one integer line, got $(printf '%q' "$c")"
  echo "$((10#${BASH_REMATCH[1]}))"
}
check() {
  [ $# -eq 2 ] || die2 "usage: $0 <branch-ratchets-dir> <ceiling-dir> | --self-test"
  local br="$1" ce="$2" f n b c bad=0 seen=0
  [ -d "$ce" ] || die2 "ceiling dir $ce is not a directory"
  [ -d "$br" ] || die2 "branch ratchets dir $br is not a directory"
  shopt -s nullglob
  for f in "$ce"/*.txt; do n="${f##*/}"; seen=$((seen+1))
    c="$(readval "$f")" || exit 2
    if [ ! -e "$br/$n" ]; then echo "  RATCHET-CEILING: $n exists on main ($c) but is missing on the branch"; bad=1; continue; fi
    b="$(readval "$br/$n")" || exit 2
    if [ "$b" -gt "$c" ]; then echo "  RATCHET-CEILING: $n raised $c -> $b (refused: a raise needs its own reviewed Arch id and a dedicated main edit)"; bad=1
    elif [ "$b" -lt "$c" ]; then echo "  ratchet $n lowered $c -> $b"
    else echo "  ratchet $n = $c"; fi
  done
  for f in "$br"/*.txt; do n="${f##*/}"; [ -e "$ce/$n" ] && continue
    b="$(readval "$f")" || exit 2; echo "  ratchet $n NEW on branch = $b (no ceiling on main)"; done
  shopt -u nullglob
  echo "ratchet-ceiling: $seen ceiling file(s) checked, $([ $bad -eq 0 ] && echo ok || echo REFUSED)"
  return $bad
}
selftest() {
  local t fail=0 rc
  t="$(mktemp -d /tmp/ratchet-ceiling-test.XXXXXX)"; trap 'rm -rf "$t"' RETURN
  run() { # <name> <want-rc> <setup...>: fresh dirs, setup via eval, run check
    local name="$1" want="$2"; shift 2
    rm -rf "$t/b" "$t/c"; mkdir -p "$t/b" "$t/c"; eval "$*"
    rc=0; ( check "$t/b" "$t/c" ) > "$t/out" 2>&1 || rc=$?
    if [ "$rc" -eq "$want" ]; then echo "  ok   $name (rc=$rc)"; else echo "  FAIL $name: rc=$rc want $want"; sed 's/^/       /' "$t/out"; fail=1; fi
  }
  run 'equal passes'            0 'echo 99 > $t/c/a.txt; echo 99 > $t/b/a.txt'
  run 'lower passes'            0 'echo 99 > $t/c/a.txt; echo 98 > $t/b/a.txt'
  run 'raised refused'          1 'echo 99 > $t/c/a.txt; echo 100 > $t/b/a.txt'
  run 'missing on branch'       1 'echo 99 > $t/c/a.txt'
  run 'new on branch allowed'   0 'echo 99 > $t/c/a.txt; echo 99 > $t/b/a.txt; echo 5 > $t/b/new.txt'
  run 'no trailing newline ok'  0 'echo 9 > $t/c/a.txt; printf 9 > $t/b/a.txt'
  run 'two lines = parse error' 2 'echo 9 > $t/c/a.txt; printf "9\n9\n" > $t/b/a.txt'
  run 'blank line after value'  2 'echo 9 > $t/c/a.txt; printf "9\n\n" > $t/b/a.txt'
  run 'non-integer'             2 'echo 9 > $t/c/a.txt; echo " 9" > $t/b/a.txt'
  run 'negative'                2 'echo 9 > $t/c/a.txt; echo -1 > $t/b/a.txt'
  run 'empty file'              2 'echo 9 > $t/c/a.txt; : > $t/b/a.txt'
  run 'bad ceiling value'       2 'echo x > $t/c/a.txt; echo 1 > $t/b/a.txt'
  run 'bad new-file value'      2 'echo 9 > $t/c/a.txt; echo 9 > $t/b/a.txt; echo y > $t/b/n.txt'
  run 'ceiling dir missing'     2 'rmdir $t/c'
  run 'branch dir missing'      2 'echo 9 > $t/c/a.txt; rm -rf $t/b'
  run 'branch entry not a file'  2 'echo 9 > $t/c/a.txt; mkdir $t/b/a.txt'
  rc=0; ( check "$t/b" ) > /dev/null 2>&1 || rc=$?
  if [ $rc -eq 2 ]; then echo "  ok   one arg = usage (rc=2)"; else echo "  FAIL one arg: rc=$rc want 2"; fail=1; fi
  echo "ratchet-ceiling self-test: $([ $fail -eq 0 ] && echo ok || echo FAILED)"
  return $fail
}
if [ "${1:-}" = --self-test ]; then [ $# -eq 1 ] || die2 "--self-test takes no arguments"; selftest; exit $?; fi
check "$@"
