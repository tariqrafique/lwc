#!/usr/bin/env bash
# Plants for scripts/land-agent-branch.sh: a bad agent/* name, a fetched tip
# that is not a descendant of main, a failed land that must leave no bot/<slice>
# ref or worktree, and a successful stub land that must also clean up.
# Uses a local bare repo as the remote.
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

assert_no_slice() {
  local slice="$1" label="$2"
  if git -C "$T/box" show-ref --verify --quiet "refs/heads/bot/${slice}"; then
    echo "FAIL $label: leftover refs/heads/bot/${slice}"
    fail=1
  else
    echo "ok   $label: no leftover bot/${slice}"
  fi
  if [ -e "$T/wt/${slice}" ]; then
    echo "FAIL $label: leftover worktree $T/wt/${slice}"
    fail=1
  else
    echo "ok   $label: no leftover worktree ${slice}"
  fi
  local line found=0
  while IFS= read -r line; do
    case "$line" in
      "worktree $T/wt/${slice}") found=1 ;;
    esac
  done < <(git -C "$T/box" worktree list --porcelain 2>/dev/null || true)
  if [ "$found" = 1 ]; then
    echo "FAIL $label: worktree list still names $T/wt/${slice}"
    fail=1
  else
    echo "ok   $label: worktree list has no ${slice}"
  fi
}

git_init() {
  local dir=$1
  git init -q -b main "$dir"
  git -C "$dir" config user.email test@example.com
  git -C "$dir" config user.name test
  git -C "$dir" config commit.gpgsign false
}

write_land_stub() {
  local rc="$1"
  mkdir -p "$T/box/scripts"
  printf '%s\n' '#!/usr/bin/env bash' "exit ${rc}" > "$T/box/scripts/land.sh"
  chmod +x "$T/box/scripts/land.sh"
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

# Descendant tip on a second bare remote, used for land.sh stub plants.
git init -q --bare "$T/desc.git"
git clone -q "$T/box" "$T/agentw"
git -C "$T/agentw" config user.email test@example.com
git -C "$T/agentw" config user.name test
git -C "$T/agentw" config commit.gpgsign false
printf 'more\n' >> "$T/agentw/README"
git -C "$T/agentw" add README
git -C "$T/agentw" commit -q -m extra
git -C "$T/agentw" push -q "$T/desc.git" HEAD:refs/heads/agent/ssr-skips
desc_tip="$(git -C "$T/agentw" rev-parse HEAD)"
export LAND_AGENT_REMOTE="$T/desc.git"

# Pre-existing bot/ssr-skips must be refused and left in place (not created by this run).
git -C "$T/box" branch bot/ssr-skips "$box_main"
check "pre-existing bot/ssr-skips refused" 1 \
  "$SCRIPT" "agent/ssr-skips"
if git -C "$T/box" show-ref --verify --quiet refs/heads/bot/ssr-skips; then
  pre="$(git -C "$T/box" rev-parse refs/heads/bot/ssr-skips)"
  if [ "$pre" != "$box_main" ]; then
    echo "FAIL pre-existing bot/ssr-skips was rewritten ($box_main -> $pre)"
    fail=1
  else
    echo "ok   pre-existing bot/ssr-skips left in place"
  fi
else
  echo "FAIL pre-existing bot/ssr-skips was deleted"
  fail=1
fi
git -C "$T/box" branch -D bot/ssr-skips >/dev/null

# A leftover bot/<other> must survive a failed land of a different slice.
git -C "$T/box" branch bot/keepme "$box_main"
write_land_stub 1
check "land.sh failure rc" 1 \
  "$SCRIPT" "agent/ssr-skips"
if ! grep -q "land.sh failed" "$T/err"; then
  echo "FAIL land.sh failure: stderr did not name land.sh"
  sed 's/^/  | stderr: /' "$T/err"
  fail=1
else
  echo "ok   land.sh failure names land.sh"
fi
assert_no_slice ssr-skips "failed land"
if git -C "$T/box" show-ref --verify --quiet refs/heads/bot/keepme; then
  echo "ok   pre-existing bot/keepme untouched after failed land"
else
  echo "FAIL pre-existing bot/keepme was deleted"
  fail=1
fi
git -C "$T/box" branch -D bot/keepme >/dev/null

# Success path: stub land.sh exits 0; helper must still remove worktree and ref.
write_land_stub 0
check "land.sh success rc" 0 \
  "$SCRIPT" "agent/ssr-skips"
if ! grep -q "landed main=" "$T/out"; then
  echo "FAIL land.sh success: stdout did not print landed main"
  sed 's/^/  | stdout: /' "$T/out"
  fail=1
else
  echo "ok   land.sh success prints landed main"
fi
assert_no_slice ssr-skips "successful land"

got="$(git -C "$T/desc.git" rev-parse refs/heads/agent/ssr-skips)"
if [ "$got" != "$desc_tip" ]; then
  echo "FAIL remote agent/ssr-skips was rewritten ($desc_tip -> $got)"
  fail=1
else
  echo "ok   remote agent/ssr-skips untouched"
fi

now="$(git -C "$T/box" rev-parse HEAD)"
if [ "$now" != "$box_main" ]; then
  echo "FAIL box main moved during stub lands ($box_main -> $now)"
  fail=1
else
  echo "ok   box main unchanged after stub lands"
fi

exit "$fail"
