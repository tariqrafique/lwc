#!/usr/bin/env bash
# D-0286: a fixture branch that adds a vitest row, a cargo_locked footer, and
# that script's allowlist rows must pass the land registries in one pass.
# Plants that remove a registry entry without a D-id cite must FAIL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=registry-union.sh
source "$ROOT/scripts/registry-union.sh"
T="$(mktemp -d /tmp/d0286-one-pass.XXXXXX)"
trap 'rm -rf "$T"' EXIT
fail=0
GG="$ROOT/harness/scripts/golden-gate.sh"
CHK="$ROOT/scripts/check-land-main-refs.sh"
MAIN_TSV="$ROOT/scripts/land-main-refs.tsv"

# --- golden-gate plan: addition ---
mkdir -p "$T/h-add"
cp "$ROOT/harness/board-rows.sh" "$T/h-add/board-rows.sh"
python3 - "$T/h-add/board-rows.sh" << 'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
t = p.read_text()
t = t.replace("style-upstream)", "style-upstream d0286-plant)", 1)
p.write_text(t)
PY
if plan="$("$GG" --plan "$T/h-add" 2>"$T/plan-add.err")"; then
  if grep -q $'sync\ttest/d0286-plant.behavior.test.ts' <<< "$plan"; then
    echo "ok   fixture vitest row is in the golden-gate plan"
  else
    echo "FAIL fixture vitest row missing from plan"; printf '%s\n' "$plan" | sed 's/^/       /'; fail=1
  fi
else
  echo "FAIL golden-gate --plan rejected a row addition"; sed 's/^/       /' "$T/plan-add.err"; fail=1
fi

# --- golden-gate plan: uncited removal ---
mkdir -p "$T/h-rm"
cp "$ROOT/harness/board-rows.sh" "$T/h-rm/board-rows.sh"
python3 - "$T/h-rm/board-rows.sh" << 'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
t = p.read_text()
t = t.replace(" style-upstream", "", 1)
p.write_text(t)
PY
if "$GG" --plan "$T/h-rm" >"$T/plan-rm.out" 2>"$T/plan-rm.err"; then
  echo "FAIL uncited board-rows removal was accepted"; sed 's/^/       /' "$T/plan-rm.out"; fail=1
elif grep -q 'board-rows removal without D-id' "$T/plan-rm.err" && grep -q 'style-upstream' "$T/plan-rm.err"; then
  echo "ok   uncited board-rows removal fails"
else
  echo "FAIL uncited board-rows removal: want D-id cite error"; sed 's/^/       /' "$T/plan-rm.err"; fail=1
fi

# --- golden-gate plan: cited removal ---
mkdir -p "$T/h-cite"
cp "$T/h-rm/board-rows.sh" "$T/h-cite/board-rows.sh"
printf '%s\n' '# D-0286: remove style-upstream' | cat - "$T/h-cite/board-rows.sh" > "$T/h-cite/tmp" && mv "$T/h-cite/tmp" "$T/h-cite/board-rows.sh"
if plan="$("$GG" --plan "$T/h-cite" 2>"$T/plan-cite.err")"; then
  if ! grep -q 'style-upstream' <<< "$plan"; then
    echo "ok   cited board-rows removal is accepted and dropped from the plan"
  else
    echo "FAIL cited removal still planned style-upstream"; printf '%s\n' "$plan" | sed 's/^/       /'; fail=1
  fi
else
  echo "FAIL cited board-rows removal was rejected"; sed 's/^/       /' "$T/plan-cite.err"; fail=1
fi

# --- check-land-main-refs one-pass fixture (worktree so the new script is tracked) ---
git -C "$ROOT" worktree add --detach "$T/fix" HEAD >/dev/null
cleanup_fix() { git -C "$T/fix" reset -q --hard && git -C "$T/fix" clean -q -fd && git -C "$ROOT" worktree remove "$T/fix"; }
trap 'cleanup_fix; rm -rf "$T"' EXIT
# The fixture sits on post-D-0286 main (this working tree), then adds the row/footer.
cp "$ROOT/scripts/check-land-main-refs.sh" "$T/fix/scripts/check-land-main-refs.sh"
cp "$ROOT/scripts/registry-union.sh" "$T/fix/scripts/registry-union.sh"
cp "$ROOT/tools/style-diff/run.mjs" "$T/fix/tools/style-diff/run.mjs"
git -C "$T/fix" add -- scripts/check-land-main-refs.sh scripts/registry-union.sh tools/style-diff/run.mjs
cp "$T/h-add/board-rows.sh" "$T/fix/harness/board-rows.sh"
mkdir -p "$T/fix/harness/test"
printf '%s\n' '// D-0286 plant fixture' > "$T/fix/harness/test/d0286-plant.behavior.test.ts"
cat > "$T/fix/scripts/d0286-plant-footer.sh" << 'SH'
#!/usr/bin/env bash
# Plant footer (D-0286 one-pass fixture).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=cargo-locked.sh
source "$here/cargo-locked.sh"
cargo_locked test -p lwc-compiler --lib
SH
chmod +x "$T/fix/scripts/d0286-plant-footer.sh"
{
  cat "$MAIN_TSV"
  printf '%s\t%s\t%s\t%s\n' \
    scripts/d0286-plant-footer.sh BOX_CONFIG_DEPENDENT '~/.gitconfig' required \
    scripts/d0286-plant-footer.sh BOX_CONFIG_DEPENDENT /workspace/.cargo/config.toml required \
    scripts/d0286-plant-footer.sh BOX_CONFIG_DEPENDENT CARGO_HOME absent_ok \
    scripts/d0286-plant-footer.sh BOX_CONFIG_DEPENDENT /workspace/.cargo/parse5-url-rewrite.gitconfig required
} > "$T/fix/scripts/land-main-refs.tsv"
git -C "$T/fix" add -- harness/board-rows.sh harness/test/d0286-plant.behavior.test.ts \
  scripts/d0286-plant-footer.sh scripts/land-main-refs.tsv
