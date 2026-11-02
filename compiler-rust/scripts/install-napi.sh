#!/usr/bin/env bash
# Build release napi and install into harness ATOMICALLY (tmp + rename).
# Never `cp` over harness/lwc-compiler-napi.node: a running vitest fork may have it mmap'd -> SIGBUS.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$HOME/.cargo/env" 2>/dev/null || true
cd "$root"
cargo build -p lwc-compiler-napi --release
src="$root/target/release/liblwc_compiler_napi.so"
dst="$root/harness/lwc-compiler-napi.node"
tmp="$(mktemp "$root/harness/.lwc-compiler-napi.XXXXXX")"
cp "$src" "$tmp"
mv -f "$tmp" "$dst"   # rename(2): atomic; old inode stays valid for mapped readers
echo "installed $(sha256sum "$dst" | cut -c1-12) -> $dst"
