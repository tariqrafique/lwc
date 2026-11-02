#!/usr/bin/env bash
# OFFICIAL board from a FROZEN source snapshot. Other agents edit crates/ concurrently; building
# from the live tree mixes edits into the binary mid-run (seen 2026-11-01 20:16, 20:18). This:
#   1. copies Cargo.toml/Cargo.lock/crates/ + ../parse5-rust to a snapshot dir, hashes it (src sha)
#   2. in the snapshot with a FRESH target dir (== cargo clean): cargo test lwc-shared/lwc-compiler/napi -> abort on failure
#   3. builds release napi from the snapshot, installs atomically into harness/
#   4. runs harness/board.sh with SHIM_ROOT=<snapshot>/crates
#   5. re-hashes the snapshot; aborts if it changed (no false-pass path)
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$HOME/.cargo/env" 2>/dev/null || true
snap="$(mktemp -d /tmp/lwc-board-snap.XXXXXX)"
trap 'rm -rf "$snap"' EXIT
# Layout mirrors /workspace so the relative path dep ../../../parse5-rust resolves inside the snapshot.
mkdir -p "$snap/repo" && cp -a "$root/Cargo.toml" "$root/Cargo.lock" "$root/crates" "$snap/repo/"
(cd "$root/../parse5-rust" && find . -type f ! -path "./target/*" ! -path "./.git/*" -print0 | tar --null -T - -cf - | (mkdir -p "$snap/parse5-rust" && tar -xf - -C "$snap/parse5-rust"))
srchash() { (cd "$snap" && find repo parse5-rust -type f ! -path '*/target/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-12); }
h0="$(srchash)"
echo "src snapshot sha256:$h0  ($snap)"
export CARGO_TARGET_DIR="$snap/target"
snap_ws="$snap/repo"
(cd "$snap_ws" && cargo check --workspace --all-targets) || { echo "!! BOARD ABORTED: snapshot $h0 does not compile" >&2; exit 3; }
(cd "$snap_ws" && cargo test -p lwc-shared -p lwc-compiler -p lwc-compiler-napi) || { echo "!! BOARD ABORTED: seam crate tests fail @ $h0" >&2; exit 3; }
(cd "$snap_ws" && cargo build -p lwc-compiler-napi --release)
dst="$root/harness/lwc-compiler-napi.node"; tmp="$(mktemp "$root/harness/.lwc-compiler-napi.XXXXXX")"
cp "$CARGO_TARGET_DIR/release/liblwc_compiler_napi.so" "$tmp"; mv -f "$tmp" "$dst"
echo "installed napi $(sha256sum "$dst" | cut -c1-12) from src $h0"
rc=0; SHIM_ROOT="$snap_ws/crates" "$root/harness/board.sh" "$@" || rc=$?
[ "$(srchash)" == "$h0" ] || { echo "!! BOARD INVALID: snapshot mutated during run" >&2; exit 4; }
echo "official: src=$h0 rc=$rc"
exit $rc
