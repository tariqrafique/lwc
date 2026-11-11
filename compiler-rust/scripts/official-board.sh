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
#   1b. Pins MAIN, NOTES, LWC_ORACLE, PARSE5_DIR, EXT_INPUTS_ROOT to the box defaults. Any override is refused
#      unless OFFICIAL_NO_PUBLISH=1 (AB-M0002).
#   2. After archive + src-hash + external-input lock verify, calls scripts/run-board.sh (the single driver:
#      golden-gate, C8, board.sh, footers). Then re-hashes; mutation => INVALID (exit 4).
#   3. VALID run => /workspace/push/official-<srcsha>.tgz + official-<srcsha>.board.txt and publish-official.sh
# Usage: BOARD_COMMIT=<ref on main> scripts/official-board.sh [rows...]
set -euo pipefail
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
  for f in board.log golden.progress style-fixtures.log style-corpus.log napi-css-errors.log row-provenance.tsv; do
    if [ -f "$snapdir/$f" ]; then cp "$snapdir/$f" "$dest/$f"; copied=1; fi
  done
  printf '%s\n' "$rc" > "$dest/run.rc"
  echo "official-board: stored run logs in $dest (rc=$rc${copied:+, copied})"
}
# <<< store-logs
# Surface orphan BOX_CONFIG_DEPENDENT allowlist rows (script absent on both
# main and incoming) without changing the board rc.
print_orphan_allowlist_warns() {
  local tree="$1" helper
  helper="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)/print-orphan-allowlist-warns.sh"
  [ -x "$helper" ] || return 0
  "$helper" "$tree" || true
}

