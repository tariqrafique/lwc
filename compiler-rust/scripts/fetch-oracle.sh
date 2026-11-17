#!/usr/bin/env bash
# Fetch salesforce/lwc at the pinned sha (harness/oracle.pin) into a dest that is
# NEVER /workspace/lwc (D-0280). Verifies HEAD == pin.
# Usage: scripts/fetch-oracle.sh [--build] [dest]
#   dest default: ${LWC_ORACLE:-<repo>/.oracle/lwc}
#   --build: yarn install --frozen-lockfile --ignore-scripts && yarn build
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=../harness/scripts/oracle-root.sh
source "$root/harness/scripts/oracle-root.sh"
pin="$(oracle_pin "$root/harness/oracle.pin")" || exit 2
build=0 dest=""
for a in "$@"; do
  case "$a" in
    --build) build=1 ;;
    -*) echo "usage: fetch-oracle.sh [--build] [dest]" >&2; exit 2 ;;
    *) [ -z "$dest" ] || { echo "usage: fetch-oracle.sh [--build] [dest]" >&2; exit 2; }; dest="$a" ;;
  esac
done
dest="${dest:-${LWC_ORACLE:-$root/.oracle/lwc}}"
abs="$(realpath -m "$dest")"
case "$abs" in
  /workspace/lwc|/workspace/lwc/*)
    echo "fetch-oracle: refused: dest $dest resolves to $abs; this script must never touch /workspace/lwc" >&2
    exit 2
    ;;
esac
url="${LWC_ORACLE_URL:-https://github.com/salesforce/lwc}"
mkdir -p "$(dirname "$dest")"
if [ -d "$dest/.git" ]; then
  git -C "$dest" fetch -q --depth 1 "$url" "$pin"
  git -C "$dest" checkout -q --detach FETCH_HEAD
else
  mkdir -p "$dest"
  git init -q "$dest"
  git -C "$dest" fetch -q --depth 1 "$url" "$pin"
  git -C "$dest" checkout -q --detach FETCH_HEAD
fi
head="$(git -C "$dest" rev-parse HEAD)"
[ "$head" = "$pin" ] || { echo "fetch-oracle: HEAD=$head want $pin" >&2; exit 1; }
if [ "$build" -eq 1 ]; then
  cd "$dest"
  export NX_DAEMON=false NX_NO_CLOUD=true
  # The box (and some images) export COREPACK_HOME=/usr/local/share/corepack, which is not writable.
  _cph="${COREPACK_HOME:-}"
  if [ -n "$_cph" ] && mkdir -p "$_cph" 2>/dev/null && [ -w "$_cph" ]; then
    export COREPACK_HOME="$_cph"
  else
    export COREPACK_HOME="${XDG_CACHE_HOME:-${HOME:-/tmp}/.cache}/corepack"
    mkdir -p "$COREPACK_HOME" || { echo "fetch-oracle: cannot create COREPACK_HOME=$COREPACK_HOME" >&2; exit 2; }
  fi
  yarn() { corepack yarn "$@"; }
  yarn install --frozen-lockfile --ignore-scripts
  yarn build
fi
echo "fetch-oracle: $abs @ $pin${build:+ (built)}"
