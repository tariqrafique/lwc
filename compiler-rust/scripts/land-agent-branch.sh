#!/usr/bin/env bash
# Fetch an agent/* branch from tariqrafique/lwc and land it through land.sh (D-0281).
# Usage: scripts/land-agent-branch.sh <branch>
# Branch names must match ^agent/[A-Za-z0-9._-]+$. The tip must be a descendant of
# current main. Never deletes a remote branch, never force-pushes, never opens a PR.
# Always removes the local bot/<slice> ref and ../wt/<slice> worktree this run created,
# including on land.sh failure, fetch/ancestry failure (no-op if not created), and
# EXIT/INT/TERM. Never uses git worktree remove --force; never touches a remote.
set -euo pipefail

die() { echo "!! land-agent-branch: $*" >&2; exit 1; }
die2() { echo "!! land-agent-branch: $*" >&2; exit 2; }

[ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ] && {
  echo "usage: land-agent-branch.sh <branch>" >&2
  exit 2
}
[ $# -eq 1 ] || die2 "usage: land-agent-branch.sh <branch>"
branch=$1
[[ "$branch" =~ ^agent/[A-Za-z0-9._-]+$ ]] || die2 "refuse name '$branch' (want ^agent/[A-Za-z0-9._-]+\$)"

HERE="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
MAIN="${LAND_AGENT_MAIN:-$(cd "$HERE/.." && pwd)}"
[ -d "$MAIN/.git" ] || [ -f "$MAIN/.git" ] || die2 "MAIN is not a git checkout: $MAIN"

GITHUB_FETCH="https://github.com/tariqrafique/lwc.git"
# Tests point LAND_AGENT_REMOTE at a local bare repo. Production leaves it unset.
remote_spec="${LAND_AGENT_REMOTE:-}"
if [ -z "$remote_spec" ]; then
  while read -r rname rurl _; do
    case "$rurl" in
      *github.com/tariqrafique/lwc.git|*github.com/tariqrafique/lwc)
        remote_spec=$rname
        break
        ;;
    esac
  done < <(git -C "$MAIN" remote -v | awk '$3=="(fetch)" { print $1, $2 }')
  if [ -z "$remote_spec" ]; then
    if git -C "$MAIN" remote get-url lwc-github >/dev/null 2>&1; then
      die2 "remote lwc-github exists but does not point at tariqrafique/lwc"
    fi
    git -C "$MAIN" remote add lwc-github "$GITHUB_FETCH"
    git -C "$MAIN" remote set-url --push lwc-github DISABLED
    remote_spec=lwc-github
  else
    # Existing matching remote: keep fetch, disable push if it is still live.
    if [ "$remote_spec" != "$GITHUB_FETCH" ] && [ "$remote_spec" != "${GITHUB_FETCH%.git}" ]; then
      pushurl="$(git -C "$MAIN" remote get-url --push "$remote_spec" 2>/dev/null || true)"
      case "$pushurl" in
        DISABLED|"") ;;
        *) git -C "$MAIN" remote set-url --push "$remote_spec" DISABLED ;;
      esac
    fi
  fi
fi

tmp_ref=refs/tmp/land-agent-tip
git -C "$MAIN" fetch --no-tags --no-write-fetch-head "$remote_spec" \
  "+refs/heads/${branch}:${tmp_ref}" \
  || die "fetch of $branch from $remote_spec failed"
tip="$(git -C "$MAIN" rev-parse --verify -q "${tmp_ref}^{commit}")" \
  || die "fetched tip for $branch is not a commit"
main_sha="$(git -C "$MAIN" rev-parse --verify -q 'main^{commit}')" \
  || die "cannot resolve main"
if ! git -C "$MAIN" merge-base --is-ancestor "$main_sha" "$tip"; then
  die "tip $tip of $branch is not a descendant of main $main_sha"
fi

if [ "${LAND_AGENT_SKIP_LAND:-}" = 1 ]; then
  echo "land-agent-branch: tip=$tip (LAND_AGENT_SKIP_LAND=1, not landing)"
  exit 0
fi

slice="${branch#agent/}"
bot="bot/${slice}"
wt="$(cd "$(dirname "$MAIN")" && pwd)/wt/${slice}"
created_branch=0
created_worktree=0
cleanup_rc=0
_land_agent_cleanup_done=0

worktree_registered() {
  local path="$1" line
  [ -n "$path" ] || return 1
  while IFS= read -r line; do
    [ "$line" = "worktree $path" ] && return 0
  done < <(git -C "$MAIN" worktree list --porcelain 2>/dev/null || true)
  return 1
}

cleanup() {
  if [ "${_land_agent_cleanup_done}" = 1 ]; then
    return 0
  fi
  _land_agent_cleanup_done=1

  if [ "${created_worktree}" = 1 ] && [ -n "${wt:-}" ] && [ -n "${MAIN:-}" ]; then
    if worktree_registered "$wt"; then
      if ! git -C "$MAIN" worktree remove "$wt"; then
        echo "!! land-agent-branch: worktree remove failed for $wt (not forcing; dirty tree must be cleaned by hand)" >&2
        cleanup_rc=2
        return 0
      fi
    fi
  fi

  # Only delete the bot/<slice> ref this run created. Never a pre-existing ref,
  # never anything outside that exact name, never a remote.
  if [ "${created_branch}" = 1 ] && [ -n "${bot:-}" ] && [ -n "${slice:-}" ]; then
    if [ "$bot" = "bot/${slice}" ] && [[ "$bot" =~ ^bot/[A-Za-z0-9._-]+$ ]]; then
      if git -C "$MAIN" show-ref --verify --quiet "refs/heads/${bot}"; then
        if ! git -C "$MAIN" branch -D -- "$bot"; then
          echo "!! land-agent-branch: could not delete $bot" >&2
          cleanup_rc=2
        fi
      fi
    fi
  fi
}

on_exit() {
  local incoming=$?
  cleanup
  if [ "$incoming" -eq 0 ] && [ "${cleanup_rc:-0}" -ne 0 ]; then
    exit "$cleanup_rc"
  fi
}
trap on_exit EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if git -C "$MAIN" show-ref --verify --quiet "refs/heads/${bot}"; then
  die "local branch $bot already exists"
fi
if [ -e "$wt" ]; then
  die "worktree path already exists: $wt"
fi
mkdir -p "$(dirname "$wt")"
add_rc=0
git -C "$MAIN" worktree add -b "$bot" "$wt" "$tip" || add_rc=$?
if git -C "$MAIN" show-ref --verify --quiet "refs/heads/${bot}"; then
  created_branch=1
fi
if worktree_registered "$wt"; then
  created_worktree=1
fi
[ "$add_rc" -eq 0 ] || die "could not create worktree $wt on $bot at $tip"

land_rc=0
(cd "$wt" && "$MAIN/scripts/land.sh") || land_rc=$?
if [ "$land_rc" -ne 0 ]; then
  die "land.sh failed (rc=$land_rc) in $wt"
fi
landed="$(git -C "$MAIN" rev-parse HEAD)"
echo "land-agent-branch: landed main=$landed"