# >>> official-input-pins (exercised by scripts/test-notes-precheck.sh)
OFFICIAL_DEFAULT_MAIN=/workspace/lwc-compiler-rust
OFFICIAL_DEFAULT_NOTES=/workspace/notes
OFFICIAL_DEFAULT_ORACLE=/workspace/lwc
OFFICIAL_DEFAULT_PARSE5=/workspace/parse5-rust
OFFICIAL_DEFAULT_EXT_INPUTS=/workspace
official_override_names() {
  [ -n "${LWC_RUST_MAIN:-}" ] && [ "$LWC_RUST_MAIN" != "$OFFICIAL_DEFAULT_MAIN" ] && printf '%s\n' LWC_RUST_MAIN
  [ -n "${NOTES_DIR:-}" ] && [ "$NOTES_DIR" != "$OFFICIAL_DEFAULT_NOTES" ] && printf '%s\n' NOTES_DIR
  [ -n "${LWC_ORACLE:-}" ] && [ "$LWC_ORACLE" != "$OFFICIAL_DEFAULT_ORACLE" ] && printf '%s\n' LWC_ORACLE
  [ -n "${PARSE5_DIR:-}" ] && [ "$PARSE5_DIR" != "$OFFICIAL_DEFAULT_PARSE5" ] && printf '%s\n' PARSE5_DIR
  [ -n "${EXT_INPUTS_ROOT:-}" ] && [ "$EXT_INPUTS_ROOT" != "$OFFICIAL_DEFAULT_EXT_INPUTS" ] && printf '%s\n' EXT_INPUTS_ROOT
  # STYLE_CORPUS is derived from the effective EXT_INPUTS_ROOT so lock paths
  # (style-corpus/...) and the tests ($STYLE_CORPUS/...) read the same tree.
  _ext="${EXT_INPUTS_ROOT:-$OFFICIAL_DEFAULT_EXT_INPUTS}"
  _style_default="$_ext/style-corpus"
  [ -n "${STYLE_CORPUS:-}" ] && [ "$STYLE_CORPUS" != "$_style_default" ] && printf '%s\n' STYLE_CORPUS
  return 0
}
official_refuse_overrides_unless_no_publish() {
  local names
  names="$(official_override_names | paste -sd' ' -)"
  if [ -n "$names" ] && [ "${OFFICIAL_NO_PUBLISH:-}" != 1 ]; then
    echo "!! PUBLISH REFUSED: overridden official inputs ($names) require OFFICIAL_NO_PUBLISH=1" >&2
    return 5
  fi
  return 0
}
official_resolve_inputs() {
  official_refuse_overrides_unless_no_publish || return $?
  MAIN="${LWC_RUST_MAIN:-$OFFICIAL_DEFAULT_MAIN}"
  NOTES="${NOTES_DIR:-$OFFICIAL_DEFAULT_NOTES}"
  P5="${PARSE5_DIR:-$OFFICIAL_DEFAULT_PARSE5}"
  EXT_INPUTS_ROOT="${EXT_INPUTS_ROOT:-$OFFICIAL_DEFAULT_EXT_INPUTS}"
  LWC_ORACLE="${LWC_ORACLE:-$OFFICIAL_DEFAULT_ORACLE}"
  STYLE_CORPUS="${STYLE_CORPUS:-$EXT_INPUTS_ROOT/style-corpus}"
  export MAIN NOTES P5 EXT_INPUTS_ROOT LWC_ORACLE LWC_ROOT="$LWC_ORACLE" PARSE5_DIR="$P5" STYLE_CORPUS
}
# <<< official-input-pins
# >>> lock-format (exercised by scripts/test-notes-precheck.sh)
inputhash1() { (cd "$EXT_INPUTS_ROOT" && find -H "$1" -type f ! -path '*/.git/*' -print0 | LC_ALL=C sort -z | xargs -0 sha256sum) | sha256sum | cut -c1-12; }
inputhash1_all() { (cd "$EXT_INPUTS_ROOT" && find "$1" -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum) | sha256sum | cut -c1-12; }
lock_hash_format() {
  local lock="$1"
  [ -f "$lock" ] || { echo "!! BOARD INVALID: missing $lock" >&2; return 4; }
  local n
  n="$(grep -cE '^# hash-format:' "$lock" || true)"
  if [ "$n" -gt 1 ]; then
    echo "!! BOARD INVALID: multiple hash-format markers in $lock" >&2
    return 4
  fi
  if grep -qE '^# hash-format: content-only[[:space:]]*$' "$lock"; then
    printf '%s\n' content-only
  elif grep -qE '^# hash-format: with-git[[:space:]]*$' "$lock"; then
    printf '%s\n' with-git
  elif grep -qE '^# hash-format:' "$lock"; then
    echo "!! BOARD INVALID: unknown hash-format marker in $lock" >&2
    return 4
  else
    printf '%s\n' with-git
  fi
}
verify_external_inputs_lock() {
  local lock="$1" fmt hashfn ipath iwant igot ok=1
  fmt="$(lock_hash_format "$lock")" || return 4
  case "$fmt" in
    content-only) hashfn=inputhash1 ;;
    with-git) hashfn=inputhash1_all ;;
    *) echo "!! BOARD INVALID: unknown lock hash format '$fmt'" >&2; return 4 ;;
  esac
  while IFS=$'\t' read -r ipath iwant; do
    [ -n "${ipath:-}" ] || continue
    igot="$($hashfn "$ipath")"
    if [ "$igot" != "$iwant" ]; then
      echo "!! BOARD INVALID: $ipath hash got=$igot want=$iwant (format=$fmt)" >&2
      ok=0
    fi
  done < <(grep -v '^#' "$lock" | sed '/^$/d')
  [ "$ok" -eq 1 ] || return 4
  return 0
}
# <<< lock-format
official_resolve_inputs || exit $?
root="$MAIN"
push="${OFFICIAL_PUSH:-/workspace/push}"
store="${OFFICIAL_STORE:-/cursor/stores/store-4280618e-f4d8-4614-b1b6-bf88ea27f849/official}"
if [ -f "$HOME/.cargo/env" ]; then
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
# D-0280: boarded commit's decisions snapshot must match archived notes HEAD
# (skip if the boarded tree predates the snapshot). Fail closed on drift.
# The notes-commit sidecar, when present, must name a commit that exists in
# NOTES whose lwc-rust-migrate/decisions.tsv equals the snapshot. It does not
# have to be notes HEAD; the cmp against the archived HEAD covers HEAD drift.
if [ -f "$ws/harness/decisions.snapshot.tsv" ]; then
  cmp -s "$snap/notes/decisions.tsv" "$ws/harness/decisions.snapshot.tsv"     || die3 "harness/decisions.snapshot.tsv != notes@$notescommit lwc-rust-migrate/decisions.tsv; run scripts/refresh-decisions-snapshot.sh"
