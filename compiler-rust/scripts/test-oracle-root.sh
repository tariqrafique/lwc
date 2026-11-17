#!/usr/bin/env bash
# Single-resolver tests (AB-M0002): oracle-root.mjs is the implementation;
# .sh wrappers call it; a directory without package.json is refused.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MJS="$ROOT/harness/scripts/oracle-root.mjs"
SH="$ROOT/harness/scripts/oracle-root.sh"
T="$(mktemp -d /tmp/oracle-root-test.XXXXXX)"; trap 'rm -rf "$T"' EXIT
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }

# missing package.json
mkdir -p "$T/not-lwc"
rc=0; LWC_ORACLE="$T/not-lwc" node "$MJS" oracle_root >"$T/out" 2>"$T/err" || rc=$?
if [ "$rc" -eq 2 ] && grep -q 'package.json' "$T/err"; then ok "mjs refuses a dir without package.json"; else bad "mjs missing-package.json rc=$rc: $(cat "$T/err")"; fi

# valid checkout
mkdir -p "$T/lwc"
echo '{"name":"lwc"}' > "$T/lwc/package.json"
got="$(LWC_ORACLE="$T/lwc" node "$MJS" oracle_root)"
want="$(cd "$T/lwc" && pwd)"
if [ "$got" = "$want" ]; then ok "mjs LWC_ORACLE with package.json"; else bad "mjs oracle_root got $got want $want"; fi

# sh wrapper matches mjs
# shellcheck source=../harness/scripts/oracle-root.sh
source "$SH"
gotsh="$(LWC_ORACLE="$T/lwc" oracle_root)"
if [ "$gotsh" = "$got" ]; then ok "sh oracle_root matches mjs"; else bad "sh oracle_root=$gotsh mjs=$got"; fi

# notesDecisionsPath: explicit env wins
echo x > "$T/decisions.tsv"
got="$(OPTGATE_DECISIONS="$T/decisions.tsv" node "$MJS" notes_decisions_path)"
if [ "$got" = "$(cd "$T" && pwd)/decisions.tsv" ]; then ok "notes_decisions_path honors OPTGATE_DECISIONS"; else bad "notes path $got"; fi

# snapshot fallback when env and box notes are hidden
got="$(env -u OPTGATE_DECISIONS -u SEMDIFF_REGISTRY node "$MJS" notes_decisions_path 2>"$T/err" || true)"
# On the box, /workspace/notes/... exists so this returns the box path — that is the
# intended precedence (snapshot only when notes are absent).
if [ -f /workspace/notes/lwc-rust-migrate/decisions.tsv ]; then
  if [ "$got" = /workspace/notes/lwc-rust-migrate/decisions.tsv ]; then ok "notes_decisions_path prefers box notes over snapshot"; else bad "box notes path $got"; fi
else
  snap="$ROOT/harness/decisions.snapshot.tsv"
  if [ "$got" = "$snap" ]; then ok "notes_decisions_path falls back to snapshot"; else bad "snapshot path $got"; fi
fi

# canonOraclePath rewrites the real root to <ORACLE_ROOT>, not the /workspace/lwc alias
canon="$(LWC_ORACLE="$T/lwc" node "$MJS" canon_oracle_path "err in $want/packages/x.js")"
if [ "$canon" = "err in <ORACLE_ROOT>/packages/x.js" ]; then ok "canonOraclePath uses <ORACLE_ROOT> token"; else bad "canon='$canon'"; fi
# A hardcoded /workspace/lwc in the text is not rewritten when the real root differs.
canon2="$(LWC_ORACLE="$T/lwc" node "$MJS" canon_oracle_path "hardcoded /workspace/lwc/foo")"
if [ "$canon2" = "hardcoded /workspace/lwc/foo" ]; then ok "canonOraclePath does not mask a hardcoded /workspace/lwc alias"; else bad "masked alias='$canon2'"; fi

exit $fail
