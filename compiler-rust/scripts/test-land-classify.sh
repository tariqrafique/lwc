#!/usr/bin/env bash
# Self-test for scripts/land-classify.sh (and scripts/lint-shell-pipes.sh, ratchet-ceiling.sh, baseline-ceiling.sh --self-test, test-board-ratchet.sh) against Arch Board's policy shape. Exit 0 iff every case classifies right.
set -euo pipefail
SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
C="$(cd "$(dirname "$0")" && pwd)/land-classify.sh"; t="$(mktemp -d)/repo"; P="$(dirname "$t")"; mkdir -p "$t"; trap "rm -rf \"$P\"" EXIT
cd "$t"; git init -q -b main; git config user.email t@t; git config user.name t
mkdir -p crates/x/src crates/x/tests docs; seq 1 50 > crates/x/src/a.rs; seq 1 5 > crates/x/tests/old.rs; echo d > docs/d.md
printf '\x00\x01bin' > crates/x/tests/blob.rs.orig; git add -A; git commit -qm base; base=$(git rev-parse HEAD)
printf 'docs/*\n*.md\n+crates/*/tests/*.rs\n' > "$P/policy"
fail=0; check() { # name expect(trivial|nontrivial) [policy]
  local out; out="$("$C" "$base" HEAD "${3:-$P/policy}")"
  local got=trivial; [ -n "$out" ] && got=nontrivial
  if [ "$got" = "$2" ]; then echo "ok   $1 -> $got ${out:+($(echo $out))}"; else echo "FAIL $1 -> $got, want $2 ${out:+($(echo $out))}"; fail=1; fi
  git reset -q --hard "$base"; git clean -qfd; }
echo x >> docs/d.md; git commit -qam c; check "docs edit" trivial
seq 1 9 > crates/x/tests/new.rs; git add -A; git commit -qm c; check "+tests addition" trivial
echo 6 >> crates/x/tests/old.rs; sed -i 1d crates/x/tests/old.rs; git commit -qam c; check "+tests edit with deletion" nontrivial
git mv crates/x/src/a.rs crates/x/tests/a.rs; git commit -qm c; check "rename src->tests (+glob)" nontrivial
git mv crates/x/src/a.rs docs/foo.md; git commit -qm c; check "rename src->docs (plain glob)" nontrivial
printf '\x00\x02\x03' > crates/x/tests/b.rs; git add -A; git commit -qm c; check "binary under +glob" nontrivial
echo x >> docs/d.md; git commit -qam c; check "missing policy file = nothing trivial" nontrivial "$P/nope"
echo x >> crates/x/src/a.rs; git commit -qam c; check "src edit" nontrivial
chmod +x crates/x/tests/old.rs; git commit -qam c; check "mode-only change under +glob" nontrivial
# classifier error (bad rev) must exit non-zero ...
if "$C" deadbeef HEAD "$P/policy" >/dev/null 2>&1; then echo "FAIL classifier bad rev -> rc 0"; fail=1; else echo "ok   classifier bad rev -> rc!=0"; fi
# ... and land.sh's actual classify block (extracted verbatim) must refuse when the classifier fails or crashes.
L="$(cd "$(dirname "$C")" && pwd)/land.sh"; blk="$(sed -n '/^# >>> classify/,/^# <<< classify/p' "$L")"
[ -n "$blk" ] || { echo "FAIL classify block not found in land.sh"; fail=1; }
for mode in "exit 3" "echo crates/x/src/a.rs; exit 1" "kill -9 \$\$"; do
  mkdir -p "$P/fm/scripts"; printf '#!/usr/bin/env bash\n%s\n' "$mode" > "$P/fm/scripts/land-classify.sh"; chmod +x "$P/fm/scripts/land-classify.sh"
  if ( set -euo pipefail; die() { exit 97; }; MAIN="$P/fm"; AB="$P"; base=$base; head=HEAD; eval "$blk"; exit 0 ) >/dev/null 2>&1; then
    echo "FAIL land classify block with failing classifier ($mode) -> did not refuse"; fail=1
  else echo "ok   land refuses when classifier fails ($mode)"; fi
done
# (B) harness vitest config / setup / globalSetup edits are gate paths (need a pid-bound Arch-Review).
POLICY="${NOTES_TRIVIAL:-/workspace/notes/arch-board/trivial-paths.txt}"
if [ -f "$POLICY" ]; then
  mkdir -p harness/test
  echo '// vitest' > harness/vitest.config.ts; git add -A; git commit -qm c
  check "harness vitest.config.ts" nontrivial "$POLICY"
  mkdir -p harness/test
  echo '// board' > harness/vitest.board.config.ts; git add -A; git commit -qm c
  check "harness vitest.board.config.ts" nontrivial "$POLICY"
  mkdir -p harness/test
  echo '// setup' > harness/test/setup.ts; git add -A; git commit -qm c
  check "harness test/setup.ts" nontrivial "$POLICY"
  mkdir -p harness/test
  echo '// globalSetup' > harness/test/globalSetup.ts; git add -A; git commit -qm c
  check "harness test/globalSetup.ts" nontrivial "$POLICY"
  mkdir -p harness/test harness/src
  echo '// test' > harness/test/extra.test.ts; git add -A; git commit -qm c
  check "harness test/** extra.test.ts" nontrivial "$POLICY"
  mkdir -p harness/src
  echo '// src' > harness/src/extra.ts; git add -A; git commit -qm c
  check "harness src/** extra.ts" nontrivial "$POLICY"
else
  echo "FAIL notes trivial-paths.txt missing at $POLICY"; fail=1
fi
"$SELF_DIR/test-parse5-pin.sh" || fail=1   # AB-L0050 (3): parse5 pin gate negatives stay proven
"$SELF_DIR/test-shim-registry.sh" || fail=1  # AB-L0070: SHIM registry gating
# AB-X0008: the shell-pipe lint's own self-test (planted violation fails, clean file passes, grep rc>=2 fails).
if "$(cd "$(dirname "$C")" && pwd)/lint-shell-pipes.sh" --self-test; then echo "ok   lint-shell-pipes --self-test"; else echo "FAIL lint-shell-pipes --self-test"; fail=1; fi
# AB-0006 rework: board-baseline/v2 parser + comparator self-test, then the board.sh / land-gate end-to-end test.
if "$(cd "$(dirname "$C")" && pwd)/baseline-ceiling.sh" --self-test; then echo "ok   baseline-ceiling --self-test"; else echo "FAIL baseline-ceiling --self-test"; fail=1; fi
if "$(cd "$(dirname "$C")" && pwd)/test-board-ratchet.sh"; then echo "ok   test-board-ratchet"; else echo "FAIL test-board-ratchet"; fail=1; fi
if "$(cd "$(dirname "$C")" && pwd)/ratchet-ceiling.sh" --self-test; then echo "ok   ratchet-ceiling --self-test"; else echo "FAIL ratchet-ceiling --self-test"; fail=1; fi
MAIN="$(cd "$SELF_DIR/.." && pwd)" "$SELF_DIR/lint-no-stripped-compare.sh" --self-test || fail=1   # D-0219: the stripped-compare lint still catches every form
MAIN="$(cd "$SELF_DIR/.." && pwd)" "$SELF_DIR/lint-no-stripped-compare.sh" "$(cd "$SELF_DIR/.." && pwd)" || fail=1  # D-0219: no new whitespace-normalizing compare in board tooling
exit $fail
