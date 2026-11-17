#!/usr/bin/env bash
# Plants for the derived rust-test input list: --check fails if a scanned
# input is dropped, board.sh still skips rust tests, and ssr-emit-pairs.ts
# selects cargo test -p semdiff (the exec_ssr input).
set -euo pipefail
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
repo="$(cd "$here/.." && pwd)"
fail=0
say() { printf '%s\n' "$*"; }
ok() { say "ok   $*"; }
bad() { say "FAIL $*"; fail=1; }

helper="$here/rust-test-inputs.sh"
list="$here/rust-test-inputs.tsv"
[ -x "$helper" ] || bad "missing $helper"
[ -f "$list" ] || bad "missing $list"

if "$helper" --check; then
  ok "--check matches the scan"
else
  bad "--check failed on the committed list"
fi

tmp="$(mktemp -d /tmp/test-rust-test-inputs.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

awk -F'\t' '$1=="#" || $1=="" || $1 != "harness/scripts/ssr-emit-pairs.ts"' "$list" > "$tmp/plant.tsv"
if RUST_TEST_INPUTS_TSV="$tmp/plant.tsv" "$helper" --check >"$tmp/check.out" 2>&1; then
  bad "list missing ssr-emit-pairs.ts was accepted"
  sed 's/^/       /' "$tmp/check.out"
else
  grep -q 'ssr-emit-pairs.ts' "$tmp/check.out" || bad "--check failure should name ssr-emit-pairs.ts"
  ok "omitting ssr-emit-pairs.ts from the list fails --check"
fi

got="$(printf '%s\n' 'harness/board.sh' | "$helper" --decide)"
if [ "$got" = skip ]; then
  ok "harness/board.sh decides skip"
else
  bad "harness/board.sh should skip rust tests, got: $got"
fi

got="$(printf '%s\n' 'harness/scripts/ssr-emit-pairs.ts' | "$helper" --decide)"
case "$got" in
  packages\ -p\ semdiff*|packages\ *\ -p\ semdiff*)
    ok "ssr-emit-pairs.ts decides $got"
    ;;
  *)
    bad "ssr-emit-pairs.ts should select packages -p semdiff, got: $got"
    ;;
esac

got="$(printf '%s\n' 'crates/lwc-ssr/src/lib.rs' | "$helper" --decide)"
if [ "$got" = workspace ]; then
  ok "a .rs change decides workspace"
else
  bad "a .rs change should decide workspace, got: $got"
fi

grep -q 'rust-test-inputs.sh --decide' "$repo/scripts/land.sh" || bad "land.sh does not call rust-test-inputs.sh --decide"
ok "land.sh uses --decide"

if [ "$fail" -ne 0 ]; then
  say "test-rust-test-inputs: FAILED"
  exit 1
fi
say "test-rust-test-inputs: ok"
