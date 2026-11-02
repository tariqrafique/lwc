#!/usr/bin/env bash
# The ONLY way onto main. Run from your worktree (/workspace/wt/<bot>, branch bot/<bot>) with everything committed.
# Under one global flock: rebase onto main -> cargo check --workspace --all-targets -> tests of changed crates
# -> shim gate -> golden-gate if harness tests/goldens/src/fixtures changed -> fast-forward main -> rebuild +
# install the live napi from main (only when crates changed). Any failure: nothing lands, main untouched.
# Conflicts are yours: resolve with `git rebase main` in your worktree and re-run.
set -euo pipefail
MAIN=/workspace/lwc-compiler-rust
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
changed="$(git diff --name-only "$base" "$head")"
echo "land: $br $(git rev-list --count "$base..$head") commit(s) onto main@${base:0:12}"
cargo check --workspace --all-targets || die "cargo check failed"
pkgs=()
for c in $(echo "$changed" | sed -n 's#^crates/\([^/]*\)/.*#\1#p' | sort -u); do
  [ -f "crates/$c/Cargo.toml" ] && pkgs+=(-p "$(sed -n 's/^name *= *"\(.*\)"/\1/p' "crates/$c/Cargo.toml" | head -1)"); done
if [ ${#pkgs[@]} -gt 0 ]; then cargo test "${pkgs[@]}" || die "crate tests failed (${pkgs[*]})"; fi
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
