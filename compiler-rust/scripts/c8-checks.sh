#!/usr/bin/env bash
# D-0280 C8 checks, in-repo, no GitHub Actions. All cargo goes through cargo_locked
# except napi-pack.sh (which already passes --cargo-flags=--locked and then asserts
# Cargo.lock is unchanged).
# Usage: scripts/c8-checks.sh [--skip-pack] [repo-root] [tarball-out]
#   cargo fmt --all --check (D-0235) is the land-path-checks row cargo-fmt below: a hard failure.
# C8 helper scripts (clippy-ratchet, ssr-oracle-gate, napi-pack) come from this
# file's tree (the runner). Cargo/clippy/pack run against [repo-root].
set -euo pipefail
runner="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
root="$(cd "$runner/.." && pwd)"
skip_pack=0
args=()
for a in "$@"; do
  case "$a" in
    --skip-pack) skip_pack=1 ;;
    --only-extra) echo "c8-checks.sh: --only-extra is gone; the shared driver always runs the full C8 set" >&2; exit 2 ;;
    -*) echo "usage: c8-checks.sh [--skip-pack] [repo-root] [tarball-out]" >&2; exit 2 ;;
    *) args+=("$a") ;;
  esac
done
[ ${#args[@]} -gt 0 ] && root="$(cd "${args[0]}" && pwd)"
tout="${args[1]:-${root}/.oracle/tarballs}"
# shellcheck source=cargo-locked.sh
source "$runner/cargo-locked.sh"

# Single oracle-root source (D-0280(b)). Do not fall back to a hardcoded box path here.
if [ -z "${LWC_ORACLE:-}" ]; then
  LWC_ORACLE="$(node "$root/harness/scripts/oracle-root.mjs" oracle_root)" \
    || { echo "!! C8 FAILED: oracle-root" >&2; exit 2; }
  export LWC_ORACLE
fi
export LWC_ROOT="$LWC_ORACLE"
if [ -z "${STYLE_CORPUS:-}" ]; then
  STYLE_CORPUS="$(node "$root/harness/scripts/oracle-root.mjs" style_corpus_root)" \
    || { echo "!! C8 FAILED: style-corpus-root" >&2; exit 2; }
  export STYLE_CORPUS
fi
# D-0282: shared land-path checks (lints, ceilings, shim-gate). Golden-gate stays
# in run-board.sh so BOARD_GOLDEN_GATE=0 can skip it on a scoped official board.
_lpc_git="${SSR_ORACLE_GIT:-}"
_lpc_head="${SSR_ORACLE_MAIN:-}"
_lpc_base=""
if [ -n "$_lpc_git" ] && [ -n "$_lpc_head" ] && git -C "$_lpc_git" rev-parse --verify "${_lpc_head}^" >/dev/null 2>&1; then
  _lpc_base="$(git -C "$_lpc_git" rev-parse "${_lpc_head}^")"
elif git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  _lpc_git="$root"
  _lpc_head="$(git -C "$root" rev-parse HEAD)"
  if git -C "$root" rev-parse --verify "${_lpc_head}^" >/dev/null 2>&1; then
    _lpc_base="$(git -C "$root" rev-parse "${_lpc_head}^")"
  fi
fi
if [ -x "$runner/land-path-checks.sh" ]; then
  if [ -n "$_lpc_base" ]; then
    LAND_PATH_SKIP_GOLDEN_GATE=1 "$runner/land-path-checks.sh" --root "$root" --git "$_lpc_git" --base "$_lpc_base" --head "$_lpc_head" --from-c8 \
      || { echo "!! C8 FAILED: land-path-checks" >&2; exit 3; }
  else
    LAND_PATH_SKIP_GOLDEN_GATE=1 "$runner/land-path-checks.sh" --root "$root" --lints-only --from-c8 \
      || { echo "!! C8 FAILED: land-path-checks" >&2; exit 3; }
  fi
else
  "$runner/workspace-path-scan.sh" "$root" \
    || { echo "!! C8 FAILED: workspace-path-scan" >&2; exit 3; }
fi
"$runner/test-source-scan-tracked.sh" "$root" \
  || { echo "!! C8 FAILED: source-scan-tracked (untracked .oracle plants must be ignored)" >&2; exit 3; }

(cd "$root" && cargo_locked check --workspace --all-targets) || { echo "!! C8 FAILED: check --workspace --all-targets --locked" >&2; exit 3; }
echo "c8: check --workspace --all-targets --locked ok"

"$runner/clippy-ratchet.sh" "$root" || { echo "!! C8 FAILED: clippy ratchet" >&2; exit 3; }

(cd "$root" && cargo_locked test --workspace) || { echo "!! C8 FAILED: test --workspace --locked" >&2; exit 3; }
echo "c8: test --workspace --locked ok"

if [ "$skip_pack" -eq 0 ]; then
  mkdir -p "$tout"
  "$runner/napi-pack.sh" "$tout" linux-x64-gnu "$root" || { echo "!! C8 FAILED: napi-pack" >&2; exit 3; }
  unset SSR_ORACLE_NAPI
  [ -x "$runner/ssr-oracle-gate.sh" ] || { echo "!! C8 FAILED: ssr-oracle-gate.sh missing next to c8-checks.sh" >&2; exit 3; }
  LWC_ROOT="$LWC_ORACLE" SSR_ORACLE_ROOT="${SSR_ORACLE_ROOT:-$root}" \
    "$runner/ssr-oracle-gate.sh" "$tout" linux-x64-gnu \
    || { echo "!! C8 FAILED: ssr-compile-js-oracle gate" >&2; exit 3; }
  echo "c8: napi tarballs + ssr-oracle-gate ok ($tout)"
fi
echo "c8: all requested checks passed"
