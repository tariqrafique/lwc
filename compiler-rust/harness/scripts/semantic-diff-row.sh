#!/usr/bin/env bash
# PLAN §3 item 4 / Arch 03112c1: the official semantic-diff row.
#   semdiff run --manifest M --out S --registry R --gate strict
#   No baseline. Registry is decisions.tsv (caller passes notes_decisions_path /
#   NOTES_COMMIT archive / snapshot). The row asserts the producer manifest's
#   corpus and case count before invoking semdiff (strict itself does not).
# Usage:
#   semantic-diff-row.sh --manifest M --registry R --expect-corpus C \
#     --expect-cases N --semdiff BIN --out SUMMARY
# Exit 0: asserts passed and --gate strict passed (row printed on stdout).
# Exit 1: ran; summary written; gate failed (row printed, gate=FAIL).
# Exit 2: internal (missing registry/manifest/bin, corpus/count mismatch).
set -euo pipefail
manifest="" registry="" expect_corpus="" expect_cases="" semdiff="" out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --manifest) manifest="${2:-}"; shift 2 ;;
    --registry) registry="${2:-}"; shift 2 ;;
    --expect-corpus) expect_corpus="${2:-}"; shift 2 ;;
    --expect-cases) expect_cases="${2:-}"; shift 2 ;;
    --semdiff) semdiff="${2:-}"; shift 2 ;;
    --out) out="${2:-}"; shift 2 ;;
    --baseline|--gate) echo "semantic-diff-row: '$1' is not accepted (row is --gate strict, no baseline)" >&2; exit 2 ;;
    *) echo "usage: semantic-diff-row.sh --manifest M --registry R --expect-corpus C --expect-cases N --semdiff BIN --out SUMMARY" >&2; exit 2 ;;
  esac
done
[ -n "$manifest" ] && [ -n "$registry" ] && [ -n "$expect_corpus" ] && [ -n "$expect_cases" ] && [ -n "$semdiff" ] && [ -n "$out" ] \
  || { echo "usage: semantic-diff-row.sh --manifest M --registry R --expect-corpus C --expect-cases N --semdiff BIN --out SUMMARY" >&2; exit 2; }
[[ "$expect_cases" =~ ^[0-9]+$ ]] || { echo "semantic-diff-row: --expect-cases must be an integer, got '$expect_cases'" >&2; exit 2; }
[ -f "$registry" ] || { echo "semantic-diff-row: missing registry: $registry" >&2; exit 2; }
[ -f "$manifest" ] || { echo "semantic-diff-row: missing manifest: $manifest" >&2; exit 2; }
[ -x "$semdiff" ] || { echo "semantic-diff-row: missing semdiff binary: $semdiff" >&2; exit 2; }

got="$(node -e '
const fs = require("fs");
const m = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
if (m.schema !== "semdiff-manifest/v0") {
  console.error("semantic-diff-row: manifest schema " + m.schema + " (want semdiff-manifest/v0)");
  process.exit(2);
}
if (typeof m.corpus !== "string" || !Array.isArray(m.cases)) {
  console.error("semantic-diff-row: manifest missing corpus or cases[]");
  process.exit(2);
}
process.stdout.write(m.corpus + "\t" + String(m.cases.length));
' "$manifest")" || exit 2
got_corpus="${got%%	*}"
got_cases="${got#*	}"
if [ "$got_corpus" != "$expect_corpus" ]; then
  echo "semantic-diff-row: corpus mismatch: manifest '$got_corpus' want '$expect_corpus'" >&2
  exit 2
fi
if [ "$got_cases" != "$expect_cases" ]; then
  echo "semantic-diff-row: case-count mismatch: manifest $got_cases want $expect_cases" >&2
  exit 2
fi

mkdir -p "$(dirname "$out")"
set +e
"$semdiff" run --manifest "$manifest" --out "$out" --registry "$registry" --gate strict > "${out}.stdout" 2> "${out}.stderr"
sdrc=$?
set -e
if [ "$sdrc" -ne 0 ]; then
  [ -s "${out}.stderr" ] && cat "${out}.stderr" >&2
  [ -s "${out}.stdout" ] && cat "${out}.stdout" >&2
fi
if [ "$sdrc" -eq 2 ] || [ ! -f "$out" ]; then
  echo "semantic-diff-row: semdiff rc=$sdrc (internal; no usable summary)" >&2
  echo "semantic-diff  eq=? diff_u=? diff_d=? unc=? crash=?  gate=FAIL  rt=?"
  exit 2
fi
line="$(node -e '
const fs = require("fs");
const s = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
if (!["semdiff-summary/v1", "semdiff-summary/v2"].includes(s.schema)) {
  console.error("semantic-diff-row: summary schema " + s.schema);
  process.exit(2);
}
const t = s.totals || {};
for (const k of ["equivalent", "different_undecided", "different_decided", "unclassified", "crash"]) {
  if (typeof t[k] !== "number") { console.error("semantic-diff-row: totals." + k + " missing"); process.exit(2); }
}
const gate = process.argv[2] === "0" ? "ok" : "FAIL";
const rt = s.rule_table_version || "?";
process.stdout.write(
  "semantic-diff  eq=" + t.equivalent +
  " diff_u=" + t.different_undecided +
  " diff_d=" + t.different_decided +
  " unc=" + t.unclassified +
  " crash=" + t.crash +
  "  gate=" + gate +
  "  rt=" + rt
);
' "$out" "$sdrc")" || exit 2
printf '%s\n' "$line"
[ "$sdrc" -eq 0 ] || exit 1
exit 0
