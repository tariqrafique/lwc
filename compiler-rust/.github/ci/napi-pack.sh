#!/usr/bin/env bash
# S5 (linux-x64-gnu; S6 adds triples): `npm run build` (napi build --platform --release) in crates/lwc-compiler-napi,
# then `npm pack` the main package and the per-triple package into $1. No publish (D-0202).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$(mkdir -p "${1:?usage: napi-pack.sh <out-dir> [triple]}" && cd "$1" && pwd)"
triple="${2:-linux-x64-gnu}"
pkg="$root/crates/lwc-compiler-napi"
cd "$pkg"
# --locked reaches `cargo build`, but napi's own unlocked `cargo metadata` can still rewrite Cargo.lock
# (AB-L0059 r2): fail if the lock changed across the build.
lock_before="$(sha256sum "$root/Cargo.lock")"
npm run build -- --cargo-flags=--locked
[ "$(sha256sum "$root/Cargo.lock")" = "$lock_before" ] || { echo "napi-pack: Cargo.lock changed during napi build (not --locked-clean)" >&2; exit 1; }
node="lwc-compiler.$triple.node"
[ -f "$node" ] || { echo "napi-pack: $pkg/$node not built" >&2; exit 1; }
cp "$node" "npm/$triple/$node"
npm pack --silent --pack-destination "$out" > /dev/null
(cd "npm/$triple" && npm pack --silent --pack-destination "$out" > /dev/null)
rm -f "npm/$triple/$node"
ls -l "$out"/*.tgz