fi
if [ -f "$ws/harness/decisions.snapshot.notes-commit" ]; then
  snapc="$(head -1 "$ws/harness/decisions.snapshot.notes-commit")"
  [[ "$snapc" =~ ^[0-9a-f]{40}$ ]] || die3 "harness/decisions.snapshot.notes-commit is not a 40-hex sha: '$snapc'"
  git -C "$NOTES" cat-file -e "$snapc^{commit}" || die3 "harness/decisions.snapshot.notes-commit $snapc is not in $NOTES"
  git -C "$NOTES" show "$snapc:lwc-rust-migrate/decisions.tsv" | cmp -s - "$ws/harness/decisions.snapshot.tsv"     || die3 "harness/decisions.snapshot.tsv != notes@$snapc:lwc-rust-migrate/decisions.tsv; run scripts/refresh-decisions-snapshot.sh"
fi
# parse5 presence is enforced in run-board.sh (parse5-pin-check). This extracts
# the lock rev for OFFICIAL_PARSE5_COMMIT only — do not re-verify against $P5.
lockrev="$(parse5_lock_rev "$ws/Cargo.lock")" || die3 "Cargo.lock parse5 rev missing (want source = git+...?rev=<rev>#<rev>)"
# ensure-deps runs once, in run-board.sh, against the snapshot harness.
# srchash: harness entries first, mapped to harness/..., then notes/parse5-rust/repo (sorted, harness excluded), mapped to
# <dir>/...: the same file list, names and order as the former separate-harness-dir layout, so src hashes stay comparable.
# srcfiles/srchash skip build outputs that C8 napi-pack may emit inside the archive
# (napi.gen.d.ts is gitignored; *.node already skipped; $ws/target if cargo misses CARGO_TARGET_DIR).
srcfiles() { ( { find "$hs" -type f ! -path "$hs/.pins/*" ! -path "$hs/.vitest/*" ! -path "$hs/.csr-render-out/*" \
      ! -path "$hs/.ssr-render-out/*" ! -path "$hs/node_modules/*" ! -name '*.node' -print0 | sort -z | xargs -0 sha256sum | sed -e "s#$hs/#harness/#"
    find "$snap/repo" "$snap/parse5-rust" "$snap/notes" -type f ! -path "$snap/target/*" ! -path "$ws/target/*" \
      ! -path "$hs/*" ! -name '*.node' ! -name 'napi.gen.d.ts' -print0 \
      | sort -z | xargs -0 sha256sum | sed -e "s#$snap/##"; } ); }
