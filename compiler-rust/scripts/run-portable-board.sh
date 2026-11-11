#!/usr/bin/env bash
# Cloud / fresh-clone board (D-0280). Advisory when run off the box.
# Does not publish. Does not touch the box oracle checkout.
# Fetches nothing itself: caller runs fetch-oracle.sh / fetch-style-corpus.sh,
# then this script snapshots HEAD (D-0286: same mutation gate as official-board),
# resolves inputs, and calls the shared driver.
# Usage (from a clone of this repo):
#   export LWC_ORACLE=$PWD/.oracle/lwc
#   export STYLE_CORPUS=$PWD/.oracle/style-corpus
#   scripts/fetch-oracle.sh --build
#   scripts/fetch-style-corpus.sh
#   scripts/run-portable-board.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
runner="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
# shellcheck source=../harness/scripts/oracle-root.sh
source "$root/harness/scripts/oracle-root.sh"

export LWC_ORACLE="$(oracle_root)"
export LWC_ROOT="$LWC_ORACLE"
export STYLE_CORPUS="$(style_corpus_root)"
# Snapshot only when notes are absent: the resolver prefers the box notes checkout when present.
reg="$(notes_decisions_path)"
export SEMDIFF_REGISTRY="$reg" OPTGATE_DECISIONS="$reg"
# Explicit remote: unset PARSE5_DIR now means the box clone (parse5-pin-check.sh).
export PARSE5_DIR="${PARSE5_DIR:-remote}"
export NO_COLOR="${NO_COLOR:-1}"

git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1 \
  || { echo "!! run-portable-board: $root is not a git work tree" >&2; exit 2; }
_commit="$(git -C "$root" rev-parse HEAD)"
snap="$(mktemp -d "${TMPDIR:-/tmp}/lwc-portable-snap.XXXXXX")"
_target_tmp=""
on_portable_exit() {
  local rc=$?
  rm -rf "$snap"
  if [ -n "${_target_tmp:-}" ]; then rm -rf "$_target_tmp"; fi
  exit "$rc"
}
trap on_portable_exit EXIT
mkdir -p "$snap/repo"
git -C "$root" archive "$_commit" | tar -x -C "$snap/repo"

if [ -z "${CARGO_TARGET_DIR:-}" ]; then
  _target_tmp="$(mktemp -d "${TMPDIR:-/tmp}/lwc-portable-target.XXXXXX")"
  export CARGO_TARGET_DIR="$_target_tmp"
fi
# The snapshot has no .git. ssr-oracle-gate reads known-diffs at SSR_ORACLE_MAIN
# from this clone (official-board sets the same pair from its main checkout).
if [ -z "${SSR_ORACLE_GIT:-}" ]; then
  export SSR_ORACLE_GIT="$root"
fi
if [ -z "${SSR_ORACLE_MAIN:-}" ]; then
  export SSR_ORACLE_MAIN="$(git -C "$root" rev-parse HEAD)"
fi
# shim-gate lists the tree with git ls-files unless SHIM_FILE_LIST is set.
# The archive has no .git; give it the same commit listing official-board uses.
if [ -z "${SHIM_FILE_LIST:-}" ]; then
  SHIM_FILE_LIST="$snap/shim-file-list"
  git -C "$root" ls-tree -r -z --name-only "$_commit" > "$SHIM_FILE_LIST"
  export SHIM_FILE_LIST
fi
export C8_TARBALLS="${PORTABLE_TARBALLS:-$snap/tarballs}"
export BOARD_FOOTER_LOG_DIR="${PORTABLE_FOOTER_LOG_DIR:-$snap}"
export SPE_LOG="${PORTABLE_SPE_LOG:-$BOARD_FOOTER_LOG_DIR/style-fixtures.log}"
export STYLE_CORPUS_LOG="${PORTABLE_STYLE_CORPUS_LOG:-$BOARD_FOOTER_LOG_DIR/style-corpus.log}"
export NAPI_CSS_LOG="${PORTABLE_NAPI_CSS_LOG:-$BOARD_FOOTER_LOG_DIR/napi-css-errors.log}"
export BOARD_TXT="${PORTABLE_BOARD_TXT:-$snap/board.txt}"
board_log="${PORTABLE_BOARD_LOG:-$snap/board.log}"

portable_srchash() {
  (cd "$snap/repo" && find . -type f \
    ! -path './harness/node_modules/*' ! -path './harness/.pins/*' \
    ! -path './harness/.vitest/*' ! -path './harness/.csr-render-out/*' \
    ! -path './harness/.ssr-render-out/*' ! -path './harness/.lbc-out/*' ! -path './target/*' \
    ! -name '*.node' ! -name 'napi.gen.d.ts' -print0 \
    | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | awk '{print $1}')
}
h0="$(portable_srchash)"
set +e
"$runner/run-board.sh" "$snap/repo" "$board_log" "$@"
rc=$?
set -e
h1="$(portable_srchash)"
if [ "$h0" != "$h1" ]; then
  echo "!! PORTABLE BOARD INVALID: snapshot mutated during run" >&2
  exit 4
fi
if [ -n "${PORTABLE_BOARD_TXT:-}" ] && [ -f "$BOARD_TXT" ]; then
  mkdir -p "$(dirname "$PORTABLE_BOARD_TXT")"
  cp "$BOARD_TXT" "$PORTABLE_BOARD_TXT"
fi
if [ -n "${PORTABLE_BOARD_LOG:-}" ] && [ -f "$board_log" ]; then
  mkdir -p "$(dirname "$PORTABLE_BOARD_LOG")"
  cp "$board_log" "$PORTABLE_BOARD_LOG"
fi
exit "$rc"
