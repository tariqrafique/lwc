#!/usr/bin/env bash
# The ONLY way onto main. Run from your worktree (/workspace/wt/<bot>, branch bot/<bot>) with everything committed.
# Under one global flock: rebase onto main -> parse5 pin identity -> cargo check --workspace --all-targets -> cargo test --workspace
# -> napi release build -> lint-shell-pipes -> lint-no-stripped-compare -> ratchet ceiling -> board-baseline ceiling (git blobs; Baseline-Raise) -> shim gate -> option gate (compiler/napi/shared/option files changed)
# -> golden-I/O guard (harness test/src changed) -> golden-gate (harness goldens/test/src/fixtures/scripts changed)
# -> refresh harness/decisions.snapshot.tsv from notes HEAD if stale (follow-on commit, D-0282) -> fast-forward main -> rebuild + install the live napi from main (only when crates changed).
# The official board built from the landed sha is the land's board (D-0282); land.sh does not run board.sh check.
# Any failure before the fast-forward: nothing lands, main untouched. Gate scripts always run from MAIN's copies
# (the base commit), fed the branch's inputs, so a branch cannot weaken the gate that judges it (AB-0005).
# Conflicts are yours: resolve with `git rebase main` in your worktree and re-run.
set -euo pipefail
# >>> main-chokepoint
# The only place a MAIN token may appear in this file. Every reach into MAIN goes
# through main_run (execute a MAIN path) or main_data (resolve a MAIN path).
MAIN=/workspace/lwc-compiler-rust
main_run() {
  local _rel="$1"; shift
  [ -n "$_rel" ] || { echo "main_run: empty path" >&2; return 2; }
  case "$_rel" in
    *.mjs|*.js) MAIN="$MAIN" node "$MAIN/$_rel" "$@" ;;
    *)          MAIN="$MAIN" "$MAIN/$_rel" "$@" ;;
  esac
}
main_data() {
  local _rel="$1"
  [ -n "$_rel" ] || { echo "main_data: empty path" >&2; return 2; }
  printf '%s\n' "$MAIN/$_rel"
}
# <<< main-chokepoint
# Always run MAIN's copy of this script, so a branch cannot weaken its own landing gates.
[ "$(readlink -f "$0")" = "$(main_data scripts/land.sh)" ] || exec "$(main_data scripts/land.sh)" "$@"
die() { echo "!! LAND REFUSED: $*" >&2; exit 1; }
# MAIN checkout directory, derived from the chokepoint (no extra MAIN token).
main_checkout="$(cd "$(dirname "$(main_data scripts/land.sh)")/.." && pwd)"

# Table-driven box-config logging (AB-L0079). Every BOX_CONFIG_DEPENDENT depends_on
# is hashed (or reported absent) so a land log records the box files that can change
# a cargo/git verdict.
log_box_configs() {
  local table inc path class dep policy f sha
  table="$(main_data scripts/land-main-refs.tsv)"
  [ -f "$table" ] || { die2 "MAIN land-main-refs.tsv missing at $table"; return $?; }
  # D-0286(2): MAIN's rows UNION the branch's added rows (check-land-main-refs judges removals).
  inc="${wt:+$wt/scripts/land-main-refs.tsv}"
  [ -n "$inc" ] && [ -f "$inc" ] || inc=/dev/null
  while IFS=$'\t' read -r path class dep policy; do
    [ -n "${path:-}" ] || continue
    case "$path" in '#'*) continue ;; esac
    [ "$class" = BOX_CONFIG_DEPENDENT ] || continue
    f="$dep"
    case "$dep" in
      '~/.gitconfig') f="${HOME}/.gitconfig" ;;
      CARGO_HOME) f="${CARGO_HOME:-$HOME/.cargo}/config.toml" ;;
    esac
    if [ -f "$f" ]; then
      sha="$(sha256sum "$f" | awk '{print $1}')"
      echo "box-config: $dep sha256=$sha"
    else
      echo "box-config: $dep absent"
      if [ "${policy:-required}" != absent_ok ]; then
        die2 "required box-config $dep is absent" || return $?
      fi
    fi
  done < <(awk -F'\t' '!seen[$1 FS $3]++' "$table" "$inc")
}

