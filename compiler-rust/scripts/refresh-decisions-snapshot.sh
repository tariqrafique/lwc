#!/usr/bin/env bash
# Copy notes lwc-rust-migrate/decisions.tsv into the repo-committed snapshot (D-0280).
# Box-only: needs a notes checkout. Cloud agents use the committed snapshot as-is.
# The snapshot is taken from git (HEAD:path), never from the working tree.
# Usage: scripts/refresh-decisions-snapshot.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
NOTES="${NOTES_DIR:-/workspace/notes}"
nhead="$(git -C "$NOTES" rev-parse --verify -q 'HEAD^{commit}')" \
  || { echo "refresh-decisions-snapshot: cannot resolve $NOTES HEAD" >&2; exit 2; }
if ! git -C "$NOTES" cat-file -e "$nhead:lwc-rust-migrate/decisions.tsv"; then
  echo "refresh-decisions-snapshot: $NOTES has no HEAD:lwc-rust-migrate/decisions.tsv" >&2
  exit 2
fi
git -C "$NOTES" show "HEAD:lwc-rust-migrate/decisions.tsv" > "$root/harness/decisions.snapshot.tsv"
printf '%s\n' "$nhead" > "$root/harness/decisions.snapshot.notes-commit"
echo "refresh-decisions-snapshot: wrote harness/decisions.snapshot.tsv from $NOTES HEAD:lwc-rust-migrate/decisions.tsv (notes@$nhead)"
