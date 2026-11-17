#!/usr/bin/env bash
# Publish a VALID official board's artifacts to the durable store: <store>/<src>/{official-<src>.tgz, board.txt, template-diag[-napi].json,
#   template-diag[-napi].soundness.json, csr-*.tsv, OFFICIAL_BOARD.txt}.
# The store FS has no rename(2): files are copied in, OFFICIAL_BOARD.txt LAST as the completion marker carrying
# sha256s of the other two. Consumers must require OFFICIAL_BOARD.txt and verify those sha256s. Refuses to
# overwrite a completed <src> dir (one that already has official-<src>.tgz or OFFICIAL_BOARD.txt) with different
# bytes. A logs-only dir created first by official-board.sh copy_official_run_logs (D-0275 store-first) is not a
# publish; artifacts are copied into it.
# Usage: publish-official.sh <srcsha> <napisha> <parse5sha> <store-official-dir>
set -euo pipefail
src="$1" napi="$2" p5="$3" store="$4"
# Provenance (git, 2026-11-01): every official board names the main commit it was built from.
[ -n "${OFFICIAL_COMMIT:-}" ] || { echo "!! PUBLISH REFUSED: no OFFICIAL_COMMIT (unnamed source)" >&2; exit 5; }
base="/workspace/push/official-$src"
[ -f "$base.tgz" ] && [ -f "$base.board.txt" ] && [ -s "$base.template-diag.json" ] && [ -s "$base.template-diag-napi.json" ] \
  || { echo "!! PUBLISH FAILED: missing $base.{tgz,board.txt,template-diag.json,template-diag-napi.json}" >&2; exit 5; }
grep -q '^!! ' "$base.board.txt" && { echo "!! PUBLISH REFUSED: board.txt contains a failure line" >&2; exit 5; }
# template-diag soundness (D-0219): derived from the report's meta.codegenCompare.mode (SOUND or SOUND-exact-only).
# The stamp is a SEPARATE artifact (template-diag-summary.mjs never rewrites the report). Refuse when a stamp is
# missing, unparseable, carries an UNSOUND label, does not match the derived mode label, or names other report bytes.
[[ "${OFFICIAL_STYLE_PARSE_ERRORS:-}" =~ ^[0-9]+/[1-9][0-9]*$ ]] || { echo "!! PUBLISH REFUSED: OFFICIAL_STYLE_PARSE_ERRORS='${OFFICIAL_STYLE_PARSE_ERRORS:-}' (want <passed>/<total>)" >&2; exit 5; }
tdsound=""
for j in "$base.template-diag.json" "$base.template-diag-napi.json"; do
  st="${j%.json}.soundness.json"
  [ -s "$st" ] || { echo "!! PUBLISH REFUSED: soundness stamp missing: $st" >&2; exit 5; }
  got="$(node -e '
const fs=require("fs"),c=require("crypto"),[st,j]=process.argv.slice(1);
const map={"exact-bytes":"SOUND-exact-only","exact-bytes+semdiff":"SOUND","exact-bytes+semdiff-exec":"SOUND"};
let s,r; try{s=JSON.parse(fs.readFileSync(st,"utf8")); r=JSON.parse(fs.readFileSync(j,"utf8"));}catch(e){process.exit(2)}
const sha=c.createHash("sha256").update(fs.readFileSync(j)).digest("hex");
const want=map[r.meta?.codegenCompare?.mode];
if(r.schema!=="template-diag-diff/v2"||!want||s.schema!=="template-diag-soundness/v1"||s.soundness!==want||String(s.soundness).startsWith("UNSOUND")||s.report_sha256!==sha) process.exit(1);
process.stdout.write(want);
' "$st" "$j")" || { echo "!! PUBLISH REFUSED: $st is not a valid template-diag-soundness/v1 stamp whose soundness matches meta.codegenCompare.mode for the bytes of $j" >&2; exit 5; }
  [ -z "$tdsound" ] && tdsound="$got"
  [ "$got" = "$tdsound" ] || { echo "!! PUBLISH REFUSED: soundness mismatch between reports: $tdsound vs $got" >&2; exit 5; }
