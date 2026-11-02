#!/usr/bin/env bash
# OFFICIAL board, built ONLY from a named commit on main (git, Migrate 2026-11-01). The only official path.
#   0. source = `git archive <commit>` of /workspace/lwc-compiler-rust (default ref: main; must be on main's history),
#      parse5-rust = `git archive HEAD` (refused if its tree is dirty), notes = a commit of the /workspace/notes repo.
#      Nothing is read from any working tree, so concurrent edits cannot leak in.
#   1. layout: $snap/{repo,parse5-rust,notes} (../../../parse5-rust resolves) + harness at $hsnap/harness (depth 3); src sha over all
#   1b. golden-gate: snapshot goldens must equal a fresh JS-backend recording (oracle b9e8007) -> abort otherwise
#   2. fresh target dir (== cargo clean): cargo check --workspace --all-targets + seam crate tests -> abort on failure
#   3. napi built from the snapshot; installed ONLY into the SNAPSHOT harness (never the live one; no writes outside the snapshot except /workspace/push + store publish)
#   4. board.sh runs from the snapshot harness (tests+goldens frozen too); shim-gate + option-gate read snapshot crates (CRATES_ROOT)
#   5. re-hash; mutation => INVALID (exit 4)
#   6. VALID run (snapshot unchanged, board rc 0) => /workspace/push/official-<srcsha>.tgz + official-<srcsha>.board.txt
#   6b. template-diag row (diagnostic): driver built into $snap/target/template-diag-driver from the snapshot repo; its
#       report JSON ($snap/template-diag, outside the hashed tree) -> official-<srcsha>.template-diag.json (store: template-diag.json)
#   7. VALID run also published to the official store (scripts/publish-official.sh), recording the commit SHAs
# Usage: BOARD_COMMIT=<ref on main> scripts/official-board.sh [rows...]
set -euo pipefail
MAIN=/workspace/lwc-compiler-rust; P5=/workspace/parse5-rust; NOTES=/workspace/notes
root="$MAIN"
push=/workspace/push
source "$HOME/.cargo/env" 2>/dev/null || true
die3() { echo "!! BOARD ABORTED: $*" >&2; exit 3; }
ref="${BOARD_COMMIT:-main}"
commit="$(git -C "$MAIN" rev-parse --verify -q "$ref^{commit}")" || die3 "unknown ref '$ref'"
git -C "$MAIN" merge-base --is-ancestor "$commit" main || die3 "$ref ($commit) is not on main (official boards build only from main)"
[ -z "$(git -C "$P5" status --porcelain)" ] || die3 "parse5-rust working tree is dirty; commit it first"
p5commit="$(git -C "$P5" rev-parse HEAD)"
# notes are edited in place (not landed): snapshot them as a commit so the board names exactly what it used
git -C "$NOTES" add -A && { git -C "$NOTES" diff --cached --quiet || git -C "$NOTES" commit -qm "notes snapshot for official board @ ${commit:0:12}"; }
notescommit="$(git -C "$NOTES" rev-parse HEAD)"
snap="$(mktemp -d /tmp/lwc-board-snap.XXXXXX)"   # Rust build: $snap/repo + $snap/parse5-rust (../../../parse5-rust resolves)
hsnap="$(mktemp -d /tmp/lwc-board-h.XXXXXX)"     # harness: $hsnap/harness (depth 3)
trap 'rm -rf "$snap" "$hsnap"' EXIT
mkdir -p "$snap/repo" "$snap/parse5-rust" "$snap/notes" "$hsnap"
git -C "$MAIN" archive "$commit" | tar -x -C "$snap/repo"
git -C "$P5" archive "$p5commit" | tar -x -C "$snap/parse5-rust"
git -C "$NOTES" archive "$notescommit" lwc-rust-migrate | tar -x -C "$snap/notes" --strip-components=1
mv "$snap/repo/harness" "$hsnap/harness"
"$MAIN/harness/scripts/ensure-deps.sh"
ln -s "$MAIN/harness/node_modules" "$hsnap/harness/node_modules"
srchash() { (find "$snap/repo" "$snap/parse5-rust" "$snap/notes" "$hsnap/harness" -type f ! -path "$snap/target/*" \
  ! -path "$hsnap/harness/.pins/*" ! -path "$hsnap/harness/.vitest/*" ! -path "$hsnap/harness/.csr-render-out/*" \
  ! -path "$hsnap/harness/.ssr-render-out/*" ! -name '*.node' -print0 | sort -z | xargs -0 sha256sum \
  | sed -e "s#$snap/##" -e "s#$hsnap/##" | sha256sum | cut -c1-12); }
