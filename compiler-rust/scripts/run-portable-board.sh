#!/usr/bin/env bash
# Cloud / fresh-clone board (D-0280). Advisory when run off the box.
# Does not publish. Does not touch /workspace/lwc.
# Fetches nothing itself: caller runs fetch-oracle.sh / fetch-style-corpus.sh,
# then this script resolves inputs and calls the shared driver.
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
# Snapshot only when notes are absent: the resolver prefers /workspace/notes.
reg="$(notes_decisions_path)"
export SEMDIFF_REGISTRY="$reg" OPTGATE_DECISIONS="$reg"
# Explicit remote: unset PARSE5_DIR now means the box clone (parse5-pin-check.sh).
export PARSE5_DIR=remote
export NO_COLOR="${NO_COLOR:-1}"
export C8_TARBALLS="${PORTABLE_TARBALLS:-$root/.oracle/tarballs}"
export SPE_LOG="${PORTABLE_SPE_LOG:-$root/.oracle/style-fixtures.log}"
export BOARD_TXT="${PORTABLE_BOARD_TXT:-$root/.oracle/board.txt}"
board_log="${PORTABLE_BOARD_LOG:-$root/.oracle/board.log}"
exec "$runner/run-board.sh" "$root" "$board_log" "$@"
