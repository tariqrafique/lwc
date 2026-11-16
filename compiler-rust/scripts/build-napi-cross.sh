#!/usr/bin/env bash
# Item 15(a): build, verify and pack lwc-compiler-napi for every napi triple from one Linux x86_64
# host, without GitHub Actions (D-0280). Plan and hardware-only steps: docs/napi-cross-build.md.
#
# usage: build-napi-cross.sh [--out DIR] [--verify-only] [--no-pack] [--no-strip] [npm-triple ...]
#   default triples: every `shipped` row of scripts/napi-triples.tsv (probe rows only when named).
#   --verify-only  skip the build; verify + stage artifacts already in the target dir.
#   --no-pack      verify + stage only, no npm pack.
#   --no-strip     dev only: stage darwin .node unstripped (release packaging strips them; the
#                  unstripped binary goes to <out>/symbols/, never into a tarball).
# Env: NAPI_CROSS_TARGET_DIR (default <repo>/target; never the shared land cache), MIN_FREE_GB (15, D-0268).
# Tools (box-local, not repo config): zig 0.13 on PATH, cargo-zigbuild, cargo-xwin, the rust-toolchain.toml
# targets (rustup target add), and the llvm-tools rustup component (llvm-readobj/objdump/nm).
# Never builds unlocked (cargo_locked plus an unchanged-Cargo.lock check) and never publishes (D-0202).
set -euo pipefail
root="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
source "$root/scripts/cargo-locked.sh"
if [ -f "$HOME/.cargo/env" ]; then source "$HOME/.cargo/env"; fi
table="$root/scripts/napi-triples.tsv"
pkg="$root/crates/lwc-compiler-napi"
out="" verify_only=0 pack=1 strip=1
want=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="${2:?--out DIR}"; shift 2 ;;
    --verify-only) verify_only=1; shift ;;
    --no-pack) pack=0; shift ;;
    --no-strip) strip=0; shift ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    -*) echo "build-napi-cross: unknown flag $1" >&2; exit 2 ;;
    *) want+=("$1"); shift ;;
  esac
