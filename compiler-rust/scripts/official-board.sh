#!/usr/bin/env bash
# OFFICIAL board, built ONLY from a named commit on main (git, Migrate 2026-11-01). The only official path.
#   0. source = `git archive <commit>` of /workspace/lwc-compiler-rust (default ref: main; must be on main's history),
#      parse5-rust = `git archive HEAD` (refused if its tree is dirty), notes = /workspace/notes HEAD as-is (never staged/committed here;
#      the notes paths the board reads must be clean at HEAD, else exit 3: scripts/notes-precheck.sh, AB-0016).
#      Nothing is read from any working tree, so concurrent edits cannot leak in.
#   1. layout: $snap/{repo,parse5-rust,notes} (../../../parse5-rust resolves); the repo layout is kept intact, harness stays at
#      $snap/repo/harness (tools/semdiff/build.rs reads ../../harness, AB-L0062 forward condition); src sha over all.
#      No parse5 [patch] is written here (D-0238/D-0273): Cargo.lock records the git source; the box resolves it via a
#      scoped git insteadOf (scripts/box-parse5-git/) outside the snapshot. Cargo invocations are --locked.
#   1b. golden-gate: snapshot goldens must equal a fresh JS-backend recording (oracle b9e8007) -> abort otherwise
#   2. fresh target dir (== cargo clean): cargo check/test --workspace --locked + seam crate tests -> abort on failure
#   3. napi built from the snapshot; installed ONLY into the SNAPSHOT harness (never the live one; no writes outside the snapshot except /workspace/push + store publish)
#   4. board.sh runs from the snapshot harness (tests+goldens frozen too); shim-gate + option-gate read snapshot crates (CRATES_ROOT);
#      the scanner ratchet baseline is the snapshot harness copy (SHIM_RATCHET_DIR=$ws/harness/ratchets); option-gate reads the
#      ARCHIVED notes ($snap/notes/decisions.tsv via OPTGATE_DECISIONS), never live /workspace/notes
#   5. re-hash; mutation => INVALID (exit 4)
#   6. VALID run (snapshot unchanged, board rc 0) => /workspace/push/official-<srcsha>.tgz + official-<srcsha>.board.txt
#   6b. template-diag row (diagnostic): driver built into $snap/target/template-diag-driver from the snapshot repo; its
#       report JSON ($snap/template-diag, outside the hashed tree) -> official-<srcsha>.template-diag.json (store: template-diag.json),
#       plus its soundness stamp template-diag-<engine>.soundness.json -> official-<srcsha>.template-diag[-napi].soundness.json
#   7. VALID run also published to the official store (scripts/publish-official.sh), recording the commit SHAs
# Usage: BOARD_COMMIT=<ref on main> scripts/official-board.sh [rows...]
set -euo pipefail
MAIN=/workspace/lwc-compiler-rust; P5=/workspace/parse5-rust; NOTES=/workspace/notes
root="$MAIN"
push=/workspace/push
store="${OFFICIAL_STORE:-/cursor/stores/store-4280618e-f4d8-4614-b1b6-bf88ea27f849/official}"