if [ -f "$HOME/.cargo/env" ]; then source "$HOME/.cargo/env"; fi
wt="$(git rev-parse --show-toplevel)"
cd "$wt"   # every relative path below is worktree-relative
rc=0; br="$(git symbolic-ref --short -q HEAD)" || rc=$?
case $rc in 0) ;; 1) br="";; *) die "git symbolic-ref failed (rc=$rc)";; esac
[ "$wt" != "$main_checkout" ] && [ "$br" != main ] && [ -n "$br" ] || die "run from your worktree on bot/<name>, not from $main_checkout / main"
[ -z "$(git status --porcelain)" ] || die "uncommitted changes in $wt (commit or stash first)"
exec 9>/workspace/.lwc-land.lock
flock -n 9 || { echo "land: waiting for another land to finish..."; flock 9; }
mst="$(git -C "$main_checkout" status --porcelain)"; [ -z "$mst" ] || die "$main_checkout has uncommitted edits (someone edited main directly): $(head -5 <<< "$mst" | tr '\n' ' ')"
if ! git rebase main >/dev/null 2>&1; then
  git rebase --abort || die "rebase onto main failed AND 'git rebase --abort' failed; clean up $wt by hand"
  die "rebase onto main conflicts; run 'git rebase main' in $wt, resolve, commit, re-run land.sh"; fi
base="$(git rev-parse main)"; head="$(git rev-parse HEAD)"
[ "$base" != "$head" ] || { echo "land: nothing to land ($br == main)"; exit 0; }
changed="$(git diff --no-renames --name-only "$base" "$head")"
echo "land: $br $(git rev-list --count "$base..$head") commit(s) onto main@${base:0:12}"
# D-0276: Arch reviews milestones only (gate flips, cutover, ownership changes). land.sh no longer
# requires a per-land 'Arch-Review: <id>' trailer or a reviews.tsv patch_id match.
AB=/workspace/notes/arch-board
# D-0255 case-set decisions: read from the notes repo's COMMITTED HEAD at land time (pre-land notes), never a working tree.
DEC_REPO=/workspace/notes; DEC_PATH=lwc-rust-migrate/decisions.tsv
# (D)(E) Notes unavailable is rc 2 (not die/rc 1). Pin HEAD once; every show/blame/pickaxe uses this sha.
die2() { echo "!! LAND REFUSED: $*" >&2; exit 2; }
DEC_SHA="$(git -C "$DEC_REPO" rev-parse --verify HEAD 2>/dev/null)" || die2 "notes unavailable: cannot resolve $DEC_REPO HEAD"
PARSE5_DIR=/workspace/parse5-rust main_run scripts/parse5-pin-check.sh "$wt" || die "parse5 pin check failed: Cargo.lock parse5 source rev must match crates/lwc-template/Cargo.toml and the rev must exist in /workspace/parse5-rust (D-0238)"
source "$(main_data scripts/cargo-locked.sh)"
log_box_configs
LWC_ORACLE="$(node "$wt/harness/scripts/oracle-root.mjs" oracle_root)" || die "oracle-root failed"
export LWC_ORACLE
export LWC_ROOT="$LWC_ORACLE"
STYLE_CORPUS="$(node "$wt/harness/scripts/oracle-root.mjs" style_corpus_root)" || die "style-corpus-root failed"
export STYLE_CORPUS
# D-0282: same Cargo.lock+toolchain-keyed rustc cache as official-board.sh.
# A fresh worktree target/ recompiles oxc from scratch; this directory is shared
# across worktrees and official snaps so a crate-change land stays incremental.
_lk="$(git show HEAD:Cargo.lock | sha256sum | awk '{print $1}')"
_tk="$(git show HEAD:rust-toolchain.toml | sha256sum | awk '{print $1}')"
_ck="$(printf '%s%s' "$_lk" "$_tk" | sha256sum | cut -c1-16)"
export CARGO_TARGET_DIR="/workspace/cache/lwc-target/$_ck"
mkdir -p "$CARGO_TARGET_DIR"
echo "land: CARGO_TARGET_DIR=$CARGO_TARGET_DIR (lock+toolchain $_ck)"
_phase() { echo "land-phase: $1 $(date '+%Y-%m-%dT%H:%M:%S%z')"; }
# Rust tests read non-Rust files (e.g. exec_ssr.rs spawns ssr-emit-pairs.ts).
# scripts/rust-test-inputs.tsv is the derived set; --decide is workspace | packages -p … | skip.
crates_changed=
_rust_decision="$(printf '%s\n' "$changed" | scripts/rust-test-inputs.sh --decide)" \
  || die "rust-test-inputs --decide failed"
