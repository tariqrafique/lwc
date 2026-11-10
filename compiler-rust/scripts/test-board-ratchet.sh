#!/usr/bin/env bash
# End-to-end test of the AB-0006 board ratchet (AB-0006 rework 2 + D-0255): the REAL harness/board.sh on one cheap
# row (js-eval, 2 cases) in a throwaway copy of this harness, plus scripts/baseline-ceiling.sh (git blobs) and
# land.sh's baseline-ceiling block (extracted verbatim) in a throwaway git repo. Nothing outside mktemp dirs is written.
#   board.sh: equal / injected / improvement / partial update / false-pass-as-equal
#   F1-F6: every proven false pass of the previous review is refused
#   Case-Set (D-0255): a removed or newly skipped id needs a cited D-id whose committed decision lists case-set:<row>
# Needs the oracle (/workspace/lwc, deps built) and a napi binary
# (harness/lwc-compiler-napi.node or target/release/liblwc_compiler_napi.so).
# land.sh runs this after the napi release build. test-land-classify does not.
# Exit 0 iff every case behaves. Missing napi is rc 2 (prerequisite).
set -euo pipefail
R="$(cd "$(dirname "$0")/.." && pwd)"
# (5) ensure MAIN's harness deps first so --root mutations actually run tests.
"$R/harness/scripts/ensure-deps.sh" "$R/harness" || { echo "!! test-board-ratchet: ensure-deps failed for MAIN harness" >&2; exit 2; }
T="$(mktemp -d /tmp/test-board-ratchet.XXXXXX)"; trap 'rm -rf "$T"' EXIT
fail=0; ok() { echo "ok   $*"; }; bad() { echo "FAIL $*"; fail=1; }
assert_observed() {
  local log="$1" name="$2"
  if grep -qE 'no observation|produced no test summary' "$log"; then
    bad "$name: mutation must not pass on 'no observation'"; return 1
  fi
  return 0
}
tar -C "$R" --exclude=harness/node_modules --exclude=harness/.pins --exclude=harness/.vitest --exclude=harness/.csr-render-out \
  --exclude=harness/.ssr-render-out -cf - harness | tar -C "$T" -xf -
H="$T/harness"
# board.sh reads SHIM_OWNERS from $here/../OWNERS (the tree board.sh itself lives in). The harness
# copy's parent is $T, so plant the repo OWNERS there (same as --root using MAIN's board.sh).
cp "$R/OWNERS" "$T/OWNERS"
mkdir -p "$T/scripts"
cp "$R/scripts/cargo-locked.sh" "$T/scripts/cargo-locked.sh"
if [ ! -f "$H/lwc-compiler-napi.node" ]; then
  if [ -f "$R/target/release/liblwc_compiler_napi.so" ]; then cp "$R/target/release/liblwc_compiler_napi.so" "$H/lwc-compiler-napi.node"
  else echo "!! test-board-ratchet: no napi binary ($R/harness/lwc-compiler-napi.node or $R/target/release)" >&2; exit 2; fi
fi
TEST="$H/test/js-eval.behavior.test.ts"; cp "$TEST" "$T/js-eval.orig"; BL0="$H/board-baseline.tsv"; cp "$BL0" "$T/baseline.orig"
INJ='board-ratchet injected failure'
inject() { cp "$T/js-eval.orig" "$TEST"; printf '\nimport { test as __boardRatchetTest } from "vitest";\n__boardRatchetTest(%s, () => { throw new Error("injected"); });\n' "'$INJ'" >> "$TEST"; }
restore() { cp "$T/js-eval.orig" "$TEST"; cp "$T/baseline.orig" "$BL0"; }
# rewrite the js-eval block: $1=file then triples kind<TAB>id..., last arg may be counts "P F S"
# usage: setrow FILE counts "P F S" [passed ID ...] --failed ID ... --skipped ID ...
setrow() {
  local f="$1"; shift
  node -e '
    const fs=require("fs"), a=process.argv.slice(1), f=a.shift();
    let counts="", kind="failed", P=[], F=[], S=[];
    for (const x of a) {
      if (x === "--passed") { kind="passed"; continue; }
      if (x === "--failed") { kind="failed"; continue; }
      if (x === "--skipped") { kind="skipped"; continue; }
      if (x.startsWith("counts=")) { counts=x.slice(7); continue; }
      ({passed:P,failed:F,skipped:S}[kind]).push(x);
    }
    if (!counts) counts = `${P.length} ${F.length} ${S.length}`;
    const L=fs.readFileSync(f,"utf8").split("\n");
    const out=[]; let done=false;
    for (const l of L) {
      if (l.startsWith("js-eval\t")) {
        if (!done) {
          out.push(`js-eval\tcounts\t${counts}`);
          for (const i of P) out.push(`js-eval\tpassed\t${i}`);
          for (const i of F) out.push(`js-eval\tfailed\t${i}`);
          for (const i of S) out.push(`js-eval\tskipped\t${i}`);
          done=true;
        }
        continue;
      }
      out.push(l);
    }
    if (!done) process.exit(3); fs.writeFileSync(f, out.join("\n"));
  ' "$f" "$@";
}
# two real js-eval case ids (the committed baseline after reseed lists them)
JP1='js-eval behavioral private-field-proxy#1'
JP2='js-eval behavioral private-field-proxy#2'
# Discover them from the committed file if present, else keep the conventional names.
if grep -q $'^js-eval\tpassed\t' "$BL0"; then
  mapfile -t _jp < <(awk -F'\t' '$1=="js-eval" && $2=="passed"{print $3}' "$BL0")
  JP1="${_jp[0]}"; JP2="${_jp[1]}"
fi
grep -q $'^js-eval\tcounts\t2 0 0$' "$BL0" || { echo "!! test-board-ratchet: committed baseline js-eval row is not '2 0 0'" >&2; exit 2; }
board() { (cd "$H" && env -u BOARD_RATCHET_UPDATE CRATES_ROOT="$R/crates" BOARD_REPO_ROOT="$R" BOARD_LOG_DIR="$T/log.$1" "${@:2}" ./board.sh js-eval) > "$T/out.$1" 2>&1; }
want_board() {
  local rc=0; board "$1" || rc=$?
  if [ "$rc" -eq "$2" ] && grep -qE -- "$3" "$T/out.$1"; then ok "board.sh $1 -> rc $rc"
  else bad "board.sh $1 -> rc $rc (want $2, /$3/)"; sed 's/^/       /' "$T/out.$1" | tail -15; fi; }