done
mkdir -p "$store"
stage="$(mktemp -d "/tmp/publish-$src.XXXXXX")"; trap 'rm -rf "$stage"' EXIT
cp "$base.tgz" "$stage/official-$src.tgz"; cp "$base.board.txt" "$stage/board.txt"
cp "$base.template-diag.json" "$stage/template-diag.json"; cp "$base.template-diag-napi.json" "$stage/template-diag-napi.json"
cp "$base.template-diag.soundness.json" "$stage/template-diag.soundness.json"; cp "$base.template-diag-napi.soundness.json" "$stage/template-diag-napi.soundness.json"
csr=(); for f in "$base".csr-*.tsv; do [ -f "$f" ] || continue; n="${f##*/official-$src.}"; cp "$f" "$stage/$n" || { echo "!! PUBLISH FAILED: cp $f" >&2; exit 5; }; csr+=("$n"); done
# Strict key=value manifest (AB-X0004): no free text in any value; comments go in keys, never after values.
cat > "$stage/OFFICIAL_BOARD.txt" << META
status=VALID
producer=scripts/official-board.sh
oracle_repo=${OFFICIAL_ORACLE_REPO:?}
oracle_commit=${OFFICIAL_ORACLE_COMMIT:?}
oracle_clean=true
gates=golden-gate,shim-gate,option-gate,snapshot-unchanged,board-rc-0
src_snapshot_sha256_12=$src
commit=$OFFICIAL_COMMIT
commit_repo=lwc-compiler-rust@main
parse5_rust_commit=${OFFICIAL_PARSE5_COMMIT:?}
parse5_rust_clone_head=${OFFICIAL_PARSE5_CLONE_HEAD:?}
notes_commit=${OFFICIAL_NOTES_COMMIT:?}
external_inputs_sha256_12=${OFFICIAL_INPUTS_SHA:?}
external_inputs_lock=harness/external-inputs.lock
napi_sha256_12=$napi
parse5_rust_sha256_12=$p5
tgz_sha256=$(sha256sum "$stage/official-$src.tgz" | cut -d' ' -f1)
board_txt_sha256=$(sha256sum "$stage/board.txt" | cut -d' ' -f1)
template_diag_json_sha256=$(sha256sum "$stage/template-diag.json" | cut -d' ' -f1)
template_diag_napi_json_sha256=$(sha256sum "$stage/template-diag-napi.json" | cut -d' ' -f1)
template_diag_soundness_json_sha256=$(sha256sum "$stage/template-diag.soundness.json" | cut -d' ' -f1)
template_diag_napi_soundness_json_sha256=$(sha256sum "$stage/template-diag-napi.soundness.json" | cut -d' ' -f1)
template_diag_napi_authoritative=false
template_diag_soundness=$tdsound
style_parse_errors=$OFFICIAL_STYLE_PARSE_ERRORS
published=$(date -Iseconds)
META
if bad="$(awk '!/^[a-z0-9_]+=[^ ]+$/ || /^[a-z0-9_]+=(unknown|\?)$/ {print NR": "$0; b=1} END {exit b ? 0 : 1}' "$stage/OFFICIAL_BOARD.txt")"; then
  echo "!! PUBLISH FAILED: OFFICIAL_BOARD.txt has non key=value or placeholder (unknown/?) line(s): $bad" >&2; exit 5
elif [ $? -ne 1 ]; then echo "!! PUBLISH FAILED: manifest check could not run" >&2; exit 5; fi
if [ -d "$store/$src" ]; then
  if [ -e "$store/$src/official-$src.tgz" ] || [ -e "$store/$src/OFFICIAL_BOARD.txt" ]; then
    if [ -f "$store/$src/official-$src.tgz" ] && cmp -s "$store/$src/official-$src.tgz" "$stage/official-$src.tgz"; then
      echo "store: $store/$src already published (identical)"; exit 0
    fi
    echo "!! PUBLISH REFUSED: $store/$src exists with different bytes" >&2; exit 5
  fi
fi
mkdir -p "$store/$src"
cp "$stage/official-$src.tgz" "$stage/board.txt" "$stage/template-diag.json" "$stage/template-diag-napi.json" "$stage/template-diag.soundness.json" "$stage/template-diag-napi.soundness.json" "$store/$src/" \
  || { echo "!! PUBLISH FAILED: store copy" >&2; exit 5; }
for n in "${csr[@]}"; do cp "$stage/$n" "$store/$src/$n" && cmp -s "$stage/$n" "$store/$src/$n" || { echo "!! PUBLISH FAILED: store copy $n" >&2; exit 5; }; done
cmp -s "$stage/official-$src.tgz" "$store/$src/official-$src.tgz" && cmp -s "$stage/board.txt" "$store/$src/board.txt" \
  && cmp -s "$stage/template-diag.json" "$store/$src/template-diag.json" && cmp -s "$stage/template-diag-napi.json" "$store/$src/template-diag-napi.json" \
  && cmp -s "$stage/template-diag.soundness.json" "$store/$src/template-diag.soundness.json" && cmp -s "$stage/template-diag-napi.soundness.json" "$store/$src/template-diag-napi.soundness.json" \
  || { echo "!! PUBLISH FAILED: store copy mismatch" >&2; exit 5; }
cp "$stage/OFFICIAL_BOARD.txt" "$store/$src/OFFICIAL_BOARD.txt"   # marker last
echo "published: $store/$src"
