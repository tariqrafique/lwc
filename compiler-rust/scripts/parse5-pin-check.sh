#!/usr/bin/env bash
# parse5 pin identity gate (D-0201/D-0218 P1; D-0238 lock source rev).
# Usage: parse5-pin-check.sh <repo-root>
# If <root>/crates/lwc-template/Cargo.toml pins parse5 by git rev, prove:
#   1. the rev exists in $PARSE5_DIR if set to a clone (presence only; HEAD/working tree
#      may differ (cargo builds the lock's git source, not whatever is sitting in that directory);
#      PARSE5_DIR unset means /workspace/parse5-rust and fails closed if that directory is
#      absent; PARSE5_DIR=remote fetches the rev from the public Cargo.toml git URL);
#   2. Cargo.lock and tools/template-diag-diff/driver/Cargo.lock (both required; missing = rc 2)
#      records parse5 source = "git+<Cargo.toml git URL>?rev=<Cargo.toml rev>#...".
# After D-0201 a plain path dep is drift (the pin was removed): exit 1 (AB-L0050 condition 1).
# Exit: 0 ok | 1 mismatch (drift) | 2 usage/internal error. Never prints a pass it did not prove.
set -euo pipefail
[ $# -eq 1 ] && [ -n "$1" ] || { echo "usage: parse5-pin-check.sh <repo-root>" >&2; exit 2; }
root="$1"
if [ "${PARSE5_DIR:-}" = remote ]; then
  P5=""
elif [ -n "${PARSE5_DIR:-}" ]; then
  P5="$PARSE5_DIR"
else
  if [ ! -d /workspace/parse5-rust ]; then
    echo "parse5-pin: PARSE5_DIR unset and /workspace/parse5-rust is absent" >&2
    exit 2
  fi
  P5=/workspace/parse5-rust
fi
toml="$root/crates/lwc-template/Cargo.toml"; [ -f "$toml" ] || { echo "parse5-pin: no $toml" >&2; exit 2; }
line="$(grep -E '^parse5[[:space:]]*=' "$toml")" || { echo "parse5-pin: no parse5 dep line in $toml" >&2; exit 2; }
[ "$(wc -l <<< "$line")" -eq 1 ] || { echo "parse5-pin: multiple parse5 dep lines" >&2; exit 2; }
if ! grep -q 'git[[:space:]]*=' <<< "$line"; then echo "parse5-pin: DRIFT path dep, not a git rev pin (D-0201): $line" >&2; exit 1; fi
rev="$(sed -nE 's/.*rev[[:space:]]*=[[:space:]]*"([0-9a-f]{40})".*/\1/p' <<< "$line")"
[ -n "$rev" ] || { echo "parse5-pin: git dep without a full 40-hex rev: $line" >&2; exit 1; }
url="$(sed -nE 's/.*git[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' <<< "$line")"
[ -n "$url" ] || { echo "parse5-pin: git dep without a git URL: $line" >&2; exit 2; }
if [ -n "$P5" ]; then
  git -C "$P5" rev-parse -q --verify "$rev^{commit}" >/dev/null || { echo "parse5-pin: rev $rev not present in $P5" >&2; exit 1; }
  p5_where="$P5"
else
  # GitHub does not advertise every commit via ls-remote; fetch the sha into a throwaway.
  _tmp="$(mktemp -d)"
  git init -q "$_tmp"
  if git -C "$_tmp" fetch -q --depth 1 "$url" "$rev"      && [ "$(git -C "$_tmp" rev-parse FETCH_HEAD)" = "$rev" ]; then
    p5_where="$url"
    rm -rf "$_tmp"
  else
    rm -rf "$_tmp"
    echo "parse5-pin: rev $rev not fetchable from $url (PARSE5_DIR=remote)" >&2
    exit 1
  fi
fi

lock_source() {
  awk '
    $0 == "[[package]]" { if (in_pkg && !saw) exit; in_pkg=0; saw=0 }
    /^name = "parse5"$/ { in_pkg=1 }
    in_pkg && /^source = / { print; saw=1; exit }
  ' "$1"
}

check_lock() {
  local lock="$1" src want
  [ -f "$lock" ] || { echo "parse5-pin: no $lock" >&2; exit 2; }
  src="$(lock_source "$lock")"
  [ -n "$src" ] || { echo "parse5-pin: DRIFT $lock parse5 has no git source (path / [patch] lock)" >&2; exit 1; }
  want="source = \"git+${url}?rev=${rev}#"
  [[ "$src" == "$want"* ]] || { echo "parse5-pin: DRIFT $lock $src != expected ${want}...\"" >&2; exit 1; }
}

check_lock "$root/Cargo.lock"
# The driver lock is REQUIRED while the driver has its own lock (D-0251 deletes it and this line together):
# a missing lock is rc 2, never a skip (AB-D0219-r4 merge condition: the D-0274 trusted gate has no fail-open path).
check_lock "$root/tools/template-diag-diff/driver/Cargo.lock"
echo "parse5-pin: ok rev ${rev:0:12} lock source matches Cargo.toml, present in $p5_where"
