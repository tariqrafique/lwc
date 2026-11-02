#!/usr/bin/env bash
# Publish a VALID official board's artifacts to the durable store: <store>/<src>/{official-<src>.tgz, board.txt, OFFICIAL_BOARD.txt}.
# The store FS has no rename(2): files are copied in, OFFICIAL_BOARD.txt LAST as the completion marker carrying
# sha256s of the other two. Consumers must require OFFICIAL_BOARD.txt and verify those sha256s. Refuses to overwrite an existing <src> dir with different bytes.
# Usage: publish-official.sh <srcsha> <napisha> <parse5sha> <store-official-dir>
set -euo pipefail
src="$1" napi="$2" p5="$3" store="$4"
base="/workspace/push/official-$src"
[ -f "$base.tgz" ] && [ -f "$base.board.txt" ] || { echo "!! PUBLISH FAILED: missing $base.{tgz,board.txt}" >&2; exit 5; }
grep -q '^!! ' "$base.board.txt" && { echo "!! PUBLISH REFUSED: board.txt contains a failure line" >&2; exit 5; }
mkdir -p "$store"
stage="$(mktemp -d "/tmp/publish-$src.XXXXXX")"; trap 'rm -rf "$stage"' EXIT
cp "$base.tgz" "$stage/official-$src.tgz"; cp "$base.board.txt" "$stage/board.txt"
for f in "$base".csr-*.tsv; do [ -f "$f" ] && cp "$f" "$stage/${f##*/official-$src.}"; done
cat > "$stage/OFFICIAL_BOARD.txt" << META
OFFICIAL BOARD (VALID): produced only by scripts/official-board.sh after every gate passed
src_snapshot_sha256_12=$src
napi_sha256_12=$napi
parse5_rust_sha256_12=$p5   (build dep; not in tgz)
oracle=/workspace/lwc @ b9e8007 (clean)
gates=golden-gate(goldens == fresh JS-oracle recording) shim-gate(untagged=0) option-gate(all mapped) snapshot-unchanged board-rc=0
tgz_sha256=$(sha256sum "$stage/official-$src.tgz" | cut -d' ' -f1)
board_txt_sha256=$(sha256sum "$stage/board.txt" | cut -d' ' -f1)
published=$(date -Iseconds)
META
if [ -d "$store/$src" ]; then
  if cmp -s "$store/$src/official-$src.tgz" "$stage/official-$src.tgz"; then echo "store: $store/$src already published (identical)"; exit 0; fi
  echo "!! PUBLISH REFUSED: $store/$src exists with different bytes" >&2; exit 5
fi
mkdir -p "$store/$src"
cp "$stage/official-$src.tgz" "$stage/board.txt" "$store/$src/"; cp "$stage"/csr-*.tsv "$store/$src/" 2>/dev/null || true
cmp -s "$stage/official-$src.tgz" "$store/$src/official-$src.tgz" && cmp -s "$stage/board.txt" "$store/$src/board.txt" \
  || { echo "!! PUBLISH FAILED: store copy mismatch" >&2; exit 5; }
cp "$stage/OFFICIAL_BOARD.txt" "$store/$src/OFFICIAL_BOARD.txt"   # marker last
echo "published: $store/$src"
