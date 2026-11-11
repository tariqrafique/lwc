#!/usr/bin/env bash
# Build release napi and install into harness ATOMICALLY (tmp + rename).
# Never `cp` over harness/lwc-compiler-napi.node: a running vitest fork may have it mmap'd -> SIGBUS.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/scripts/cargo-locked.sh"
if [ -f "$HOME/.cargo/env" ]; then source "$HOME/.cargo/env"; fi
cd "$root"
cargo_locked build -p lwc-compiler-napi --release
# The artifact lives in the target dir the build used: CARGO_TARGET_DIR (land.sh exports the shared
# lock+toolchain cache, D-0282) or else cargo metadata's target_directory. Never assume ./target.
tdir="${CARGO_TARGET_DIR:-}" how="CARGO_TARGET_DIR"
if [ -z "$tdir" ]; then
  how="cargo_locked metadata target_directory"
  tdir="$(cargo_locked metadata --format-version 1 --no-deps \
    | node -e 'process.stdout.write(JSON.parse(require("fs").readFileSync(0, "utf8")).target_directory)')" \
    || { echo "!! install-napi: cannot resolve the build target dir (cargo_locked metadata failed)" >&2; exit 1; }
fi
src="$tdir/release/liblwc_compiler_napi.so"
[ -f "$src" ] || { echo "!! install-napi: built artifact missing: $src (target dir $tdir from $how)" >&2; exit 1; }
dst="$root/harness/lwc-compiler-napi.node"
tmp="$(mktemp "$root/harness/.lwc-compiler-napi.XXXXXX")"
cp "$src" "$tmp"
mv -f "$tmp" "$dst"   # rename(2): atomic; old inode stays valid for mapped readers
echo "installed $(sha256sum "$dst" | cut -c1-12) -> $dst"