# >>> parse5-lock-rev (exercised by scripts/test-parse5-pin.sh)
# Parse Cargo.lock's parse5 `source = "git+...?rev=<rev>#<rev>"` line. Empty stdout = missing.
parse5_lock_rev() {
  local lock="$1" src rev frag
  [ -f "$lock" ] || return 1
  src="$(awk '
    $0 == "[[package]]" { if (in_pkg && !saw) exit; in_pkg=0; saw=0 }
    /^name = "parse5"$/ { in_pkg=1 }
    in_pkg && /^source = / { print; saw=1; exit }
  ' "$lock")"
  [ -n "$src" ] || return 1
  rev="$(sed -nE 's/.*[?]rev=([0-9a-f]+)#([0-9a-f]+)".*/\1/p' <<< "$src")"
  frag="$(sed -nE 's/.*[?]rev=([0-9a-f]+)#([0-9a-f]+)".*/\2/p' <<< "$src")"
  [ -n "$rev" ] || return 1
  if [ "$rev" != "$frag" ]; then
    echo "!! parse5 lock rev mismatch (?rev=$rev #$frag)" >&2
    return 1
  fi
  printf '%s\n' "$rev"
}
require_parse5_lock_rev() {
  local lock="$1" p5dir="$2" rev err
  err="$(mktemp)"
  rev="$(parse5_lock_rev "$lock" 2>"$err")" || {
    if grep -q mismatch "$err"; then
      cat "$err" >&2
    else
      echo "!! parse5 lock rev missing from $lock (want source = git+...?rev=<rev>#<rev>)" >&2
    fi
    rm -f "$err"
    return 2
  }
  rm -f "$err"
  [[ "$rev" =~ ^[0-9a-f]{40}$ ]] || { echo "!! parse5 lock rev '$rev' is not a 40-hex commit" >&2; return 2; }
  git -C "$p5dir" rev-parse -q --verify "$rev^{commit}" >/dev/null || { echo "!! parse5 lock rev $rev not present in $p5dir" >&2; return 2; }
  printf '%s\n' "$rev"
}
# <<< parse5-lock-rev
# >>> store-logs (exercised by scripts/test-notes-precheck.sh)
# Copy run logs into the durable store before this process exits (D-0275). VALID/aborted
# runs go to official/<src12>/logs/; INVALID (rc 4) goes to official/invalid/<src12>-<timestamp>/logs/.
copy_official_run_logs() {
  local rc="$1" src12="$2" snapdir="$3" storedir="$4" dest ts
  [ -n "$src12" ] || { echo "!! copy_official_run_logs: missing src12" >&2; return 2; }
  [ -n "$storedir" ] || { echo "!! copy_official_run_logs: missing storedir" >&2; return 2; }
  [ -n "$snapdir" ] && [ -d "$snapdir" ] || { echo "!! copy_official_run_logs: snapshot dir missing" >&2; return 2; }
  if [ "$rc" -eq 4 ]; then
    ts="$(date +%Y%m%dT%H%M%S)"
    dest="$storedir/invalid/${src12}-${ts}/logs"
  else
    dest="$storedir/${src12}/logs"
  fi
  mkdir -p "$dest"
  local f copied=0
  for f in board.log golden.progress style-fixtures.log; do
    if [ -f "$snapdir/$f" ]; then cp "$snapdir/$f" "$dest/$f"; copied=1; fi
  done
  printf '%s\n' "$rc" > "$dest/run.rc"
  echo "official-board: stored run logs in $dest (rc=$rc${copied:+, copied})"
}
# <<< store-logs
source "$(cd "$(dirname "$(readlink -f "$0")")" && pwd)/cargo-locked.sh"
if [ -f "$HOME/.cargo/env" ]; then
  # surface a sourcing error; do not hide it behind || true
  source "$HOME/.cargo/env" || { echo "!! BOARD ABORTED: failed to source $HOME/.cargo/env" >&2; exit 3; }
