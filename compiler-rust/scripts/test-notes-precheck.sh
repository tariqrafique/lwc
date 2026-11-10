#!/usr/bin/env bash
# Guard test for AB-0016 in a THROWAWAY clone of /workspace/notes (never the real one): scripts/notes-precheck.sh and
# official-board.sh's notes block (extracted verbatim between '# >>> notes' / '# <<< notes') must
#   - leave an unrelated dirty notes file untouched: rc 0, HEAD unchanged, nothing staged/committed;
#   - exit 3 on a dirty (modified / untracked / staged) read path, a read path missing from HEAD, or a git error.
set -euo pipefail
S="$(cd "$(dirname "$0")" && pwd)"; PC="$S/notes-precheck.sh"; OB="$S/official-board.sh"
src="${NOTES_TEST_SOURCE:-/workspace/notes}"
P="$(mktemp -d /tmp/test-notes-precheck.XXXXXX)"; trap 'rm -rf "$P"' EXIT
git clone -q --no-hardlinks "$src" "$P/notes"; N="$P/notes"; git -C "$N" config user.email t@t; git -C "$N" config user.name t
blk="$(sed -n '/^# >>> notes/,/^# <<< notes/p' "$OB")"
fail=0; ok() { echo "ok   $*"; }; bad() { echo "FAIL $*"; fail=1; }
reset() { git -C "$N" reset -q --hard "$h0"; git -C "$N" clean -qfdx; }
h0="$(git -C "$N" rev-parse HEAD)"
[ -n "$blk" ] || bad "notes block not found in official-board.sh"
# official-board's block, run against the clone: prints notescommit; must not touch the repo.
runblk() { ( set -euo pipefail; NOTES="$N"; NOTES_PRECHECK="$PC"; die3() { echo "!! BOARD ABORTED: $*" >&2; exit 3; }; eval "$blk"; echo "notescommit=$notescommit" ) ; }
expect() { # name want-rc cmd...
  local name="$1" want="$2" rc=0; shift 2; "$@" >"$P/out" 2>&1 || rc=$?
  if [ "$rc" -eq "$want" ]; then ok "$name -> rc $rc"; else bad "$name -> rc $rc, want $want: $(tail -3 "$P/out")"; fi; }
untouched() { # name: HEAD unchanged, nothing staged
  [ "$(git -C "$N" rev-parse HEAD)" = "$h0" ] && ok "$1: HEAD unchanged" || bad "$1: HEAD moved"
  [ -z "$(git -C "$N" diff --cached --name-only)" ] && ok "$1: nothing staged" || bad "$1: index changed"; }
# 1. unrelated dirty notes (modified tracked file outside the read paths + untracked file): pass, untouched.
# first tracked file [under <pathspec>] [not under <exclude-prefix>]; no `| head` (SIGPIPE -> rc 141 under pipefail)
first_tracked() { local all l; all="$(git -C "$N" ls-files -- ${1:+"$1"})" || { bad "git ls-files failed"; return 0; }
  while IFS= read -r l; do if [ -n "$2" ] && [[ "$l" == "$2"* ]]; then continue; fi; echo "$l"; return 0; done <<< "$all"; }
other="$(first_tracked "" lwc-rust-migrate/)"
[ -n "$other" ] || bad "clone has no tracked file outside lwc-rust-migrate/"
echo "half-written by another bot" >> "$N/$other"; echo wip > "$N/arch-board/zz-untracked-wip.md"
sum0="$(sha256sum "$N/$other" "$N/arch-board/zz-untracked-wip.md")"
expect "unrelated dirty file: precheck" 0 "$PC" "$N" lwc-rust-migrate
expect "unrelated dirty file: official-board notes block" 0 runblk
grep -q "^notescommit=$h0$" "$P/out" && ok "notes block records HEAD as-is ($h0)" || bad "notes block notescommit != HEAD: $(cat "$P/out")"
untouched "unrelated dirty file"
[ "$(sha256sum "$N/$other" "$N/arch-board/zz-untracked-wip.md")" = "$sum0" ] && ok "unrelated dirty files byte-identical" || bad "unrelated dirty files changed"
grep -q 'zz-untracked-wip' <<< "$(git -C "$N" status --porcelain)" && ok "untracked wip still untracked" || bad "untracked wip vanished/added"
reset
# 2. dirty read path: modified / untracked / staged -> exit 3, naming the path, repo untouched.
f="$(first_tracked lwc-rust-migrate "")"
[ -n "$f" ] || bad "clone has no tracked file under lwc-rust-migrate/"
echo x >> "$N/$f"; expect "modified read path ($f)" 3 runblk; grep -q "$f" "$P/out" && ok "message names $f" || bad "message does not name $f"; untouched "modified read path"; reset
echo x > "$N/lwc-rust-migrate/zz-new.md"; expect "untracked file under read path" 3 "$PC" "$N" lwc-rust-migrate; untouched "untracked read path"; reset
echo x >> "$N/$f"; git -C "$N" add "$f"; h1="$(git -C "$N" rev-parse HEAD)"; expect "staged read path" 3 "$PC" "$N" lwc-rust-migrate
[ "$(git -C "$N" rev-parse HEAD)" = "$h1" ] && ok "staged read path: HEAD unchanged" || bad "staged read path: HEAD moved"; reset
# 3. read path missing from HEAD, and git errors -> exit 3.
expect "read path missing from HEAD" 3 "$PC" "$N" no-such-dir
expect "not a git repo" 3 "$PC" "$P/nonrepo" lwc-rust-migrate
mkdir -p "$P/empty" && git -C "$P/empty" init -q && expect "repo without HEAD" 3 "$PC" "$P/empty" lwc-rust-migrate
expect "no paths given" 3 "$PC" "$N"