# (1) equal
restore; want_board equal 0 '^js-eval +Tests +2 passed'
# (2) injected failure
inject; want_board injected 1 "RATCHET REGRESSION: js-eval: .*newly failing 1: \"$INJ\""
# (3) improvement without reseed: baseline claims a real case fails; it passes
restore; setrow "$BL0" counts='1 1 0' --passed "$JP1" --failed "$JP2"; want_board improved 1 'RATCHET IMPROVED .*js-eval: .*no longer failing 1'
# (4) update refused on a partial run
restore; rc=0; (cd "$H" && BOARD_RATCHET_UPDATE=1 CRATES_ROOT="$R/crates" ./board.sh js-eval) > "$T/out.upd" 2>&1 || rc=$?
if [ $rc -eq 2 ] && grep -q 'needs a full run' "$T/out.upd"; then ok "BOARD_RATCHET_UPDATE=1 partial -> rc 2"; else bad "BOARD_RATCHET_UPDATE=1 partial -> rc $rc"; fi
rc=0; (cd "$H" && BOARD_RATCHET_UPDATE=1 CRATES_ROOT="$R/crates" ./board.sh check js-eval) > "$T/out.chkarg" 2>&1 || rc=$?
if [ $rc -eq 2 ]; then ok "board.sh check with args -> rc 2"; else bad "board.sh check with args -> rc $rc"; fi
# (5) Arch's original false pass: injected failure + baseline edited to carry it (equal to the run)
inject; setrow "$BL0" counts='2 1 0' --passed "$JP1" --passed "$JP2" --failed "$INJ"; want_board falsepass 0 '^js-eval +Tests +1 failed \| 2 passed'
cp "$BL0" "$T/worse.tsv"; restore
# --- git-backed ceiling fixture (the production API) ---
M="$T/main"; mkdir -p "$M/scripts" "$M/harness/scripts"
cp "$R/scripts/baseline-ceiling.sh" "$M/scripts/"; cp "$R/harness/scripts/board-baseline.mjs" "$M/harness/scripts/"; cp "$R/harness/board-rows.sh" "$M/harness/"
G="$T/repo"; mkdir -p "$G/harness"; cd "$G"; git init -q -b main; git config user.email t@t; git config user.name t
cp "$T/baseline.orig" harness/board-baseline.tsv; git add -A; git commit -qm base; gbase="$(git rev-parse HEAD)"
# also a notes repo for Case-Set lookups (committed decisions.tsv; land.sh uses DEC_REPO HEAD, never a working tree)
N="$T/notes"; mkdir -p "$N/lwc-rust-migrate"; cd "$N"; git init -q -b main
printf 'id\tts\tphase\tdecision\twhy\tevidence\tresult\nD-0001\t2026-11-01\tharness\tunrelated decision mentions D-5555\twhy\tev\tordered\n' > lwc-rust-migrate/decisions.tsv
git -c user.name=harness -c user.email=harness@lwc-bots add -A
git -c user.name=harness -c user.email=harness@lwc-bots commit -qm 'header + D-0001'
# D-9999 minted by migrate, lists a SUPERSET of the id drop-one.tsv removes (JP2).
printf 'D-9999\t2026-11-09\tharness\tcase-set:js-eval: %s|%s\twhy\tev\tordered\n' "$JP1" "$JP2" >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-9999 migrate case-set'
# D-8888 minted by migrate but lists an id that is NOT the one being removed
printf 'D-8888\t2026-11-09\tharness\tcase-set:js-eval: not-this-case-id\twhy\tev\tordered\n' >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-8888 migrate, wrong id'
# D-7777 lists the right id but was added by a non-migrate author
printf 'D-7777\t2026-11-09\tharness\tcase-set:js-eval: %s\twhy\tev\tordered\n' "$JP2" >> lwc-rust-migrate/decisions.tsv
git -c user.name=harness -c user.email=harness@lwc-bots add -A
git -c user.name=harness -c user.email=harness@lwc-bots commit -qm 'D-7777 harness case-set'
# D-6666 added by migrate with the right id, then rewritten by harness (blame != migrate)
printf 'D-6666\t2026-11-09\tharness\tcase-set:js-eval: %s\twhy\tev\tordered\n' "$JP2" >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-6666 migrate add'
# rewrite the D-6666 line in place (keep the id, change why) so blame is harness
awk -F'\t' -v OFS='\t' '$1=="D-6666"{$5="rewritten by harness"} {print}' lwc-rust-migrate/decisions.tsv > "$T/decisions.rewritten"
mv "$T/decisions.rewritten" lwc-rust-migrate/decisions.tsv
git -c user.name=harness -c user.email=harness@lwc-bots add -A
git -c user.name=harness -c user.email=harness@lwc-bots commit -qm 'D-6666 rewritten by harness'
# D-4444: migrate-authored but the grant is 'foocase-set:js-eval:' (not a field-anchored case-set: token) — rc 2.
printf 'D-4444\t2026-11-09\tharness\tfoocase-set:js-eval: %s\twhy\tev\tordered\n' "$JP2" >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-4444 migrate foocase-set'
# D-5555: real migrate mint AFTER an earlier non-migrate commit mentioned D-5555\t in another row (D-0001).
printf 'D-5555\t2026-11-09\tharness\tcase-set:js-eval: %s\twhy\tev\tordered\n' "$JP2" >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-5555 migrate case-set'
# Tokenizer negatives (rc 2): trailing '|', space-only list, empty list, duplicate ids.
printf 'D-3434\t2026-11-09\tharness\tcase-set:js-eval: %s|\twhy\tev\tordered\n' "$JP2" >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-3434 trailing pipe'
printf 'D-2323\t2026-11-09\tharness\tcase-set:js-eval:    \twhy\tev\tordered\n' >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-2323 space-only list'
printf 'D-1212\t2026-11-09\tharness\tcase-set:js-eval:\twhy\tev\tordered\n' >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-1212 empty list'
printf 'D-4545\t2026-11-09\tharness\tcase-set:js-eval: %s|%s\twhy\tev\tordered\n' "$JP2" "$JP2" >> lwc-rust-migrate/decisions.tsv
git -c user.name=migrate -c user.email=migrate@lwc-bots add -A
git -c user.name=migrate -c user.email=migrate@lwc-bots commit -qm 'D-4545 duplicate ids'
# helper: commit a file as the branch baseline (or delete / symlink) and run MAIN's ceiling against gbase
ceil() { # name want-rc regex [extra-args...]
  local name="$1" want="$2" re="$3"; shift 3
  local rc=0; "$M/scripts/baseline-ceiling.sh" "$G" "$gbase" HEAD "$@" > "$T/out.ceil.$name" 2>&1 || rc=$?
  if [ "$rc" -eq "$want" ] && grep -qE -- "$re" "$T/out.ceil.$name"; then ok "ceiling $name -> rc $rc"
  else bad "ceiling $name -> rc $rc (want $want, /$re/)"; sed 's/^/       /' "$T/out.ceil.$name"; fi
}
commit_bl() { git -C "$G" reset -q --hard "$gbase"; cp "$1" "$G/harness/board-baseline.tsv"; git -C "$G" add -A
  if git -C "$G" diff --cached --quiet && git -C "$G" diff --quiet; then :; else git -C "$G" commit -qm "$2"; fi; }
