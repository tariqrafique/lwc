#!/usr/bin/env bash
# Land self-tests covering this file: test-board-ratchet (see scripts/land-self-tests.tsv). A board.sh-only land should select that test and skip rust rebuilds.
# Official board: pin ONE immutable napi binary for the whole run, log its hash.
# Usage: ./board.sh [check] [--root=<tree>] [suite ...]   (default: all rows below)
#        ./board.sh check [--root=<tree>]   full run, set-equality judge only; BOARD_RATCHET_UPDATE refused
# --root=<tree>: run the tree's tests/fixtures, crates and napi (the tree under test). The runner
#   logic, obs writing, row list (board-rows.sh), comparator and pinned vitest.board.config.ts come from THIS script's tree.
#   official-board.sh runs this on a git-archive of the landed sha (D-0282: that run is the land's board).
# Guards: (1) oracle must be pristine ORACLE_SHA, clean tree -> else abort before running anything.
#         (3) shim gate: untagged shim/compat/workaround/hack/mask/rescan line in crates/ (scripts/shim-gate.mjs, allowlist shim-allowlist.tsv w/ reason) OR untracked/stale tagged-shim vs shim-registry.tsv => exit 1.
#         (2) option-inventory gate (scripts/option-gate.mjs via option_gate in board-rows.sh) -> any unmapped/pending field => exit 1.
#         (2b) coverage gate (scripts/coverage-gate.mjs) immediately after option-gate. Report-only unless COVGATE_STRICT=1.
#         (4) golden-I/O guard (scripts/golden-io-guard.sh): golden writes / UPDATE_GOLDENS outside src/optionRows.ts => exit 1.
# template-diag row (diagnostic): template-diag-diff corpus (scripts/template-diag-diff.mjs). Mismatches never gate
#   (clean/mismatch are not recorded in the baseline: only crash and oracle-error, D-0219). crash/panic > 0 or a
#   missing/invalid report => exit 1.
#   Clean = result in the closed enum {STATIC_EQUAL, REWRITE_EQUAL} (byte-identical, or semdiff EQUIVALENT-BY-REWRITE).
#   Anything else, including EQUIVALENT-BY-EXECUTION or an unknown value, is unclean.
#   Semdiff binary + its crate are built from THIS script's tree (MAIN when land.sh runs MAIN's board.sh; the official
#   snapshot when official-board.sh runs the snapshot's board.sh), never the tree under test. Registry =
#   ${SEMDIFF_REGISTRY} or notesDecisionsPath() (official-board points it at the hashed notes
#   snapshot). Fail-closed if neither is set/present. Exec leg stays off (D-0256): do not set TDD_SEMDIFF_EXEC.
#   Driver is built into ${TDD_DRIVER_TARGET:-${CARGO_TARGET_DIR:-<repo>/target}/template-diag-driver} from the tree under test.
#   Report JSON: ${TEMPLATE_DIAG_OUT_DIR:-<row log dir>}/template-diag-<engine>.json, never modified after the run; soundness
#   stamp template-diag-<engine>.soundness.json next to it is derived from meta.codegenCompare.mode (no UNSOUND path).
#   Repo root (tools/, crates/) = $BOARD_REPO_ROOT, else dirname $CRATES_ROOT, else this harness's parent.
# (5) per-case EQUALITY ratchet (AB-0006, the AB-L0024 equality model) against harness/board-baseline.tsv (board-baseline/v2,
#   committed, rust backend). Format, parser and comparator: harness/scripts/board-baseline.mjs (the ONE implementation;
#   land.sh's scripts/baseline-ceiling.sh uses the same module). Per row: a counts line + the SET of failing and skipped
#   case ids (vitest fullName within the row's -t filter; read from a per-row vitest JSON report). template-diag* rows:
#   counts only (crash oracle-error; > 0 also fails the row).
#   EVERY row (DIAG rows included: no ungated number in the baseline, AB-0006 rework 2) FAILS on: vitest rc not 0/1, worker crash, no (or >1, or unparseable) Tests summary, rc/failed mismatch,
#     case-id report unusable or inconsistent with the summary, or ANY difference from the committed row, in EITHER direction:
#     a newly failing/skipped case (REGRESSION) or a fixed one (IMPROVED: reseed in the same commit). No non-fatal state.
#   DIAG (board-rows.sh) now only means: option-gate does not count the row as coverage for an option field. Their
#     failing cases are allowed exactly as recorded, like every row.
#   official-board.sh (D-0282) is the land's board: it runs this file from the archived landed sha.
#   BOARD_RATCHET_UPDATE=1 (full run only, rust backend only; official-board.sh unsets it): rewrites board-baseline.tsv
#     from the observed sets iff no row got WORSE (board-baseline.mjs worse(): new failing/skipped id, a swap, fewer
#     passed, fixed ids without as many new passes, row total drop; template-diag*: crash/oracle-error up) and no
#     other gate failed. Worse can only be accepted by hand-editing the baseline in a commit carrying
#     'Baseline-Raise: <AB-id>' (its own APPROVE review; land.sh's baseline ceiling checks it).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"   # THIS script's harness: runner, rows, obs writer, comparator
# D-0282: every mktemp dir this script creates is tracked and removed on EXIT/INT/TERM.
board_tmp_dirs=()
cleanup_board_tmps() {
  local d
  for d in "${board_tmp_dirs[@]+"${board_tmp_dirs[@]}"}"; do
    [ -n "$d" ] || continue
    rm -rf "$d"
  done
}
trap cleanup_board_tmps EXIT INT TERM
source "$here/../scripts/cargo-locked.sh"
# shellcheck source=oracle-root.sh
source "$here/scripts/oracle-root.sh"
ORACLE="$(oracle_root)" || exit 2
export LWC_ORACLE="$ORACLE" LWC_ROOT="$ORACLE"
ORACLE_SHA="$(oracle_pin "$here/oracle.pin")" || exit 2
if [ -z "${SEMDIFF_REGISTRY:-}" ]; then
  SEMDIFF_REGISTRY="$(notes_decisions_path)" || exit 2
  export SEMDIFF_REGISTRY
