#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=oracle-root.sh
source "$(cd "$(dirname "$0")" && pwd)/oracle-root.sh"
o="$(oracle_root)" || exit 2
mkdir -p node_modules/@lwc
ln -sfn "$o/packages/@lwc/compiler" node_modules/@lwc/compiler
ln -sfn "$o/node_modules/vitest" node_modules/vitest
echo "link:lwc -> $o"