# D-0275: official-board copies run logs into the store (INVALID -> official/invalid/...).
sblk="$(sed -n '/^# >>> store-logs/,/^# <<< store-logs/p' "$OB")"
[ -n "$sblk" ] || { echo "FAIL store-logs block not found in official-board.sh"; fail=1; sblk=':'; }
eval "$sblk"
SL="$P/storelogs"; mkdir -p "$SL/snap"
echo board > "$SL/snap/board.log"
echo gold > "$SL/snap/golden.progress"
echo style > "$SL/snap/style-fixtures.log"
copy_official_run_logs 0 abcdef123456 "$SL/snap" "$SL/store"
if [ -f "$SL/store/abcdef123456/logs/board.log" ] && [ "$(cat "$SL/store/abcdef123456/logs/run.rc")" = 0 ] \
   && cmp -s "$SL/snap/board.log" "$SL/store/abcdef123456/logs/board.log" \
   && [ -f "$SL/store/abcdef123456/logs/golden.progress" ] && [ -f "$SL/store/abcdef123456/logs/style-fixtures.log" ]; then
  ok "store-logs: rc 0 -> official/<src12>/logs/"
else
  bad "store-logs: rc 0 did not land under official/abcdef123456/logs"
  find "$SL/store" -type f 2>/dev/null | sed 's/^/       /' || true
fi
copy_official_run_logs 4 abcdef123456 "$SL/snap" "$SL/store"
inv="$(find "$SL/store/invalid" -type d -name 'logs' -print -quit 2>/dev/null)"
if [ -n "$inv" ] && [[ "$inv" == *"/invalid/abcdef123456-"* ]] && [ -f "$inv/board.log" ] && [ "$(cat "$inv/run.rc")" = 4 ]; then
  ok "store-logs: rc 4 -> official/invalid/<src12>-<timestamp>/logs/"
else
  bad "store-logs: rc 4 did not land under official/invalid/<src12>-<timestamp>/logs"
  find "$SL/store" -type f 2>/dev/null | sed 's/^/       /' || true
fi
# INVALID path must not be the VALID dest
if [ ! -d "$SL/store/invalid" ] || [ "$(cat "$SL/store/abcdef123456/logs/run.rc")" = 0 ]; then
  ok "store-logs: INVALID copy did not overwrite official/<src12>/logs/run.rc"
else
  bad "store-logs: INVALID copy clobbered official/<src12>/logs"
fi
# Copy failure (missing snapdir) is rc 2, not success.
cfrc=0
copy_official_run_logs 0 abcdef123456 "$SL/no-such-snap" "$SL/store" >"$P/out.copyfail" 2>&1 || cfrc=$?
if [ "$cfrc" -eq 2 ] && grep -q 'snapshot dir missing' "$P/out.copyfail"; then
  ok "store-logs: missing snapdir is rc 2"
else
  bad "store-logs: missing snapdir rc=$cfrc (want 2): $(cat "$P/out.copyfail")"
fi
# EXIT trap copies logs when h0 is set, and a missing snapdir makes the process rc 2.
eval "$(sed -n "/^on_official_exit()/,/^}/p" "$OB")"
snap="$SL/trapsnap"; mkdir -p "$snap"; echo board > "$snap/board.log"
h0=trapok000001; store="$SL/trapstore"; logs_copied=0
trc=0
( trap on_official_exit EXIT; exit 0 ) >"$P/out.trapok" 2>&1 || trc=$?
if [ "$trc" -eq 0 ] && [ -f "$SL/trapstore/trapok000001/logs/board.log" ] && [ "$(cat "$SL/trapstore/trapok000001/logs/run.rc")" = 0 ]; then
  ok "store-logs: EXIT trap copies logs on rc 0"
else
  bad "store-logs: EXIT trap rc=$trc did not store logs"; sed "s/^/       /" "$P/out.trapok"; find "$SL/trapstore" -type f 2>/dev/null | sed "s/^/       /" || true
fi
snap="$SL/missing-trap-snap"; unset snap; snap="$SL/does-not-exist"
h0=trapfail00001; store="$SL/trapstore2"; logs_copied=0
trc=0
( trap on_official_exit EXIT; exit 0 ) >"$P/out.trapfail" 2>&1 || trc=$?
if [ "$trc" -eq 2 ]; then
  ok "store-logs: EXIT trap copy failure is rc 2"
else
  bad "store-logs: EXIT trap copy failure rc=$trc (want 2): $(cat "$P/out.trapfail")"
