#!/usr/bin/env bash
# Shared cargo chokepoint (AB-L0079): every cargo invocation in land.sh, board.sh,
# official-board.sh, and the gates they run must go through cargo_locked, which
# always passes --locked. Source this file; do not call cargo directly.
#
# There is exactly one `command cargo` line in this file (_cargo_exec). Every
# other caller goes through a named wrapper.
_cargo_exec() {
  command cargo "$@"
}
cargo_locked() {
  if [ "${1:-}" = --help ]; then
    echo "usage: cargo_locked [+toolchain] <locked-args...>" >&2
    return 2
  fi
  local -a _pre=()
  if [[ "${1:-}" == +* ]]; then
    _pre+=("$1")
    shift
  fi
  _pre+=(--locked)
  _cargo_exec "${_pre[@]}" "$@"
}
# generate-lockfile creates a lockfile, so --locked cannot apply. Hermetic
# fixture tests (scratch CARGO_HOME) call this so they contain no bare token.
cargo_generate_lockfile() {
  local -a _pre=()
  if [[ "${1:-}" == +* ]]; then
    _pre+=("$1")
    shift
  fi
  _cargo_exec "${_pre[@]}" generate-lockfile "$@"
}
