#!/usr/bin/env bash
# D-0280 C8 checks, in-repo, no GitHub Actions. All cargo goes through cargo_locked
# except napi-pack.sh (which already passes --cargo-flags=--locked and then asserts
# Cargo.lock is unchanged).
# Usage: scripts/c8-checks.sh [--skip-fmt] [--skip-pack] [repo-root] [tarball-out]
#   cargo fmt --all --check is reported always. It aborts only when C8_FMT_GATE=1
#   (D-0235 has not landed; fmt is known red on main as of 3ecff19).
# C8 helper scripts (clippy-ratchet, ssr-oracle-gate, napi-pack) come from this
# file's tree (the runner). Cargo/clippy/pack run against [repo-root].
set -euo pipefail
runner="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
root="$(cd "$runner/.." && pwd)"
skip_fmt=0 skip_pack=0
args=()
for a in "$@"; do
  case "$a" in
    --skip-fmt) skip_fmt=1 ;;
    --skip-pack) skip_pack=1 ;;
    --only-extra) echo "c8-checks.sh: --only-extra is gone; the shared driver always runs the full C8 set" >&2; exit 2 ;;
    -*) echo "usage: c8-checks.sh [--skip-fmt] [--skip-pack] [repo-root] [tarball-out]" >&2; exit 2 ;;
    *) args+=("$a") ;;
  esac
done
[ ${#args[@]} -gt 0 ] && root="$(cd "${args[0]}" && pwd)"
tout="${args[1]:-${root}/.oracle/tarballs}"
# shellcheck source=cargo-locked.sh
source "$runner/cargo-locked.sh"

fmt_rc=0
if [ "$skip_fmt" -eq 0 ]; then
  (cd "$root" && cargo_locked fmt --all --check) || fmt_rc=$?
  if [ "$fmt_rc" -ne 0 ]; then
    echo "c8: fmt --all --check exited $fmt_rc (D-0235 not yet gating)"
    if [ "${C8_FMT_GATE:-}" = 1 ]; then
      echo "!! C8 FAILED: fmt --all --check (C8_FMT_GATE=1)" >&2
      exit 3
    fi
  else
    echo "c8: fmt --all --check ok"
  fi
fi

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
  LWC_ROOT="${LWC_ROOT:-${LWC_ORACLE:-/workspace/lwc}}" SSR_ORACLE_ROOT="${SSR_ORACLE_ROOT:-$root}" \
    "$runner/ssr-oracle-gate.sh" "$tout" linux-x64-gnu \
    || { echo "!! C8 FAILED: ssr-compile-js-oracle gate" >&2; exit 3; }
  echo "c8: napi tarballs + ssr-oracle-gate ok ($tout)"
fi
echo "c8: all requested checks passed"
