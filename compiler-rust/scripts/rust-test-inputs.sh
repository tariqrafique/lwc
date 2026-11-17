#!/usr/bin/env bash
# Derive and check the set of non-crate-source paths Rust tests/build.rs read.
# Usage:
#   rust-test-inputs.sh --check
#   rust-test-inputs.sh --write
#   rust-test-inputs.sh --scan
#   rust-test-inputs.sh --decide   # changed paths on stdin; prints workspace | packages -p … | skip
set -euo pipefail
SELF="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SELF/.." && pwd)"
SCANNER="$SELF/rust-test-inputs.py"
LIST="${RUST_TEST_INPUTS_TSV:-$SELF/rust-test-inputs.tsv}"

usage() { echo "usage: rust-test-inputs.sh --check|--write|--scan|--decide" >&2; exit 2; }
[ $# -ge 1 ] || usage
cmd="$1"
case "$cmd" in
  --scan) exec python3 "$SCANNER" "$ROOT" --scan ;;
  --write) exec python3 "$SCANNER" "$ROOT" --write "$LIST" ;;
  --check) exec python3 "$SCANNER" "$ROOT" --check "$LIST" ;;
  --decide) exec python3 "$SCANNER" "$ROOT" --decide "$LIST" ;;
  *) usage ;;
esac