fi
do_check=0; root=""; suites=()
for a in "$@"; do
  case "$a" in
    check) do_check=1 ;;
    --root=*) root="${a#--root=}"; [ -n "$root" ] || { echo "!! board.sh: --root= needs a directory" >&2; exit 2; } ;;
    --root) echo "!! board.sh: use --root=<tree>" >&2; exit 2 ;;
    -*) echo "!! board.sh: unknown flag $a (want --root=<tree>)" >&2; exit 2 ;;
    *) suites+=("$a") ;;
  esac
done
if [ -n "$root" ]; then
  [ -d "$root" ] || { echo "!! board.sh: --root=$root is not a directory" >&2; exit 2; }
  root="$(cd "$root" && pwd)"
  testdir="$root/harness"
  [ -d "$testdir" ] || { echo "!! board.sh: --root=$root has no harness/" >&2; exit 2; }
else
  root="$(cd "$here/.." && pwd)"; testdir="$here"
fi
cd "$testdir"  # suites resolve fixture/virtual paths relative to the tree-under-test's harness/
source "$here/board-rows.sh"   # ROWS, DIAG, option_gate: THIS script's list (MAIN's when land.sh runs MAIN's board.sh)
BASELINE="$testdir/board-baseline.tsv"   # the claim of the tree under test
BB=(node "$here/scripts/board-baseline.mjs")   # board-baseline/v2 parser + comparator (AB-0006): THIS script's
if [ "$do_check" -eq 1 ]; then
  [ ${#suites[@]} -eq 0 ] || { echo "!! board.sh check takes no row arguments (it is the full run)" >&2; exit 2; }
  [ -z "${BOARD_RATCHET_UPDATE:-}" ] || { echo "!! board.sh check: BOARD_RATCHET_UPDATE must be unset" >&2; exit 2; }
fi
update="${BOARD_RATCHET_UPDATE:-}"
case "$update" in ''|1) ;; *) echo "!! BOARD_RATCHET_UPDATE must be unset or 1, got '$update'" >&2; exit 2;; esac
if [ -n "$update" ]; then
  [ ${#suites[@]} -eq 0 ] || { echo "!! BOARD_RATCHET_UPDATE=1 needs a full run (no row arguments)" >&2; exit 2; }
  [ "${LWC_COMPILER_BACKEND:-rust}" = rust ] || { echo "!! BOARD_RATCHET_UPDATE=1 only on the rust backend (LWC_COMPILER_BACKEND=$LWC_COMPILER_BACKEND)" >&2; exit 2; }
fi
# Strict baseline parse (rows exactly ROWS in order); any defect -> blerr (=> board failure, nothing compared as passing).
blerr=(); blrc=0; blout="$("${BB[@]}" check "$BASELINE" "${ROWS[@]}" 2>&1)" || blrc=$?
[ $blrc -eq 0 ] || blerr+=("$blout (rc=$blrc)")
head_sha="$(git -C "$ORACLE" rev-parse HEAD)"
[[ "$head_sha" == "$ORACLE_SHA" ]] || { echo "!! ORACLE NOT PINNED: $ORACLE HEAD=$head_sha, want $ORACLE_SHA" >&2; exit 2; }
dirty="$(git -C "$ORACLE" status --porcelain --untracked-files=all)"
[ -z "$dirty" ] || { echo "!! ORACLE DIRTY ($ORACLE @ $ORACLE_SHA):" >&2; head -20 <<< "$dirty" >&2; exit 2; }
"$here/scripts/ensure-deps.sh" "$testdir" || exit 2
VITEST="$ORACLE/node_modules/.bin/vitest"   # never npx: it silently fetches an unrelated vitest when links are missing
norun=(); rowfail=()
export CRATES_ROOT="${CRATES_ROOT:-$root/crates}" BOARD_REPO_ROOT="${BOARD_REPO_ROOT:-$root}"
export OPTGATE_INVENTORY="${OPTGATE_INVENTORY:-$testdir/option-inventory.json}"
export SHIM_ALLOWLIST="${SHIM_ALLOWLIST:-$testdir/shim-allowlist.tsv}"
repo="${BOARD_REPO_ROOT:-${CRATES_ROOT:+$(dirname "$CRATES_ROOT")}}"; repo="$(cd "${repo:-$root}" && pwd)"
logdir="${BOARD_LOG_DIR:-$(mktemp -d /tmp/board-run.$(date +%Y%m%dT%H%M%S).$$.XXXX)}"; mkdir -p "$logdir"; echo "row logs: $logdir"
obsdir="$logdir/obs"; rm -rf "$obsdir"; mkdir -p "$obsdir"   # one <row>.tsv per observed row (board-baseline/v2 row block)
src="$testdir/lwc-compiler-napi.node"
pins="$testdir/.pins"; mkdir -p "$pins"
hash="$(sha256sum "$src" | cut -c1-12)"
pinned="$pins/lwc-compiler-napi.$hash.node"
[ -f "$pinned" ] || { tmp="$(mktemp "$pins/.tmp.XXXXXX")"; cp "$src" "$tmp"; chmod 0555 "$tmp"; mv -f "$tmp" "$pinned"; }
# prune pins older than 1 day
prc=0; find "$pins" -name 'lwc-compiler-napi.*.node' -mtime +1 -delete || prc=$?
[ $prc -eq 0 ] || echo "warn: pin prune exit $prc (housekeeping only, non-gating; a concurrent board may have pruned first)"
export LWC_COMPILER_BACKEND=rust LWC_COMPILER_NAPI_PATH="$pinned"
export PATH="$ORACLE/node_modules/.bin:$PATH"
# Template diagnostics corpus row: $1=row $2=engine. Prints one board line; any failure -> rowfail (=> !! BOARD FAILED).
template_diag_row() {
  local row="$1" engine="$2" t0=$SECONDS why="" line
  local out="$logdir/board-$row-$hash.out" dir="${TEMPLATE_DIAG_OUT_DIR:-$logdir}"
  local json="$dir/template-diag-$engine.json"
  mkdir -p "$dir"; rm -f "$json" "${json%.json}.soundness.json"   # never read a stale report or stamp
  local tenv=(TDD_ENGINE="$engine" TDD_OUT="$json")
  # D-0219: semdiff comes from THIS script's tree ($here/.. = MAIN or the official snapshot), never $repo (the branch).
  # Registry is SEMDIFF_REGISTRY (official-board: hashed notes snapshot). Exec leg stays unset (D-0256).
  local sdroot; sdroot="$(cd "$here/.." && pwd)"
  local sdtgt="${CARGO_TARGET_DIR:-$sdroot/target}"
  unset TDD_SEMDIFF_EXEC
  # D-0238: committed lock records parse5 as a git source; no per-call --config and no snapshot .cargo copy.
  if ! (cd "$sdroot" && cargo_locked build --release -q -p semdiff --target-dir "$sdtgt") >"$out" 2>&1; then why="semdiff build failed"
  else tenv+=(TDD_SEMDIFF="$sdtgt/release/semdiff" TDD_SEMDIFF_REGISTRY="$SEMDIFF_REGISTRY"); SEMDIFF_BIN="$sdtgt/release/semdiff"; fi
  case "$engine" in
    crate)
      local manifest="$repo/tools/template-diag-diff/driver/Cargo.toml"
      local tgt="${TDD_DRIVER_TARGET:-${CARGO_TARGET_DIR:-$repo/target}/template-diag-driver}"
      if [ -n "$why" ]; then :
      elif [ ! -f "$manifest" ]; then why="driver manifest missing: $manifest"
      elif ! cargo_locked build --release -q --manifest-path "$manifest" --target-dir "$tgt" >>"$out" 2>&1; then why="driver build failed"
      else tenv+=(TDD_DRIVER="$tgt/release/template-diag-driver" TDD_EMIT_PAIRS="$dir/h1-pairs"); fi;;
    # napi = the transformSync seam through this board's pinned binary. NON-AUTHORITATIVE (Migrate 2026-11-01): ~1120
    #   seam-rejects ("invalid compile options") under root-cause by Template Diagnostics; only crash/oracle-error gate it.
    napi) tenv+=(TDD_NAPI="$pinned");;
    *) why="engine '$engine' not wired into board.sh";;
  esac
  if [ -z "$why" ]; then
    env "${tenv[@]}" node "$here/scripts/template-diag-diff.mjs" >>"$out" 2>&1 || why="template-diag-diff.mjs exit $?"
  fi
  echo "row elapsed $((SECONDS - t0))s  report $json" >>"$out"
  line="$(node "$here/scripts/template-diag-summary.mjs" "$json" "$engine" "$row" 2>>"$out")" || why="${why:-$(tail -1 "$out")}"
  printf '%s\n' "${line:-$(printf '%-22s (no summary)' "$row")}"
  local re='  crash ([0-9]+)  oracle-error ([0-9]+)  '   # clean/mismatch are printed, never recorded (D-0219)
  if [[ "$line" =~ $re ]]; then
    _tdobs="$(mktemp /tmp/board-tdobs.XXXXXX)"
    printf '%s\tcounts\t%s %s\n' "$row" "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" > "$_tdobs"
    record_derived_obs "$row" "$_tdobs"
    rm -f "$_tdobs"
    measured_counts["$row"]="${BASH_REMATCH[1]} ${BASH_REMATCH[2]}"
  else why="${why:-summary line unparseable (crash/oracle-error)}"; fi
  [ -z "$why" ] || rowfail+=("$row: $why (see $out)")
}
# Parse a vitest 'Tests  F failed | P passed | S skipped (T)' line -> "P F S"; rc 1 on anything unexpected.
parse_tests() {
  local s="$1" p= f= k= tok toks
  [[ "$s" =~ ^Tests\ +(.+)\ \(([0-9]+)\)$ ]] || return 1
  local tot="${BASH_REMATCH[2]}"; IFS='|' read -ra toks <<< "${BASH_REMATCH[1]}"
  for tok in "${toks[@]}"; do tok="${tok#"${tok%%[! ]*}"}"; tok="${tok%"${tok##*[! ]}"}"
    [[ "$tok" =~ ^([0-9]+)\ (passed|failed|skipped)$ ]] || return 1
    case "${BASH_REMATCH[2]}" in
      passed) [ -z "$p" ] || return 1; p="${BASH_REMATCH[1]}";; failed) [ -z "$f" ] || return 1; f="${BASH_REMATCH[1]}";;
      skipped) [ -z "$k" ] || return 1; k="${BASH_REMATCH[1]}";; esac
  done
  p="${p:-0}" f="${f:-0}" k="${k:-0}"; [ $((p + f + k)) -eq "$tot" ] || return 1
  echo "$p $f $k"
}
# Isolated vitest (A)(B): no board output path in the child environment; new session via setsid;
# after exit, kill the process group and refuse (rc 2) if any member survives. --config is THIS
# tree's vitest.board.config.ts (no plugins; setupFiles/globalSetup from that file's closed lists).
run_isolated_vitest() {
  local out="$1" json="$2"; shift 2
  local vrc=0 vpid leftover
  setsid env -u BOARD_LOG_DIR -u TEMPLATE_DIAG_OUT_DIR -u BOARD_REPO_ROOT -- \
    "$VITEST" run "$@" --config "$here/vitest.board.config.ts" --root "$testdir" --pool=threads --testTimeout="${row_timeout:-5000}" \
    --reporter=dot --reporter=json --outputFile.json="$json" >"$out" 2>&1 &
  vpid=$!
  wait "$vpid" || vrc=$?
  # (1) Copy stdout + json into a directory created AFTER exit — a path the child never saw.
  capdir="$(mktemp -d /tmp/board-cap.XXXXXX)"
  board_tmp_dirs+=("$capdir")
  cp -a -- "$out" "$capdir/out"
  if [ -f "$json" ]; then cp -a -- "$json" "$capdir/json"; fi
  cap_out="$capdir/out"
  cap_json="$capdir/json"
  if kill -0 -- -"$vpid" 2>/dev/null; then
    kill -TERM -- -"$vpid" 2>/dev/null || true
    sleep 0.2
    if kill -0 -- -"$vpid" 2>/dev/null; then
      kill -KILL -- -"$vpid" 2>/dev/null || true
      sleep 0.1
    fi
  fi
  leftover=0
  if pgrep -g "$vpid" >/dev/null 2>&1; then leftover=1; fi
  if [ "$leftover" -eq 1 ]; then
    echo "!! board.sh: vitest process group $vpid still has live members after kill" >&2
    exit 2
  fi
  return "$vrc"
}