srchash() { srcfiles | sha256sum | cut -c1-12; }
h0="$(srcfiles | tee "$snap/srcfiles.before" | sha256sum | cut -c1-12)"
lock="$hs/external-inputs.lock"
[ -f "$lock" ] || { echo "!! BOARD INVALID: missing harness/external-inputs.lock at $commit" >&2; exit 4; }
mapfile -t EXT_INPUTS < <(grep -v '^#' "$lock" | sed '/^$/d' | cut -f1)
[ ${#EXT_INPUTS[@]} -gt 0 ] || { echo "!! BOARD INVALID: external-inputs.lock lists no inputs" >&2; exit 4; }
verify_external_inputs_lock "$lock" || exit 4
_fmt="$(lock_hash_format "$lock")" || exit 4
if [ "$_fmt" = content-only ]; then
  inputshash() { for i in "${EXT_INPUTS[@]}"; do printf '%s\t%s\n' "$i" "$(inputhash1 "$i")"; done; }
else
  inputshash() { for i in "${EXT_INPUTS[@]}"; do printf '%s\t%s\n' "$i" "$(inputhash1_all "$i")"; done; }
fi
in0="$(inputshash)"
in0sha="$(echo "$in0" | sha256sum | cut -c1-12)"
p5="$(cd "$snap/parse5-rust" && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-12)"
echo "commit $commit  src snapshot sha256:$h0  parse5-rust ${p5commit:0:12} sha256:$p5  notes ${notescommit:0:12}  ($snap)"
# D-0282: share rustc incremental output across official boards with the same lock+toolchain.
_lk="$(git -C "$MAIN" show "$commit:Cargo.lock" | sha256sum | awk '{print $1}')"
_tk="$(git -C "$MAIN" show "$commit:rust-toolchain.toml" | sha256sum | awk '{print $1}')"
_ck="$(printf '%s%s' "$_lk" "$_tk" | sha256sum | cut -c1-16)"
export CARGO_TARGET_DIR="/workspace/cache/lwc-target/$_ck"
mkdir -p "$CARGO_TARGET_DIR"
git -C "$MAIN" ls-tree -r -z --name-only "$commit" > "$snap/shim-file-list"
export SHIM_FILE_LIST="$snap/shim-file-list"
export SSR_ORACLE_GIT="$MAIN" SSR_ORACLE_MAIN="$commit"
tdiag="$snap/template-diag"
export CRATES_ROOT="$ws/crates" SHIM_RATCHET_DIR="$hs/ratchets" OPTGATE_DECISIONS="$snap/notes/decisions.tsv"
export BOARD_REPO_ROOT="$ws" TEMPLATE_DIAG_OUT_DIR="$tdiag" SEMDIFF_REGISTRY="$snap/notes/decisions.tsv"
export C8_TARBALLS="$snap/tarballs" BOARD_FOOTER_LOG_DIR="$snap"
_run="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)/run-board.sh"
[ -x "$_run" ] || die3 "run-board.sh missing next to official-board.sh"
_rt="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)/board-runtime.mjs"
if [ -z "${BOARD_SELECT_ORACLE_SHA:-}" ] && [ -n "${LWC_ORACLE:-}" ]; then
  BOARD_SELECT_ORACLE_SHA="$(git -C "$LWC_ORACLE" rev-parse HEAD 2>/dev/null || true)"
  export BOARD_SELECT_ORACLE_SHA
fi
if [ -x "$_rt" ] && [ $# -eq 0 ]; then
  eval "$(node "$_rt" decide "$MAIN" "$commit" "$store")"
  export BOARD_SCOPE BOARD_COLD BOARD_GOLDEN_GATE BOARD_ROWS BOARD_CARRY_ROWS
  export BOARD_PREV_SRC BOARD_PREV_COMMIT BOARD_CARRY_DISABLED BOARD_LANDS_SINCE_COLD
  if [ -n "${BOARD_PREV_SRC:-}" ] && [ -f "$store/$BOARD_PREV_SRC/board.txt" ]; then
    export BOARD_PREV_BOARD_TXT="$store/$BOARD_PREV_SRC/board.txt"
  fi
  echo "official-board: D-0282 scope=${BOARD_SCOPE:-FULL} cold=${BOARD_COLD:-1} golden=${BOARD_GOLDEN_GATE:-1} rows=${BOARD_ROWS:-<all>} carry=${BOARD_CARRY_ROWS:-<none>} prev=${BOARD_PREV_SRC:-<none>}"
else
  BOARD_SCOPE="${BOARD_SCOPE:-FULL}"
  BOARD_COLD="${BOARD_COLD:-1}"
  BOARD_GOLDEN_GATE="${BOARD_GOLDEN_GATE:-1}"
  export BOARD_SCOPE BOARD_COLD BOARD_GOLDEN_GATE
fi
set +e
if [ $# -eq 0 ] && [ "${BOARD_SCOPE:-FULL}" = scoped ]; then
  export BOARD_SUITES_EXPLICIT=1
  # shellcheck disable=SC2206
  _sel=( ${BOARD_ROWS:-} )
  if [ "${#_sel[@]}" -gt 0 ]; then
    "$_run" "$ws" "$snap/board.log" "${_sel[@]}"
  else
    "$_run" "$ws" "$snap/board.log"
  fi
else
  "$_run" "$ws" "$snap/board.log" "$@"
fi
rc=$?
set -e
if [ "$(srchash)" != "$h0" ]; then
  echo "!! BOARD INVALID: snapshot mutated during run" >&2
  srcfiles > "$snap/srcfiles.after" || true
  srcdiff="$(diff -u "$snap/srcfiles.before" "$snap/srcfiles.after" || true)"
  head -n 80 <<< "$srcdiff" >&2 || true
  exit 4
fi
[ "$(inputshash)" == "$in0" ] || { echo "!! BOARD INVALID: external inputs (style-corpus/lbc*, oracle template-compiler dist) changed during run" >&2; exit 4; }
so="$CARGO_TARGET_DIR/release/liblwc_compiler_napi.so"
napi="$(sha256sum "$so" | cut -c1-12)"
spe="$(sed -nE 's/^style-parse-errors[[:space:]]+([0-9]+\/[0-9]+).*/\1/p' "$snap/board.log" | tail -1)"
echo "official: src=$h0 napi=$napi rc=$rc"
orphan_warns="$(print_orphan_allowlist_warns "$ws")"
if [ -n "${orphan_warns:-}" ]; then
  printf '%s\n' "$orphan_warns"
fi
_rt="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)/board-runtime.mjs"
if [ "$rc" -eq 0 ] && [ $# -eq 0 ] && [ "${BOARD_SCOPE:-FULL}" = scoped ] && [ -n "${BOARD_PREV_BOARD_TXT:-}" ] && [ -x "$_rt" ]; then
  node "$_rt" compose "$snap/board.log" "$BOARD_PREV_BOARD_TXT" "$snap/board.log" "$MAIN" --ran "${BOARD_ROWS:-}"
fi
if [ $# -eq 0 ] && [ -n "${BOARD_PREV_SRC:-}" ]; then
  mkdir -p "$tdiag"
  _prev="$store/$BOARD_PREV_SRC"
  _ran_td=0 _ran_tdn=0
  for _r in ${BOARD_ROWS:-}; do
    [ "$_r" = template-diag ] && _ran_td=1
    [ "$_r" = template-diag-napi ] && _ran_tdn=1
  done
  if [ "$_ran_td" -eq 0 ]; then
    [ -s "$tdiag/template-diag-crate.json" ] || cp "$_prev/template-diag.json" "$tdiag/template-diag-crate.json" 2>/dev/null || true
    [ -s "$tdiag/template-diag-crate.soundness.json" ] || cp "$_prev/template-diag.soundness.json" "$tdiag/template-diag-crate.soundness.json" 2>/dev/null || true
  fi
  if [ "$_ran_tdn" -eq 0 ]; then
    [ -s "$tdiag/template-diag-napi.json" ] || cp "$_prev/template-diag-napi.json" "$tdiag/template-diag-napi.json" 2>/dev/null || true
    [ -s "$tdiag/template-diag-napi.soundness.json" ] || cp "$_prev/template-diag-napi.soundness.json" "$tdiag/template-diag-napi.soundness.json" 2>/dev/null || true
  fi
fi
if [ $# -eq 0 ]; then
  {
    printf 'row\tstatus\tsource_board_id\n'
    # shellcheck disable=SC2086
    for _r in style js js-eval template ssr ssr-async ssr-asyncYield ssr-render ssr-render-async ssr-render-asyncYield csr-render csr-render-template template-renderer js-options js-api template-options ssr_dynamic_imports style-api template-api ssr_render_api template-diag template-diag-napi style-upstream; do
      _st=carried
      for _x in ${BOARD_ROWS:-}; do
        if [ "$_x" = "$_r" ]; then _st=rerun; break; fi
      done
      if [ "${BOARD_SCOPE:-FULL}" = FULL ] || [ "${BOARD_COLD:-1}" = 1 ]; then _st=rerun; fi
      if [ "$_st" = carried ]; then printf '%s\t%s\t%s\n' "$_r" carried "${BOARD_PREV_SRC:-}"
      else printf '%s\trerun\t\n' "$_r"
      fi
    done
  } > "$snap/row-provenance.tsv"
fi
if [ "$rc" -eq 0 ] && [ $# -eq 0 ] && [ "${BOARD_COLD:-0}" = 1 ] && [ -n "${BOARD_PREV_COMMIT:-}" ] && [ -n "${BOARD_PREV_BOARD_TXT:-}" ] && [ -x "$_rt" ]; then
  if ! node "$_rt" check-carry-agreement "$snap/board.log" "$BOARD_PREV_BOARD_TXT" "$MAIN" "$commit" "$BOARD_PREV_COMMIT"; then
    node "$_rt" set-carry-disabled "$store" --reason "cold run disagreed with a would-be-carried row"
    echo "!! BOARD FAILED: cold carry disagreement; carrying disabled until fixed" >&2
    rc=1
  fi
fi
for e in crate napi; do for f in "$tdiag/template-diag-$e.json" "$tdiag/template-diag-$e.soundness.json"; do
  if [ "$rc" -eq 0 ] && [ $# -eq 0 ] && [ ! -s "$f" ]; then echo "!! BOARD FAILED: template-diag artifact missing ($f); forcing rc=1" >&2; rc=1; fi; done; done
if [ "$rc" -eq 0 ] && [ $# -eq 0 ]; then
  mkdir -p "$push"
  base="$push/official-$h0"
  { echo "official board  commit=${commit:0:12}  src=$h0  napi=$napi  parse5-rust=$p5 (build dep, NOT in tgz)  $(date -Iseconds)"
    sed -n '/^napi sha256:/,$p' "$snap/board.log"
    if [ -n "${orphan_warns:-}" ]; then printf '%s\n' "$orphan_warns"; fi
  } > "$base.board.txt.tmp"
  for c in ssr template; do rep="$hs/.csr-render-out/rust-$c/report-$c.json"; [ -f "$rep" ] || continue
    node -e 'const r=require(process.argv[1]);for(const x of r.rows)console.log([x.fixture,x.pass,x.status,x.goldenStatus].join("\t"))' "$rep" > "$base.csr-$c.tsv"; done
  if [ -n "${BOARD_PREV_SRC:-}" ]; then
    for c in ssr template; do
      if [ ! -f "$base.csr-$c.tsv" ] && [ -f "$store/$BOARD_PREV_SRC/csr-$c.tsv" ]; then
        cp "$store/$BOARD_PREV_SRC/csr-$c.tsv" "$base.csr-$c.tsv"
      fi
    done
  fi
  cp "$tdiag/template-diag-crate.json" "$base.template-diag.json"; cp "$tdiag/template-diag-napi.json" "$base.template-diag-napi.json"
  cp "$tdiag/template-diag-crate.soundness.json" "$base.template-diag.soundness.json"; cp "$tdiag/template-diag-napi.soundness.json" "$base.template-diag-napi.soundness.json"
  rm -rf "$ws/harness/node_modules"
  rm -f "$ws/harness/lwc-compiler-napi.node"
  rm -rf "$ws/harness/.pins" "$ws/harness/.vitest" "$ws/harness/.csr-render-out" "$ws/harness/.ssr-render-out"
  tar -C "$snap" --exclude=./repo/target -czf "$base.tgz.tmp" --transform 's#^repo/##;s#^repo$#.#' repo notes
  mv -f "$base.tgz.tmp" "$base.tgz"; mv -f "$base.board.txt.tmp" "$base.board.txt"
  echo "push artifacts: $base.tgz  $base.board.txt  $base.template-diag.json  $base.template-diag-napi.json"
  hdr="$(grep -E '^napi sha256:' "$snap/board.log")" || { echo "!! PUBLISH REFUSED: no board.sh header in board.log" >&2; exit 5; }
  [ "$(wc -l <<< "$hdr")" -eq 1 ] || { echo "!! PUBLISH REFUSED: expected 1 board.sh header, got $(wc -l <<< "$hdr")" >&2; exit 5; }
  ocommit="$(sed -nE 's/.* oracle=([0-9a-f]{40})\(clean\) .*/\1/p' <<< "$hdr")"; orepo="$(sed -nE 's/.* oracle_repo=([^ ]+) .*/\1/p' <<< "$hdr")"
  [ -n "$ocommit" ] && [ -n "$orepo" ] || { echo "!! PUBLISH REFUSED: cannot parse verified oracle from header: $hdr" >&2; exit 5; }
  copy_official_run_logs "$rc" "$h0" "$snap" "$store" || { echo "!! BOARD INVALID: failed to store run logs before publish" >&2; exit 2; }
  logs_copied=1
  if [ "${OFFICIAL_NO_PUBLISH:-}" = 1 ]; then
    echo "official-board: OFFICIAL_NO_PUBLISH=1; wrote $base.board.txt, skipping publish"
  else
  pub_rc=0
  OFFICIAL_COMMIT="$commit" OFFICIAL_PARSE5_COMMIT="$lockrev" OFFICIAL_PARSE5_CLONE_HEAD="$p5commit" OFFICIAL_NOTES_COMMIT="$notescommit" OFFICIAL_INPUTS_SHA="$in0sha" OFFICIAL_STYLE_PARSE_ERRORS="$spe" OFFICIAL_ORACLE_COMMIT="$ocommit" OFFICIAL_ORACLE_REPO="$orepo" \
    "$snap/repo/scripts/publish-official.sh" "$h0" "$napi" "$p5" "$store" || pub_rc=$?
  if [ "$pub_rc" -ne 0 ]; then
    printf '%s\n' "$pub_rc" > "$store/${h0}/logs/run.rc"
    exit "$pub_rc"
  fi
  if [ -x "$_rt" ]; then
    node "$_rt" record "$store" --cold "${BOARD_COLD:-1}" --src "$h0" --commit "$commit"
  fi
  fi
else
  echo "no push artifacts (rc=$rc${1:+, partial row set})"
fi
exit $rc