# equal committed files
commit_bl "$T/baseline.orig" equal; ceil equal 0 'main row\(s\) checked, ok'
# F1: branch baseline is a symlink to main's absolute path (ls-tree mode 120000) -> rc 2, never a raise
git -C "$G" reset -q --hard "$gbase"
rm -f "$G/harness/board-baseline.tsv"
ln -s "$G/harness/board-baseline.tsv.MISSING" "$G/harness/board-baseline.tsv" 2>/dev/null || true
# symlink to the committed blob's checkout path on disk of another tree: use the worktree file at gbase via absolute
rm -f "$G/harness/board-baseline.tsv"
# recreate as a symlink pointing at the just-committed path of a side copy
mkdir -p "$T/abs"; cp "$T/baseline.orig" "$T/abs/board-baseline.tsv"
ln -s "$T/abs/board-baseline.tsv" "$G/harness/board-baseline.tsv"
git -C "$G" add -A; git -C "$G" commit -qm 'F1 symlink'
ceil F1-symlink 2 'not a regular file|mode 120000|symlink'
# F2: deleted branch baseline -> rc 2 (never raise-acceptable), including after an approved raise would have applied
git -C "$G" reset -q --hard "$gbase"; git -C "$G" rm -q harness/board-baseline.tsv; git -C "$G" commit -qm 'F2 delete'
ceil F2-deleted 2 'has no harness/board-baseline.tsv'
# F3: dropping a failing id without raising passed (csr-render-template 567/2/0 analogue on js-eval: invent a failing id then drop it)
# build a "main" with one extra failing id by rewriting the BASE commit? No: gbase is the seed; treat a side main.
# We compare HEAD (cand) vs gbase (main). So drop an id that exists on gbase.
# gbase js-eval is 2/0/0: dropping one passed id is a case-set + worse (passed drop + universe shrink).
node -e '
  const fs=require("fs"); const f=process.argv[1];
  let t=fs.readFileSync(f,"utf8");
  t=t.replace(/^js-eval\tcounts\t2 0 0\njs-eval\tpassed\t[^\n]+\njs-eval\tpassed\t[^\n]+\n/m,
              `js-eval\tcounts\t1 0 0\njs-eval\tpassed\t${process.argv[2]}\n`);
  fs.writeFileSync(f,t);
