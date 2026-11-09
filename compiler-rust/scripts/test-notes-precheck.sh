#!/usr/bin/env bash
# Guard test for AB-0016 in a THROWAWAY clone of /workspace/notes (never the real one): scripts/notes-precheck.sh and
# official-board.sh's notes block (extracted verbatim between '# >>> notes' / '# <<< notes') must
#   - leave an unrelated dirty notes file untouched: rc 0, HEAD unchanged, nothing staged/committed;
#   - exit 3 on a dirty (modified / untracked / staged) read path, a read path missing from HEAD, or a git error.
set -euo pipefail
S="$(cd "$(dirname "$0")" && pwd)"; PC="$S/notes-precheck.sh"; OB="$S/official-board.sh"
src="${NOTES_TEST_SOURCE:-/workspace/notes}"
P="$(mktemp -d /tmp/test-notes-precheck.XXXXXX)"; trap 'rm -rf "$P"' EXIT
git clone -q --no-hardlinks "$src" "$P/notes"; N="$P/notes"; git -C "$N" config user.email t@t; git -C "$N" config user.name t
blk="$(sed -n '/^# >>> notes/,/^# <<< notes/p' "$OB")"
fail=0; ok() { echo "ok   $*"; }; bad() { echo "FAIL $*"; fail=1; }
reset() { git -C "$N" reset -q --hard "$h0"; git -C "$N" clean -qfdx; }
h0="$(git -C "$N" rev-parse HEAD)"
[ -n "$blk" ] || bad "notes block not found in official-board.sh"
# official-board's block, run against the clone: prints notescommit; must not touch the repo.
runblk() { ( set -euo pipefail; NOTES="$N"; NOTES_PRECHECK="$PC"; die3() { echo "!! BOARD ABORTED: $*" >&2; exit 3; }; eval "$blk"; echo "notescommit=$notescommit" ) ; }
expect() { # name want-rc cmd...
  local name="$1" want="$2" rc=0; shift 2; "$@" >"$P/out" 2>&1 || rc=$?
  if [ "$rc" -eq "$want" ]; then ok "$name -> rc $rc"; else bad "$name -> rc $rc, want $want: $(tail -3 "$P/out")"; fi; }
untouched() { # name: HEAD unchanged, nothing staged
  [ "$(git -C "$N" rev-parse HEAD)" = "$h0" ] && ok "$1: HEAD unchanged" || bad "$1: HEAD moved"
  [ -z "$(git -C "$N" diff --cached --name-only)" ] && ok "$1: nothing staged" || bad "$1: index changed"; }
# 1. unrelated dirty notes (modified tracked file outside the read paths + untracked file): pass, untouched.
# first tracked file [under <pathspec>] [not under <exclude-prefix>]; no `| head` (SIGPIPE -> rc 141 under pipefail)
first_tracked() { local all l; all="$(git -C "$N" ls-files -- ${1:+"$1"})" || { bad "git ls-files failed"; return 0; }
  while IFS= read -r l; do if [ -n "$2" ] && [[ "$l" == "$2"* ]]; then continue; fi; echo "$l"; return 0; done <<< "$all"; }
other="$(first_tracked "" lwc-rust-migrate/)"
[ -n "$other" ] || bad "clone has no tracked file outside lwc-rust-migrate/"
echo "half-written by another bot" >> "$N/$other"; echo wip > "$N/arch-board/zz-untracked-wip.md"
sum0="$(sha256sum "$N/$other" "$N/arch-board/zz-untracked-wip.md")"
expect "unrelated dirty file: precheck" 0 "$PC" "$N" lwc-rust-migrate
expect "unrelated dirty file: official-board notes block" 0 runblk
grep -q "^notescommit=$h0$" "$P/out" && ok "notes block records HEAD as-is ($h0)" || bad "notes block notescommit != HEAD: $(cat "$P/out")"
untouched "unrelated dirty file"
[ "$(sha256sum "$N/$other" "$N/arch-board/zz-untracked-wip.md")" = "$sum0" ] && ok "unrelated dirty files byte-identical" || bad "unrelated dirty files changed"
grep -q 'zz-untracked-wip' <<< "$(git -C "$N" status --porcelain)" && ok "untracked wip still untracked" || bad "untracked wip vanished/added"
reset
# 2. dirty read path: modified / untracked / staged -> exit 3, naming the path, repo untouched.
f="$(first_tracked lwc-rust-migrate "")"
[ -n "$f" ] || bad "clone has no tracked file under lwc-rust-migrate/"
echo x >> "$N/$f"; expect "modified read path ($f)" 3 runblk; grep -q "$f" "$P/out" && ok "message names $f" || bad "message does not name $f"; untouched "modified read path"; reset
echo x > "$N/lwc-rust-migrate/zz-new.md"; expect "untracked file under read path" 3 "$PC" "$N" lwc-rust-migrate; untouched "untracked read path"; reset
echo x >> "$N/$f"; git -C "$N" add "$f"; h1="$(git -C "$N" rev-parse HEAD)"; expect "staged read path" 3 "$PC" "$N" lwc-rust-migrate
[ "$(git -C "$N" rev-parse HEAD)" = "$h1" ] && ok "staged read path: HEAD unchanged" || bad "staged read path: HEAD moved"; reset
# 3. read path missing from HEAD, and git errors -> exit 3.
expect "read path missing from HEAD" 3 "$PC" "$N" no-such-dir
expect "not a git repo" 3 "$PC" "$P/nonrepo" lwc-rust-migrate
mkdir -p "$P/empty" && git -C "$P/empty" init -q && expect "repo without HEAD" 3 "$PC" "$P/empty" lwc-rust-migrate
expect "no paths given" 3 "$PC" "$N"
exit $fail
