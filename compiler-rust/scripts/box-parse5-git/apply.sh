#!/usr/bin/env bash
# Apply D-0238 box resolution. Call ONLY at Migrate cutover, under the land flock,
# immediately before (or as the first step of) landing the git-source lock.
# Refuses unless APPLY_PARSE5_GIT_CUTOVER=1.
set -euo pipefail
[ "${APPLY_PARSE5_GIT_CUTOVER:-}" = 1 ] || {
  echo "apply.sh: refusing to touch live box config (set APPLY_PARSE5_GIT_CUTOVER=1 at cutover)" >&2
  exit 2
}
HERE="$(cd "$(dirname "$0")" && pwd)"
CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}"
DEST_CARGO="${DEST_CARGO:-/workspace/.cargo/config.toml}"
DEST_REWRITE="${DEST_REWRITE:-/workspace/.cargo/parse5-url-rewrite.gitconfig}"
DEST_GITCONFIG="${DEST_GITCONFIG:-${HOME}/.gitconfig}"
mkdir -p "$(dirname "$DEST_CARGO")"
cp "$HERE/cargo-config.toml" "$DEST_CARGO"
cp "$HERE/url-rewrite.gitconfig" "$DEST_REWRITE"
marker='includeIf "gitdir:'"$CARGO_HOME"'/git/"'
if [ -f "$DEST_GITCONFIG" ] && grep -Fq "$marker" "$DEST_GITCONFIG"; then
  echo "apply.sh: includeIf already present in $DEST_GITCONFIG"
else
  {
    echo
    echo "[includeIf \"gitdir:${CARGO_HOME}/git/\"]"
    echo "	path = ${DEST_REWRITE}"
  } >> "$DEST_GITCONFIG"
fi
echo "apply.sh: wrote $DEST_CARGO (no [patch], net.git-fetch-with-cli=true)"
echo "apply.sh: wrote $DEST_REWRITE and includeIf in $DEST_GITCONFIG"