' "$T/drop-one.tsv" "$JP1" 2>/dev/null || true
# write drop-one from orig
cp "$T/baseline.orig" "$T/drop-one.tsv"
setrow "$T/drop-one.tsv" --passed "$JP1"
commit_bl "$T/drop-one.tsv" 'F3 drop one passed'
ceil F3-drop-passed 1 'removed from the case universe|CASE-SET CHANGE: js-eval'
# F3b: skip list emptied without passed rising — invent on a side "main" via measured? Use a private main commit:
# rebuild: create main2 that has js-eval 2 0 1 (one skip), branch drops the skip (2 0 0) without adding a pass... wait 2/0/0 vs 2/0/1 would be IMPROVED skip drop = case-set + passed same = worse via count rule.
# easier: take gbase (2/0/0) and add a skip on the BRANCH? that's newly skipped = case-set + worse.
cp "$T/baseline.orig" "$T/newskip.tsv"
setrow "$T/newskip.tsv" --passed "$JP1" --skipped "$JP2"
commit_bl "$T/newskip.tsv" 'F3 newly skipped'
ceil F3-newly-skipped 1 'newly skipped|CASE-SET CHANGE: js-eval'
# F3c: drop a failing id, passed unchanged (Arch's 567/2/0). Need a main that already has a fail.
# Use measured? No — rewrite gbase comparison by swapping: put the fail-row on a NEW main? ceiling compares HEAD vs $gbase.
# So put the fail-row ON gbase by making a parallel pair.
P2="$T/pair"; mkdir -p "$P2/harness"; cd "$P2"; git init -q -b main; git config user.email t@t; git config user.name t
cp "$T/baseline.orig" harness/board-baseline.tsv
setrow harness/board-baseline.tsv --passed "$JP1" --failed "$JP2"   # 1 1 0
git add -A; git commit -qm 'main with a fail'
p2base=$(git rev-parse HEAD)
setrow harness/board-baseline.tsv --passed "$JP1"                  # drop fail, passed same
git add -A; git commit -qm 'drop fail'
rc=0; "$M/scripts/baseline-ceiling.sh" "$P2" "$p2base" HEAD > "$T/out.ceil.F3-drop-fail" 2>&1 || rc=$?
if [ $rc -eq 1 ] && grep -qE 'no longer failing/skipped|removed from the case universe|CASE-SET CHANGE' "$T/out.ceil.F3-drop-fail"; then ok "ceiling F3-drop-fail -> rc 1"
else bad "ceiling F3-drop-fail -> rc $rc"; sed 's/^/       /' "$T/out.ceil.F3-drop-fail"; fi
# F4: drop the template-diag row
cp "$T/baseline.orig" "$T/nodiag.tsv"
node -e 'const fs=require("fs");let t=fs.readFileSync(process.argv[1],"utf8");t=t.replace(/^template-diag\tcounts\t[^\n]+\n/m,"");fs.writeFileSync(process.argv[1],t);' "$T/nodiag.tsv"
commit_bl "$T/nodiag.tsv" 'F4 drop template-diag'
ceil F4-drop-diag 1 'template-diag exists on main but is missing'
# F4b: 9 crashes
cp "$T/baseline.orig" "$T/tdcrash.tsv"
node -e 'const fs=require("fs");let t=fs.readFileSync(process.argv[1],"utf8");t=t.replace(/^template-diag\tcounts\t[^\n]+/m,"template-diag\tcounts\t9 0");fs.writeFileSync(process.argv[1],t);' "$T/tdcrash.tsv"
commit_bl "$T/tdcrash.tsv" 'F4 9 crashes'
ceil F4-td-crash 1 'template-diag WORSE than main: crash'
# F5: a shell error inside baseline-ceiling.sh (TMPDIR unwritable -> mktemp fails) is rc 2, never 1
commit_bl "$T/baseline.orig" equal
rc=0; TMPDIR=/nonexistent "$M/scripts/baseline-ceiling.sh" "$G" "$gbase" HEAD > "$T/out.ceil.F5" 2>&1 || rc=$?
if [ $rc -eq 2 ] && grep -q 'treated as rc 2' "$T/out.ceil.F5"; then ok "ceiling F5 shell error -> rc 2"
else bad "ceiling F5 shell error -> rc $rc"; sed 's/^/       /' "$T/out.ceil.F5"; fi
# v1 malformed
printf 'board-baseline/v1\n#row\tpassed|clean\tfailed|crash\tskipped|oracle-error\njs-eval\t2\t1\t0\n' > "$T/v1.tsv"
commit_bl "$T/v1.tsv" v1
ceil F-v1 2 'board-baseline'
# Missing pre-land baseline (the old NEW-seed path) is rc 2 now that the seed has landed.
Q="$T/newrepo"; mkdir -p "$Q/harness"; cd "$Q"; git init -q -b main; git config user.email t@t; git config user.name t
git commit -qm empty --allow-empty; qempty=$(git rev-parse HEAD)
cp "$T/baseline.orig" harness/board-baseline.tsv; git add -A; git commit -qm seed
ceil_new=0; "$M/scripts/baseline-ceiling.sh" "$Q" "$qempty" HEAD > "$T/out.ceil.NEW" 2>&1 || ceil_new=$?
if [ $ceil_new -eq 2 ] && grep -qE 'pre-land .* has no harness/board-baseline.tsv' "$T/out.ceil.NEW"; then ok "ceiling missing pre-land baseline -> rc 2"
else bad "ceiling missing pre-land baseline -> rc $ceil_new (want 2)"; sed 's/^/       /' "$T/out.ceil.NEW"; fi
# D-0269 / AB-0006: land.sh must not judge MAIN file contents for the seed follow-up.
# Judges the BRANCH copy ($R), not MAIN content. The chokepoint already forbids raw
# MAIN reads; this keeps an explicit check that the deleted MAIN-content probe
# (grep NEW-seed path against $MAIN/scripts/baseline-ceiling.sh) has not returned.
if grep -nE "NEW-seed path" "$R/scripts/land.sh"    || grep -qF $'grep -q \'NEW-seed path\' "$MAIN/' "$R/scripts/land.sh"; then
  bad "D-0269: land.sh contains a NEW-seed-path MAIN-content probe"
else
  ok "D-0269: land.sh does not contain a NEW-seed-path MAIN-content probe"