fi
# First non-zero rc is kept: copy failure must not overwrite INVALID (4) with 2.
snap="$SL/does-not-exist"
h0=trapkeep00004; store="$SL/trapstore4"; logs_copied=0
trc=0
( trap on_official_exit EXIT; exit 4 ) >"$P/out.trapkeep" 2>&1 || trc=$?
if [ "$trc" -eq 4 ]; then
  ok "store-logs: EXIT trap keeps first non-zero rc 4 (does not overwrite with 2)"
else
  bad "store-logs: EXIT trap rc=$trc (want 4, must not overwrite with 2): $(cat "$P/out.trapkeep")"
fi

# D-0275: copy_official_run_logs mkdir official/<src>/logs before publish-official
# runs. A logs-only dest is not a completed publish; a completed dest (tgz or
# OFFICIAL_BOARD.txt present) with different bytes is still refused.
PUB="$P/publish"
mkdir -p "$PUB"
src=pubtestd0275x
base="/workspace/push/official-$src"
cleanup_pub() { rm -f "$base" "$base".tgz "$base".board.txt "$base".template-diag.json "$base".template-diag-napi.json "$base".template-diag.soundness.json "$base".template-diag-napi.soundness.json "$base".csr-ssr.tsv; }
cleanup_pub
trap 'cleanup_pub; rm -rf "$P"' EXIT
echo dummy-tgz-v1 > "$base.tgz"
echo 'official board test (no failure marker)' > "$base.board.txt"
node -e '
const fs=require("fs"),c=require("crypto"),p=process.argv[1];
const body=JSON.stringify({schema:"template-diag-diff/v2",meta:{codegenCompare:{mode:"exact-bytes+semdiff"}}});
for (const n of [".template-diag.json",".template-diag-napi.json"]) {
  const f=p+n; fs.writeFileSync(f, body);
  const sha=c.createHash("sha256").update(fs.readFileSync(f)).digest("hex");
  fs.writeFileSync(f.replace(/\.json$/, ".soundness.json"),
    JSON.stringify({schema:"template-diag-soundness/v1",soundness:"SOUND",report_sha256:sha}));
}
' "$base"
runpub() {
  env OFFICIAL_COMMIT=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
      OFFICIAL_PARSE5_COMMIT=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb \
      OFFICIAL_PARSE5_CLONE_HEAD=cccccccccccccccccccccccccccccccccccccccc \
      OFFICIAL_NOTES_COMMIT=dddddddddddddddddddddddddddddddddddddddd \
      OFFICIAL_INPUTS_SHA=eeeeeeeeeeee \
      OFFICIAL_STYLE_PARSE_ERRORS=67/67 \
      OFFICIAL_ORACLE_COMMIT=ffffffffffffffffffffffffffffffffffffffff \
      OFFICIAL_ORACLE_REPO=/workspace/lwc \
      "$S/publish-official.sh" "$src" napishatest p5shatest "$1"
}
mkdir -p "$PUB/logs-only/$src/logs"
echo 0 > "$PUB/logs-only/$src/logs/run.rc"
prc=0
runpub "$PUB/logs-only" >"$P/out.publogs" 2>&1 || prc=$?
if [ "$prc" -eq 0 ] && [ -f "$PUB/logs-only/$src/OFFICIAL_BOARD.txt" ] && [ -f "$PUB/logs-only/$src/official-$src.tgz" ] \
   && [ "$(cat "$PUB/logs-only/$src/logs/run.rc")" = 0 ] && grep -q '^published:' "$P/out.publogs"; then
  ok "publish-official: logs-only dest from store-first copy is published into"
else
  bad "publish-official: logs-only dest rc=$prc (want 0): $(cat "$P/out.publogs")"
fi
prc=0
runpub "$PUB/logs-only" >"$P/out.pubid" 2>&1 || prc=$?
if [ "$prc" -eq 0 ] && grep -q 'already published (identical)' "$P/out.pubid"; then
  ok "publish-official: identical completed dest is rc 0"
else
  bad "publish-official: identical dest rc=$prc (want 0): $(cat "$P/out.pubid")"
fi
echo dummy-tgz-v2 > "$base.tgz"
prc=0
runpub "$PUB/logs-only" >"$P/out.pubdiff" 2>&1 || prc=$?
if [ "$prc" -eq 5 ] && grep -q 'exists with different bytes' "$P/out.pubdiff"; then
  ok "publish-official: completed dest with different tgz is refused"
else
  bad "publish-official: different completed dest rc=$prc (want 5): $(cat "$P/out.pubdiff")"
fi
# restore v1 so a fresh empty dest still works (sanity)
echo dummy-tgz-v1 > "$base.tgz"
mkdir -p "$PUB/fresh"
prc=0
runpub "$PUB/fresh" >"$P/out.pubfresh" 2>&1 || prc=$?
if [ "$prc" -eq 0 ] && [ -f "$PUB/fresh/$src/OFFICIAL_BOARD.txt" ]; then
  ok "publish-official: empty dest still publishes"
else
  bad "publish-official: empty dest rc=$prc (want 0): $(cat "$P/out.pubfresh")"
fi
cleanup_pub

exit $fail
