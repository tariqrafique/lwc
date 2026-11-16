# napi cross builds for macOS and Windows without GitHub Actions (item 15(a))

D-0280 removed GitHub Actions, so the old ship plan (`ship-packaging-plan.md` §4: macos-14 and
windows-latest runners) is replaced. Every napi triple is built from one Linux x86_64 host. Real
macOS and Windows hardware is needed only for the runtime smoke in §4.

## 1. Triples and tools
`scripts/napi-triples.tsv` is the single table. `scripts/build-napi-cross.sh` builds from it, and
`scripts/test-napi-loader-resolution.mjs` asserts it equals `index.js` `SUPPORTED`,
`package.json` `optionalDependencies` and `napi.triples.additional`.

| npm triple | rust target | linker | floor | status |
|---|---|---|---|---|
| linux-x64-gnu | x86_64-unknown-linux-gnu | cargo-zigbuild, `.2.17` glibc suffix | glibc 2.17 symbols (documented floor 2.28, D-0220) | D-0213 |
| linux-arm64-gnu | aarch64-unknown-linux-gnu | cargo-zigbuild, `.2.17` | same | D-0213 |
| darwin-x64 | x86_64-apple-darwin | cargo-zigbuild (zig's bundled libSystem stubs; no Apple SDK) | `MACOSX_DEPLOYMENT_TARGET=10.15` | D-0213 |
| darwin-arm64 | aarch64-apple-darwin | cargo-zigbuild | `11.0` | D-0213 |
| win32-x64-msvc | x86_64-pc-windows-msvc | cargo-xwin (lld-link + downloaded MSVC CRT / Windows SDK) | `+crt-static` (no VCRUNTIME140 / UCRT dlls) | D-0213 |
| win32-arm64-msvc | aarch64-pc-windows-msvc | cargo-xwin | `+crt-static` | D-0290 |

All six rows are `shipped` (D-0213 plus D-0290). A `probe` status stays available for a triple
that is built and verified but not in the package.

Box tool setup (box-local, not repo config; nothing in Cargo.toml / .cargo changes):
- `rustup target add --toolchain 1.99.1 <the 6 targets>`; `rustup component add llvm-tools --toolchain 1.99.1`
  (llvm-readobj / llvm-objdump / llvm-nm for verification).
- zig 0.13.0 from ziglang.org (sha256 checked against download/index.json) on PATH.
- `cargo install --locked cargo-zigbuild` (0.24.1) and `cargo install --locked cargo-xwin` (0.23.1).
- cargo-xwin downloads the MSVC CRT and Windows SDK into `~/.cache/cargo-xwin` on first use, under
  Microsoft's license terms. **D-0291 (Tariq, 2026-11-15): cargo-xwin is the release path for the
  win32 triples and that license is accepted.** No opt-in flag is needed.
- macOS signatures: `scripts/macho-codesign-check.py` (stdlib Python) re-hashes every code page
  against the embedded CodeDirectory, which is the check the kernel makes for ad-hoc and
  linker-signed code. `rcodesign` (`cargo install --locked apple-codesign`, 0.29.0) is needed only
  as the ad-hoc re-sign fallback. Its `verify` is not used, because it reports false failures on
  ad-hoc signatures.

## 2. Build, verify, pack
```
scripts/build-napi-cross.sh [--out DIR] [--verify-only] [--no-pack] [--no-strip] [npm-triple ...]
```
- Default triples are the `shipped` rows; probe rows only when named.
- Builds go through `cargo_locked ... --locked`, and the script fails if Cargo.lock changed. It
  refuses the shared land cache (`*/cache/lwc-target/*`) as a target dir (default `<repo>/target`), and checks ≥15G free (D-0268).
- Verification per artifact, all fail-closed:
  - **linux:** ELF64 of the right machine, ET_DYN, exports `napi_register_module_v1`, `napi_*` left
    undefined for node to resolve, and the highest `GLIBC_x.y` symbol version ≤ the floor.
  - **darwin:** Mach-O of the right CPU, MH_DYLIB, exports `_napi_register_module_v1`,
    `LC_BUILD_VERSION minos` equals the deployment target, `LC_CODE_SIGNATURE` present on arm64 (the
    linker's ad-hoc signature, which arm64 macOS requires), and only `/usr/lib` or `/System` dylibs.
  - **win32:** PE DLL of the right machine, exports `napi_register_module_v1`, does not import
    `node.exe` (napi-sys resolves napi symbols at runtime), and imports no VCRUNTIME or `api-ms-win-crt-*` dlls.
- **darwin strip (release packaging, Migrate O3 ruling):**
  - The staged darwin `.node` gets `llvm-strip -x` (local symbols only; exports and the export trie stay).
  - The unstripped binary is kept as `<out>/symbols/<triple>/lwc-compiler.<triple>.node.unstripped`.
    It has the same `LC_UUID`, so atos and lldb can symbolicate crash logs with it. It is never packed.
  - After stripping, the script checks `_napi_register_module_v1` is still exported and `LC_UUID` is
    unchanged. On arm64 it requires a valid ad-hoc signature (llvm-strip re-signs linker-signed code)
    and falls back to `rcodesign sign` ad hoc if the page hashes don't match.
  - Each per-triple tarball must contain exactly `package/package.json` and the `.node`.
  - `--no-strip` is for dev runs only.
- Staging: `<out>/npm/<triple>/{package.json,lwc-compiler.<triple>.node}`. The per-triple
  package.json is generated from the table: `os`, `cpu`, `libc` (linux), `main`, `files`, and
  `engines.node >=20`, with the version taken from the main package. Then `npm pack` produces each
  triple plus the main package into `<out>/tgz`. `<out>/MANIFEST.sha256` covers every file, and
  `<out>/MANIFEST.tsv` adds bytes and a role (`tarball`, `staged`, or `not-packed` for symbols).
- **Name (D-0291):** the main package is **`@lwc/rust-compiler`**, and the platform packages are
  **`@lwc/rust-compiler-<triple>`**, so the tarballs are `lwc-rust-compiler[-<triple>]-<version>.tgz`.
  The native file keeps the napi name `lwc-compiler.<triple>.node`.
  **No npm publish until Tariq says so (D-0202, D-0291).**
- npm layout (unchanged from S1): the main `@lwc/rust-compiler` package lists every shipped
  `@lwc/rust-compiler-<triple>` as an exact-version optionalDependency. npm installs only the one
  whose `os`/`cpu`/`libc` match. The ESM loader `index.js` tries `./lwc-compiler.<triple>.node`
  next to itself first, then `@lwc/rust-compiler-<triple>` via `createRequire`, and otherwise throws
  `no native binary for <triple>` listing both causes. There is no JS fallback.

## 3. Tested on Linux (box, main a492147)
- All 6 triples built and passed every check above. The darwin .node files were stripped, still
  export `_napi_register_module_v1`, kept their `LC_UUID`, and darwin-arm64 kept a valid ad-hoc
  signature (3295/3295 pages; checked again inside the packed tarball). Every tarball holds exactly
  `package.json` plus the `.node`.
- `macho-codesign-check.py`: the original and stripped arm64 binaries pass. A one-byte change in a
  code page fails, and so does x64 with no signature.
- `test-napi-loader-resolution.mjs` simulates platform, arch and glibc in a child node:
  - each shipped triple loads its own package;
  - a missing package gives the documented error;
  - win32-arm64 resolves `@lwc/rust-compiler-win32-arm64-msvc` (D-0290);
  - musl, win32-ia32, freebsd and ppc64 fail closed;
  - an unloadable sibling falls through to the package;
  - the real sibling `.node` beats the package;
  - the real linux-x64-gnu (glibc-2.17 zig build) `.node` compiles .html and .js.

  Mutation-checked: removing the `-msvc` suffix, treating musl as gnu, or dropping an
  optionalDependency each fail it. Swapped artifacts (x64 dylib as arm64, arm64 dll as x64, x64 .so
  as arm64) fail the verifier.
- npm smoke: `npm install --offline` of the main and linux-x64-gnu tarballs into a scratch project,
  then `import '@lwc/rust-compiler'` compiles .html, .js and .css. With the platform package moved away,
  the import fails with `no native binary for linux-x64-gnu`.

## 4. Needs real hardware (cannot be tested on this host)
For each of darwin-arm64, darwin-x64 (or Rosetta on arm64), win32-x64 and win32-arm64,
with official Node 20 and 22:
1. `npm install` the main and that triple's tarball offline. Then `import {transform} from '@lwc/rust-compiler'`
   (transformSync after S3) compiles the S1 (b) .html/.js/.css fixtures. This proves dlopen, napi
   symbol lookup, and on darwin-arm64 that the ad-hoc signature is accepted. The same for an
   instrumentation callback fixture.
2. `node scripts/test-napi-loader-resolution.mjs --real-node <that .node>` natively. The simulated
   cases also run unsimulated for the host triple.
3. darwin: `codesign -v --verbose` on the stripped .node; symbolicate a crash with the unstripped
   `symbols/` copy (`atos -o <unstripped> -l <load addr> <pc>`); load it from a quarantined copy
   (`xattr -w com.apple.quarantine ...`) to confirm npm-installed files are not blocked.
   windows: load on a machine without the VC++ redistributable installed, which proves `+crt-static`.
4. Optional and strongest: the in-repo portable board (D-0280(b)) on that machine, with the napi rows
   matching the latest official board.

Linux runtime gaps of the same kind: linux-arm64-gnu was never loaded (no arm64 or qemu on the box),
and the glibc-2.17 claim is proven statically only (no old-glibc container).

## 5. Decisions and open questions
- D-0290: six triples, win32-arm64-msvc included.
- D-0291: cargo-xwin is the win32 release path (Microsoft license accepted); the name is
  `@lwc/rust-compiler`; no publish.
- O3 ruling (Migrate): darwin is stripped in packaging with symbols kept unshipped (§2). Measured:
  darwin-arm64 15497696 → 13598848 bytes, and darwin-x64 15417656 → 13534600. The gap to
  linux/windows (about 6 MB) is mostly code, not symbols: `__text` is 6.7 MB and `__const` 3.7 MB on
  arm64. The likely cause is that zig's Mach-O linker drops less dead code than ld64. A build with
  Apple's ld64 on a Mac would show whether it closes the gap; this is open.

Earlier open questions (superseded):
- O1: answered by D-0291.
- O2: answered by D-0290.
- O3: answered by the strip ruling above. `LC_ID_DYLIB` still embeds the build path.
- O4: answered by D-0291.
