#!/usr/bin/env bash
# Clippy shrink-only ratchet (AB-D0005 #6, CUTOVER C8). Counts distinct warning/error diagnostics
# (level, lint code, primary span, message; a lib+test target emitting the same one counts once) from
# `cargo clippy --workspace --all-targets --locked --message-format=json` and compares to the committed baseline.
# count > baseline: fail (new warnings). count < baseline: fail with "lower the baseline to N" (the ratchet only moves down).
set -euo pipefail
runner="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
if [ -n "${1:-}" ]; then
  root="$(cd "$1" && pwd)"
else
  root="$(cd "$runner/.." && pwd)"
fi
# shellcheck source=cargo-locked.sh
source "$runner/cargo-locked.sh"
base_file="$root/scripts/clippy-baseline.txt"
cd "$root"
# Fail closed unless rustc/clippy are exactly the rust-toolchain.toml channel
# (so an env -i / rustup-default drift cannot silently change the count).
toml="$root/rust-toolchain.toml"
[ -f "$toml" ] || { echo "clippy-ratchet: missing $toml" >&2; exit 2; }
pin="$(sed -nE 's/^channel[[:space:]]*=[[:space:]]*"([^"]+)"/\1/p' "$toml")"
[ "$(wc -l <<< "$pin")" -eq 1 ] && [ -n "$pin" ] || { echo "clippy-ratchet: no channel in $toml" >&2; exit 2; }
rel="$(rustup run "$pin" rustc -vV | sed -n 's/^release: //p')"
[ "$rel" = "$pin" ] || { echo "clippy-ratchet: rustc release '$rel' != rust-toolchain channel '$pin'" >&2; exit 2; }
echo "clippy-ratchet: toolchain=$pin rustc=$(rustup run "$pin" rustc --version) clippy=$(cargo_locked "+${pin}" clippy -V)"
json="$(mktemp)"; trap 'rm -f "$json"' EXIT
cargo_locked "+${pin}" clippy --workspace --all-targets --message-format=json > "$json"
n="$(jq -r 'select(.reason=="compiler-message") | .message | select(.level=="warning" or .level=="error")
  | [.level, (.code.code // ""), ((.spans[] | select(.is_primary) | "\(.file_name):\(.line_start):\(.column_start)") // ""), .message]
  | @json' "$json" | LC_ALL=C sort -u | awk 'END{print NR+0}')"
baseline="$(awk 'NF{print; exit}' "$base_file")"
[[ "$baseline" =~ ^[0-9]+$ ]] || { echo "clippy-ratchet: baseline is not an integer: '$baseline'" >&2; exit 2; }
echo "clippy: count=$n baseline=$baseline"
if [ "$n" -gt "$baseline" ]; then
  echo "clippy RATCHET-FAIL: $n warnings > baseline $baseline; fix the new ones" >&2
  jq -r 'select(.reason=="compiler-message") | .message | select(.level=="warning" or .level=="error") | .rendered' "$json" > "$json.rendered" || true
  head -c 20000 "$json.rendered" >&2 || true
  rm -f "$json.rendered"
  exit 1
fi
if [ "$n" -lt "$baseline" ]; then
  echo "clippy RATCHET-FAIL: lower the baseline to $n (scripts/clippy-baseline.txt) in the same commit" >&2
  exit 1
fi
