#!/usr/bin/env bash
# Plants for harness/scripts/semantic-diff-row.sh (PLAN §3 item 4).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ROW="$ROOT/harness/scripts/semantic-diff-row.sh"
T="$(mktemp -d /tmp/test-semantic-diff-row.XXXXXX)"; trap 'rm -rf "$T"' EXIT
fail=0
check() {
  local name="$1" want="$2"; shift 2
  local rc=0; "$@" >"$T/out" 2>"$T/err" || rc=$?
  if [ "$rc" = "$want" ]; then
    echo "ok   $name -> rc $rc"
  else
    echo "FAIL $name: want rc $want got $rc"; sed 's/^/  | stdout: /' "$T/out"; sed 's/^/  | stderr: /' "$T/err"; fail=1
  fi
}

# Tiny valid manifest (schema + corpus + cases). Pair files are not needed: we mock semdiff.
write_manifest() {
  local corpus="$1" n="$2" dest="$3" i
  {
    printf '%s\n' '{'
    printf '%s\n' ' "schema": "semdiff-manifest/v0",'
    printf '%s\n' " \"corpus\": \"${corpus}\","
    printf '%s\n' ' "cases": ['
    i=0
    while [ "$i" -lt "$n" ]; do
      [ "$i" -eq 0 ] || printf ',\n'
      printf '  {"id":"c%s","oracle":"o.js","rust":"r.js"}' "$i"
      i=$((i + 1))
    done
    printf '\n ]\n}\n'
  } > "$dest"
}

# Mock semdiff: write a v2 summary whose totals.cases matches the manifest, exit 0 unless SEMDIFF_RC is set.
cat > "$T/fake-semdiff" << 'MOCK'
#!/usr/bin/env bash
set -euo pipefail
out="" manifest=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --manifest) manifest="$2"; shift 2 ;;
    --registry|--gate) shift 2 ;;
    run) shift ;;
    *) shift ;;
  esac
done
n="$(node -e 'const m=require(process.argv[1]); process.stdout.write(String(m.cases.length))' "$manifest")"
mkdir -p "$(dirname "$out")"
node -e '
const fs = require("fs");
const n = Number(process.argv[1]);
const s = {
  schema: "semdiff-summary/v2",
  corpus: "template-diag-codegen-only",
  rule_table_version: "rt-test",
  totals: { cases: n, equivalent: n, different_undecided: 0, different_decided: 0, unclassified: 0, crash: 0 },
  cases: [],
};
fs.writeFileSync(process.argv[2], JSON.stringify(s));
' "$n" "$out"
exit "${SEMDIFF_RC:-0}"
MOCK
chmod +x "$T/fake-semdiff"

mkdir -p "$T/ok"
write_manifest template-diag-codegen-only 3 "$T/ok/manifest.json"
printf '%s\n' 'D-0000	x' > "$T/ok/registry.tsv"

check "happy path" 0 \
  "$ROW" --manifest "$T/ok/manifest.json" --registry "$T/ok/registry.tsv" \
    --expect-corpus template-diag-codegen-only --expect-cases 3 \
    --semdiff "$T/fake-semdiff" --out "$T/ok/summary.json"

write_manifest template-diag-codegen-only 2 "$T/ok/wrong-count.json"
check "wrong case count fails" 2 \
  "$ROW" --manifest "$T/ok/wrong-count.json" --registry "$T/ok/registry.tsv" \
    --expect-corpus template-diag-codegen-only --expect-cases 3 \
    --semdiff "$T/fake-semdiff" --out "$T/ok/summary-wc.json"

write_manifest template-diag-codegen-only-napi 3 "$T/ok/wrong-corpus.json"
check "wrong corpus fails" 2 \
  "$ROW" --manifest "$T/ok/wrong-corpus.json" --registry "$T/ok/registry.tsv" \
    --expect-corpus template-diag-codegen-only --expect-cases 3 \
    --semdiff "$T/fake-semdiff" --out "$T/ok/summary-wco.json"

check "missing registry fails" 2 \
  "$ROW" --manifest "$T/ok/manifest.json" --registry "$T/ok/no-such-registry.tsv" \
    --expect-corpus template-diag-codegen-only --expect-cases 3 \
    --semdiff "$T/fake-semdiff" --out "$T/ok/summary-mr.json"

# The helper must refuse a baseline flag (row is strict, no baseline).
check "baseline flag is refused" 2 \
  "$ROW" --baseline "$T/ok/summary.json" --manifest "$T/ok/manifest.json" --registry "$T/ok/registry.tsv" \
    --expect-corpus template-diag-codegen-only --expect-cases 3 \
    --semdiff "$T/fake-semdiff" --out "$T/ok/summary-bl.json"

exit $fail
