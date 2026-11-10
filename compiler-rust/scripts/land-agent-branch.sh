#!/usr/bin/env bash
# Fetch an agent/* branch from tariqrafique/lwc and land it through land.sh (D-0281).
# Usage: scripts/land-agent-branch.sh <branch>
# Branch names must match ^agent/[A-Za-z0-9._-]+$. The tip must be a descendant of
# current main. Never deletes a remote branch, never force-pushes, never opens a PR.
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
if git -C "$MAIN" show-ref --verify --quiet "refs/heads/${bot}"; then
  die "local branch $bot already exists"
fi
if [ -e "$wt" ]; then
  die "worktree path already exists: $wt"
fi
mkdir -p "$(dirname "$wt")"
git -C "$MAIN" worktree add -b "$bot" "$wt" "$tip" \
  || die "could not create worktree $wt on $bot at $tip"
land_rc=0
(cd "$wt" && "$MAIN/scripts/land.sh") || land_rc=$?
if [ "$land_rc" -ne 0 ]; then
  die "land.sh failed (rc=$land_rc) in $wt"
fi
landed="$(git -C "$MAIN" rev-parse HEAD)"
echo "land-agent-branch: landed main=$landed"
if ! git -C "$MAIN" worktree remove "$wt"; then
  echo "!! land-agent-branch: landed, but worktree remove failed for $wt" >&2
  exit 2
fi
if ! git -C "$MAIN" branch -d "$bot"; then
  echo "!! land-agent-branch: landed, but could not delete $bot" >&2
  exit 2
fi