fi
# --- land.sh block, verbatim ---
blk="$(sed -n '/^# >>> baseline-ceiling/,/^# <<< baseline-ceiling/p' "$R/scripts/land.sh")"
[ -n "$blk" ] || { bad "baseline-ceiling block not found in scripts/land.sh"; exit 1; }
cd "$G"
landblk() { # name want(accept|refuse) <commit-msg> <how: file|delete|symlink> <reviews-rows> <regex> [decisions-extra]
  git reset -q --hard "$gbase"
  case "$4" in
    delete) git rm -q harness/board-baseline.tsv;;
    symlink) rm -f harness/board-baseline.tsv; ln -s "$T/abs/board-baseline.tsv" harness/board-baseline.tsv; git add -A;;
    *) cp "$4" harness/board-baseline.tsv; git add -A;;
  esac
  echo x >> marker; git add -A; git commit -qm "$3"
  local pid; pid="$(git diff "$gbase" HEAD | git patch-id --stable | cut -d' ' -f1)"
  printf 'id\tverdict\tpatch_id\n%s' "$(sed "s/@PID@/$pid/g" <<< "$5")" > "$T/reviews.tsv"
  local rc=0
  ( set -euo pipefail
    die() { echo "DIE: $*"; exit 97; }
    MAIN="$M"; AB="$T"; wt="$G"; base="$gbase"; head="$(git rev-parse HEAD)"
    DEC_REPO="$N"; DEC_PATH=lwc-rust-migrate/decisions.tsv
    DEC_SHA="$(git -C "$N" rev-parse --verify HEAD)"
    main_run() { local r="$1"; shift; "$MAIN/$r" "$@"; }
    main_data() { printf "%s\n" "$MAIN/$1"; }
    eval "$blk"; exit 0
  ) > "$T/out.land" 2>&1 || rc=$?
  local got=refuse; [ $rc -eq 0 ] && got=accept
  if [ "$got" = "$2" ] && { [ $rc -eq 0 ] || [ $rc -eq 97 ] || [ $rc -eq 2 ]; } && grep -qE -- "$6" "$T/out.land"; then ok "land block: $1 -> $got"
  else bad "land block: $1 -> $got (rc $rc), want $2"; sed 's/^/       /' "$T/out.land"; fi
}
# existing raise matrix
landblk 'equal baseline, no trailer'            accept $'c' "$T/baseline.orig" $'' 'baseline-ceiling: [0-9]+ main row\(s\) checked, ok'
landblk 'worse, no Baseline-Raise'               refuse $'c' "$T/worse.tsv" $'' 'DIE: harness/board-baseline.tsv is WORSE'
landblk 'worse, Baseline-Raise not reviewed'     refuse $'c\n\nBaseline-Raise: AB-R9' "$T/worse.tsv" $'' 'Baseline-Raise AB-R9 -> <not found>'
landblk 'worse, Baseline-Raise REWORK'           refuse $'c\n\nBaseline-Raise: AB-R1' "$T/worse.tsv" $'AB-R1\tREWORK\t@PID@\n' 'Baseline-Raise AB-R1 -> REWORK'
landblk 'worse, Baseline-Raise stale pid'        refuse $'c\n\nBaseline-Raise: AB-R1' "$T/worse.tsv" $'AB-R1\tAPPROVE\t0000000000000000000000000000000000000000\n' 'reviewed patch-id 000000000000'
landblk 'worse, last row REWORK after APPROVE'   refuse $'c\n\nBaseline-Raise: AB-R1' "$T/worse.tsv" $'AB-R1\tAPPROVE\t@PID@\nAB-R1\tREWORK\t@PID@\n' 'Baseline-Raise AB-R1 -> REWORK'
landblk 'worse, own APPROVE Baseline-Raise'      accept $'c\n\nBaseline-Raise: AB-R1' "$T/worse.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'raise accepted under Baseline-Raise'
landblk 'malformed (v1) baseline'                refuse $'c\n\nBaseline-Raise: AB-R1' "$T/v1.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'DIE: baseline-ceiling refused \(rc=2\)'
# F1: symlink + approved raise still refused (rc 2)
landblk 'F1 symlink + approved raise'            refuse $'c\n\nBaseline-Raise: AB-R1' symlink $'AB-R1\tAPPROVE\t@PID@\n' 'DIE: baseline-ceiling refused \(rc=2\).*non-regular|mode 120000|not a regular file'
# F2: deleted + approved raise still refused (rc 2)
landblk 'F2 deleted + approved raise'            refuse $'c\n\nBaseline-Raise: AB-R1' delete $'AB-R1\tAPPROVE\t@PID@\n' 'DIE: baseline-ceiling refused \(rc=2\)'
# F3: drop a case + approved raise but NO Case-Set trailer
landblk 'F3 drop case, raise, no Case-Set'       refuse $'c\n\nBaseline-Raise: AB-R1' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'DIE: case-set change'
# F3: drop a case + raise + Case-Set citing a D-id that does NOT list case-set:js-eval
landblk 'F3 Case-Set D-0001 (no token)'          refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-0001' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'no cited Case-Set D-id'
# F3: drop a case + raise + Case-Set D-9999 which lists case-set:js-eval (accepted)
landblk 'F3 Case-Set D-9999 (listed subset)'     accept $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-9999' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'case-set change in js-eval accepted under D-9999'
landblk 'Case-Set D-8888 (id not listed)'        refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-8888' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'not a superset|no cited Case-Set D-id'
landblk 'Case-Set D-7777 (non-migrate author)'   refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-7777' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'added by .harness <harness@lwc-bots>|no cited Case-Set D-id'
landblk 'Case-Set D-6666 (rewritten by non-migrate)' refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-6666' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'last written by .harness <harness@lwc-bots>|no cited Case-Set D-id'
landblk 'foocase-set refused' refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-4444' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'not at the start of a tab-separated field|foocase-set'
landblk 'pickaxe decoy does not count as a mint' accept $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-5555' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'case-set change in js-eval accepted under D-5555'
landblk 'Case-Set trailing |' refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-3434' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'malformed'
landblk 'Case-Set space-only list' refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-2323' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'malformed'
landblk 'Case-Set empty list' refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-1212' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'malformed'
landblk 'Case-Set duplicate ids' refuse $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-4545' "$T/drop-one.tsv" $'AB-R1\tAPPROVE\t@PID@\n' 'malformed'
# F4: dropped DIAG row + approved raise (rc 1, raise can accept the worse row — that is intended; the point is it is WORSE)
landblk 'F4 drop template-diag, no raise'        refuse $'c' "$T/nodiag.tsv" $'' 'DIE: harness/board-baseline.tsv is WORSE'
# F5: TMPDIR unwritable makes baseline-ceiling.sh's mktemp fail (trap -> rc 2); an approved raise cannot accept it.
# csfile mktemp uses an explicit /tmp path so only the ceiling's mktemp -d (honors TMPDIR) fails.
landblk_f5() {
  git reset -q --hard "$gbase"; cp "$T/baseline.orig" harness/board-baseline.tsv; echo x >> marker; git add -A
  git commit -qm $'c\n\nBaseline-Raise: AB-R1'
  local pid; pid="$(git diff "$gbase" HEAD | git patch-id --stable | cut -d' ' -f1)"
  printf 'id\tverdict\tpatch_id\nAB-R1\tAPPROVE\t%s\n' "$pid" > "$T/reviews.tsv"
  local rc=0
  ( set -euo pipefail
    die() { echo "DIE: $*"; exit 97; }
    MAIN="$M"; AB="$T"; wt="$G"; base="$gbase"; head="$(git rev-parse HEAD)"
    DEC_REPO="$N"; DEC_PATH=lwc-rust-migrate/decisions.tsv
    DEC_SHA="$(git -C "$N" rev-parse --verify HEAD)"
    main_run() { local r="$1"; shift; "$MAIN/$r" "$@"; }
    main_data() { printf "%s\n" "$MAIN/$1"; }
    export TMPDIR=/nonexistent
    eval "$blk"; exit 0
  ) > "$T/out.land" 2>&1 || rc=$?
  if [ $rc -eq 97 ] && grep -qE 'DIE: baseline-ceiling refused \(rc=2\)|treated as rc 2' "$T/out.land"; then ok "land block: F5 shell error + approved raise -> refuse"
  else bad "land block: F5 -> rc $rc"; sed 's/^/       /' "$T/out.land"; fi
}
landblk_f5

