#!/usr/bin/env bash
# D-0235: unformatted Rust is a hard failure. One check, run from scripts/land-path-checks.tsv, so it
# gates both the land (preflight row path:cargo-fmt) and the board (c8-checks.sh -> land-path-checks).
# Usage: scripts/cargo-fmt-check.sh <repo-root>
# Exit: 0 formatted; 1 unformatted .rs (files listed; fix with `cargo_locked fmt --all`); 2 cannot run.
set -euo pipefail
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
# shellcheck source=cargo-locked.sh
source "$here/cargo-locked.sh"
[ $# -eq 1 ] || { echo "usage: cargo-fmt-check.sh <repo-root>" >&2; exit 2; }
root="$(cd "$1" && pwd)"
[ -f "$root/Cargo.toml" ] || { echo "!! cargo-fmt-check: no Cargo.toml in $root" >&2; exit 2; }
out="$(mktemp /tmp/cargo-fmt-check.XXXXXX)"
trap 'rm -f "$out"' EXIT
rc=0
(cd "$root" && cargo_locked fmt --all --check) > "$out" 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then echo "cargo-fmt-check: ok"; exit 0; fi
files="$(sed -nE 's/^Diff in (.+\.rs):[0-9]+:.*$/\1/p' "$out" | sed "s|^$root/||" | sort -u)"
if [ -z "$files" ]; then
  echo "!! cargo-fmt-check: cargo_locked fmt --all --check could not run (rc=$rc)" >&2
  head -n 40 "$out" | sed 's/^/  /' >&2
  exit 2
fi
n="$(wc -l <<< "$files")"
echo "!! cargo-fmt-check: unformatted Rust in $n file(s), fix with cargo_locked fmt --all: $(head -n 1 <<< "$files")" >&2
sed 's/^/  /' <<< "$files" >&2
head -60 "$out" | sed 's/^/  /' >&2
exit 1