echo "land: rust-test-inputs --decide $_rust_decision"
case "$_rust_decision" in
  workspace)
    crates_changed=1
    _phase workspace-check
    cargo_locked check --workspace --all-targets || die "workspace check failed"
    _phase workspace-test
    cargo_locked test --workspace || die "workspace test failed"   # all crates: dependents of a changed crate too
    # napi release build gates the landing (before the fast-forward). The official board
    # (built from the landed sha) is the land's board (D-0282). The live install from main stays post-merge.
    _phase napi-release
    cargo_locked build -p lwc-compiler-napi --release || die "napi release build failed"
    ;;
  packages\ *)
    _phase workspace-test
    # Word-split the -p flags. JS-only inputs do not rebuild napi.
    cargo_locked test ${_rust_decision#packages } || die "workspace test failed"
    ;;
  skip)
    echo "land: no rust sources or rust-test inputs changed; skipping workspace check/test and napi release build"
    ;;
  *)
    die "rust-test-inputs --decide produced '$_rust_decision'"
    ;;
esac
# CARGO_TARGET_DIR above writes the .so outside the worktree. test-board-ratchet
# (and anything else that looks at target/release) still expects the worktree path.
_napi_so="$CARGO_TARGET_DIR/release/liblwc_compiler_napi.so"
if [ -n "$crates_changed" ]; then
  [ -f "$_napi_so" ] || die "napi release build produced no $_napi_so"
fi
if [ -f "$_napi_so" ]; then
  mkdir -p "$wt/target/release"
  ln -f "$_napi_so" "$wt/target/release/liblwc_compiler_napi.so" 2>/dev/null   || cp -f "$_napi_so" "$wt/target/release/liblwc_compiler_napi.so"
  echo "land: napi so $_napi_so -> $wt/target/release/liblwc_compiler_napi.so"
fi
# Every land check (self-tests, land-main-refs, path checks, baseline-ceiling + Baseline-Raise/Case-Set,
# shim, option, golden-I/O and golden gates) runs in scripts/preflight.sh, judged by base's gate code and
# lists (git archive of $base). It runs them all and prints one table; any failure refuses the land.
_phase preflight
main_run scripts/preflight.sh --base "$base" --tree "$wt" --head "$head" || die "preflight failed (rc=$?; see the table above)"
# D-0282: no in-land board.sh check. The official board on the landed sha is the measurement.
# Refresh the committed decisions snapshot from notes HEAD if it drifted (do not relax official-board's cmp).
# A stale snapshot becomes a follow-on commit on this branch so ff still carries a matching snapshot.
if [ -x "$wt/scripts/refresh-decisions-snapshot.sh" ]; then
  (cd "$wt" && scripts/refresh-decisions-snapshot.sh) || die2 "decisions snapshot refresh failed"
  if ! git -C "$wt" diff --quiet -- harness/decisions.snapshot.tsv harness/decisions.snapshot.notes-commit; then
    git -C "$wt" add -- harness/decisions.snapshot.tsv harness/decisions.snapshot.notes-commit
    _nsha="$(head -1 "$wt/harness/decisions.snapshot.notes-commit")"
    git -C "$wt" commit --only -m "Refresh decisions snapshot to notes@${_nsha:0:12} (D-0282)" -- \
      harness/decisions.snapshot.tsv harness/decisions.snapshot.notes-commit \
      || die "decisions snapshot follow-on commit failed"
    head="$(git -C "$wt" rev-parse HEAD)"
    echo "land: snapshot refresh follow-on commit ${head:0:12} (notes@${_nsha:0:12})"
  fi
fi
[ "$(git rev-parse main)" = "$base" ] || die "main moved during land (should be impossible under the lock)"
git -C "$main_checkout" merge --ff-only -q "$head" || die "fast-forward of main failed"
echo "landed: main=${head:0:12} ($br)"
if [ -n "$crates_changed" ]; then
  # Post-ff: main already contains the landing. A failed live napi rebuild cannot roll
  # back the ff, and die() would print LAND REFUSED for a land that already succeeded.
  main_run scripts/install-napi.sh || echo "!! landed, but live napi rebuild from main failed (main is still correct)" >&2
fi