# F6: empty OPTGATE_DECISIONS is rc 2 (does not fall back to live notes)
rc=0; (cd "$R/harness" && OPTGATE_DECISIONS= node scripts/option-gate.mjs "style") > "$T/out.F6" 2>&1 || rc=$?
if [ $rc -eq 2 ] && grep -q 'OPTGATE_DECISIONS is set but empty' "$T/out.F6"; then ok "F6 empty OPTGATE_DECISIONS -> rc 2"
else bad "F6 empty OPTGATE_DECISIONS -> rc $rc"; sed 's/^/       /' "$T/out.F6"; fi
# unset still uses the live default (must not crash on the path existing)
rc=0; env -u OPTGATE_DECISIONS node "$R/harness/scripts/option-gate.mjs" "style js js-eval template ssr ssr-async ssr-asyncYield ssr-render ssr-render-async ssr-render-asyncYield csr-render csr-render-template template-renderer js-options js-api template-options ssr_dynamic_imports style-api template-api ssr_render_api template-diag template-diag-napi" > "$T/out.F6ok" 2>&1 || rc=$?
if [ $rc -eq 0 ]; then ok "F6 unset OPTGATE_DECISIONS uses live default (rc 0)"; else bad "F6 unset -> rc $rc"; sed 's/^/       /' "$T/out.F6ok"; fi

# F5-window: a `false` planted immediately after set -euo pipefail must be rc 2 (trap is installed first).
# If the trap were still after set -e, this would leak out as rc 1 (the old window).
awk '1; /set -euo pipefail/{print "false  # planted old-window failure"}' "$R/scripts/baseline-ceiling.sh" > "$T/ceil-trap.sh"
chmod +x "$T/ceil-trap.sh"
# point its script-relative MJS at the real one via running from a copy next to a fake ../harness/... 
# baseline-ceiling.sh finds MJS at $here/../harness/scripts/board-baseline.mjs; use the real tree as cwd? 
# $0 dirname is $T, so it would look at $T/../harness — instead run via bash with a wrapper dir:
mkdir -p "$T/trapwrap/scripts" "$T/trapwrap/harness/scripts"
cp "$T/ceil-trap.sh" "$T/trapwrap/scripts/baseline-ceiling.sh"
cp "$R/harness/scripts/board-baseline.mjs" "$T/trapwrap/harness/scripts/"
rc=0; bash "$T/trapwrap/scripts/baseline-ceiling.sh" "$G" "$gbase" HEAD > "$T/out.ceil.trapwindow" 2>&1 || rc=$?
if [ $rc -eq 2 ] && grep -q 'treated as rc 2' "$T/out.ceil.trapwindow"; then ok "ceiling trap installed before set -e (planted false -> rc 2)"
else bad "ceiling trap window: rc $rc (want 2)"; sed 's/^/       /' "$T/out.ceil.trapwindow"; fi

# Mutation: a branch board.sh that fakes obs matching its (green) claim must not be the measurement producer.
# MAIN's board.sh --root=<branch tree> runs the real tests (injected failure) and never executes the branch board.sh.
restore
inject
FAKE="$T/fakewt"; mkdir -p "$FAKE"
ln -sfn "$R/crates" "$FAKE/crates"
rm -rf "$FAKE/harness"; cp -a "$H" "$FAKE/harness"
cat > "$FAKE/harness/board.sh" << 'FAKEBOARD'
#!/usr/bin/env bash
# malicious measurement producer: write green js-eval obs without running tests
set -euo pipefail
: "${BOARD_LOG_DIR:?}"
mkdir -p "$BOARD_LOG_DIR/obs"
awk 'BEGIN{p=0} $1=="js-eval"{print; p=1; next} p && $1!="js-eval"{exit} p' "$(dirname "$0")/board-baseline.tsv" > "$BOARD_LOG_DIR/obs/js-eval.tsv"
printf 'FAKE_RAN\n' > "$BOARD_LOG_DIR/FAKE_RAN"
exit 0
FAKEBOARD
chmod +x "$FAKE/harness/board.sh"
rc=0; BOARD_LOG_DIR="$T/log.mut" CRATES_ROOT="$R/crates" BOARD_REPO_ROOT="$R" \
  "$R/harness/board.sh" --root="$FAKE" js-eval > "$T/out.mut" 2>&1 || rc=$?
assert_observed "$T/out.mut" "mutation --root" || true
if [ $rc -eq 1 ] && grep -qE 'RATCHET REGRESSION: js-eval' "$T/out.mut" && [ ! -f "$T/log.mut/FAKE_RAN" ]; then
  ok "MAIN board.sh --root=wt ignores a faking branch board.sh (injected failure visible, fake marker absent)"
else
  bad "mutation --root: rc $rc (want 1, REGRESSION, no FAKE_RAN)"; sed 's/^/       /' "$T/out.mut" | tail -20
  [ -f "$T/log.mut/FAKE_RAN" ] && echo "       FAKE_RAN was written — branch board.sh ran"
fi
restore

# --- r4 mutations ---
# Missing notes: rc 2 (not die/rc 1).
rc=0
( set -euo pipefail
  die() { echo "DIE: $*"; exit 97; }
  MAIN="$M"; AB="$T"; wt="$G"; base="$gbase"; head="$(git -C "$G" rev-parse HEAD)"
  DEC_REPO="/nonexistent/notes-missing-$$"; DEC_PATH=lwc-rust-migrate/decisions.tsv
  unset DEC_SHA
  git -C "$G" reset -q --hard "$gbase"
  cp "$T/drop-one.tsv" "$G/harness/board-baseline.tsv"; echo x >> "$G/marker"; git -C "$G" add -A
  git -C "$G" commit -qm $'c\n\nBaseline-Raise: AB-R1\nCase-Set: D-9999'
  head="$(git -C "$G" rev-parse HEAD)"
  eval "$blk"; exit 0
) > "$T/out.notes-missing" 2>&1 || rc=$?
if [ $rc -eq 2 ] && grep -qE 'notes unavailable' "$T/out.notes-missing"; then ok "mutation missing notes -> rc 2"
else bad "mutation missing notes -> rc $rc (want 2)"; sed 's/^/       /' "$T/out.notes-missing" | tail -20; fi

# --- r5 mutations ---
MSG_OBS='obs sha256 mismatch for js-eval'