fi
die3() { echo "!! BOARD ABORTED: $*" >&2; exit 3; }
ref="${BOARD_COMMIT:-main}"
commit="$(git -C "$MAIN" rev-parse --verify -q "$ref^{commit}")" || die3 "unknown ref '$ref'"
git -C "$MAIN" merge-base --is-ancestor "$commit" main || die3 "$ref ($commit) is not on main (official boards build only from main)"
[ -z "$(git -C "$P5" status --porcelain)" ] || die3 "parse5-rust working tree is dirty; commit it first"
p5commit="$(git -C "$P5" rev-parse HEAD)"
# >>> notes (exercised verbatim by scripts/test-notes-precheck.sh; AB-0016)
# Notes are other bots' live working tree: NEVER stage or commit there. Record HEAD as it is; the board reads only
# NOTES_READ (archived from that HEAD below; option-gate.mjs reads the archived decisions.tsv via OPTGATE_DECISIONS),
# and those paths must be clean at HEAD, else exit 3. Unrelated dirty notes files are ignored and left untouched.
NOTES_READ=(lwc-rust-migrate)
nrc=0; notescommit="$(git -C "$NOTES" rev-parse --verify -q 'HEAD^{commit}')" || nrc=$?
[ $nrc -eq 0 ] || die3 "cannot resolve $NOTES HEAD (git rc=$nrc)"
"${NOTES_PRECHECK:-$(dirname "$(readlink -f "$0")")/notes-precheck.sh}" "$NOTES" "${NOTES_READ[@]}" || die3 "notes precheck failed (rc=$?)"
nrc=0; nhead="$(git -C "$NOTES" rev-parse --verify -q 'HEAD^{commit}')" || nrc=$?
[ $nrc -eq 0 ] && [ "$nhead" = "$notescommit" ] || die3 "$NOTES HEAD moved during the precheck (${notescommit:0:12} -> ${nhead:-?}); re-run"
# <<< notes
snap="$(mktemp -d /tmp/lwc-board-snap.XXXXXX)"   # $snap/repo (harness at $snap/repo/harness) + $snap/parse5-rust (../../../parse5-rust resolves)
h0=""
logs_copied=0
on_official_exit() {
  local rc=$?
  local copy_rc=0
  if [ -n "${h0:-}" ] && [ "${logs_copied:-0}" != 1 ]; then
    copy_official_run_logs "$rc" "$h0" "$snap" "$store" || copy_rc=$?
  fi
  rm -rf "$snap"
  # Keep the first non-zero rc: a copy failure must not overwrite INVALID (4) with 2.
  if [ "$rc" -ne 0 ]; then
    exit "$rc"
  fi
  if [ "$copy_rc" -ne 0 ]; then
    exit 2
  fi
}
trap on_official_exit EXIT
mkdir -p "$snap/repo" "$snap/parse5-rust" "$snap/notes"
ws="$snap/repo"; hs="$ws/harness"
git -C "$MAIN" archive "$commit" | tar -x -C "$snap/repo"
git -C "$P5" archive "$p5commit" | tar -x -C "$snap/parse5-rust"
git -C "$NOTES" archive "$notescommit" "${NOTES_READ[@]}" | tar -x -C "$snap/notes" --strip-components=1   # NOTES_READ is one dir: strip it
lockrev="$(require_parse5_lock_rev "$ws/Cargo.lock" "$P5")" || { echo "!! BOARD ABORTED: Cargo.lock parse5 rev missing or not in $P5" >&2; exit 2; }
"$MAIN/harness/scripts/ensure-deps.sh"
ln -s "$MAIN/harness/node_modules" "$hs/node_modules"
# srchash: harness entries first, mapped to harness/..., then notes/parse5-rust/repo (sorted, harness excluded), mapped to
# <dir>/...: the same file list, names and order as the former separate-harness-dir layout, so src hashes stay comparable.
srchash() { ( { find "$hs" -type f ! -path "$hs/.pins/*" ! -path "$hs/.vitest/*" ! -path "$hs/.csr-render-out/*" \
      ! -path "$hs/.ssr-render-out/*" ! -name '*.node' -print0 | sort -z | xargs -0 sha256sum | sed -e "s#$hs/#harness/#"
    find "$snap/repo" "$snap/parse5-rust" "$snap/notes" -type f ! -path "$snap/target/*" ! -path "$hs/*" ! -name '*.node' -print0 \
      | sort -z | xargs -0 sha256sum | sed -e "s#$snap/##"; } | sha256sum | cut -c1-12); }
