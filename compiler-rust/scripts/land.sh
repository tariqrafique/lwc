#!/usr/bin/env bash
# The ONLY way onto main. Run from your worktree (/workspace/wt/<bot>, branch bot/<bot>) with everything committed.
# Under one global flock: rebase onto main -> cargo check --workspace --all-targets -> tests of changed crates
# -> shim gate -> golden-gate if harness tests/goldens/src/fixtures changed -> fast-forward main -> rebuild +
# install the live napi from main (only when crates changed). Any failure: nothing lands, main untouched.
# Conflicts are yours: resolve with `git rebase main` in your worktree and re-run.
set -euo pipefail
MAIN=/workspace/lwc-compiler-rust
# Always run main's copy of this script, so a branch cannot weaken its own landing gates.
[ "$(readlink -f "$0")" = "$MAIN/scripts/land.sh" ] || exec "$MAIN/scripts/land.sh" "$@"
die() { echo "!! LAND REFUSED: $*" >&2; exit 1; }
source "$HOME/.cargo/env" 2>/dev/null || true
wt="$(git rev-parse --show-toplevel)"
br="$(git symbolic-ref --short -q HEAD || true)"
[ "$wt" != "$MAIN" ] && [ "$br" != main ] && [ -n "$br" ] || die "run from your worktree on bot/<name>, not from $MAIN / main"
[ -z "$(git status --porcelain)" ] || die "uncommitted changes in $wt (commit or stash first)"
exec 9>/workspace/.lwc-land.lock
flock -n 9 || { echo "land: waiting for another land to finish..."; flock 9; }
[ -z "$(git -C "$MAIN" status --porcelain)" ] || die "$MAIN has uncommitted edits (someone edited main directly): $(git -C "$MAIN" status --porcelain | head -5 | tr '\n' ' ')"
git rebase main >/dev/null 2>&1 || { git rebase --abort 2>/dev/null || true; die "rebase onto main conflicts; run 'git rebase main' in $wt, resolve, commit, re-run land.sh"; }
base="$(git rev-parse main)"; head="$(git rev-parse HEAD)"
[ "$base" != "$head" ] || { echo "land: nothing to land ($br == main)"; exit 0; }
changed="$(git diff --no-renames --name-only "$base" "$head")"
echo "land: $br $(git rev-list --count "$base..$head") commit(s) onto main@${base:0:12}"
# Arch review gate (policy owned by LWC Arch Board in /workspace/notes/arch-board/):
#   trivial-paths.txt : one glob per line ('*' crosses '/'; '+glob' = additions-only); a landing is trivial iff EVERY
#                       changed path matches. Policy files are read ONLY from /workspace/notes, never from the branch.
#   reviews.tsv       : header row with columns id, verdict, patch_id (git patch-id --stable of the whole landing diff,
#                       scripts/review-patch-id.sh); the landing's recomputed patch-id must equal the row's. non-trivial landings need a commit in the range
#                       carrying trailer 'Arch-Review: <id>' whose row has verdict APPROVE. Fails closed.
AB=/workspace/notes/arch-board
# >>> classify (exercised verbatim by scripts/test-land-classify.sh)
cls_out="$("$MAIN/scripts/land-classify.sh" "$base" "$head" "$AB/trivial-paths.txt")" || die "trivial classifier failed (rc=$?)"
nontrivial=(); [ -z "$cls_out" ] || mapfile -t nontrivial <<< "$cls_out"
# <<< classify
if [ ${#nontrivial[@]} -gt 0 ]; then
  ids="$(git log --format='%(trailers:key=Arch-Review,valueonly)' "$base..$head" | sed '/^\s*$/d' | xargs -n1 2>/dev/null | sort -u)"
  [ -n "$ids" ] || die "non-trivial landing (${nontrivial[*]:0:5}...) needs an 'Arch-Review: <id>' trailer with an APPROVE row in $AB/reviews.tsv (ask LWC Arch Board)"
  pid="$(git diff "$base" "$head" | git patch-id --stable | cut -d' ' -f1)"
  [ -f "$AB/reviews.tsv" ] || die "no $AB/reviews.tsv; non-trivial landings cannot be approved yet"
  ok=; for id in $ids; do
    row="$(awk -F'\t' -v id="$id" 'NR==1{for(i=1;i<=NF;i++){h=tolower($i); if(h=="id")c=i; if(h=="verdict")d=i; if(h=="patch_id")p=i}; next} c&&d&&p&&$c==id{print toupper($d) "\t" $p}' "$AB/reviews.tsv" | tail -1)"
    v="${row%%$'\t'*}"; rp="${row#*$'\t'}"
    echo "land: Arch-Review $id -> ${v:-<not found>} (reviewed patch-id ${rp:0:12}, landing ${pid:0:12})"
    [ "$v" = APPROVE ] && [ -n "$rp" ] && [ "$rp" = "$pid" ] && ok=1; done   # LAST row per id wins
  [ -n "$ok" ] || die "no APPROVE row in $AB/reviews.tsv whose patch_id matches this landing ($pid) for Arch-Review id(s): $ids. Diff changed after review (or rebase shifted it): get it re-reviewed. Compute with scripts/review-patch-id.sh"
else echo "land: trivial landing (all paths match $AB/trivial-paths.txt), no Arch-Review needed"; fi
cargo check --workspace --all-targets || die "cargo check failed"
pkgs=()
for c in $(echo "$changed" | sed -n 's#^crates/\([^/]*\)/.*#\1#p' | sort -u); do
  [ -f "crates/$c/Cargo.toml" ] && pkgs+=(-p "$(sed -n 's/^name *= *"\(.*\)"/\1/p' "crates/$c/Cargo.toml" | head -1)"); done
if [ ${#pkgs[@]} -gt 0 ]; then cargo test "${pkgs[@]}" || die "crate tests failed (${pkgs[*]})"; fi
if echo "$changed" | grep -qE '^scripts/(land|land-classify|test-land-classify)\.sh$'; then scripts/test-land-classify.sh || die "land-classify self-test failed"; fi
CRATES_ROOT="$wt/crates" node harness/scripts/shim-gate.mjs || die "shim gate failed"
if echo "$changed" | grep -qE '^harness/(goldens|test|src|fixtures)/'; then
  harness/scripts/ensure-deps.sh
  harness/scripts/golden-gate.sh "$wt/harness" || die "golden-gate: goldens != fresh JS-oracle recording"
fi
[ "$(git rev-parse main)" = "$base" ] || die "main moved during land (should be impossible under the lock)"
git -C "$MAIN" merge --ff-only -q "$head" || die "fast-forward of main failed"
echo "landed: main=${head:0:12} ($br)"
if echo "$changed" | grep -q '^crates/\|^Cargo\.'; then
  (cd "$MAIN" && scripts/install-napi.sh) || echo "!! landed, but live napi rebuild from main failed (main is still correct)" >&2
fi