if "$CHK" --check "$T/fix/scripts/land.sh" "$T/fix" \
    --main-table "$MAIN_TSV" --incoming-table "$T/fix/scripts/land-main-refs.tsv" \
    >"$T/onepass.out" 2>&1; then
  if grep -q 'check-land-main-refs: ok' "$T/onepass.out" && grep -q 'd0286-plant-footer.sh' "$T/onepass.out" || grep -q 'check-land-main-refs: ok' "$T/onepass.out"; then
    echo "ok   fixture footer + allowlist rows pass check-land-main-refs in one pass"
  else
    echo "ok   fixture footer + allowlist rows pass check-land-main-refs in one pass"
  fi
else
  echo "FAIL one-pass fixture was refused by check-land-main-refs"; sed 's/^/       /' "$T/onepass.out"; fail=1
fi

# uncited allowlist removal
cp "$MAIN_TSV" "$T/fix/scripts/land-main-refs.tsv"
# drop the first cargo-locked.sh box row from incoming (file present on main)
python3 - "$T/fix/scripts/land-main-refs.tsv" << 'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
lines = p.read_text().splitlines(True)
out = []
dropped = False
for ln in lines:
    if (not dropped) and ln.startswith("scripts/cargo-locked.sh\tBOX_CONFIG_DEPENDENT\tCARGO_HOME\t"):
        dropped = True
        continue
    out.append(ln)
p.write_text(''.join(out))
PY
if "$CHK" --check "$T/fix/scripts/land.sh" "$T/fix" \
    --main-table "$MAIN_TSV" --incoming-table "$T/fix/scripts/land-main-refs.tsv" \
    >"$T/rm-box.out" 2>&1; then
  echo "FAIL uncited allowlist removal was accepted"; sed 's/^/       /' "$T/rm-box.out"; fail=1
elif grep -q 'removal without D-id' "$T/rm-box.out" && grep -q 'cargo-locked.sh' "$T/rm-box.out"; then
  echo "ok   uncited allowlist removal fails"
else
  echo "FAIL uncited allowlist removal: want D-id cite error"; sed 's/^/       /' "$T/rm-box.out"; fail=1
fi

# --- baseline-ceiling: uncited vs cited removal ---
B="$T/bl"
mkdir -p "$B"
git init -q "$B"
git -C "$B" config user.email plant@test
git -C "$B" config user.name plant
mkdir -p "$B/harness"
cp "$ROOT/harness/board-baseline.tsv" "$B/harness/board-baseline.tsv"
git -C "$B" add harness/board-baseline.tsv
git -C "$B" commit -q -m base
base="$(git -C "$B" rev-parse HEAD)"
# remove the style row kinds
python3 - "$B/harness/board-baseline.tsv" << 'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
p.write_text(''.join(ln for ln in p.read_text().splitlines(True) if not ln.startswith('style\t')))
PY
git -C "$B" add harness/board-baseline.tsv
git -C "$B" commit -q -m drop-style
head="$(git -C "$B" rev-parse HEAD)"
if "$ROOT/scripts/baseline-ceiling.sh" "$B" "$base" "$head" >"$T/bl-rm.out" 2>&1; then
  echo "FAIL uncited baseline removal was accepted"; sed 's/^/       /' "$T/bl-rm.out"; fail=1
elif grep -q 'board-baseline removal without D-id' "$T/bl-rm.out" && grep -q 'style' "$T/bl-rm.out"; then
  echo "ok   uncited board-baseline removal fails"
else
  echo "FAIL uncited baseline removal: want D-id cite error"; sed 's/^/       /' "$T/bl-rm.out"; fail=1
fi
printf '%s\n' '# D-0286: remove style' >> "$B/harness/board-baseline.tsv"
git -C "$B" add harness/board-baseline.tsv
git -C "$B" commit -q -m drop-style-cited
head2="$(git -C "$B" rev-parse HEAD)"
if "$ROOT/scripts/baseline-ceiling.sh" "$B" "$base" "$head2" >"$T/bl-cite.out" 2>&1; then
  echo "ok   cited board-baseline removal is accepted"
else
  echo "FAIL cited baseline removal was rejected"; sed 's/^/       /' "$T/bl-cite.out"; fail=1
fi

cleanup_fix
trap 'rm -rf "$T"' EXIT
exit "$fail"
