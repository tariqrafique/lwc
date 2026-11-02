#!/usr/bin/env bash
# Idempotently (re)create the harness's untracked deps and fail loudly if the oracle build is missing.
# These are hand links, not an install: they vanished in the 2026-11-01 ~20:28 box restore.
set -euo pipefail
h="$(cd "$(dirname "$0")/.." && pwd)"; ORACLE=/workspace/lwc
mkdir -p "$h/node_modules/@lwc"
ln -sfn "$ORACLE/packages/@lwc/compiler" "$h/node_modules/@lwc/compiler"
ln -sfn "$ORACLE/node_modules/vitest" "$h/node_modules/vitest"
miss=()
[ -x "$ORACLE/node_modules/.bin/vitest" ] || miss+=("$ORACLE/node_modules/.bin/vitest (run yarn install in $ORACLE)")
for p in compiler engine-dom engine-server ssr-runtime; do
  [ -d "$ORACLE/packages/@lwc/$p/dist" ] || miss+=("$ORACLE/packages/@lwc/$p/dist (run yarn build in $ORACLE)"); done
[ ${#miss[@]} -eq 0 ] || { printf '!! HARNESS DEPS MISSING: %s\n' "${miss[@]}" >&2; exit 2; }