done
out="${out:-$root/target/napi-cross/out}"
tdir="${NAPI_CROSS_TARGET_DIR:-$root/target}"
case "$tdir/" in */cache/lwc-target/*) echo "build-napi-cross: refusing the shared land cache $tdir" >&2; exit 2 ;; esac

die() { echo "build-napi-cross: $*" >&2; exit 1; }
rows() { grep -v '^#' "$table" | grep -v '^[[:space:]]*$'; }
row_of() { rows | awk -F'\t' -v t="$1" '$1==t'; }
if [ ${#want[@]} -eq 0 ]; then
  mapfile -t want < <(rows | awk -F'\t' '$8=="shipped"{print $1}')
fi
for t in "${want[@]}"; do [ -n "$(row_of "$t")" ] || die "unknown triple $t (see $table)"; done

host_bin="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin"
READOBJ="$host_bin/llvm-readobj" OBJDUMP="$host_bin/llvm-objdump" NM="$host_bin/llvm-nm" STRIP="$host_bin/llvm-strip"
CSCHECK="$root/scripts/macho-codesign-check.py"
for b in "$READOBJ" "$OBJDUMP" "$NM" "$STRIP"; do [ -x "$b" ] || die "missing $b (rustup component add llvm-tools)"; done

if [ "$verify_only" -eq 0 ]; then
  need_zig=0 need_xwin=0
  for t in "${want[@]}"; do
    case "$(row_of "$t" | cut -f3)" in zigbuild) need_zig=1 ;; xwin) need_xwin=1 ;; esac
    rt="$(row_of "$t" | cut -f2)"
    grep -qx "$rt" <<<"$(rustup target list --installed)" || die "rust target $rt not installed (rustup target add $rt)"
  done
  if [ "$need_zig" -eq 1 ]; then
    command -v zig >/dev/null || die "zig not on PATH"
    command -v cargo-zigbuild >/dev/null || die "cargo-zigbuild not installed"
  fi
  if [ "$need_xwin" -eq 1 ]; then command -v cargo-xwin >/dev/null || die "cargo-xwin not installed"; fi
  free_gb="$(df -BG --output=avail "$root" | tail -1 | tr -dc 0-9)"
  [ "$free_gb" -ge "${MIN_FREE_GB:-15}" ] || die "only ${free_gb}G free, need ${MIN_FREE_GB:-15}G (D-0268)"
fi

lock_before="$(sha256sum "$root/Cargo.lock")"
cd "$root"
# cargo-zigbuild drops a zero-byte probe object in the cwd; remove it unless it was already there.
probe_obj="$root/.intentionally-empty-file.o"
[ -e "$probe_obj" ] && keep_probe=1 || keep_probe=0
artifact() { # npm-triple -> path of the built cdylib
  local rt="$1" os="$2"
  case "$os" in
    linux) echo "$tdir/$rt/release/liblwc_compiler_napi.so" ;;
    darwin) echo "$tdir/$rt/release/liblwc_compiler_napi.dylib" ;;
    win32) echo "$tdir/$rt/release/lwc_compiler_napi.dll" ;;
  esac
}

build_one() {
  local t="$1" rt tool os min
  IFS=$'\t' read -r _ rt tool os _ _ min _ <<<"$(row_of "$t")"
  echo "== build $t ($rt via $tool)"
  case "$tool" in
    zigbuild)
      local zt="$rt"
      if [ "$os" = linux ]; then zt="$rt.$min"; fi
      if [ "$os" = darwin ]; then
        MACOSX_DEPLOYMENT_TARGET="$min" cargo_locked zigbuild --locked --release -p lwc-compiler-napi --target "$zt" --target-dir "$tdir"
      else
        cargo_locked zigbuild --locked --release -p lwc-compiler-napi --target "$zt" --target-dir "$tdir"
      fi
      ;;
    xwin)
      # Static CRT: the .node must not need the VC++ redistributable (VCRUNTIME140.dll).
      local envvar="CARGO_TARGET_$(echo "$rt" | tr 'a-z-' 'A-Z_')_RUSTFLAGS"
      (
        export "$envvar=-C target-feature=+crt-static"
        cargo_locked xwin build --locked --release -p lwc-compiler-napi --target "$rt" --target-dir "$tdir"
      )
      ;;
    *) die "unknown tool $tool" ;;
  esac
}

fail=0
bad() { echo "  FAIL $t: $*"; fail=1; }
verify_one() {
  local t="$1" rt os cpu min f
  IFS=$'\t' read -r _ rt _ os cpu _ min _ <<<"$(row_of "$t")"
  f="$(artifact "$rt" "$os")"
  echo "== verify $t: $f"
  [ -f "$f" ] || { bad "artifact missing"; return; }
  local hdr; hdr="$("$READOBJ" --file-headers "$f")"
  local fmt; fmt="$(sed -n 's/^Format: //p' <<<"$hdr")"
  echo "  format: $fmt"
  case "$os/$cpu" in
    linux/x64) [ "$fmt" = elf64-x86-64 ] || bad "want elf64-x86-64" ;;
    linux/arm64) [ "$fmt" = elf64-littleaarch64 ] || bad "want elf64-littleaarch64" ;;
    darwin/x64) [ "$fmt" = "Mach-O 64-bit x86-64" ] || bad "want Mach-O 64-bit x86-64" ;;
    darwin/arm64) [ "$fmt" = "Mach-O arm64" ] || bad "want Mach-O arm64" ;;
    win32/x64) [ "$fmt" = COFF-x86-64 ] || bad "want COFF-x86-64" ;;
    win32/arm64) [ "$fmt" = COFF-ARM64 ] || bad "want COFF-ARM64" ;;
  esac
  case "$os" in
    linux)
      grep -q 'Type: SharedObject' <<<"$hdr" || bad "not ET_DYN"
      grep -qw napi_register_module_v1 <<<"$("$NM" -D --defined-only "$f")" || bad "napi_register_module_v1 not exported"
      local undef; undef="$(grep -cw 'napi_[a-z_0-9]*' <<<"$("$NM" -D --undefined-only "$f")" || true)"
      echo "  napi_* imports resolved from node at load: $undef"
      [ "$undef" -gt 0 ] || bad "no napi_* undefined symbols (expected dynamic lookup from node)"
      local g; g="$("$OBJDUMP" -T "$f" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)"
      echo "  max glibc symbol version: ${g:-none} (floor $min)"
      [ -n "$g" ] && [ "$(printf '%s\n%s\n' "${g#GLIBC_}" "$min" | sort -V | tail -1)" = "$min" ] || bad "needs $g > GLIBC_$min"
      ;;
    darwin)
      grep -q 'FileType: DynamicLibrary' <<<"$hdr" || bad "not MH_DYLIB"
      grep -qw _napi_register_module_v1 <<<"$("$NM" -gU "$f")" || bad "_napi_register_module_v1 not exported"
      local lc; lc="$("$OBJDUMP" --macho --private-headers "$f")"
      local minos; minos="$(awk '/cmd LC_BUILD_VERSION/{b=1} b&&/minos/{print $2; exit}' <<<"$lc")"
      echo "  LC_BUILD_VERSION minos: $minos (want $min)"
      [ "$minos" = "$min" ] || bad "minos $minos != $min"
      if [ "$cpu" = arm64 ]; then
        grep -q 'cmd LC_CODE_SIGNATURE' <<<"$lc" || bad "no LC_CODE_SIGNATURE (arm64 macOS refuses unsigned code)"
      fi
      local libs; libs="$(awk '/cmd LC_LOAD_DYLIB/{getline; getline; print $2}' <<<"$lc")"
      echo "  load dylibs: $(tr '\n' ' ' <<<"$libs")"
      if grep -qv '^/usr/lib/\|^/System/' <<<"$libs"; then bad "non-system dylib dependency"; fi
      ;;
    win32)
      grep -q 'IMAGE_FILE_DLL' <<<"$hdr" || bad "not a DLL"
      local pe; pe="$("$OBJDUMP" -p "$f")"
      grep -qE '^[[:space:]]+[0-9]+[[:space:]]+0x[0-9a-f]+[[:space:]]+napi_register_module_v1$' <<<"$pe" || bad "napi_register_module_v1 not exported"
      local dlls; dlls="$(sed -n 's/^[[:space:]]*DLL Name: //p' <<<"$pe" | sort -u)"
      echo "  imports: $(tr '\n' ' ' <<<"$dlls")"
      if grep -qi '^node\.exe$' <<<"$dlls"; then bad "links node.exe (napi symbols must be looked up at runtime)"; fi
      if grep -qi '^vcruntime' <<<"$dlls"; then bad "dynamic CRT (VCRUNTIME*.dll); want +crt-static"; fi
      if grep -qi '^api-ms-win-crt-' <<<"$dlls"; then bad "dynamic UCRT (api-ms-win-crt-*); want +crt-static"; fi
      ;;
  esac
}

uuid_of() { awk '/cmd LC_UUID/{getline; getline; print $2; exit}' <<<"$("$OBJDUMP" --macho --private-headers "$1")"; }
# D-0291 O3: release packaging strips darwin .node (llvm-strip -x: local symbols only; exports and the
# export trie stay). The unstripped binary (same LC_UUID, so atos/lldb can symbolicate) is kept in
# <out>/symbols/ and never packed. llvm-strip re-signs linker-signed ad-hoc code; the page hashes are
# re-checked, and arm64 falls back to `rcodesign sign` (ad-hoc) if they do not match.
strip_darwin() {
  local t="$1" cpu="$2" node="$3" sym="$out/symbols/$t/lwc-compiler.$t.node.unstripped"
  mkdir -p "$(dirname "$sym")"
  cp "$node" "$sym"
  "$STRIP" -x "$node"
  echo "  strip $t: $(stat -c %s "$sym") -> $(stat -c %s "$node") bytes"
  grep -qw _napi_register_module_v1 <<<"$("$NM" -gU "$node")" || die "$t: stripped .node lost _napi_register_module_v1"
  [ "$(uuid_of "$node")" = "$(uuid_of "$sym")" ] || die "$t: LC_UUID changed by strip"
  if [ "$cpu" = arm64 ]; then
    if ! python3 "$CSCHECK" "$node"; then
      command -v rcodesign >/dev/null || die "$t: signature invalid after strip and rcodesign is not installed"
      echo "  re-signing $t ad hoc (rcodesign)"
      rcodesign sign "$node" >/dev/null
      python3 "$CSCHECK" "$node" || die "$t: ad-hoc re-sign did not produce a valid signature"
    fi
  fi
}

stage_one() {
  local t="$1" rt os cpu libc f d ver
  IFS=$'\t' read -r _ rt _ os cpu libc _ _ <<<"$(row_of "$t")"
  f="$(artifact "$rt" "$os")" d="$out/npm/$t"
  ver="$(node -p 'require(process.argv[1]).version' "$pkg/package.json")"
  rm -rf "$d"; mkdir -p "$d"
  cp "$f" "$d/lwc-compiler.$t.node"
  if [ "$os" = darwin ] && [ "$strip" -eq 1 ]; then strip_darwin "$t" "$cpu" "$d/lwc-compiler.$t.node"; fi
  node - "$d/package.json" "$t" "$os" "$cpu" "$libc" "$ver" <<'JS'
const [file, t, os, cpu, libc, version] = process.argv.slice(2);
const p = {
  name: `@lwc/rust-compiler-${t}`, version, private: true,
  description: `${t} native binary for @lwc/rust-compiler. Not published (D-0202).`,
  os: [os], cpu: [cpu], ...(libc !== '-' ? { libc: [libc] } : {}),
  main: `lwc-compiler.${t}.node`, files: [`lwc-compiler.${t}.node`], engines: { node: '>=20' },
};
require('node:fs').writeFileSync(file, JSON.stringify(p, null, 2) + '\n');
JS
  if [ "$pack" -eq 1 ]; then
    local tgz; tgz="$(cd "$d" && npm pack --silent --pack-destination "$out/tgz")"
    local want_list got_list
    want_list="$(printf '%s\n' package/lwc-compiler."$t".node package/package.json)"
    got_list="$(tar -tzf "$out/tgz/$tgz" | sort)"
    [ "$got_list" = "$want_list" ] || die "$t: $tgz holds $(tr '\n' ' ' <<<"$got_list"), want only the .node and package.json"
  fi
}

for t in "${want[@]}"; do
  if [ "$verify_only" -eq 0 ]; then build_one "$t"; fi
  verify_one "$t"
done
if [ "$keep_probe" -eq 0 ]; then rm -f "$probe_obj"; fi
[ "$(sha256sum "$root/Cargo.lock")" = "$lock_before" ] || die "Cargo.lock changed during the build (not --locked-clean)"
[ "$fail" -eq 0 ] || die "verification failed"

rm -rf "$out"; mkdir -p "$out/tgz"
for t in "${want[@]}"; do stage_one "$t"; done
if [ "$pack" -eq 1 ]; then (cd "$pkg" && npm pack --silent --pack-destination "$out/tgz" >/dev/null); fi
# MANIFEST.sha256 covers every file in <out>; MANIFEST.tsv says which are shipped:
# tarball (an npm pack output), staged (inside a tarball), not-packed (symbols; never shipped).
(
  cd "$out"
  dirs=(npm tgz); if [ -d symbols ]; then dirs+=(symbols); fi
  find "${dirs[@]}" -type f | sort | xargs sha256sum > MANIFEST.sha256
  printf 'sha256\tbytes\tpath\trole\n' > MANIFEST.tsv
  while read -r h f; do
    case "$f" in tgz/*) r=tarball ;; symbols/*) r=not-packed ;; *) r=staged ;; esac
    printf '%s\t%s\t%s\t%s\n' "$h" "$(stat -c %s "$f")" "$f" "$r" >> MANIFEST.tsv
  done < MANIFEST.sha256
)
echo "== staged in $out (strip=$strip)"
cat "$out/MANIFEST.tsv"