# Record the sha256 of a just-derived obs file in a bash variable, then publish a copy to $obsdir.
record_derived_obs() {
  local row="$1" src="$2"
  # Hash and keep the private copy first; $obsdir is only a published copy (land/judge-all).
  if [ -z "${priv_obs:-}" ]; then
    priv_obs="$(mktemp -d /tmp/board-priv-obs.XXXXXX)"
    board_tmp_dirs+=("$priv_obs")
  fi
  cp -a -- "$src" "$priv_obs/$row.tsv"
  obs_sha["$row"]="$(sha256sum -- "$priv_obs/$row.tsv" | awk '{print $1}')"
  cp -a -- "$priv_obs/$row.tsv" "$obsdir/$row.tsv"
  obs_written["$row"]="$(awk -F'\t' '$2=="counts"{print $3; exit}' "$priv_obs/$row.tsv")"
}

# SHA256SUMS is written from the variables, never by re-hashing the (possibly overwritten) files.
write_obs_manifest() {
  local row dest
  if [ -z "${priv_obs:-}" ]; then
    priv_obs="$(mktemp -d /tmp/board-priv-obs.XXXXXX)"
    board_tmp_dirs+=("$priv_obs")
  fi
  for dest in "$priv_obs" "$obsdir"; do
    : > "$dest/SHA256SUMS"
    for row in "${!obs_sha[@]}"; do
      printf '%s  %s\n' "${obs_sha[$row]}" "$row.tsv" >> "$dest/SHA256SUMS"
    done
  done
}

