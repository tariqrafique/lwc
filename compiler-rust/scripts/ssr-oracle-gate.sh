#!/usr/bin/env bash
# D-0226 CI half: tools/ssr-compile-js-oracle/run.mjs against the napi built and packed by napi-pack.sh.
# The .node is taken from the per-triple tarball and passed with --napi=<path> (run.mjs since 6ddb2d3
# defaults to target/release/liblwc_compiler_napi.so), so the gate tests the packed artifact. SSR_ORACLE_NAPI is never used.
# LWC_ROOT: the oracle checkout at b9e8007 with @lwc/ssr-compiler built.
set -euo pipefail
root="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
tgz_dir="${1:?usage: ssr-oracle-gate.sh <tarball-dir> [triple]}"
triple="${2:-linux-x64-gnu}"
unset SSR_ORACLE_NAPI
tgz="$(ls "$tgz_dir"/lwc-compiler-rs-"$triple"-*.tgz)"
x="$(mktemp -d)"; trap 'rm -rf "$x"' EXIT
tar -xzf "$tgz" -C "$x"
napi="$x/package/lwc-compiler.$triple.node"
LWC_ROOT="${LWC_ROOT:-${LWC_ORACLE:-/workspace/lwc}}"
export LWC_ROOT LWC_ORACLE="${LWC_ORACLE:-$LWC_ROOT}"
echo "ssr-oracle-gate: $(sha256sum "$napi" | cut -c1-12) from $(basename "$tgz"), LWC_ROOT=$LWC_ROOT"
args=(--napi="$napi")
# Tested tree (official-board snapshot) may differ from this script's tree.
if [ -n "${SSR_ORACLE_ROOT:-}" ]; then
  args+=(--root="$SSR_ORACLE_ROOT")
fi
if [ -n "${SSR_ORACLE_MAIN:-}" ]; then
  args+=(--main="$SSR_ORACLE_MAIN")
fi
node "$root/tools/ssr-compile-js-oracle/run.mjs" "${args[@]}"
