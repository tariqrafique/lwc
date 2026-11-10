#!/usr/bin/env bash
# D-0282 plants for the path→rows map and reuse hashes.
# Exit 0 iff every case passes, including a planted-bug negative for each rule.
set -euo pipefail
SELF="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
REPO="$(cd "$SELF/.." && pwd)"
SEL="$SELF/board-select.mjs"
MAP="$REPO/harness/board-select.tsv"
T="$(mktemp -d /tmp/test-board-select.XXXXXX)"
trap 'rm -rf "$T"' EXIT
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }

# --- map covers every workspace crate ---
if node "$SEL" check-map "$REPO" >"$T/check.out" 2>"$T/check.err"; then
  ok "map covers every workspace crate"
else
  bad "check-map failed"; sed 's/^/       /' "$T/check.out" "$T/check.err"
fi

# Plant: drop crates/repo-tracked/ from a copy of the map → check-map fails.
awk -F'\t' '!($1=="path" && $2=="crates/repo-tracked/")' "$MAP" > "$T/map.nocover.tsv"
if BOARD_SELECT_MAP="$T/map.nocover.tsv" node "$SEL" check-map "$REPO" >"$T/nocover.out" 2>"$T/nocover.err"; then
  bad "check-map accepted a map that omits repo-tracked"
else
  if grep -q 'repo-tracked' "$T/nocover.err"; then
    ok "planted missing crate coverage fails check-map"
  else
    bad "check-map failed but did not name repo-tracked"; sed 's/^/       /' "$T/nocover.err"
  fi
fi

# --- unmapped path forces FULL ---
out="$(node "$SEL" select "$REPO" "docs/does-not-exist-unmapped.md" 2>"$T/unmap.err" || true)"
if [ "$out" = FULL ]; then
  ok "unmapped path selects FULL"
else
  bad "unmapped path selected '$out' (want FULL)"
fi

# Plant: map that file to style → select is no longer FULL.
{
  cat "$MAP"
  printf 'path\tdocs/does-not-exist-unmapped.md\tstyle\n'
} > "$T/map.mapped-unmap.tsv"
out="$(BOARD_SELECT_MAP="$T/map.mapped-unmap.tsv" node "$SEL" select "$REPO" "docs/does-not-exist-unmapped.md" 2>/dev/null || true)"
if [ "$out" = style ]; then
  ok "planted mapping of that path no longer yields FULL (proves the unmapped rule)"
else
  bad "planted mapping selected '$out' (want style)"
fi

# --- SSR crate selects ssr rows, not FULL ---
out="$(node "$SEL" select "$REPO" "crates/lwc-ssr/src/lib.rs")"
if [ "$out" = FULL ]; then
  bad "lwc-ssr/src/lib.rs selected FULL"
elif grep -qx ssr <<< "$out" && grep -qx ssr_render_api <<< "$out" && ! grep -qx style <<< "$out"; then
  ok "lwc-ssr/src/lib.rs selects ssr rows, not style"
else
  bad "lwc-ssr selection unexpected:"; printf '%s\n' "$out" | sed 's/^/       /'
fi

# --- lwc-shared (and compiler, napi) are FULL ---
for p in crates/lwc-shared/src/lib.rs crates/lwc-compiler/src/lib.rs crates/lwc-compiler-napi/src/lib.rs Cargo.lock rust-toolchain.toml harness/board.sh; do
  out="$(node "$SEL" select "$REPO" "$p")"
  if [ "$out" = FULL ]; then
    ok "$p selects FULL"
  else
    bad "$p selected '$out' (want FULL)"
  fi
done

# --- transitive-only crate (lwc-template is in ssr's normal closure) changes ssr hash ---
tpl="$(git -C "$REPO" ls-files 'crates/lwc-template/*.rs' 'crates/lwc-template/src/*.rs')"
tpl="$(head -n 1 <<< "$tpl")"
[ -n "$tpl" ] || { bad "no lwc-template rust file to plant"; tpl="crates/lwc-template/src/lib.rs"; }
h1="$(node "$SEL" hash "$REPO" ssr --oracle-sha testoracle)"
h2="$(node "$SEL" hash "$REPO" ssr --oracle-sha testoracle --pretend-edit "$tpl")"
if [ -n "$h1" ] && [ "$h1" != "$h2" ]; then
  ok "ssr hash changes when a transitive crate file (lwc-template) is edited"
else
  bad "ssr hash did not change on pretend-edit $tpl (h1=$h1 h2=$h2)"
fi

# style should NOT change when lwc-ssr is edited (not in style's normal closure)
ssr_rs="$(git -C "$REPO" ls-files 'crates/lwc-ssr/src/*.rs')"
ssr_rs="$(head -n 1 <<< "$ssr_rs")"
hs1="$(node "$SEL" hash "$REPO" style --oracle-sha testoracle)"
hs2="$(node "$SEL" hash "$REPO" style --oracle-sha testoracle --pretend-edit "$ssr_rs")"
if [ -n "$hs1" ] && [ "$hs1" = "$hs2" ]; then
  ok "style hash unchanged when lwc-ssr (outside its closure) is edited"
else
  bad "style hash changed on ssr edit (hs1=$hs1 hs2=$hs2)"
fi

# Plant: a hasher that ignores workspace deps (closure = [root] only) → ssr hash no longer moves.
awk '
  /function closure\(byName, root\)/ { print; print "  return [root];"; skip=1; next }
  skip && /return \[\.\.\.seen\]\.sort\(\);/ { skip=0; next }
  skip { next }
  { print }
' "$SEL" > "$T/broken-select.mjs"
h1b="$(node "$T/broken-select.mjs" hash "$REPO" ssr --oracle-sha testoracle)"
h2b="$(node "$T/broken-select.mjs" hash "$REPO" ssr --oracle-sha testoracle --pretend-edit "$tpl")"
if [ "$h1b" = "$h2b" ]; then
  ok "planted no-transitive hasher no longer changes ssr hash on a template edit"
else
  bad "planted hasher still changed ssr hash (plant did not take); h1b=$h1b h2b=$h2b"
fi

# crates-for-row ssr includes lwc-template and lwc-shared
crates="$(node "$SEL" crates-for-row "$REPO" ssr)"
if grep -qx lwc-ssr <<< "$crates" && grep -qx lwc-template <<< "$crates" && grep -qx lwc-shared <<< "$crates"; then
  ok "ssr crate closure includes lwc-ssr, lwc-template, lwc-shared"
else
  bad "ssr closure unexpected:"; printf '%s\n' "$crates" | sed 's/^/       /'
fi

if [ "$fail" -eq 0 ]; then
  echo "test-board-select: ok"
  exit 0
fi
echo "test-board-select: FAILED" >&2
exit 1
