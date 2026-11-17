#!/usr/bin/env bash
# Plants for scripts/install-napi.sh: the live napi comes from the target dir the build used.
# A stub build tool on PATH (no real build) stands in for the release build:
#   1. CARGO_TARGET_DIR set, artifact missing -> rc != 0, message names the missing path;
#   2. CARGO_TARGET_DIR set, artifact present -> harness/lwc-compiler-napi.node is that exact file;
#   3. CARGO_TARGET_DIR unset -> resolved from metadata target_directory, exact file installed;
#   4. a stale ./target/release copy is never used.
set -uo pipefail
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
T="$(mktemp -d /tmp/test-install-napi.XXXXXX)"
trap 'rm -rf "$T"' EXIT
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }
R="$T/root"; mkdir -p "$R/scripts" "$R/harness" "$T/bin" "$T/home"
cp "$here/install-napi.sh" "$here/cargo-locked.sh" "$R/scripts/"
bin='c'argo
# stub: 'build' is a no-op (the plant decides whether the artifact exists); 'metadata' prints the
# target_directory from STUB_META_TARGET.
cat > "$T/bin/$bin" << 'S'
#!/usr/bin/env bash
for a in "$@"; do
  case "$a" in
    metadata) printf '{"target_directory":"%s","packages":[]}\n' "$STUB_META_TARGET"; exit 0 ;;
    build) exit 0 ;;
  esac
done
exit 0
S
chmod +x "$T/bin/$bin"
run() { # name [env...] -> rc in $T/<name>.rc, output in $T/<name>.out
  local name="$1"; shift
  local rc=0
  env -u CARGO_TARGET_DIR HOME="$T/home" PATH="$T/bin:$PATH" STUB_META_TARGET="$T/meta" "$@" "$R/scripts/install-napi.sh" > "$T/$name.out" 2>&1 || rc=$?
  echo "$rc" > "$T/$name.rc"
}
sha() { sha256sum "$1" | cut -c1-64; }
live="$R/harness/lwc-compiler-napi.node"

# 4 (setup): a stale ./target copy that must never be installed
mkdir -p "$R/target/release"; printf 'STALE ./target napi\n' > "$R/target/release/liblwc_compiler_napi.so"

# 1. missing artifact
run missing CARGO_TARGET_DIR="$T/tgt"
if [ "$(cat "$T/missing.rc")" != 0 ] && grep -qF "!! install-napi: built artifact missing: $T/tgt/release/liblwc_compiler_napi.so" "$T/missing.out" && [ ! -e "$live" ]; then
  ok "missing artifact fails loudly (rc $(cat "$T/missing.rc")): $(grep -m1 '!!' "$T/missing.out")"
else bad "missing artifact: rc=$(cat "$T/missing.rc")"; sed 's/^/       /' "$T/missing.out"; fi

# 2. artifact present under CARGO_TARGET_DIR
mkdir -p "$T/tgt/release"; head -c 4096 /dev/urandom > "$T/tgt/release/liblwc_compiler_napi.so"
run present CARGO_TARGET_DIR="$T/tgt"
if [ "$(cat "$T/present.rc")" = 0 ] && [ "$(sha "$live")" = "$(sha "$T/tgt/release/liblwc_compiler_napi.so")" ]; then
  ok "CARGO_TARGET_DIR artifact installed byte-exact ($(sha "$live" | cut -c1-12)); stale ./target ignored"
else bad "present: rc=$(cat "$T/present.rc")"; sed 's/^/       /' "$T/present.out"; fi

# 3. CARGO_TARGET_DIR unset -> metadata target_directory
mkdir -p "$T/meta/release"; head -c 4096 /dev/urandom > "$T/meta/release/liblwc_compiler_napi.so"
run meta
if [ "$(cat "$T/meta.rc")" = 0 ] && [ "$(sha "$live")" = "$(sha "$T/meta/release/liblwc_compiler_napi.so")" ]; then
  ok "no CARGO_TARGET_DIR: metadata target_directory artifact installed byte-exact ($(sha "$live" | cut -c1-12))"
else bad "metadata: rc=$(cat "$T/meta.rc")"; sed 's/^/       /' "$T/meta.out"; fi

# 3b. metadata target_directory without the artifact -> loud failure naming it
rm -f "$T/meta/release/liblwc_compiler_napi.so"
run metamissing
if [ "$(cat "$T/metamissing.rc")" != 0 ] && grep -qF "built artifact missing: $T/meta/release/liblwc_compiler_napi.so (target dir $T/meta from cargo_locked metadata target_directory)" "$T/metamissing.out"; then
  ok "metadata target dir without artifact fails loudly"
else bad "metadata missing: rc=$(cat "$T/metamissing.rc")"; sed 's/^/       /' "$T/metamissing.out"; fi

[ "$fail" -eq 0 ] && { echo "test-install-napi: ok"; exit 0; }
echo "test-install-napi: FAILED"; exit 1
