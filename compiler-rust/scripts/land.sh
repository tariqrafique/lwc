#!/usr/bin/env bash
# The ONLY way onto main. Run from your worktree (/workspace/wt/<bot>, branch bot/<bot>) with everything committed.
# Under one global flock: rebase onto main -> parse5 pin identity -> cargo check --workspace --all-targets -> cargo test --workspace
# -> napi release build -> lint-shell-pipes -> lint-no-stripped-compare -> ratchet ceiling -> board-baseline ceiling (git blobs; Baseline-Raise) -> shim gate -> option gate (compiler/napi/shared/option files changed)
# -> golden-I/O guard (harness test/src changed) -> golden-gate (harness goldens/test/src/fixtures/scripts changed)
# -> board check (full dev board on the rebased tree; the committed baseline must EQUAL the measured run) -> fast-forward main -> rebuild + install the live napi from main (only when crates changed).
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
  local table path class dep policy f sha
  table="$(main_data scripts/land-main-refs.tsv)"
  [ -f "$table" ] || { die2 "MAIN land-main-refs.tsv missing at $table"; return $?; }
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
  done < "$table"
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
cargo_locked check --workspace --all-targets || die "workspace check failed"
cargo_locked test --workspace || die "workspace test failed"   # all crates: dependents of a changed crate too
crates_changed=; if grep -q '^crates/\|^Cargo\.' <<< "$changed"; then crates_changed=1; fi
# napi release build gates the landing (before the fast-forward) and feeds the board check below (always: the board must
# measure THIS tree's compiler); the live install from main stays post-merge.
cargo_locked build -p lwc-compiler-napi --release || die "napi release build failed"
if grep -qE '^(scripts/(land|test-land-classify|parse5-pin-check|test-parse5-pin|test-shim-registry|lint-shell-pipes|lint-no-stripped-compare|check-land-main-refs|cargo-locked|ratchet-ceiling|baseline-ceiling|test-board-ratchet)\.sh|scripts/land-main-refs\.tsv|harness/(scripts/shim-gate\.mjs|scripts/board-baseline\.mjs|shim-registry\.tsv|board\.sh|board-rows\.sh|vitest\.config\.ts|vitest\.board\.config\.ts)|harness/test/.*([Ss]etup|globalSetup).*))$' <<< "$changed"; then
  scripts/test-land-classify.sh || die "test-land-classify / parse5-pin / lint-shell-pipes / ratchet-ceiling / shim-registry / main-refs self-test failed"
  # test-board-ratchet is an e2e board proof and needs the napi binary land just built
  # (harness/lwc-compiler-napi.node or target/release/liblwc_compiler_napi.so). It is
  # not part of test-land-classify, so a scratch tree without napi can still classify.
  scripts/test-board-ratchet.sh || die "test-board-ratchet failed: needs napi from the release build above (harness/lwc-compiler-napi.node or target/release/liblwc_compiler_napi.so)"
fi
# Unconditional (D-0274(5)/AB-L0079): MAIN's checker + MAIN's table judge the branch land.sh.
# A branch cannot widen the table in the same landing. Missing MAIN table is rc 2.
# MAIN table is git show of the sha being landed onto, never MAIN's working tree.
_main_refs="$(mktemp)"
if ! git -C "$main_checkout" show "${base}:scripts/land-main-refs.tsv" > "$_main_refs"; then
  rm -f "$_main_refs"
  die "cannot materialize MAIN land-main-refs.tsv from git ($base)"
fi
main_run scripts/check-land-main-refs.sh --check "$wt/scripts/land.sh" "$wt" \
  --main-table "$_main_refs" --incoming-table "$wt/scripts/land-main-refs.tsv" \
  || { _rc=$?; rm -f "$_main_refs"; die "land-main-refs check failed (rc=$_rc)"; }
