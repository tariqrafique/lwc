#!/usr/bin/env bash
# D-0219: one equivalence authority. Board/harness comparators decide on exact bytes or ask semdiff (tools/semdiff,
# D-0196); they never strip, collapse or trim whitespace (or strip banner comments) before comparing code. This lint
# finds those normalizations in harness/**, tools/** (except tools/semdiff, the authority) and scripts/*.
# Allowlist (D-0256/D-0257): a closed exact path+line+rule list read only from $MAIN
# (scripts/lint-no-stripped-compare.allowlist, columns: path<TAB>line<TAB>rule<TAB>D-id). A branch cannot widen it.
# A hit not on the list fails; a list entry with no hit fails (delete it); every entry must cite a D-id (^D-[0-9]{4}$).
#
# Usage: scripts/lint-no-stripped-compare.sh [--self-test] [<repo-root>]   (works from any cwd)
# Exit: 0 pass; 1 lint failure; 2 usage/self-test failure.
set -euo pipefail

SELF_TEST=0
ROOT=""
for a in "$@"; do
  case "$a" in
    --self-test) SELF_TEST=1 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    -*) echo "unknown option: $a" >&2; exit 2 ;;
    *) ROOT="$a" ;;
  esac
done
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ -n "$ROOT" ] || ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$ROOT" && pwd)"
command -v rg >/dev/null || { echo "lint-no-stripped-compare: rg (ripgrep) is required" >&2; exit 2; }

# Allowlist is read only from $MAIN (explicit, or the repo root this script lives in). A missing file is rc 2.
# No git-show-main: fallback and no sibling fallback: a branch cannot widen the list, and an absent file is not silent.
load_allowlist() {
  local dest="$1"
  local main="${MAIN:-$(cd "$SCRIPT_DIR/.." && pwd)}"
  local src="$main/scripts/lint-no-stripped-compare.allowlist"
  if [ ! -f "$src" ]; then
    echo "lint-no-stripped-compare: no allowlist at $src (MAIN=${MAIN:-unset})" >&2
    exit 2
  fi
  cp "$src" "$dest"
}

