#!/usr/bin/env bash
# Golden-I/O guard (AB-0005/AB-0007): harness/src/optionRows.ts (readGolden/writeGolden) owns ALL golden writes and the
# only read of UPDATE_GOLDENS. Fails (exit 1, matches printed) if any other file under <harness>/test or <harness>/src:
#   (a) calls an fs write/move/delete API whose first argument mentions golden(s) (e.g. writeFileSync('goldens/x.json'),
#       mkdirSync(GOLDEN_DIR), rmSync(goldenFile)), or
#   (b) references UPDATE_GOLDENS other than the prose form "UPDATE_GOLDENS=1" after a space or '(' (doc comments,
#       error messages). process.env.UPDATE_GOLDENS, env['UPDATE_GOLDENS'], { UPDATE_GOLDENS } = ... all match.
# Lexical guard: a golden path laundered through a variable not named *golden* is not caught (review covers that).
# Exit: 0 pass (rg rc 1, no match), 1 violation (rg rc 0), 2 guard failure (rg rc >= 2, e.g. missing dir).
# Usage: scripts/golden-io-guard.sh [harness-dir]   (default: this harness)
set -euo pipefail
h="$(cd "${1:-$(dirname "$0")/..}" && pwd)"
for d in test src; do [ -d "$h/$d" ] || { echo "!! GOLDEN-IO GUARD ERROR: missing $h/$d" >&2; exit 2; }; done
[ -f "$h/src/optionRows.ts" ] || { echo "!! GOLDEN-IO GUARD ERROR: missing $h/src/optionRows.ts (the sole golden writer)" >&2; exit 2; }
api='\b(writeFile|writeFileSync|appendFile|appendFileSync|mkdir|mkdirSync|mkdtemp|mkdtempSync|rm|rmSync|rmdir|rmdirSync|unlink|unlinkSync|rename|renameSync|copyFile|copyFileSync|cp|cpSync|truncate|truncateSync|symlink|symlinkSync|link|linkSync|createWriteStream|outputFile|outputFileSync|outputJson|outputJsonSync|writeJson|writeJsonSync)\s*\(\s*[^,)]*golden'
env='(?<![\s(])UPDATE_GOLDENS|UPDATE_GOLDENS(?!=1\b)'
repo="$(cd "$h/.." && pwd)"
helper="$(cd "$(dirname "$0")/../../scripts" && pwd)/tracked-files.sh"
[ -x "$helper" ] || { echo "!! GOLDEN-IO GUARD ERROR: missing $helper" >&2; exit 2; }
gfiles=()
while IFS= read -r -d '' f; do
  [ -n "$f" ] || continue
  [ "$f" = "harness/src/optionRows.ts" ] && continue
  gfiles+=("$f")
done < <("$helper" "$repo" harness/test harness/src)
[ ${#gfiles[@]} -gt 0 ] || { echo "!! GOLDEN-IO GUARD ERROR: no tracked files under harness/test or harness/src" >&2; exit 2; }
rc=0
out="$(cd "$repo" && rg -n -i --pcre2 --no-config -e "$api" -- "${gfiles[@]}")" || rc=$?
rc2=0
outenv="$(cd "$repo" && rg -n --pcre2 --no-config -e "$env" -- "${gfiles[@]}")" || rc2=$?
[ "$rc" -le 1 ] && [ "$rc2" -le 1 ] || { echo "!! GOLDEN-IO GUARD ERROR: rg failed (rc=$rc/$rc2)" >&2; exit 2; }
if [ "$rc" -eq 1 ] && [ "$rc2" -eq 1 ]; then echo "golden-io-guard: ok (golden writes + UPDATE_GOLDENS only in src/optionRows.ts)"; exit 0; fi
echo "!! GOLDEN-IO GUARD FAILED: only src/optionRows.ts (readGolden/writeGolden, UPDATE) may write goldens or read UPDATE_GOLDENS:"
if [ -n "$out" ]; then sed 's/^/  golden write: /' <<< "$out"; fi
if [ -n "$outenv" ]; then sed 's/^/  UPDATE_GOLDENS ref: /' <<< "$outenv"; fi
exit 1