rm -f "$_main_refs"
if grep -qE '^harness/scripts/shim-gate\.mjs$' <<< "$changed"; then node harness/scripts/shim-gate.mjs --self-test || die "shim-gate self-test failed (AB-X0010)"; fi
if grep -qE '^scripts/(official-board|publish-official|notes-precheck|test-notes-precheck|run-board|run-portable-board|test-board-footers)\.sh$' <<< "$changed"; then scripts/test-notes-precheck.sh && scripts/test-board-footers.sh || die "notes-precheck / board-footers guard test failed (AB-0016 / AB-M0002)"; fi
# Unconditional (AB-X0008): no pipe into a quiet grep in the branch's shell scripts (SIGPIPE fail-open under pipefail).
main_run scripts/lint-shell-pipes.sh "$wt" || die "lint-shell-pipes failed (rc=$?)"
# Unconditional (D-0219, D-0256): no whitespace-stripped or otherwise normalized compare of compiler output.
# Allowlist is the closed path+rule+D-id list next to the script; the lint reads only MAIN's copy.
main_run scripts/lint-no-stripped-compare.sh "$wt" || die "lint-no-stripped-compare failed (rc=$?)"
# Unconditional (AB-X0010): ratchets only go down; every raise is refused here (needs its own reviewed id + dedicated main edit).
ratchets="$wt/harness/ratchets"   # the ONE baseline dir: ceiling-checked here, then read by the shim gate below
main_run scripts/ratchet-ceiling.sh "$ratchets" "$(main_data harness/ratchets)" || die "ratchet ceiling: a harness/ratchets/*.txt value is above main's, missing, or unparseable (rc=$?)"
# Unconditional (AB-0006): the board baseline only gets better. Main's baseline-ceiling.sh reads BOTH sides from git
# (harness/board-baseline.tsv at $base = pre-land main, and at $head): a non-regular blob (symlink) or a missing/renamed
# branch baseline is rc 2 (die, never raise-acceptable). A worse row (any row: new failing/skipped id, fewer passed, fixed
# ids without as many new passes, row total drop, row dropped, template-diag crash/oracle-error up) is rc 1, refused unless
# a commit in the range carries 'Baseline-Raise: <AB-id>' whose LAST reviews.tsv row is APPROVE with patch_id == this
# landing's (a raise lowers a gate, which is a milestone).
# The file is a claim; the board check before the fast-forward proves it equals a measured run and re-ratchets the
# MEASURED run against the pre-land blob (D-0255). A case-set change (D-0255: an id removed from a row's
# passed+failed+skipped universe, or newly skipped) additionally needs a 'Case-Set: <D-id>' trailer whose row in the
# COMMITTED $DEC_REPO $DEC_SHA:$DEC_PATH contains a tab-separated field that is exactly 'case-set:<row>:<id>|<id>'
# (anchored at the start of the field; ids are |-separated tokens from the id charset, running to the end of the field).
# Anything else, such as 'foocase-set:<row>: x', is rc 2. The listed ids are a SUPERSET of the removed/skipped ids
# (D-0255/D-0261). That decisions.tsv line must have been ADDED by migrate <migrate@lwc-bots>
# (git log -G '^D-NNNN\t', line-anchored) and not last rewritten by anyone else (git blame $DEC_SHA); minting is
# migrate-only (notes b0b4450, bin/d-id). Notes unavailable is rc 2, not die/rc 1.
# >>> baseline-ceiling (exercised verbatim by scripts/test-board-ratchet.sh)
die2() { echo "!! LAND REFUSED: $*" >&2; exit 2; }
if [ -z "${DEC_SHA:-}" ]; then
  DEC_SHA="$(git -C "$DEC_REPO" rev-parse --verify HEAD 2>/dev/null)" || die2 "notes unavailable: cannot resolve $DEC_REPO HEAD"
fi
csfile="$(mktemp /tmp/land-caseset.XXXXXX)"
blrc=0; main_run scripts/baseline-ceiling.sh "$wt" "$base" "$head" "$csfile" || blrc=$?
case $blrc in
  0) ;;
  1) raise_ids="$(git log --format='%(trailers:key=Baseline-Raise,valueonly)' "$base..$head" | awk 'NF' | sort -u)"
     [ -n "$raise_ids" ] || die "harness/board-baseline.tsv is WORSE than main's (see above); a raise needs its own reviewed id: commit trailer 'Baseline-Raise: <AB-id>' with an APPROVE row in $AB/reviews.tsv"
     bl_pid="$(git diff "$base" "$head" | git patch-id --stable | cut -d' ' -f1)"
     bl_ok=; for id in $raise_ids; do
       row="$(awk -F'\t' -v id="$id" 'NR==1{for(i=1;i<=NF;i++){h=tolower($i); if(h=="id")c=i; if(h=="verdict")d=i; if(h=="patch_id")p=i}; next} c&&d&&p&&$c==id{print toupper($d) "\t" $p}' "$AB/reviews.tsv" | tail -1)"
       v="${row%%$'\t'*}"; rp="${row#*$'\t'}"
       echo "land: Baseline-Raise $id -> ${v:-<not found>} (reviewed patch-id ${rp:0:12}, landing ${bl_pid:0:12})"
       if [ "$v" = APPROVE ] && [ -n "$rp" ] && [ "$rp" = "$bl_pid" ]; then bl_ok=1; fi; done   # LAST row per id wins
     [ -n "$bl_ok" ] || die "board baseline raise not approved: no Baseline-Raise id with its own APPROVE row (patch_id $bl_pid) in $AB/reviews.tsv: $(echo $raise_ids)"
     echo "land: board baseline raise accepted under Baseline-Raise review";;
  *) die "baseline-ceiling refused (rc=$blrc): could not run, malformed baseline, or a missing / renamed / non-regular baseline blob (never raise-acceptable)";;
