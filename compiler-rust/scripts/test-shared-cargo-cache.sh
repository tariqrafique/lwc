#!/usr/bin/env bash
# D-0282: a build-script or test binary compiled under shared CARGO_TARGET_DIR
# must work after the source tree it was compiled from is deleted (official
# boards and worktrees do not share a path). The production cache is keyed by
# Cargo.lock+toolchain only — NOT the source root — because every official
# snap is a fresh /tmp/lwc-board-snap.* and keying on that path would discard
# oxc reuse.
#
# Plant: git-archive HEAD into A, locked check -p semdiff and locked test
# -p lwc-compiler-napi --test loader_guard, delete A, archive into B, same
# two commands with the same CARGO_TARGET_DIR. Must pass.
set -euo pipefail
R="$(cd "$(dirname "$0")/.." && pwd)"
source "$R/scripts/cargo-locked.sh"
LWC_ORACLE="$(node "$R/harness/scripts/oracle-root.mjs" oracle_root)" || {
  echo "!! test-shared-cargo-cache: oracle-root failed" >&2
  exit 2
}
export LWC_ORACLE
export LWC_ROOT="$LWC_ORACLE"
CACHE="${CARGO_TARGET_DIR:-}"
if [ -z "$CACHE" ]; then
  CACHE="$(mktemp -d /tmp/shared-cache-target.XXXXXX)"
fi
mkdir -p "$CACHE"
export CARGO_TARGET_DIR="$CACHE"
A="$(mktemp -d /tmp/shared-cache-A.XXXXXX)"
B="$(mktemp -d /tmp/shared-cache-B.XXXXXX)"
cleanup() {
  rm -rf "$A" "$B"
  # Do not leave a build-script or test binary compiled against a now-deleted
  # archive path in the shared cache for the next official/land to trip over.
  rm -rf "$CACHE/debug/build/semdiff-"*
  rm -f "$CACHE/debug/deps/loader_guard-"*
}
trap cleanup EXIT
echo "shared-cargo-cache: CARGO_TARGET_DIR=$CACHE"
git -C "$R" archive HEAD | tar -C "$A" -xf -
echo "shared-cargo-cache: locked check -p semdiff in $A"
( cd "$A" && cargo_locked check -p semdiff )
echo "shared-cargo-cache: locked test -p lwc-compiler-napi --test loader_guard in $A"
( cd "$A" && cargo_locked test -p lwc-compiler-napi --test loader_guard )
echo "shared-cargo-cache: deleting $A"
rm -rf "$A"
A=""
git -C "$R" archive HEAD | tar -C "$B" -xf -
echo "shared-cargo-cache: locked check -p semdiff in $B after deleting A"
( cd "$B" && cargo_locked check -p semdiff )
echo "shared-cargo-cache: locked test -p lwc-compiler-napi --test loader_guard in $B after deleting A"
( cd "$B" && cargo_locked test -p lwc-compiler-napi --test loader_guard )
# Every workspace build.rs must read CARGO_MANIFEST_DIR at runtime, not bake it.
while IFS= read -r -d '' f; do
  if grep -n 'env!("CARGO_MANIFEST_DIR")' "$f" >/dev/null; then
    echo "!! test-shared-cargo-cache: $f bakes env!(\"CARGO_MANIFEST_DIR\") (use std::env::var at runtime)" >&2
    exit 1
  fi
done < <(find "$R/tools" "$R/crates" -name build.rs -print0)
# loader_guard is the test-binary representative: it must not bake env! itself.
if grep -n 'env!("CARGO_MANIFEST_DIR")' "$R/crates/lwc-compiler-napi/tests/loader_guard.rs" >/dev/null; then
  echo "!! test-shared-cargo-cache: loader_guard.rs bakes env!(\"CARGO_MANIFEST_DIR\")" >&2
  exit 1
fi
echo "ok   semdiff builds from path B after path A was deleted (shared CARGO_TARGET_DIR)"
echo "ok   loader_guard tests pass from path B after path A was deleted (shared CARGO_TARGET_DIR)"
