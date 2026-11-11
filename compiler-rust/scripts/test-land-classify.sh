#!/usr/bin/env bash
# Compatibility entry: every mapped land self-test except those that need a
# napi binary or a shared rust cache (test-board-ratchet, test-shared-cargo-cache).
# land.sh selects tests from scripts/land-self-tests.tsv instead of always
# running this bundle.
set -euo pipefail
SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$SELF_DIR/run-land-self-tests.sh" --all
