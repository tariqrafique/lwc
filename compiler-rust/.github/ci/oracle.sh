#!/usr/bin/env bash
# The pinned @lwc oracle for CI: salesforce/lwc @ $LWC_SHA at $LWC_ROOT (tests hardcode /workspace/lwc), then
# `yarn install` and the build command given as arguments (e.g. `yarn build`, or `npx nx run @lwc/ssr-compiler:build`).
set -euo pipefail
: "${LWC_SHA:?}" "${LWC_ROOT:?}"
[ $# -gt 0 ] || { echo "usage: oracle.sh <build command...>" >&2; exit 1; }
parent="$(dirname "$LWC_ROOT")"
if [ ! -d "$parent" ] || [ ! -w "$parent" ]; then sudo mkdir -p "$parent"; sudo chown "$(id -u):$(id -g)" "$parent"; fi
git init -q "$LWC_ROOT"
git -C "$LWC_ROOT" fetch -q --depth 1 https://github.com/salesforce/lwc "$LWC_SHA"
git -C "$LWC_ROOT" checkout -q --detach FETCH_HEAD
[ "$(git -C "$LWC_ROOT" rev-parse HEAD)" = "$LWC_SHA" ] || { echo "oracle.sh: HEAD != $LWC_SHA" >&2; exit 1; }
cd "$LWC_ROOT"
export NX_DAEMON=false NX_NO_CLOUD=true
# yarn per package.json packageManager (yarn@1.22.22) without `corepack enable`, which writes next to the node binary.
yarn() { corepack yarn "$@"; }
yarn install --frozen-lockfile --ignore-scripts
"$@"