h0="$(srchash)"
# External inputs not in the source tree (template-diag corpus + oracle's git-ignored template-compiler dist).
# Expected hashes are pinned in harness/external-inputs.lock (committed; changing a pin is a reviewed landing).
# Mismatch vs pin, or any change during the run => BOARD INVALID.
inputhash1() { (cd /workspace && find "$1" -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum) | sha256sum | cut -c1-12; }
inputshash() { for i in "${EXT_INPUTS[@]}"; do printf '%s\t%s\n' "$i" "$(inputhash1 "$i")"; done; }
lock="$hsnap/harness/external-inputs.lock"
[ -f "$lock" ] || { echo "!! BOARD INVALID: missing harness/external-inputs.lock at $commit" >&2; exit 4; }
mapfile -t EXT_INPUTS < <(grep -v '^#' "$lock" | sed '/^$/d' | cut -f1)   # input list = the lock's paths (single source)
[ ${#EXT_INPUTS[@]} -gt 0 ] || { echo "!! BOARD INVALID: external-inputs.lock lists no inputs" >&2; exit 4; }
in0="$(inputshash)"
if [ "$(grep -v '^#' "$lock" | sed '/^$/d')" != "$in0" ]; then
  echo "!! BOARD INVALID: external inputs differ from harness/external-inputs.lock (drift since pin):" >&2
  diff <(grep -v '^#' "$lock" | sed '/^$/d') <(echo "$in0") >&2 || true; exit 4; fi
in0sha="$(echo "$in0" | sha256sum | cut -c1-12)"
p5="$(cd "$snap/parse5-rust" && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-12)"
echo "commit $commit  src snapshot sha256:$h0  parse5-rust ${p5commit:0:12} sha256:$p5  notes ${notescommit:0:12}  ($snap, $hsnap)"
export CARGO_TARGET_DIR="$snap/target"
ws="$snap/repo"
GOLDEN_GATE_PROGRESS="$snap/golden.progress" "$hsnap/harness/scripts/golden-gate.sh" "$hsnap/harness" \
  || { echo "!! BOARD ABORTED: goldens @ $h0 != fresh JS-oracle recording" >&2; exit 3; }
(cd "$ws" && cargo check --workspace --all-targets) || { echo "!! BOARD ABORTED: snapshot $h0 does not compile" >&2; exit 3; }
(cd "$ws" && cargo test -p lwc-shared -p lwc-compiler -p lwc-compiler-napi) || { echo "!! BOARD ABORTED: seam crate tests fail @ $h0" >&2; exit 3; }
(cd "$ws" && cargo build -p lwc-compiler-napi --release)
so="$CARGO_TARGET_DIR/release/liblwc_compiler_napi.so"
cp "$so" "$hsnap/harness/lwc-compiler-napi.node"
napi="$(sha256sum "$so" | cut -c1-12)"
echo "built napi $napi from src $h0"
tdiag="$snap/template-diag"   # board.sh writes template-diag-<engine>.json here (not hashed, not in the tgz)
set +e; CRATES_ROOT="$ws/crates" BOARD_REPO_ROOT="$ws" TEMPLATE_DIAG_OUT_DIR="$tdiag" "$hsnap/harness/board.sh" "$@" 2>&1 | tee "$snap/board.log"; rc=${PIPESTATUS[0]}; set -e
# belt and braces: any gate failure line voids the run regardless of rc plumbing
grep -q '^!! ' "$snap/board.log" && [ "$rc" -eq 0 ] && { echo "!! rc=0 but board printed a failure; forcing rc=1" >&2; rc=1; }
[ "$(srchash)" == "$h0" ] || { echo "!! BOARD INVALID: snapshot mutated during run" >&2; exit 4; }
[ "$(inputshash)" == "$in0" ] || { echo "!! BOARD INVALID: external inputs (style-corpus/lbc*, oracle template-compiler dist) changed during run" >&2; exit 4; }
echo "official: src=$h0 napi=$napi rc=$rc"
# a full official run must carry the template-diag report (board.sh already fails the row if it is missing/invalid)
for e in crate napi; do if [ "$rc" -eq 0 ] && [ $# -eq 0 ] && [ ! -s "$tdiag/template-diag-$e.json" ]; then
  echo "!! BOARD FAILED: template-diag report missing ($tdiag/template-diag-$e.json); forcing rc=1" >&2; rc=1; fi; done
if [ "$rc" -eq 0 ] && [ $# -eq 0 ]; then
  mkdir -p "$push"
  base="$push/official-$h0"
  { echo "official board  commit=${commit:0:12}  src=$h0  napi=$napi  parse5-rust=$p5 (build dep, NOT in tgz)  $(date -Iseconds)"
    sed -n '/^napi sha256:/,$p' "$snap/board.log"
    # CSR failure clusters (Migrate 2026-11-01): counts per cluster for each corpus
    for c in ssr template; do rep="$hsnap/harness/.csr-render-out/rust-$c/report-$c.json"
      [ -f "$rep" ] || { echo "!! csr clusters: missing $rep"; continue; }
      echo "csr-clusters[$c]: $(node "$hsnap/harness/scripts/csr-render-clusters.mjs" "$rep" | grep -E '^(pass |\[)' | sed -E 's/^\[([0-9]+)\] ([a-z-]+):.*/\2=\1/' | paste -sd' ')"
    done; } > "$base.board.txt.tmp"
  # per-fixture pass/fail (fixture<TAB>pass<TAB>status<TAB>goldenStatus) so any two official runs can be diffed
  for c in ssr template; do rep="$hsnap/harness/.csr-render-out/rust-$c/report-$c.json"; [ -f "$rep" ] || continue
    node -e 'const r=require(process.argv[1]);for(const x of r.rows)console.log([x.fixture,x.pass,x.status,x.goldenStatus].join("\t"))' "$rep" > "$base.csr-$c.tsv"; done
  cp "$tdiag/template-diag-crate.json" "$base.template-diag.json"; cp "$tdiag/template-diag-napi.json" "$base.template-diag-napi.json"
  # tgz layout = repo root (harness under harness/) + notes/ (CUTOVER, DIAG_HELPER, decisions as of run start)
  cp -a "$hsnap/harness" "$ws/harness"; rm -f "$ws/harness/node_modules" "$ws/harness/lwc-compiler-napi.node"
  rm -rf "$ws/harness/.pins" "$ws/harness/.vitest" "$ws/harness/.csr-render-out" "$ws/harness/.ssr-render-out"
  tar -C "$snap" --exclude=./repo/target -czf "$base.tgz.tmp" --transform 's#^repo/##;s#^repo$#.#' repo notes
  mv -f "$base.tgz.tmp" "$base.tgz"; mv -f "$base.board.txt.tmp" "$base.board.txt"
  echo "push artifacts: $base.tgz  $base.board.txt  $base.template-diag.json  $base.template-diag-napi.json"
  # Durable publish (Migrate standing rule): survives box restores; Migrate pushes ONLY from here.
  # Oracle provenance = what board.sh actually verified (pinned + clean), parsed from its single header line.
  hdr="$(grep -E '^napi sha256:' "$snap/board.log")" || { echo "!! PUBLISH REFUSED: no board.sh header in board.log" >&2; exit 5; }
  [ "$(wc -l <<< "$hdr")" -eq 1 ] || { echo "!! PUBLISH REFUSED: expected 1 board.sh header, got $(wc -l <<< "$hdr")" >&2; exit 5; }
  ocommit="$(sed -nE 's/.* oracle=([0-9a-f]{40})\(clean\) .*/\1/p' <<< "$hdr")"; orepo="$(sed -nE 's/.* oracle_repo=([^ ]+) .*/\1/p' <<< "$hdr")"
  [ -n "$ocommit" ] && [ -n "$orepo" ] || { echo "!! PUBLISH REFUSED: cannot parse verified oracle from header: $hdr" >&2; exit 5; }
  store="${OFFICIAL_STORE:-/cursor/stores/store-4280618e-f4d8-4614-b1b6-bf88ea27f849/official}"
  OFFICIAL_COMMIT="$commit" OFFICIAL_PARSE5_COMMIT="$p5commit" OFFICIAL_NOTES_COMMIT="$notescommit" OFFICIAL_INPUTS_SHA="$in0sha" OFFICIAL_ORACLE_COMMIT="$ocommit" OFFICIAL_ORACLE_REPO="$orepo" \
    "$snap/repo/scripts/publish-official.sh" "$h0" "$napi" "$p5" "$store"
else
  echo "no push artifacts (rc=$rc${1:+, partial row set})"
fi
exit $rc
