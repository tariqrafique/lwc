#!/usr/bin/env bash
# S5 (linux-x64-gnu; S6 adds triples): `npm run build` (napi build --platform --release) in crates/lwc-compiler-napi,
# then `npm pack` the main package and the per-triple package into $1. No publish (D-0202).
set -euo pipefail
# usage: napi-pack.sh <out-dir> [triple] [repo-root]
# repo-root defaults to this script's repository (the runner); official-board
# passes the snapshot so the runner packs the boarded tree.
if [ -n "${3:-}" ]; then
  root="$(cd "$3" && pwd)"
else
  root="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
fi
out="$(mkdir -p "${1:?usage: napi-pack.sh <out-dir> [triple] [repo-root]}" && cd "$1" && pwd)"
triple="${2:-linux-x64-gnu}"
pkg="$root/crates/lwc-compiler-napi"
cd "$pkg"
# --locked reaches `cargo build`, but napi's own unlocked `cargo metadata` can still rewrite Cargo.lock
# (AB-L0059 r2): fail if the lock changed across the build.
# napi's CLI wraps rustc; --cargo-flags=--locked plus an unchanged-lock
# check is the equivalent of cargo_locked (AB-L0059).
lock_before="$(sha256sum "$root/Cargo.lock")"
npm run build -- --cargo-flags=--locked
[ "$(sha256sum "$root/Cargo.lock")" = "$lock_before" ] || { echo "napi-pack: Cargo.lock changed during napi build (not --locked-clean)" >&2; exit 1; }
node="lwc-compiler.$triple.node"
[ -f "$node" ] || { echo "napi-pack: $pkg/$node not built" >&2; exit 1; }
cp "$node" "npm/$triple/$node"
npm pack --silent --pack-destination "$out" > /dev/null
(cd "npm/$triple" && npm pack --silent --pack-destination "$out" > /dev/null)
rm -f "npm/$triple/$node" napi.gen.d.ts
ls -l "$out"/*.tgz
