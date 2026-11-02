#!/usr/bin/env bash
# OFFICIAL board from a FROZEN source snapshot (the only official path; Migrate 2026-11-01).
# Other agents edit the tree concurrently; live-tree runs mixed edits into the binary mid-run (20:16, 20:18).
#   1. snapshot: Rust tree {Cargo.*, crates, scripts, tools, docs, README.md, decisions.tsv} + ../parse5-rust (build dep)
#      + harness source/goldens (no node_modules/.pins/render-out/.vitest/*.node) + ../notes/lwc-rust-migrate; hash it (src sha)
#   2. fresh target dir (== cargo clean): cargo check --workspace --all-targets + seam crate tests -> abort on failure
#   3. napi built from the snapshot; installed into the SNAPSHOT harness (and atomically into the live harness for others)
#   4. board.sh runs from the snapshot harness (tests+goldens frozen too); shim-gate + option-gate read snapshot crates (CRATES_ROOT)
#   5. re-hash; mutation => INVALID (exit 4)
#   6. VALID run (snapshot unchanged, board rc 0) => /workspace/push/official-<srcsha>.tgz + official-<srcsha>.board.txt
# Harness snapshot sits at depth 3 (/tmp/<dir>/harness) like the live one: rollup warning paths (../../../virtual-csr) are cwd-relative.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
notes="$root/../notes/lwc-rust-migrate"
push=/workspace/push
source "$HOME/.cargo/env" 2>/dev/null || true
snap="$(mktemp -d /tmp/lwc-board-snap.XXXXXX)"   # Rust build: $snap/repo + $snap/parse5-rust (../../../parse5-rust resolves)
hsnap="$(mktemp -d /tmp/lwc-board-h.XXXXXX)"     # harness: $hsnap/harness (depth 3)
trap 'rm -rf "$snap" "$hsnap"' EXIT
# copy <src-dir> <dst-dir>: regular files minus generated/heavy paths
copy() { mkdir -p "$2"; (cd "$1" && find . -type f \
  ! -path './target/*' ! -path '*/node_modules/*' ! -path './.git/*' ! -path './.pins/*' ! -path './.vitest/*' \
  ! -path './.csr-render-out/*' ! -path './.ssr-render-out/*' ! -name '*.node' ! -name '.lwc-compiler-napi.*' -print0 \
  | tar --null -T - -cf -) | tar -xf - -C "$2"; }
mkdir -p "$snap/repo"
cp -a "$root/Cargo.toml" "$root/Cargo.lock" "$root/README.md" "$root/decisions.tsv" "$snap/repo/"
for d in crates scripts tools docs; do copy "$root/$d" "$snap/repo/$d"; done
copy "$root/../parse5-rust" "$snap/parse5-rust"
copy "$root/harness" "$hsnap/harness"
copy "$notes" "$snap/notes"
ln -s "$root/harness/node_modules" "$hsnap/harness/node_modules"
srchash() { (find "$snap/repo" "$snap/parse5-rust" "$snap/notes" "$hsnap/harness" -type f ! -path "$snap/target/*" \
  ! -path "$hsnap/harness/.pins/*" ! -path "$hsnap/harness/.vitest/*" ! -path "$hsnap/harness/.csr-render-out/*" \
  ! -path "$hsnap/harness/.ssr-render-out/*" ! -name '*.node' -print0 | sort -z | xargs -0 sha256sum \
  | sed -e "s#$snap/##" -e "s#$hsnap/##" | sha256sum | cut -c1-12); }
h0="$(srchash)"
p5="$(cd "$snap/parse5-rust" && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-12)"
echo "src snapshot sha256:$h0  parse5-rust sha256:$p5  ($snap, $hsnap)"
export CARGO_TARGET_DIR="$snap/target"
ws="$snap/repo"
(cd "$ws" && cargo check --workspace --all-targets) || { echo "!! BOARD ABORTED: snapshot $h0 does not compile" >&2; exit 3; }
(cd "$ws" && cargo test -p lwc-shared -p lwc-compiler -p lwc-compiler-napi) || { echo "!! BOARD ABORTED: seam crate tests fail @ $h0" >&2; exit 3; }
(cd "$ws" && cargo build -p lwc-compiler-napi --release)
so="$CARGO_TARGET_DIR/release/liblwc_compiler_napi.so"
cp "$so" "$hsnap/harness/lwc-compiler-napi.node"
dst="$root/harness/lwc-compiler-napi.node"; tmp="$(mktemp "$root/harness/.lwc-compiler-napi.XXXXXX")"
cp "$so" "$tmp"; mv -f "$tmp" "$dst"
napi="$(sha256sum "$so" | cut -c1-12)"
echo "built napi $napi from src $h0"
set +e; CRATES_ROOT="$ws/crates" "$hsnap/harness/board.sh" "$@" 2>&1 | tee "$snap/board.log"; rc=${PIPESTATUS[0]}; set -e
# belt and braces: any gate failure line voids the run regardless of rc plumbing
grep -q '^!! ' "$snap/board.log" && [ "$rc" -eq 0 ] && { echo "!! rc=0 but board printed a failure; forcing rc=1" >&2; rc=1; }
[ "$(srchash)" == "$h0" ] || { echo "!! BOARD INVALID: snapshot mutated during run" >&2; exit 4; }
echo "official: src=$h0 napi=$napi rc=$rc"
if [ "$rc" -eq 0 ] && [ $# -eq 0 ]; then
  mkdir -p "$push"
  base="$push/official-$h0"
  { echo "official board  src=$h0  napi=$napi  parse5-rust=$p5 (build dep, NOT in tgz)  $(date -Iseconds)"
    sed -n '/^napi sha256:/,$p' "$snap/board.log"; } > "$base.board.txt.tmp"
  # tgz layout = repo root (harness under harness/) + notes/ (CUTOVER, DIAG_HELPER, decisions as of run start)
  cp -a "$hsnap/harness" "$ws/harness"; rm -f "$ws/harness/node_modules" "$ws/harness/lwc-compiler-napi.node"
  rm -rf "$ws/harness/.pins" "$ws/harness/.vitest" "$ws/harness/.csr-render-out" "$ws/harness/.ssr-render-out"
  tar -C "$snap" --exclude=./repo/target -czf "$base.tgz.tmp" --transform 's#^repo/##;s#^repo$#.#' repo notes
  mv -f "$base.tgz.tmp" "$base.tgz"; mv -f "$base.board.txt.tmp" "$base.board.txt"
  echo "push artifacts: $base.tgz  $base.board.txt"
else
  echo "no push artifacts (rc=$rc${1:+, partial row set})"
fi
exit $rc
