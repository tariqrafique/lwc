#!/usr/bin/env bash
# Plants for scripts/land-agent-branch.sh: a bad agent/* name, and a fetched tip
# that is not a descendant of main. Uses a local bare repo as the remote.
set -euo pipefail
HERE="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
SCRIPT="$HERE/land-agent-branch.sh"
T="$(mktemp -d "${TMPDIR:-/tmp}/test-land-agent-branch.XXXXXX")"
trap 'rm -rf "$T"' EXIT
fail=0

check() {
  local name="$1" want="$2"; shift 2
  local rc=0
  "$@" >"$T/out" 2>"$T/err" || rc=$?
  if [ "$rc" = "$want" ]; then
    echo "ok   $name -> rc $rc"
  else
    echo "FAIL $name: want rc $want got $rc"
    sed 's/^/  | stdout: /' "$T/out"
    sed 's/^/  | stderr: /' "$T/err"
    fail=1
  fi
}

git_init() {
  local dir=$1
  git init -q -b main "$dir"
  git -C "$dir" config user.email test@example.com
  git -C "$dir" config user.name test
  git -C "$dir" config commit.gpgsign false
}

git_init "$T/box"
printf 'box\n' > "$T/box/README"
git -C "$T/box" add README
git -C "$T/box" commit -q -m init
box_main="$(git -C "$T/box" rev-parse HEAD)"

# Unrelated history pushed to a bare remote as agent/s6.
git_init "$T/other"
printf 'other\n' > "$T/other/README"
git -C "$T/other" add README
git -C "$T/other" commit -q -m other
git init -q --bare "$T/remote.git"
git -C "$T/other" remote add origin "$T/remote.git"
git -C "$T/other" push -q origin HEAD:refs/heads/agent/s6
other_tip="$(git -C "$T/other" rev-parse HEAD)"

export LAND_AGENT_MAIN="$T/box"
export LAND_AGENT_REMOTE="$T/remote.git"

check "bad name (nested slash)" 2 \
  "$SCRIPT" "agent/foo/bar"
check "bad name (not agent prefix)" 2 \
  "$SCRIPT" "main"
check "non-descendant tip" 1 \
  "$SCRIPT" "agent/s6"

if ! grep -q "not a descendant of main" "$T/err"; then
  echo "FAIL non-descendant tip: stderr did not name the ancestor check"
  sed 's/^/  | stderr: /' "$T/err"
  fail=1
else
  echo "ok   non-descendant tip names the ancestor check"
fi

now="$(git -C "$T/box" rev-parse HEAD)"
if [ "$now" != "$box_main" ]; then
  echo "FAIL box main moved ($box_main -> $now)"
  fail=1
else
  echo "ok   box main unchanged"
fi
if git -C "$T/box" show-ref --verify --quiet refs/heads/bot/s6; then
  echo "FAIL created local bot/s6 on a refused land"
  fail=1
else
  echo "ok   no local bot/s6"
fi
if [ -e "$T/wt/s6" ]; then
  echo "FAIL created a worktree after refuse"
  fail=1
else
  echo "ok   no worktree created"
fi
# The refused fetch still records a tmp tip ref; that is local and not a remote write.
if git -C "$T/remote.git" rev-parse --verify -q refs/heads/agent/s6 >/dev/null; then
  got="$(git -C "$T/remote.git" rev-parse refs/heads/agent/s6)"
  if [ "$got" != "$other_tip" ]; then
    echo "FAIL remote agent/s6 was rewritten ($other_tip -> $got)"
    fail=1
  else
    echo "ok   remote agent/s6 untouched"
  fi
fi

exit "$fail"