h0="$(srchash)"
# External inputs not in the source tree (template-diag corpus + oracle's git-ignored template-compiler dist).
# Expected hashes are pinned in harness/external-inputs.lock (committed; changing a pin is a reviewed landing).
# Mismatch vs pin, or any change during the run => BOARD INVALID.
inputhash1() { (cd /workspace && find "$1" -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum) | sha256sum | cut -c1-12; }
inputshash() { for i in "${EXT_INPUTS[@]}"; do printf '%s\t%s\n' "$i" "$(inputhash1 "$i")"; done; }
lock="$hs/external-inputs.lock"
[ -f "$lock" ] || { echo "!! BOARD INVALID: missing harness/external-inputs.lock at $commit" >&2; exit 4; }
mapfile -t EXT_INPUTS < <(grep -v '^#' "$lock" | sed '/^$/d' | cut -f1)   # input list = the lock's paths (single source)
[ ${#EXT_INPUTS[@]} -gt 0 ] || { echo "!! BOARD INVALID: external-inputs.lock lists no inputs" >&2; exit 4; }
in0="$(inputshash)"
if [ "$(grep -v '^#' "$lock" | sed '/^$/d')" != "$in0" ]; then
  echo "!! BOARD INVALID: external inputs differ from harness/external-inputs.lock (drift since pin):" >&2
  diff <(grep -v '^#' "$lock" | sed '/^$/d') <(echo "$in0") >&2 || true; exit 4; fi
in0sha="$(echo "$in0" | sha256sum | cut -c1-12)"
p5="$(cd "$snap/parse5-rust" && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-12)"
echo "commit $commit  src snapshot sha256:$h0  parse5-rust ${p5commit:0:12} sha256:$p5  notes ${notescommit:0:12}  ($snap)"
export CARGO_TARGET_DIR="$snap/target"
# SHIM_FILE_LIST lives next to the snapshot (not inside hashed trees). Official archives have no .git, so
# shim-gate cannot git-ls-files; official-board supplies the boarded commit's ls-tree (D-0273).
git -C "$MAIN" ls-tree -r -z --name-only "$commit" > "$snap/shim-file-list"
export SHIM_FILE_LIST="$snap/shim-file-list"
PARSE5_DIR="$P5" "$ws/scripts/parse5-pin-check.sh" "$ws" || die3 "parse5 pin check failed @ $h0 (lock source rev != Cargo.toml, or rev absent from $P5)"
GOLDEN_GATE_PROGRESS="$snap/golden.progress" "$hs/scripts/golden-gate.sh" "$hs" \
  || { echo "!! BOARD ABORTED: goldens @ $h0 != fresh JS-oracle recording" >&2; exit 3; }
(cd "$ws" && cargo_locked check --workspace --all-targets) || { echo "!! BOARD ABORTED: snapshot $h0 does not compile" >&2; exit 3; }
# every crate's tests, as land.sh (AB-R002: brings lwc-style's gates + the parse-errors fixtures in)
(cd "$ws" && cargo_locked test --workspace) || { echo "!! BOARD ABORTED: workspace test --locked fails @ $h0" >&2; exit 3; }
# style parse-error set made explicit (manifest style_parse_errors=<passed>/<total>); strict run, exit 3 on any failure
spe="$("$ws/scripts/style-parse-errors.sh" "$ws" "$snap/style-fixtures.log")" || die3 "style parse-error fixtures @ $h0 (see $snap/style-fixtures.log)"
[[ "$spe" =~ ^[0-9]+/[0-9]+$ ]] || die3 "style-parse-errors.sh printed '$spe', want <passed>/<total>"
echo "style-parse-errors     $spe (lwc-style fixture tests, LWC_STYLE_STRICT=1)"
(cd "$ws" && cargo_locked build -p lwc-compiler-napi --release)
so="$CARGO_TARGET_DIR/release/liblwc_compiler_napi.so"
cp "$so" "$hs/lwc-compiler-napi.node"
napi="$(sha256sum "$so" | cut -c1-12)"
echo "built napi $napi from src $h0"
tdiag="$snap/template-diag"   # board.sh writes template-diag-<engine>.json here (not hashed, not in the tgz)
# official boards compare against BOARD_COMMIT's committed harness/board-baseline.tsv exactly: never rewrite it here.
# D-0219: template-diag codegen asks the snapshot's semdiff (board.sh builds -p semdiff from $hs/..), validated against
# the hashed notes snapshot (not live notes). Exec leg stays off (D-0256): TDD_SEMDIFF_EXEC is unset.
set +e; env -u BOARD_RATCHET_UPDATE -u TDD_SEMDIFF_EXEC CRATES_ROOT="$ws/crates" SHIM_RATCHET_DIR="$hs/ratchets" OPTGATE_DECISIONS="$snap/notes/decisions.tsv" BOARD_REPO_ROOT="$ws" TEMPLATE_DIAG_OUT_DIR="$tdiag" SEMDIFF_REGISTRY="$snap/notes/decisions.tsv" "$hs/board.sh" "$@" 2>&1 | tee "$snap/board.log"; rc=${PIPESTATUS[0]}; set -e
# belt and braces: any gate failure line voids the run regardless of rc plumbing
grep -q '^!! ' "$snap/board.log" && [ "$rc" -eq 0 ] && { echo "!! rc=0 but board printed a failure; forcing rc=1" >&2; rc=1; }
[ "$(srchash)" == "$h0" ] || { echo "!! BOARD INVALID: snapshot mutated during run" >&2; exit 4; }
[ "$(inputshash)" == "$in0" ] || { echo "!! BOARD INVALID: external inputs (style-corpus/lbc*, oracle template-compiler dist) changed during run" >&2; exit 4; }
echo "official: src=$h0 napi=$napi rc=$rc"
# a full official run must carry the template-diag report (board.sh already fails the row if it is missing/invalid)
for e in crate napi; do for f in "$tdiag/template-diag-$e.json" "$tdiag/template-diag-$e.soundness.json"; do
  if [ "$rc" -eq 0 ] && [ $# -eq 0 ] && [ ! -s "$f" ]; then echo "!! BOARD FAILED: template-diag artifact missing ($f); forcing rc=1" >&2; rc=1; fi; done; done
if [ "$rc" -eq 0 ] && [ $# -eq 0 ]; then
  mkdir -p "$push"
  base="$push/official-$h0"
  { echo "official board  commit=${commit:0:12}  src=$h0  napi=$napi  parse5-rust=$p5 (build dep, NOT in tgz)  $(date -Iseconds)"
    sed -n '/^napi sha256:/,$p' "$snap/board.log"
    echo "style-parse-errors     $spe (lwc-style fixture tests, LWC_STYLE_STRICT=1)"
    # CSR failure clusters (Migrate 2026-11-01): counts per cluster for each corpus
    for c in ssr template; do rep="$hs/.csr-render-out/rust-$c/report-$c.json"
      [ -f "$rep" ] || { echo "!! csr clusters: missing $rep"; continue; }
      echo "csr-clusters[$c]: $(node "$hs/scripts/csr-render-clusters.mjs" "$rep" | grep -E '^(pass |\[)' | sed -E 's/^\[([0-9]+)\] ([a-z-]+):.*/\2=\1/' | paste -sd' ')"
    done; } > "$base.board.txt.tmp"
  # per-fixture pass/fail (fixture<TAB>pass<TAB>status<TAB>goldenStatus) so any two official runs can be diffed
  for c in ssr template; do rep="$hs/.csr-render-out/rust-$c/report-$c.json"; [ -f "$rep" ] || continue
    node -e 'const r=require(process.argv[1]);for(const x of r.rows)console.log([x.fixture,x.pass,x.status,x.goldenStatus].join("\t"))' "$rep" > "$base.csr-$c.tsv"; done
  cp "$tdiag/template-diag-crate.json" "$base.template-diag.json"; cp "$tdiag/template-diag-napi.json" "$base.template-diag-napi.json"
  # soundness stamps (separate artifacts; the reports are never rewritten): publish-official.sh verifies them against the reports
  cp "$tdiag/template-diag-crate.soundness.json" "$base.template-diag.soundness.json"; cp "$tdiag/template-diag-napi.soundness.json" "$base.template-diag-napi.soundness.json"
  # tgz layout = repo root (harness under harness/) + notes/ (CUTOVER, DIAG_HELPER, decisions as of run start)
  rm -f "$ws/harness/node_modules" "$ws/harness/lwc-compiler-napi.node"
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
  copy_official_run_logs "$rc" "$h0" "$snap" "$store" || { echo "!! BOARD INVALID: failed to store run logs before publish" >&2; exit 2; }
  logs_copied=1
  pub_rc=0
  OFFICIAL_COMMIT="$commit" OFFICIAL_PARSE5_COMMIT="$lockrev" OFFICIAL_PARSE5_CLONE_HEAD="$p5commit" OFFICIAL_NOTES_COMMIT="$notescommit" OFFICIAL_INPUTS_SHA="$in0sha" OFFICIAL_STYLE_PARSE_ERRORS="$spe" OFFICIAL_ORACLE_COMMIT="$ocommit" OFFICIAL_ORACLE_REPO="$orepo" \
    "$snap/repo/scripts/publish-official.sh" "$h0" "$napi" "$p5" "$store" || pub_rc=$?
  if [ "$pub_rc" -ne 0 ]; then
    printf '%s\n' "$pub_rc" > "$store/${h0}/logs/run.rc"
    exit "$pub_rc"
  fi
else
  echo "no push artifacts (rc=$rc${1:+, partial row set})"
fi
exit $rc
