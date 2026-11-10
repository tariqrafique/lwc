#!/usr/bin/env bash
# Clippy shrink-only ratchet (AB-D0005 #6, CUTOVER C8). Counts distinct warning/error diagnostics
# (level, lint code, primary span, message; a lib+test target emitting the same one counts once) from
# `cargo clippy --workspace --all-targets --locked --message-format=json` and compares to the committed baseline.
# count > baseline: fail (new warnings). count < baseline: fail with "lower the baseline to N" (the ratchet only moves down).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
base_file="$root/.github/ci/clippy-baseline.txt"
cd "$root"
json="$(mktemp)"; trap 'rm -f "$json"' EXIT
cargo clippy --workspace --all-targets --locked --message-format=json > "$json"
n="$(jq -r 'select(.reason=="compiler-message") | .message | select(.level=="warning" or .level=="error")
  | [.level, (.code.code // ""), ((.spans[] | select(.is_primary) | "\(.file_name):\(.line_start):\(.column_start)") // ""), .message]
  | @json' "$json" | sort -u | wc -l)"
baseline="$(tr -d '[:space:]' < "$base_file")"
echo "clippy: count=$n baseline=$baseline"
if [ "$n" -gt "$baseline" ]; then
  echo "clippy RATCHET-FAIL: $n warnings > baseline $baseline; fix the new ones" >&2
  jq -r 'select(.reason=="compiler-message") | .message | select(.level=="warning" or .level=="error") | .rendered' "$json" | head -c 20000 >&2 || true
  exit 1
fi
if [ "$n" -lt "$baseline" ]; then
  echo "clippy RATCHET-FAIL: lower the baseline to $n (.github/ci/clippy-baseline.txt) in the same commit" >&2
  exit 1
fi