esac
# csfile lines: <row><TAB>removed|skipped<TAB><id>  or  <row><TAB>missing<TAB>  (a dropped counts-only DIAG row)
cs_rows=(); declare -A cs_need=()
while IFS=$'\t' read -r _cs_row _cs_kind _cs_id; do
  [ -n "${_cs_row:-}" ] || continue
  if [[ " ${cs_rows[*]-} " != *" $_cs_row "* ]]; then cs_rows+=("$_cs_row"); fi
  [ "$_cs_kind" = missing ] && continue
  [ -n "${_cs_id:-}" ] || continue
  cs_need["$_cs_row"]+="$_cs_id"$'\n'
done < "$csfile"
# D-0261: the decisions.tsv line for <id> must have been ADDED by migrate <migrate@lwc-bots> and not last
# rewritten by anyone else. Identity on the shared box is self-declared; Arch audits each citation.
# Id charset for case-set tokens (vitest fullName-ish): letters, digits, space, and _.:/=#@',()+[]-
# Validated with grep -xE (bash [[ =~ ]] character classes treat ] unreliably).
CS_ID_PAT='[]A-Za-z0-9_ .:/=#@'"'"',()+[-]+'
caseset_ids_ok() {
  # Tokenize on '|' ourselves: `IFS='|' read` drops a trailing empty field, so
  # 'id|' would look well-formed. Reject empty / space-only tokens and duplicates (rc 2).
  local rest="$1" remain tok trimmed seen more
  trimmed="${rest#"${rest%%[![:space:]]*}"}"
  trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
  [ -n "$trimmed" ] || return 1
  remain="$rest"; seen=$'\n'
  while :; do
    more=0
    case "$remain" in *'|'*) more=1; tok="${remain%%|*}"; remain="${remain#*|}" ;; *) tok="$remain" ;; esac
    trimmed="${tok#"${tok%%[![:space:]]*}"}"
    trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
    [ -n "$trimmed" ] || return 1
    grep -qxE "$CS_ID_PAT" <<< "$tok" || return 1
    case "$seen" in *$'\n'"$trimmed"$'\n'*) return 1 ;; esac
    seen+="$trimmed"$'\n'
    [ "$more" -eq 1 ] || break
  done
  return 0
}
caseset_minted_by_migrate() {
  local id="$1" line add_author blame_author
  line="$(git -C "$DEC_REPO" show "${DEC_SHA}:$DEC_PATH" | awk -F'\t' -v id="$id" 'NR>1 && $1==id {n=NR} END{print n}')" || die2 "notes unavailable: cannot git show $DEC_SHA:$DEC_PATH"
  [ -n "$line" ] || { echo "land: Case-Set $id: no row in committed $DEC_PATH"; return 1; }
  blame_author="$(git -C "$DEC_REPO" blame -p -L "$line,$line" "$DEC_SHA" -- "$DEC_PATH" | awk '/^author /{a=substr($0,8)} /^author-mail /{print a, $2; exit}')" || die2 "notes unavailable: git blame $DEC_SHA $DEC_PATH failed"
  add_author="$(git -C "$DEC_REPO" log --reverse -G "^${id}"$'\t' --format='%an <%ae>' "$DEC_SHA" -- "$DEC_PATH")" || die2 "notes unavailable: git log -G pickaxe failed"
  add_author="${add_author%%$'\n'*}"
  [ -n "$add_author" ] || { echo "land: Case-Set $id: cannot find the adding notes commit (git log -G '^${id}\\t')"; return 1; }
  [ "$add_author" = "migrate <migrate@lwc-bots>" ] || { echo "land: Case-Set $id was added by '${add_author}', want migrate <migrate@lwc-bots> (D-0261)"; return 1; }
  [ -n "$blame_author" ] || { echo "land: Case-Set $id: git blame produced no author"; return 1; }
  [ "$blame_author" = "migrate <migrate@lwc-bots>" ] || { echo "land: Case-Set $id was last written by '${blame_author}', want migrate <migrate@lwc-bots> (a later rewrite by another author is refused, D-0261)"; return 1; }
  return 0
}
if [ ${#cs_rows[@]} -gt 0 ]; then
  cs_ids="$(git log --format='%(trailers:key=Case-Set,valueonly)' "$base..$head" | awk 'NF' | sort -u)" || die "git log Case-Set trailers failed"
  [ -n "$cs_ids" ] || die "case-set change in row(s) ${cs_rows[*]} (removed or newly skipped case ids, D-0255): needs a commit trailer 'Case-Set: <D-id>' whose decision lists case-set:<row>:<id>|<id>"
  dec="$(git -C "$DEC_REPO" show "${DEC_SHA}:$DEC_PATH")" || die2 "notes unavailable: cannot read committed $DEC_REPO $DEC_SHA:$DEC_PATH for Case-Set"
  for cs_row in "${cs_rows[@]}"; do
    cs_ok=; for id in $cs_ids; do
      [[ "$id" =~ ^D-[0-9]{4}$ ]] || die "Case-Set trailer '$id' is not a D-id (D-NNNN)"
      drow="$(awk -F'\t' -v id="$id" 'NR>1 && $1==id' <<< "$dec" | tail -1)"
      [ -n "$drow" ] || continue
      # (C) case-set:<row>: matches only at the start of a tab-separated field. Ids are |-separated tokens
      # from the id charset, running to the end of the field. 'foocase-set:<row>: x' is rc 2.
      listed=
      prefix="case-set:${cs_row}:"
      while IFS= read -r _fld; do
        if [[ "$_fld" == "$prefix"* ]]; then
          rest="${_fld#"$prefix"}"
          if ! caseset_ids_ok "$rest"; then
            die2 "Case-Set $id: case-set:$cs_row: field is malformed (ids must be |-separated tokens from the id charset, running to the end of the field; a trailing '|', an empty or space-only list, or duplicate ids is rc 2)"
          fi
          listed="$rest"
        elif [[ "$_fld" == *"case-set:${cs_row}:"* ]]; then
          die2 "Case-Set $id: 'case-set:$cs_row:' is not at the start of a tab-separated field (e.g. foocase-set: is refused)"
        fi
      done < <(awk -F'\t' '{for(i=1;i<=NF;i++) print $i}' <<< "$drow")
      [ -n "$listed" ] || continue
      need_ok=1
      while IFS= read -r nid; do
        [ -z "$nid" ] && continue
        found=0
        IFS='|' read -ra _lids <<< "$listed"
        for x in "${_lids[@]}"; do
          x="${x#"${x%%[![:space:]]*}"}"; x="${x%"${x##*[![:space:]]}"}"
          [ "$x" = "$nid" ] && found=1
        done
        [ "$found" -eq 1 ] || need_ok=0
      done <<< "${cs_need[$cs_row]-}"
      [ "$need_ok" -eq 1 ] || { echo "land: Case-Set $id lists case-set:$cs_row:... but is not a superset of the removed/skipped ids"; continue; }
      if caseset_minted_by_migrate "$id"; then cs_ok="$id"; break; fi
    done
    [ -n "$cs_ok" ] || die "case-set change in row $cs_row: no cited Case-Set D-id ($(echo $cs_ids)) has ONE committed $DEC_PATH row listing case-set:$cs_row:<ids> that cover every removed/skipped id and was minted by migrate <migrate@lwc-bots> (D-0261, notes b0b4450)"
    echo "land: case-set change in $cs_row accepted under $cs_ok"
  done
fi
# <<< baseline-ceiling
# Gates below run MAIN's scripts against the branch's inputs ($wt). A changed gate script takes effect only once landed.
# Combined AB-L0072 form (SHIM_RATCHET_DIR kept from AB-0006 / AB-X0010).
CRATES_ROOT="$wt/crates" SHIM_ALLOWLIST="$wt/harness/shim-allowlist.tsv" SHIM_REGISTRY="$wt/harness/shim-registry.tsv" SHIM_RATCHET_DIR="$ratchets" SHIM_OWNERS="$(main_data OWNERS)" SHIM_REPO_ROOT="$wt" main_run harness/scripts/shim-gate.mjs || die "shim gate failed"
if grep -qE '^(crates/(lwc-compiler|lwc-compiler-napi|lwc-shared)/|harness/(board\.sh|board-rows\.sh|option-inventory\.json|src/backend\.ts|scripts/option-(gate|fields)\.mjs)$)' <<< "$changed"; then
  source "$(main_data harness/board-rows.sh)"   # ROWS/DIAG + option_gate: same source of truth as harness/board.sh
  CRATES_ROOT="$wt/crates" OPTGATE_INVENTORY="$wt/harness/option-inventory.json" option_gate || die "option-inventory gate failed"
fi
if grep -qE '^harness/(test|src)/' <<< "$changed"; then
  main_run harness/scripts/golden-io-guard.sh "$wt/harness" || die "golden-I/O guard: only harness/src/optionRows.ts may write goldens or read UPDATE_GOLDENS"
fi
if grep -qE '^harness/(goldens|test|src|fixtures|scripts)/' <<< "$changed"; then
  main_run harness/scripts/ensure-deps.sh "$wt/harness" || die "harness deps missing"
  main_run harness/scripts/golden-gate.sh "$wt/harness" || die "golden-gate: goldens != fresh JS-oracle recording"
fi
# Unconditional (AB-0006 rework 3 / D-0255): the committed baseline must EQUAL a measured run of the rebased tree.
# MAIN's board.sh is the measurement producer: it runs the branch tree's tests/fixtures/crates/napi via --root=$wt.
# The branch's board.sh is never executed. MAIN's comparator then re-judges obs (judge-all). Wall time is printed.
napi_tmp="$(mktemp "$wt/harness/.lwc-compiler-napi.XXXXXX")"; cp "${CARGO_TARGET_DIR:-$wt/target}/release/liblwc_compiler_napi.so" "$napi_tmp"
mv -f "$napi_tmp" "$wt/harness/lwc-compiler-napi.node"   # atomic, as scripts/install-napi.sh (gitignored)
# >>> board-check (exercised verbatim by scripts/test-board-ratchet.sh)
bdir="$(mktemp -d /tmp/land-board.XXXXXX)"; bt0=$SECONDS
log_box_configs
brc=0; ( unset BOARD_RATCHET_UPDATE TEMPLATE_DIAG_OUT_DIR OPTGATE_DECISIONS
         export BOARD_LOG_DIR="$bdir" CRATES_ROOT="$wt/crates" BOARD_REPO_ROOT="$wt"
         main_run harness/board.sh check --root="$wt" > "$bdir/board.out" 2>&1 ) || brc=$?
echo "land: board check: board.sh rc=$brc in $((SECONDS - bt0))s (log $bdir/board.out)"
[ $brc -eq 0 ] || { tail -25 "$bdir/board.out" >&2; die "board check: the rebased tree's board run failed or differs from harness/board-baseline.tsv (rc=$brc)"; }
bst="$(git status --porcelain)"; [ -z "$bst" ] || die "board check: board.sh modified the worktree: $(head -5 <<< "$bst" | tr '\n' ' ')"
git show "$head:harness/board-baseline.tsv" > "$bdir/baseline.head.tsv"
main_run harness/scripts/board-baseline.mjs judge-all "$bdir/baseline.head.tsv" "$bdir/obs" "$bdir/measured.tsv" || die "board check: $head:harness/board-baseline.tsv != the measured run (main's judge-all)"
# D-0255: ratchet the MEASURED run against the pre-land blob; its verdict and case-set rows must be the ones already
# accepted above for the committed claim (so the Baseline-Raise / Case-Set checks covered exactly this measurement).
mrc=0; main_run scripts/baseline-ceiling.sh "$wt" "$base" "$head" "$bdir/case-set.measured" "$bdir/measured.tsv" || mrc=$?
[ $mrc -eq $blrc ] && cmp -s "$bdir/case-set.measured" "$csfile" || die "board check: measured run vs pre-land baseline gave rc=$mrc case-set [$(echo $(cat "$bdir/case-set.measured"))], the committed claim gave rc=$blrc case-set [$(echo "${cs_rows[@]}")]"
echo "land: board check ok in $((SECONDS - bt0))s"
# <<< board-check
[ "$(git rev-parse main)" = "$base" ] || die "main moved during land (should be impossible under the lock)"
git -C "$main_checkout" merge --ff-only -q "$head" || die "fast-forward of main failed"
echo "landed: main=${head:0:12} ($br)"
if [ -n "$crates_changed" ]; then
  # Post-ff: main already contains the landing. A failed live napi rebuild cannot roll
  # back the ff, and die() would print LAND REFUSED for a land that already succeeded.
  main_run scripts/install-napi.sh || echo "!! landed, but live napi rebuild from main failed (main is still correct)" >&2
fi