# id <TAB> what <TAB> regex (rg / Rust regex syntax)
WS='(\\s|\[(\\[stnrfv]| |\[:space:\]|\\u00a0|\\u3000)+\])'
RULES=$(cat <<R
N1	JS whitespace strip/collapse	\.replace(All)?\(\s*/${WS}[+*]?/[a-z]*\s*,\s*(''|""|' '|" "|\`\`|\`\s\`)
N2	JS whitespace split+join	\.split\(\s*/${WS}[+*]?/[a-z]*\s*\)\s*\.(filter\([^)]*\)\s*\.)?join\(
N3	trim() before ==	\.trim(Start|End)?\(\)\s*[!=]==?\s*[^'"\`\s=]|[!=]==?\s*[\w\$.\[\]()]+\.trim(Start|End)?\(\)
N4	diff ignoring whitespace	\bdiff\b[^|;&\n]*\s(-[a-zA-Z]*[wbBZ]\b|--ignore-(all-space|space-change|blank-lines|trailing-space|space-at-eol))
N5	tr -d whitespace	\btr\s+(-[a-zA-Z]*d[a-zA-Z]*|--delete)\s+['"]?(\[:space:\]|\[:blank:\]|[ \\\\tnr]+)['"]?
N6	sed global whitespace delete	s/(\[\[:space:\]\]|\[\[:blank:\]\]|\\\\s|\[ \\\\t\])(\*|\+|\\\\\+)?//g
N7	ignore-whitespace option	\b(ignoreWhitespace|ignore_whitespace|ignoreAllSpace)\b
N8	banner/comment strip	\.replace(All)?\(\s*/\\\\/\\\\\*
N9	Rust whitespace normalize	\.split_whitespace\(\)|\.replace\(\s*char::is_whitespace|is_whitespace\(\)\)\s*\.collect
N10	literal space replace	\.replace(All)?\(\s*(['"] [ '"]|/ /[gimsuy]*)\s*,\s*(''|""|\`\`)
N11	sed literal space delete	s/ //g
N12	Python re.sub literal space	re\.sub\(\s*r?['"] [ '"]\s*,\s*['"]['"]
N13	decoded token value compare	JSON\.stringify\(\s*[A-Za-z_]\w*\.value\s*\)
N14	JS literal-space split+join	\.split\(\s*(' '|" ")\s*\)\s*\.join\(\s*(''|"")\s*\)
N15	JS replace runs of spaces	\.replace(All)?\(\s*/ \+/g[a-z]*\s*,\s*(''|"")
N16	Python join of split()	(''|"")\.join\(\s*[^)\n]+\.split\(\s*\)\s*\)
N17	awk gsub delete spaces	\bgsub\(\s*/ /\s*,\s*(''|"")
R
)

# scan <dir> -> lines "rule<TAB>what<TAB>file<TAB>line<TAB>text"
# File list comes from tracked-files.sh (git ls-files). Fetched inputs are not listed.
scan() {
  local dir="$1"
  local helper listed=() f
  helper="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/tracked-files.sh"
  [ -x "$helper" ] || { echo "lint-no-stripped-compare: missing $helper" >&2; exit 2; }
  while IFS= read -r -d '' f; do
    case "$f" in
      tools/semdiff/*|harness/fixtures/*|harness/goldens/*|scripts/lint-no-stripped-compare.sh|harness/./*) continue ;;
    esac
    case "$f" in
      *.mjs|*.cjs|*.js|*.ts|*.sh|*.rs|*.py) listed+=("$f") ;;
    esac
  done < <("$helper" "$dir" harness tools scripts)
  [ ${#listed[@]} -gt 0 ] || return 0
  while IFS=$'\t' read -r id what re; do
    [ -n "$id" ] || continue
    local out rc=0
    out="$(cd "$dir" && rg -n --no-heading --color never --no-config \
       -e "$re" -- "${listed[@]}")" || rc=$?
    # rg: 0 hits, 1 none, >=2 error (bad regex, unreadable file): never read an error as "clean"
    [ "$rc" -le 1 ] || { echo "lint-no-stripped-compare: rg failed (rc=$rc) on rule $id" >&2; exit 2; }
    [ -n "$out" ] || continue
    printf '%s\n' "$out" |
      while IFS= read -r hit; do
        local file="${hit%%:*}"; file="${file#./}"
        local rest="${hit#*:}"; local line="${rest%%:*}"; local text="${rest#*:}"
        text="$(printf '%s' "$text" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
        printf '%s\t%s\t%s\t%s\t%s\n' "$id" "$what" "$file" "$line" "$text"
      done
  done <<<"$RULES"
}

if [ "$SELF_TEST" = 1 ]; then
  T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
  mkdir -p "$T/harness/scripts" "$T/tools/x/src" "$T/tools/semdiff/src" "$T/scripts" "$T/harness/fixtures/a"
  # One positive per rule form (tagged "HIT <rule>") and near-misses that must not match.
  cat >"$T/harness/scripts/cmp.mjs" <<'F'
const a = s.replace(/[ \t\n\r\f\v]+/g, ''); // HIT N1
const b = s.replace(/\s+/g, ' '); // HIT N1
const c = s.replaceAll(/\s/g, ""); // HIT N1
const d = s.split(/\s+/).join(''); // HIT N2
if (x.trim() === y) {} // HIT N3
if (x === y.trim()) {} // HIT N3
const o = { ignoreWhitespace: true }; // HIT N7
const e = s.replace(/\/\*LWC compiler v[\s\S]*?\*\//g, ''); // HIT N8
const k = s.split(/\s+/)[0];
const m = s.replace(/\n/g, ' ');
const n = s.replace(/^import\s+(\w+)\s+from/gm, 'x');
const t = s.trim();
const q = a === b;
if (s.trim() !== '') {}
const lit1 = s.replaceAll(' ', ''); // HIT N10
const lit2 = s.replace(/ /g, ''); // HIT N10
const tok = x.map((t) => `${t.type.label}:${JSON.stringify(t.value)}`).join(' '); // HIT N13
const sj = s.split(' ').join(''); // HIT N14
const rj = s.replace(/ +/g,''); // HIT N15
const sj_ok = s.split(',').join('');
const rj_ok = s.replace(/ +/g, ' ');
const sj_idx = s.split(' ')[0];
F
  cat >"$T/scripts/cmp.sh" <<'F'
diff -w a b # HIT N4
git diff --ignore-all-space a b # HIT N4
tr -d '[:space:]' < a # HIT N5
sed 's/[[:space:]]//g' a # HIT N6
sed -e 's/\s\+//g' a # HIT N6
sed 's/^[[:space:]]*//;s/[[:space:]]*$//' a
sed 's/ //g' a # HIT N11
tr -d ' ' < a # HIT N5
diff -u a b
cmp -s a b
tr -d '\r' < a # HIT N5
tr -d 'x' < a
awk '{gsub(/ /,"")}1' a # HIT N17
awk '{gsub(/,/,"")}1' a
awk '{gsub(/ /, "-")}1' a
F
  cat >"$T/tools/x/src/main.rs" <<'F'
let w: Vec<_> = s.split_whitespace().collect(); // HIT N9
if a.trim() == b {} // HIT N3
let t = s.trim();
F
  cat >"$T/harness/scripts/cmp.py" <<'F'
import re
x = s.replace(' ', '')  # HIT N10
y = re.sub(r' ', '', s)  # HIT N12
z = re.sub(r'\s+', '', s)
w = "".join(s.split())  # HIT N16
w_ok = "".join(s.split(','))
w_sp = " ".join(s.split())
F
  printf '%s\n' 'const z = s.replace(/\s+/g, "");' >"$T/tools/semdiff/src/ok.mjs"
  printf '%s\n' 'const z = s.replace(/\s+/g, "");' >"$T/harness/fixtures/a/fx.js"
  git -C "$T" init -q
  git -C "$T" add harness tools scripts
  got="$(scan "$T" | awk -F'\t' '{print $3":"$4" "$1}' | sort -u)"
  want="$(cd "$T" && grep -rn '# HIT \|// HIT ' harness scripts tools | sed -E 's/^([^:]+):([0-9]+):.*HIT (\w+)$/\1:\2 \3/' | sort -u)"
  n="$(printf '%s\n' "$want" | grep -c .)"
  if [ "$got" != "$want" ] || [ "$n" -ne 27 ]; then
    echo "SELF-TEST FAILED ($n planted)"; echo "--- want"; echo "$want"; echo "--- got"; echo "$got"; exit 2
  fi
  for r in N1 N2 N3 N4 N5 N6 N7 N8 N9 N10 N11 N12 N13 N14 N15 N16 N17; do grep -q " $r\$" <<<"$want" || { echo "SELF-TEST FAILED: rule $r has no planted hit"; exit 2; }; done
  # Allowlist: full list passes; a missing entry (new hit) or a stale entry fails; MAIN's copy is the one read.
  A="$T/scripts/lint-no-stripped-compare.allowlist"
  scan "$T" | awk -F'\t' '{print $3"\t"$4"\t"$1"\tD-0256"}' | sort -u >"$A"
  MAIN="$T" "$0" "$T" >/dev/null || { echo "SELF-TEST FAILED: full allowlist rejected"; exit 2; }
  sed -i 1d "$A"
  if MAIN="$T" "$0" "$T" >/dev/null; then echo "SELF-TEST FAILED: unlisted hit accepted"; exit 2; fi
  scan "$T" | awk -F'\t' '{print $3"\t"$4"\t"$1"\tD-0256"}' | sort -u >"$A"
  printf 'harness/scripts/gone.mjs\t1\tN1\tD-0256\n' >>"$A"
  if MAIN="$T" "$0" "$T" >/dev/null; then echo "SELF-TEST FAILED: stale allowlist entry accepted"; exit 2; fi
  scan "$T" | awk -F'\t' '{print $3"\t"$4"\t"$1"\tD-xxxx"}' | sort -u >"$A"
  if MAIN="$T" "$0" "$T" >/dev/null; then echo "SELF-TEST FAILED: bad D-id accepted"; exit 2; fi
  # $MAIN wins over the tree-under-test's copy.
  M="$(mktemp -d)"; mkdir -p "$M/scripts"
  scan "$T" | awk -F'\t' '{print $3"\t"$4"\t"$1"\tD-0256"}' | sort -u >"$T/scripts/lint-no-stripped-compare.allowlist"
  : >"$M/scripts/lint-no-stripped-compare.allowlist"
  if MAIN="$M" "$0" "$T" >/dev/null; then echo "SELF-TEST FAILED: empty MAIN allowlist accepted tree hits"; exit 2; fi
  rm -rf "$M"
  # Missing $MAIN allowlist is rc 2: no git-show main: fallback and no sibling fallback.
  M="$(mktemp -d)"; mkdir -p "$M/scripts"
  miss_rc=0; MAIN="$M" "$0" "$T" >/dev/null 2>"$T/missing.err" || miss_rc=$?
  if [ "$miss_rc" -ne 2 ]; then echo "SELF-TEST FAILED: missing allowlist rc=$miss_rc (want 2)"; sed 's/^/       /' "$T/missing.err"; exit 2; fi
  rm -rf "$M"
  echo "lint-no-stripped-compare self-test: ok ($n planted hits over 17 rules detected, near-misses and semdiff/fixtures clean, allowlist + MAIN source enforced)"
  exit 0
fi

ALLOW="$(mktemp)"
trap 'rm -f "$ALLOW"' EXIT
load_allowlist "$ALLOW"
HITS="$(scan "$ROOT")"
# key = path<TAB>line<TAB>rule
KEYS="$(printf '%s\n' "$HITS" | awk -F'\t' 'NF{print $3"\t"$4"\t"$1}' | sort -u)"
fail=0
declare -A AL=()
while IFS=$'\t' read -r file line rule did rest; do
  [[ "$file" =~ ^# ]] && continue
  [ -n "$file" ] || continue
  if [ -n "$rest" ] || [ -z "$did" ]; then
    echo "allowlist malformed (want path<TAB>line<TAB>rule<TAB>D-id): $file $line $rule $did $rest"; fail=1; continue
  fi
  if [[ ! "$line" =~ ^[0-9]+$ ]]; then echo "allowlist $file: line must be a number, got $line"; fail=1; continue; fi
  if [[ ! "$did" =~ ^D-[0-9]{4}$ ]]; then echo "allowlist $file:$line ($rule) missing/invalid D-id $did"; fail=1; continue; fi
  key="$file"$'\t'"$line"$'\t'"$rule"
  AL["$key"]="$did"
done < <(grep -v '^#' "$ALLOW" | grep -v '^[[:space:]]*$' || true)
while IFS=$'\t' read -r id what file line text; do
  [ -n "$id" ] || continue
  key="$file"$'\t'"$line"$'\t'"$id"
  if [ -z "${AL[$key]+x}" ]; then
    echo "$file:$line: $what ($id) before a compare is banned (D-0219): compare exact bytes or ask semdiff"; fail=1
  fi
done <<<"$HITS"
for key in "${!AL[@]}"; do
  grep -qxF "$key" <<<"$KEYS" || { echo "stale allowlist entry (hit removed; delete it from scripts/lint-no-stripped-compare.allowlist): $key (cite ${AL[$key]})"; fail=1; }
done
nhits=$(printf '%s' "$KEYS" | grep -c . || true)
nall=${#AL[@]}
if [ "$fail" = 0 ]; then
  echo "lint-no-stripped-compare: ok ($nhits known sites, allowlist $nall)"
fi
exit "$fail"