verify_obs_bytes() {
  local when="$1" dir label row f got
  if [ ${#obs_sha[@]} -gt 0 ] && [ -z "${priv_obs:-}" ]; then
    echo "!! BOARD FAILED: private obs dir missing ($when)" >&2; exit 2
  fi
  for dir in ${priv_obs:+"$priv_obs"} "$obsdir"; do
    [ "$dir" = "${priv_obs:-}" ] && label="$when (private)" || label="$when (published)"
    for row in "${!obs_sha[@]}"; do
      f="$dir/$row.tsv"
      [ -f "$f" ] || { echo "!! BOARD FAILED: obs missing for $row ($label)" >&2; exit 2; }
      got="$(sha256sum -- "$f" | awk '{print $1}')"
      if [ "$got" != "${obs_sha[$row]}" ]; then
        echo "!! BOARD FAILED: obs sha256 mismatch for $row (bytes no longer match the hash recorded at derivation)" >&2
        exit 2
      fi
    done
  done
}

# Judge one vitest row: $1=row $2=vitest rc $3=summary line(s) $4=crash count $5=log $6=vitest JSON report $7=-t pattern.
# Derives case-id sets (cross-checked against the summary) into the private dir and publishes $obsdir/<row>.tsv.
judge_row() {
  local row="$1" rc="$2" lines="$3" crashes="$4" out="$5" json="$6" pat="$7" obs nl why=() irc=0
  case "$rc" in 0|1) ;; *) why+=("vitest exit $rc (crash/signal)");; esac
  [ "$crashes" -eq 0 ] || why+=("worker crash x$crashes (SIGBUS/SIGSEGV/Worker exited unexpectedly)")
  nl=0; [ -z "$lines" ] || nl="$(wc -l <<< "$lines")"
  if [ "$nl" -eq 0 ]; then norun+=("$row")
  elif [ "$nl" -ne 1 ]; then why+=("$nl Tests summary lines (want exactly 1)")
  elif ! obs="$(parse_tests "$lines")"; then why+=("unparseable Tests summary: '$lines'")
  else
    local p f k; read -r p f k <<< "$obs"
    if [ "$rc" -eq 0 ] && [ "$f" -ne 0 ]; then why+=("vitest exit 0 but $f failed"); fi
    if [ "$rc" -eq 1 ] && [ "$f" -eq 0 ]; then why+=("vitest exit 1 with 0 failed tests (load/unhandled error)"); fi
    "${BB[@]}" ids "$row" "$json" "$p" "$f" "$k" ${pat:+"$pat"} > "$capdir/$row.tsv" 2>>"$out" || irc=$?
    if [ $irc -ne 0 ]; then rm -f "$capdir/$row.tsv"; why+=("case-id report unusable (board-baseline.mjs ids rc=$irc; see $out)")
    else record_derived_obs "$row" "$capdir/$row.tsv"; measured_counts["$row"]="$p $f $k"; fi
  fi
  [ ${#why[@]} -eq 0 ] || for w in "${why[@]}"; do rowfail+=("$row: $w (see $out)"); done
}
declare -A measured_counts=() obs_written=() obs_sha=() cap_out_for=()
SEMDIFF_BIN="${SEMDIFF_BIN:-}"
if [ ${#suites[@]} -eq 0 ] && [ "${BOARD_SUITES_EXPLICIT:-}" != 1 ]; then suites=("${ROWS[@]}"); fi
echo "napi sha256:$hash  pinned=$pinned  oracle=$head_sha(clean)  oracle_repo=$ORACLE  $(date -Iseconds)"   # official-board.sh parses oracle=/oracle_repo= (verified values)
for s in "${suites[@]}"; do
  case "$s" in template-diag|template-diag-*) template_diag_row "$s" "$( [ "$s" = template-diag ] && echo crate || echo "${s#template-diag-}")"; continue;; esac
  out="$logdir/board-$s-$hash.out"
  test="test/$s.behavior.test.ts"; mode=sync; extra=(); row_timeout=5000
  unset LBC_SUMMARY
  case "$s" in
    js-api) test=test/js-options.behavior.test.ts; extra=(-t '^js-options: api ');;
    js-options) extra=(-t '^js-options: (lws|explicit|sourcemap|epm|dyn) ');;
    template-renderer) test=test/template.behavior.test.ts;;   # D-0217: whole file (was -t 'renderer-hooks/|directive-spread/no-renderer-hook': 14 passed, 554 skipped)
    csr-render) test=test/csr.render.test.ts; export CORPUS=ssr;;
    csr-render-template) test=test/csr.render.test.ts; export CORPUS=template;;
    ssr-render) test=test/ssr.render.test.ts;;
    ssr-render-*) test=test/ssr.render.test.ts; mode="${s#ssr-render-}";;
    ssr-*) test=test/ssr.behavior.test.ts; mode="${s#ssr-}";;
    lbc) row_timeout=20000; LBC_SUMMARY="$logdir/lbc-summary.txt"; export LBC_SUMMARY; rm -f "$LBC_SUMMARY";;
  esac
  json="$logdir/board-$s-$hash.json"; rm -f "$json"; pat=""; [ ${#extra[@]} -eq 0 ] || pat="${extra[1]}"   # extra is (-t <pattern>) or empty
  vrc=0; SSR_MODE="$mode" run_isolated_vitest "$out" "$json" "$test" "${extra[@]}" || vrc=$?
  cap_out_for["$s"]="$cap_out"
  grc=0; lines="$(grep -E '^\s+Tests ' "$cap_out")" || grc=$?
  [ $grc -le 1 ] || { rowfail+=("$s: grep Tests summary failed (rc=$grc)"); lines=""; }
  lines="$(sed 's/^ *//' <<< "$lines")"
  crc=0; err="$(grep -cE 'SIGBUS|SIGSEGV|Worker exited unexpectedly' "$cap_out")" || crc=$?
  [ $crc -le 1 ] || { rowfail+=("$s: grep crash markers failed (rc=$crc)"); err=0; }
  tag=""; if [[ "$DIAG" == *" $s "* ]]; then tag="  (diagnostic row: not option coverage; case set still ratcheted)"; fi
  printf '%-22s %s%s%s%s\n' "$s" "$(tail -1 <<< "$lines")" "$tag" "$([ "$err" != 0 ] && echo '  !! WORKER CRASH')" "$([ "$vrc" -gt 1 ] && echo "  !! vitest exit $vrc")"
  if [ "$s" = lbc ]; then
    if [ ! -s "${LBC_SUMMARY:-}" ]; then
      rowfail+=("lbc: missing exclusion summary (${LBC_SUMMARY:-unset})")
    else
      cat -- "$LBC_SUMMARY"
    fi
  fi
  # D-0242: scope-token async passes (0 skipped) and is printed here. The file bytes are
  # SCOPE_TOKEN_ASYNC_LIMITATION.line plus one LF, written by test/ssr.render.test.ts.
  # That writer emits no CR. Compare the file as stored.
  if [[ "$s" == ssr-render || "$s" == ssr-render-* ]]; then
    _rt="${SSR_RUNTIME:-dist}"
    case "$_rt" in dist|src) ;; *) rowfail+=("$s: SSR_RUNTIME must be dist or src, got '$_rt'"); _rt=dist;; esac
    klf="$testdir/.ssr-render-out/rust-${mode}-${_rt}/known-limitation.txt"
    kl_want='known-limitation: ssr-render-async scope-token (D-0242) both backends render [object Promise]'
    if [ "$mode" = async ]; then
      if [ ! -f "$klf" ]; then
        rowfail+=("$s: missing D-0242 known-limitation log ($klf)")
      else
        cat -- "$klf"
        kl_expect="$(mktemp)"
        printf '%s\n' "$kl_want" > "$kl_expect"
        kl_cmp=0
        cmp -s "$kl_expect" "$klf" || kl_cmp=$?
        rm -f "$kl_expect"
        if [ "$kl_cmp" -ne 0 ]; then
          rowfail+=("$s: known-limitation log is not the D-0242 scope-token line")
        fi
      fi
    elif [ -f "$klf" ]; then
      rowfail+=("$s: unexpected known-limitation log (D-0242 names async scope-token only)")
      cat -- "$klf"
    fi
  fi
  judge_row "$s" "$vrc" "$lines" "$err" "$cap_out" "${cap_json:-}" "$pat"