# Both forgers latch only onto a js-eval run whose --outputFile is under the scope dir (that
# mutation's BOARD_LOG_DIR). Unscoped, they matched any concurrent board's js-eval (another land,
# the official board), clobbered that run's obs instead of ours (our mutation then FAILs with no
# mismatch line) and made the foreign board fail with a spurious obs sha256 mismatch.
start_obs_writer() {
  local mode="$1" swap="$2" scope="$3"
  [ -n "$scope" ] || { echo "start_obs_writer: scope dir required" >&2; exit 2; }
  python3 -c '
import os, sys, time
mode, swap = sys.argv[1], sys.argv[2]
scope = os.path.realpath(sys.argv[3]) + os.sep
logdir = ""
while True:
    if not logdir:
        try:
            pids = [p for p in os.listdir("/proc") if p.isdigit()]
        except OSError:
            pids = []
        for pid in pids:
            try:
                data = open("/proc/%s/cmdline" % pid, "rb").read()
            except OSError:
                continue
            if b"js-eval" not in data:
                continue
            args = [a.decode("utf-8", "replace") for a in data.split(b"\0") if a]
            path = ""
            for i, a in enumerate(args):
                if a.startswith("--outputFile.json="):
                    path = a.split("=", 1)[1]
                elif a == "--outputFile.json" and i + 1 < len(args):
                    path = args[i + 1]
                elif "board-js-eval" in a and a.endswith(".json"):
                    path = a
            if path and os.path.realpath(path).startswith(scope):
                logdir = os.path.dirname(path)
                break
    if logdir:
        obs = os.path.join(logdir, "obs", "js-eval.tsv")
        if os.path.isfile(obs):
            payload = "forged\n" if mode == "garbage" else (swap if swap.endswith("\n") else swap + "\n")
            try:
                open(obs, "w").write(payload)
            except OSError:
                pass
    time.sleep(0.05)
' "$mode" "$swap" "$scope" &
  WRITER_PID=$!
}

assert_mut() {
  local name="$1" rc="$2" log="$3" msg="$4"
  assert_observed "$log" "$name" || return
  if [ "$rc" -ne 0 ] && grep -qE -- "$msg" "$log"; then ok "$name -> rc $rc ($msg)"
  else bad "$name -> rc $rc (want non-zero + /$msg/)"; sed 's/^/       /' "$log" | tail -25; fi
}

# (4) Detached overwrite: discover logdir via --outputFile on /proc cmdline and clobber obs.
restore
inject
FAKE="$T/fakewt2"; mkdir -p "$FAKE"
ln -sfn "$R/crates" "$FAKE/crates"
rm -rf "$FAKE/harness"; cp -a "$H" "$FAKE/harness"
mkdir -p "$T/log.ow"; start_obs_writer garbage "" "$T/log.ow"; wpid=$WRITER_PID
rc=0; BOARD_LOG_DIR="$T/log.ow" CRATES_ROOT="$R/crates" BOARD_REPO_ROOT="$R" \
  "$R/harness/board.sh" --root="$FAKE" js-eval > "$T/out.ow" 2>&1 || rc=$?
kill "$wpid" 2>/dev/null || true; wait "$wpid" 2>/dev/null || true
assert_mut "mutation detached overwrite" "$rc" "$T/out.ow" "$MSG_OBS"
restore

# (4) Same-count ID swap: overwrite obs with 2/1/0 but a different failing id than the injected one.
inject
FAKE="$T/fakewt4"; mkdir -p "$FAKE"
ln -sfn "$R/crates" "$FAKE/crates"
rm -rf "$FAKE/harness"; cp -a "$H" "$FAKE/harness"
swap="$(printf 'js-eval\tcounts\t2 1 0\njs-eval\tpassed\t%s\njs-eval\tpassed\t%s\njs-eval\tfailed\t%s\n' "$JP1" "$INJ" "$JP2")"
# passed ids must be sorted (code-unit). JP1 vs INJ vs JP2:
swap="$(python3 -c '
jp1, jp2, inj = """'"$JP1"'""", """'"$JP2"'""", """'"$INJ"'"""
P = sorted([jp1, inj])
print("js-eval\\tcounts\\t2 1 0")
for i in P: print("js-eval\\tpassed\\t"+i)
print("js-eval\\tfailed\\t"+jp2)
')"
mkdir -p "$T/log.swap"; start_obs_writer idswap "$swap" "$T/log.swap"; wpid=$WRITER_PID
rc=0; BOARD_LOG_DIR="$T/log.swap" CRATES_ROOT="$R/crates" BOARD_REPO_ROOT="$R" \
  "$R/harness/board.sh" --root="$FAKE" js-eval > "$T/out.swap" 2>&1 || rc=$?
kill "$wpid" 2>/dev/null || true; wait "$wpid" 2>/dev/null || true
assert_mut "mutation same-count id-swap" "$rc" "$T/out.swap" "$MSG_OBS"
restore