done
# PLAN §3 item 4 / Arch 03112c1: semantic-diff row on this run's crate H-1 pairs.
# --gate strict, no baseline. Registry is SEMDIFF_REGISTRY (notes on the box;
# official-board's NOTES_COMMIT archive; portable snapshot only when notes are absent).
sdrow=0
_td_ran=0
for _s in "${suites[@]}"; do [ "$_s" = template-diag ] && _td_ran=1; done
if [ "$_td_ran" -eq 1 ]; then
  _tdir="${TEMPLATE_DIAG_OUT_DIR:-$logdir}"
  _tdjson="$_tdir/template-diag-crate.json"
  _tdman="$_tdir/h1-pairs/manifest.json"
  _tdout="$_tdir/semantic-diff.json"
  if [ -z "${SEMDIFF_BIN:-}" ] || [ ! -x "${SEMDIFF_BIN:-}" ]; then
    echo "!! BOARD FAILED: semantic-diff: semdiff binary missing"; sdrow=1
  elif [ ! -s "$_tdjson" ]; then
    echo "!! BOARD FAILED: semantic-diff: producer report missing ($_tdjson)"; sdrow=1
  else
    _exp=""
    if ! _exp="$(node -e '
const fs=require("fs");
const r=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));
if (r.schema!=="template-diag-diff/v2" || typeof r.totals?.codegenOnly!=="number") process.exit(2);
process.stdout.write(String(r.totals.codegenOnly));
' "$_tdjson")"; then
      echo "!! BOARD FAILED: semantic-diff: cannot read totals.codegenOnly from $_tdjson"; sdrow=1
    else
      _sdrc=0
      _sdline="$("$here/scripts/semantic-diff-row.sh"         --manifest "$_tdman"         --registry "$SEMDIFF_REGISTRY"         --expect-corpus template-diag-codegen-only         --expect-cases "$_exp"         --semdiff "$SEMDIFF_BIN"         --out "$_tdout")" || _sdrc=$?
      printf '%s
' "$_sdline"
      if [ "$_sdrc" -ne 0 ]; then
        echo "!! BOARD FAILED: semantic-diff row (rc=$_sdrc)"; sdrow=1
      fi
    fi
  fi
fi
# SD-2b: SSR pairs from the harness bundleFixture emitter, then semdiff --exec (engine=ssr-runtime).
# Distinct from the H-1 semantic-diff row above: other corpus, no --gate strict. A full board only.
if [ ${#suites[@]} -eq ${#ROWS[@]} ]; then
  _ssrdir="$logdir/ssr-pairs"
  _ssrout="$logdir/semantic-diff-ssr.json"
  _ssrlog="$logdir/board-semantic-diff-ssr-$hash.out"
  : > "$_ssrlog"
  _ssrrc=0
  if [ -z "${SEMDIFF_BIN:-}" ] || [ ! -x "${SEMDIFF_BIN:-}" ]; then
    _sdroot="$(cd "$here/.." && pwd)"
    _sdtgt="${CARGO_TARGET_DIR:-$_sdroot/target}"
    if ! (cd "$_sdroot" && cargo_locked build --release -q -p semdiff --target-dir "$_sdtgt") >>"$_ssrlog" 2>&1; then
      echo "!! BOARD FAILED: semantic-diff-ssr: semdiff build failed"
      _ssrrc=1
    else
      SEMDIFF_BIN="$_sdtgt/release/semdiff"
    fi
  fi
  if [ "$_ssrrc" -eq 0 ]; then
    if ! node --experimental-transform-types --import "$here/scripts/ts-register.mjs" "$here/scripts/ssr-emit-pairs.ts" --out "$_ssrdir" --expect-fixtures 387 >>"$_ssrlog" 2>&1; then
      echo "!! BOARD FAILED: semantic-diff-ssr: emitter failed (completeness exits 2 when a fixture is dropped)"
      _ssrrc=2
    fi
  fi
  if [ "$_ssrrc" -eq 0 ]; then
    _sdrc=0
    "$SEMDIFF_BIN" run --manifest "$_ssrdir/manifest.json" --out "$_ssrout" --registry "$SEMDIFF_REGISTRY" --gate none --exec >>"$_ssrlog" 2>&1 || _sdrc=$?
    if [ "$_sdrc" -ne 0 ] && [ ! -s "$_ssrout" ]; then
      echo "!! BOARD FAILED: semantic-diff-ssr: semdiff rc=$_sdrc and no summary"
      _ssrrc=2
    fi
  fi
  if [ "$_ssrrc" -eq 0 ]; then
    _ssrline=""
    _linerc=0
    _ssrline="$(node "$here/scripts/semantic-diff-ssr-line.mjs" --summary "$_ssrout" --meta "$_ssrdir/meta.jsonl" --exclusions "$_ssrdir/exclusions.json" 2>>"$_ssrlog")" || _linerc=$?
    printf '%s\n' "$_ssrline"
    if [ "$_linerc" -ne 0 ]; then
      echo "!! BOARD FAILED: semantic-diff-ssr line (rc=$_linerc)"
      _ssrrc=1
    fi
  fi
  if [ "$_ssrrc" -ne 0 ]; then
    rowfail+=("semantic-diff-ssr rc=$_ssrrc (see $_ssrlog)")
  fi
fi
shimgate=0; SHIM_REPO_ROOT="$repo" SHIM_OWNERS="$here/../OWNERS" SHIM_REGISTRY="${SHIM_REGISTRY:-$testdir/shim-registry.tsv}" SHIM_ALLOWLIST="${SHIM_ALLOWLIST:-$testdir/shim-allowlist.tsv}" node "$here/scripts/shim-gate.mjs" || shimgate=1
gate=0; option_gate || gate=1
# Coverage gate (v2 spec): report-only unless COVGATE_STRICT=1. Same ROWS the option gate saw.
covgate=0
COVGATE_DIAG="$DIAG" COVGATE_RATCHET="${RATCHET_FILE:-}" SEMDIFF_BIN="${SEMDIFF_BIN:-}" \
  node "$here/scripts/coverage-gate.mjs" ${COVGATE_STRICT:+--strict} "${ROWS[*]}" || covgate=1
gioguard=0; "$here/scripts/golden-io-guard.sh" "$testdir" || gioguard=1
# Construct the tag spell from parts so this file has no raw tag-open sequence (registry scanner).
_shim_tok=SHIM; _shim_lit="${_shim_tok}("
# Write SHA256SUMS from the derivation-time hashes (never by re-hashing files), then
# re-verify the published obs bytes against those variables before judge (1).
write_obs_manifest
verify_obs_bytes "before judge"
# Ratchet (AB-0006): equality per run row, or (BOARD_RATCHET_UPDATE=1) rewrite iff no gating row got worse and nothing else failed.
ratchet=0
if [ ${#blerr[@]} -ne 0 ]; then ratchet=1
elif [ -n "$update" ]; then
  if [ $gate -eq 0 ] && [ $covgate -eq 0 ] && [ $shimgate -eq 0 ] && [ $gioguard -eq 0 ] && [ $sdrow -eq 0 ] && [ ${#norun[@]} -eq 0 ] && [ ${#rowfail[@]} -eq 0 ]; then
    tmpb="$(mktemp "$BASELINE.tmp.XXXXXX")"; urc=0
    "${BB[@]}" update "$BASELINE" "$priv_obs" "$tmpb" "${ROWS[@]}" || urc=$?
    if [ $urc -eq 0 ]; then mv -f "$tmpb" "$BASELINE"; echo "RATCHET-UPDATE: wrote $BASELINE (commit it)"
    else rm -f "$tmpb"; rowfail+=("BOARD_RATCHET_UPDATE=1: baseline NOT rewritten (board-baseline.mjs update rc=$urc)"); fi
  else rowfail+=("BOARD_RATCHET_UPDATE=1: baseline NOT rewritten (failures above)"); fi
else
  jrc=0; "${BB[@]}" judge "$BASELINE" "$priv_obs" "${suites[@]}" || jrc=$?
  [ $jrc -eq 0 ] || { ratchet=1; rowfail+=("ratchet: measured case sets != harness/board-baseline.tsv (judge rc=$jrc; lines '!! RATCHET' above)"); }
fi
if [ $shimgate -ne 0 ]; then echo "!! BOARD FAILED: shim gate — untagged word, or ${_shim_lit} untracked/stale vs harness/shim-registry.tsv"; fi
if [ $gate -ne 0 ]; then echo "!! BOARD FAILED: option-inventory gate"; fi
if [ $covgate -ne 0 ]; then echo "!! BOARD FAILED: coverage gate"; fi
if [ $gioguard -ne 0 ]; then echo "!! BOARD FAILED: golden-I/O guard (only src/optionRows.ts may write goldens or read UPDATE_GOLDENS)"; fi
if [ ${#norun[@]} -ne 0 ]; then echo "!! BOARD FAILED: row(s) produced no test summary: ${norun[*]} (see $logdir/board-<row>-$hash.out)"; fi
for f in "${rowfail[@]}"; do echo "!! BOARD FAILED: $f"; done
for f in "${blerr[@]}"; do echo "!! BOARD FAILED: baseline $BASELINE: $f"; done
# Re-check the captured run summary and the judged obs right before return (A).
# stdout counts (including out-of-filter skips) are re-read from the row log;
# obs counts are the filter-scoped set written by ids() and must still match that write.
for _r in "${!measured_counts[@]}"; do
  [ -f "$priv_obs/$_r.tsv" ] || { echo "!! BOARD FAILED: obs missing for $_r before return"; exit 2; }
  _got="$(awk -F'	' -v r="$_r" '$1==r && $2=="counts"{print $3; exit}' "$priv_obs/$_r.tsv")"
  if [ "$_got" != "${obs_written[$_r]}" ]; then
    echo "!! BOARD FAILED: obs counts for $_r are '$_got', but the judged write recorded '${obs_written[$_r]}'" >&2
    exit 2
  fi
  _log="${cap_out_for[$_r]:-}"
  if [ -n "$_log" ] && [ -f "$_log" ] && grep -qE '^[[:space:]]*Tests ' "$_log"; then
    _lines="$(grep -E '^[[:space:]]*Tests ' "$_log" | sed 's/^ *//')"
    _nl=0; [ -z "$_lines" ] || _nl="$(wc -l <<< "$_lines")"
    _now=""
    if [ "$_nl" -eq 1 ]; then _now="$(parse_tests "$_lines" || true)"; fi
    if [ "$_now" != "${measured_counts[$_r]}" ]; then
      echo "!! BOARD FAILED: stdout Tests counts for $_r are '${_now:-<unparseable>}', run summary had '${measured_counts[$_r]}'" >&2
      exit 2
    fi
  fi
done
verify_obs_bytes "before return"
[ $gate -eq 0 ] && [ $covgate -eq 0 ] && [ $shimgate -eq 0 ] && [ $gioguard -eq 0 ] && [ $ratchet -eq 0 ] && [ $sdrow -eq 0 ] && [ ${#norun[@]} -eq 0 ] && [ ${#rowfail[@]} -eq 0 ] && [ ${#blerr[@]} -eq 0 ] || exit 1