# Flipper: alternate forged (green baseline claim) and true derived bytes in the published
# obs/js-eval.tsv for the whole run. No board.sh hooks. Judge reads the private dir, so the
# outcome must be rc 2 + obs sha256 mismatch, or the true RATCHET REGRESSION — never green
# and never a forged (equal/hidden-regression) verdict.
start_obs_flipper() {
  local forged="$1" wlog="$2" scope="$3"
  [ -n "$scope" ] || { echo "start_obs_flipper: scope dir required" >&2; exit 2; }
  python3 -c '
import os, sys, time
forged = open(sys.argv[1]).read()
scope = os.path.realpath(sys.argv[2]) + os.sep
if not forged.endswith("\n"):
    forged += "\n"
deadline = time.time() + 180
logdir = ""
obs = None
while time.time() < deadline:
    if not logdir:
        try:
            pids = [p for p in os.listdir("/proc") if p.isdigit()]
        except OSError:
            pids = []
        for pid in pids:
            try:
                data = open("/proc/%s/cmdline" % pid, "rb").read()
            except OSError:
                continue
            if b"js-eval" not in data:
                continue
            args = [a.decode("utf-8", "replace") for a in data.split(b"\0") if a]
            path = ""
            for i, a in enumerate(args):
                if a.startswith("--outputFile.json="):
                    path = a.split("=", 1)[1]
                elif a == "--outputFile.json" and i + 1 < len(args):
                    path = args[i + 1]
                elif "board-js-eval" in a and a.endswith(".json"):
                    path = a
            if path and os.path.realpath(path).startswith(scope):
                logdir = os.path.dirname(path)
                obs = os.path.join(logdir, "obs", "js-eval.tsv")
                break
    if logdir and os.path.isfile(obs):
        true = open(obs).read()
        if "\tcounts\t" not in true:
            time.sleep(0.01)
            continue
        print("flipper: captured true bytes (%d)" % len(true), flush=True)
        tmp = obs + ".flip-tmp"
        n = 0
        while True:
            payload = forged if (n % 2 == 0) else true
            try:
                open(tmp, "w").write(payload)
                os.replace(tmp, obs)
                n += 1
                if n == 1 or n % 50 == 0:
                    print("flipper: writes %d" % n, flush=True)
            except OSError:
                pass
            time.sleep(0.01)
    time.sleep(0.01)
print("flipper: timeout waiting for published obs", flush=True)
sys.exit(1)
' "$forged" "$scope" > "$wlog" 2>&1 &
  WRITER_PID=$!
}
FAKE="$T/fakewt-flip"; mkdir -p "$FAKE"
ln -sfn "$R/crates" "$FAKE/crates"
awk -F'\t' '$1=="js-eval"{print}' "$T/baseline.orig" > "$T/flip.forged"
for i in 1 2 3; do
  restore
  inject
  rm -rf "$FAKE/harness"; cp -a "$H" "$FAKE/harness"
  mkdir -p "$T/log.flip.$i"; start_obs_flipper "$T/flip.forged" "$T/flip.$i.writer.log" "$T/log.flip.$i"; wpid=$WRITER_PID
  rc=0; BOARD_LOG_DIR="$T/log.flip.$i" CRATES_ROOT="$R/crates" BOARD_REPO_ROOT="$R" \
    "$R/harness/board.sh" --root="$FAKE" js-eval > "$T/out.flip.$i" 2>&1 || rc=$?
  kill "$wpid" 2>/dev/null || true; wait "$wpid" 2>/dev/null || true
  name="mutation published-obs flipper run $i"
  assert_observed "$T/out.flip.$i" "$name" || true
  mismatch=0; grep -qE 'obs sha256 mismatch' "$T/out.flip.$i" && mismatch=1
  regression=0; grep -qE 'RATCHET REGRESSION: js-eval' "$T/out.flip.$i" && regression=1
  improved=0; grep -qE 'RATCHET IMPROVED' "$T/out.flip.$i" && improved=1
  flipped=0; grep -qE 'flipper: writes ' "$T/flip.$i.writer.log" && flipped=1
  # Allowed: rc 2 + hash mismatch, or the true injected regression.
  # Forbidden: green (rc 0 / equal) or a forged hidden-regression / IMPROVED verdict.
  if [ "$flipped" -ne 1 ]; then
    bad "$name: flipper never wrote published obs (rc $rc)"
    sed 's/^/       writer: /' "$T/flip.$i.writer.log" | tail -10
  elif [ "$improved" -eq 1 ] || [ "$rc" -eq 0 ]; then
    bad "$name: green or forged verdict (rc $rc)"
    sed 's/^/       /' "$T/out.flip.$i" | tail -25
  elif [ "$rc" -eq 2 ] && [ "$mismatch" -eq 1 ]; then
    ok "$name -> rc 2 (obs sha256 mismatch)"
  elif [ "$regression" -eq 1 ] && [ "$mismatch" -eq 0 ]; then
    ok "$name -> rc $rc (RATCHET REGRESSION: js-eval)"
  else
    bad "$name: rc $rc (want rc 2 + mismatch, or true REGRESSION; never green/forged)"
    sed 's/^/       /' "$T/out.flip.$i" | tail -25
    sed 's/^/       writer: /' "$T/flip.$i.writer.log" | tail -10
  fi
done
restore

# (2) Branch harness/.env must have no effect (envDir disabled in MAIN's vitest.board.config.ts).
restore
FAKE="$T/fakewt-env"; mkdir -p "$FAKE"
ln -sfn "$R/crates" "$FAKE/crates"
rm -rf "$FAKE/harness"; cp -a "$H" "$FAKE/harness"
printf 'BOARD_ENV_PROBE=should-not-load\n' > "$FAKE/harness/.env"
printf '%s\n' 'if (process.env.BOARD_ENV_PROBE) throw new Error("env leaked from .env");' | cat - "$FAKE/harness/test/js-eval.behavior.test.ts" > "$T/js-eval.env.ts"
mv "$T/js-eval.env.ts" "$FAKE/harness/test/js-eval.behavior.test.ts"
rc=0; BOARD_LOG_DIR="$T/log.env" CRATES_ROOT="$R/crates" BOARD_REPO_ROOT="$R" \
  "$R/harness/board.sh" --root="$FAKE" js-eval > "$T/out.env" 2>&1 || rc=$?
assert_observed "$T/out.env" "mutation branch .env" || true
if [ "$rc" -eq 0 ] && ! grep -q 'env leaked from .env' "$T/out.env"; then ok "mutation branch .env has no effect (rc 0)"
else bad "mutation branch .env -> rc $rc (want 0, no leak)"; sed 's/^/       /' "$T/out.env" | tail -20; fi
restore

# Branch Vite plugin that strips the injected throw must have no effect (pinned --config).
inject
FAKE="$T/fakewt3"; mkdir -p "$FAKE"
ln -sfn "$R/crates" "$FAKE/crates"
rm -rf "$FAKE/harness"; cp -a "$H" "$FAKE/harness"
cat > "$FAKE/harness/vitest.config.ts" << 'VCFG'
import { defineConfig } from 'vitest/config';
export default defineConfig({
  plugins: [{
    name: 'strip-throw',
    transform(code, id) {
      if (String(id).includes('js-eval.behavior.test.ts')) {
        return code.replace(/throw new Error\("injected"\)/g, 'return');
      }
    },
  }],
  test: { include: ['test/**/*.test.ts'] },
});
VCFG
rc=0; BOARD_LOG_DIR="$T/log.plug" CRATES_ROOT="$R/crates" BOARD_REPO_ROOT="$R" \
  "$R/harness/board.sh" --root="$FAKE" js-eval > "$T/out.plug" 2>&1 || rc=$?
assert_observed "$T/out.plug" "mutation Vite plugin" || true
if [ "$rc" -ne 0 ] && grep -qE 'RATCHET REGRESSION: js-eval' "$T/out.plug"; then ok "mutation branch Vite plugin strip-throw has no effect (test still fails)"
else bad "mutation Vite plugin: rc $rc (want non-zero + REGRESSION)"; sed 's/^/       /' "$T/out.plug" | tail -20; fi
restore

echo "test-board-ratchet: $([ $fail -eq 0 ] && echo ok || echo FAILED)"
exit $fail
