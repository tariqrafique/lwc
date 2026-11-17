# S0: napi packaging plan (ship, for LWC Arch Board review)

Rev 2, 2026-11-01 ~23:30 PST: rework for **AB-D0005** (REWORK on 5da7e91). Rev 1 folded D-0205..D-0214. Docs only: no product code, no oracle writes. Rust main @ 6ad019e.
Inputs: ship-survey.md, CUTOVER.md, arch-board/ARCHITECTURE.md, docs/WORKFLOW.md, decisions D-0198..D-0214, reviews.tsv AB-D0005.

## AB-D0005 rework (changelog)
| # | Arch item | resolved in |
|---|---|---|
| 1 | No env override in product loader; pin by package location | §1 Loader, §1 Pinning, §5 S1 (c), §6, §7 HARNESS-LANDS H1-H6. The env var is gone from the plan (`rg LWC_COMPILER_NAPI_PATH` on the target tree == 0 is gate G1) |
| 2 | AsyncTask: arg checks + deserialize + normalize sync; Task owns normalized options; one compile entry | §2 `transform`, §2 Single compile entry, §5 S3 seam test |
| 3 | TransformOptions d.ts + serde field list generated from AB-0008 schema; no key-diff check; S2 waits on AB-0008; non-option types hand-written | §2 d.ts, §5 S1 untyped / S2 |
| 4 | Package in-crate | kept, §1 Location |
| 5 | D-0205/D-0212: Rust decides, loader replays/prints once; no option inspection in JS; guard on option-key literals | §2 Replay protocol, §3 owners, gate G2 |
| 6 | clippy = shrink-only ratchet | §4 CI job 1 |
| 7 | INVALID_SOURCE/INVALID_ID from lwc-shared error-info | §2 Argument checks, §7 js-style J2 |
| 8 | glibc floor proven on an old-glibc image | §4 glibc floor (honest floor stated) |
| 9 | parse5 [patch] HEAD == Cargo.lock rev, fail-closed | §4 parse5, §7 PROPOSE-TO-HARNESS P1 |
| (gate list) | `rg LWC_COMPILER_NAPI_PATH`==0; no option-key literals in loader; clippy baseline shrink-only; parse5 HEAD==lock rev; await transform == transformSync on js-api rows | §8 gates G1-G5 |
`version` stays blocked on AB-0010 (§2).

Invariants honoured (ARCHITECTURE.md "lwc-compiler-napi"; PLAN §1):
- ESM only. No AMD, no CJS build, no JS shim (D-0198).
- The loader holds no compile logic and inspects no options (AB-D0005 #5).
- napi owns ONLY serde deserialization (`deny_unknown_fields`, D-0010/D-0203) and the JS error shape (errors.rs, D-0200). Defaults, normalization and validation live in `lwc-compiler` options.rs (AB-0008, harness).
- One compile entry is shared by sync and async.
- The dependency arrow stays `lwc-compiler <- lwc-compiler-napi`. Everything lands via `scripts/land.sh` with an `Arch-Review:` trailer (package files are not `docs/*`, so non-trivial).

## 1. Target npm layout
Name: registry and scope are Tariq-only and deferred (D-0202). Working name for local tarballs: **`lwc-compiler-rs`**, with per-triple packages `lwc-compiler-rs-<triple>`. The final name is pending Tariq. `@lwc/compiler` is not reused (CUTOVER.md).
Location (approved, AB-D0005 #4): inside the crate.
```
crates/lwc-compiler-napi/
  Cargo.toml, build.rs (napi_build::setup(), unchanged)
  package.json        "type":"module", "exports":{".":{"types":"./index.d.ts","default":"./index.js"}},
                      "files":["index.js","index.d.ts"], "napi":{"name":"lwc-compiler","triples":{...}},
                      optionalDependencies: lwc-compiler-rs-<triple> (exact version), engines.node ">=20" (D-0213)
  index.js            ESM loader + replay (hand-written, ~80 lines; §1, §2)
  index.d.ts          S1: untyped (options: unknown); S2: generated from AB-0008 schema + hand-written non-option types (§2)
  npm/<triple>/package.json   {"name":"lwc-compiler-rs-<triple>","os":[..],"cpu":[..],"libc":["glibc"]?,"main":"lwc-compiler.<triple>.node","files":[that .node]}
  tests/loader_guard.rs       gate G2 (§8)
  .gitignore          *.node
```
Triples (D-0213):
| triple | buildable on this box today |
|---|---|
| linux-x64-gnu | **yes** (x86_64, glibc 2.41 host, target installed). The box build is dev-only because it links host glibc; release builds use zig with a pinned glibc (§4) |
| linux-arm64-gnu | no: needs `rustup target add aarch64-unknown-linux-gnu` + zig/cargo-zigbuild (not installed) |
| darwin-x64, darwin-arm64 | no (CI macOS runner) |
| win32-x64-msvc | no (CI windows runner) |
musl is out of v1. The loader detects it and fails with the message below.

Tooling: **@napi-rs/cli 2.18.4** (latest 2.x; matches `napi` 2.16.17 / `napi-derive` 2.16.13). cli 3 / napi 3 would be a separate review.
- Build: `napi build --platform --release --js false` (cli v2's generated loader is CJS, so it is suppressed).
- Pack: `napi artifacts` places `.node` into `npm/<triple>/`, then `npm pack`. No publish (D-0202).

Loader `index.js` resolution order (no env override, AB-D0005 #1):
1. `./lwc-compiler.<triple>.node` sibling of index.js (local `napi build --platform` output; also what a pin dir contains).
2. `lwc-compiler-rs-<triple>` via `createRequire(import.meta.url)` (a `.node` needs require; the module stays ESM).
Triple: `process.platform`-`process.arch`; on linux, libc = gnu iff `process.report.getReport().header.glibcVersionRuntime` is present.
Failure: throw `Error("lwc-compiler-rs: no native binary for <platform>-<arch>[-<libc>]. Tried: <sibling path>: <cause>; <pkg>: <cause>. Supported: linux-x64-gnu, linux-arm64-gnu, darwin-x64, darwin-arm64, win32-x64-msvc. Reinstall without --omit=optional.", {cause: lastError})`. There is no JS fallback.
Exports: named `transformSync`, `transform`, `version` (after AB-0010); no default export (matches @lwc/compiler index.ts).

Pinning by package location (replaces the env var):
- `board.sh` / `official-board.sh` build the package (`napi build --platform --release --js false` in a snapshot of `crates/lwc-compiler-napi/`), then copy exactly `{package.json, index.js, index.d.ts, lwc-compiler.<triple>.node}` into `harness/.pins/<sha12>/`, `chmod -R a-w`.
- `<sha12>` = first 12 hex of sha256 over `sha256sum` lines of those 4 files, sorted by name (content-addressed: same inputs give the same pin).
- The harness imports `harness/.pins/<sha12>/index.js` (absolute file URL). The loader resolves the sibling `.node` (step 1), so the loaded binary is the pinned one by construction.
- Board/report `napi` field := pin sha12 (replaces the `.node` sha). OFFICIAL_BOARD.txt `napi_sha256_12` likewise; the `.node` sha stays as an extra field for continuity.
- Atomicity: copy to `.pins/.tmp.<pid>/`, then `mv` to `.pins/<sha12>/`; if it already exists, keep the existing dir. Never overwrite a pinned `.node` (the existing SIGBUS rule, install-napi.sh:3).

## 2. Exported JS API (lib.rs = ship, D-0200)
### Single compile entry
- `lib.rs` has one private `fn compile(src: String, filename: String, opts: NormalizedOptions) -> CompileOutcome`.
  - It calls `lwc_compiler::dispatch` (harness-owned; after AB-0008 it takes normalized options).
  - `CompileOutcome = { result: Result<CompileResult, CompileError>, events: Vec<InstrumentationEvent>, deprecations: Vec<DeprecationId> }`.
- `fn prepare(env, src: JsUnknown, filename: JsUnknown, options: JsUnknown) -> Result<Prepared>` runs **synchronously on the main thread** for both exports, in upstream order (transformer.ts:86-91; options.ts:180+):
  1. Arg checks (INVALID_SOURCE / INVALID_ID).
  2. Pull instrumentation callbacks out of options as JS function refs (the only non-JSON option).
  3. serde deserialize (`deny_unknown_fields`).
  4. `lwc_compiler::options::normalize` (AB-0008; it returns deprecation ids, D-0212).
  Any failure here throws synchronously, like upstream `validateArguments` / `validateTransformOptions`.
- Exports:
  - `#[napi(js_name="transformSync")]` (binding-level `transformSyncRaw`): `prepare` → `compile` → return a replay envelope (below).
  - `#[napi(js_name="transform")]` (binding-level `transformRaw`): `prepare` on the main thread → `AsyncTask<CompileTask>`. `CompileTask` **owns** `{src, filename, NormalizedOptions}` plus `Ref`s to the callbacks. `compute()` (libuv pool) calls the **same** `compile`. `resolve()` (main thread) builds the same envelope. There is no second path (AB-D0005 #2, D-0209).
- `version`: `#[napi]` getter from `lwc_shared::meta::LWC_VERSION`. **Blocked on AB-0010**; no 4th version copy meanwhile.

### Replay protocol (D-0205, D-0212; Rust decides, loader replays)
The binding returns `{ok: TransformResult} | {err: Error}` plus `replay: [fn, args][]` and `warnOnce: [id, text][]`.
- `replay`: Rust maps each `InstrumentationEvent` (`Log{errorInfo, config}` / `IncrementCounter{metric}`) to the caller's actual callback reference it extracted in `prepare`. JS never looks up `instrumentation`, `log` or `incrementCounter`.
- `warnOnce`: Rust emits deprecation ids with text from `lwc_shared` (upstream wording: options.ts:188-195 stylesheetConfig, :215 OutputConfig.minify).
Loader (whole logic):
```js
const seen = new Set();
function settle(r) {
  for (const [id, text] of r.warnOnce) if (!seen.has(id)) { seen.add(id); console.warn(text); }
  for (const [fn, args] of r.replay) fn(...args);
  if (r.err) throw r.err;            // real Error instance built in Rust (D-0210)
  return r.ok;
}
export const transformSync = (s, f, o) => settle(b.transformSyncRaw(s, f, o));
export const transform = (s, f, o) => b.transformRaw(s, f, o).then(settle);
```
- Sync throws from `prepare` propagate unchanged (upstream `transform` throws arg errors sync too: transformer.ts:45).
- Order is warnings, then instrumentation, then return/throw. This matches upstream: validate warns before compile; log/counter fire during compile, which precedes return/throw.
- "Once per process" = per loaded module instance. Upstream uses module-level `alreadyWarnedOn*` flags (options.ts:22) and does the same.
- No option-key literals in index.js: gate G2.

### Errors (D-0200, D-0210)
- Shape = upstream `CompilerError extends Error {code, filename?, location?, level, url?}` (@lwc/errors utils.ts:24-46) and `CompilerAggregateError extends AggregateError` (utils.ts:74).
- Built as real `Error` / `AggregateError` instances in `errors.rs` (js-style supplies; ship wires in lib.rs). Never plain objects.
- Warnings keep the CompilerDiagnostic shape (shared.ts:17).
- Argument checks: INVALID_SOURCE / INVALID_ID come from `lwc_shared` error-info (generated from errors/src/compiler/error-info/compiler.ts; js-style sole writer, AB-D0005 #7). lib.rs references the named constants; no code or message literals in napi.

### d.ts (AB-D0005 #3)
- **S1: untyped**, i.e. `index.d.ts` declares `transformSync(src: string, filename: string, options?: unknown): unknown` etc., so the package works without types.
- **S2 (waits on AB-0008):**
  - Generated from the AB-0008 option schema (one source, harness-owned): the `TransformOptions` / `StylesheetConfig` / `CustomPropertiesResolution` / `DynamicImportConfig` / `OutputConfig` d.ts, and the napi serde struct field list (`JsCompileOptions` + nested). No key-diff check (generation makes drift impossible).
  - Generator location and owner: with the schema (harness). Ship consumes generated files.
  - Hand-written non-option types: `TransformResult {code; map: SourceMapV3|null (D-0211); warnings?: CompilerDiagnostic[]; cssScopeTokens?: string[]}`, `CompilerDiagnostic`, `CompilerError`, `CompilerAggregateError`, `InstrumentationObject`. Checked by a `tsc --noEmit --strict` fixture plus seam tests asserting the real runtime values have exactly those fields.
- 7 @lwc/compiler type exports:
| type | S2 source |
|---|---|
| TransformOptions | generated (AB-0008) |
| StylesheetConfig | generated |
| CustomPropertiesResolution | generated |
| DynamicImportConfig | generated |
| OutputConfig | generated |
| TransformResult | hand-written |
| NormalizedTransformOptions | dropped (D-0214) |

Breaking diffs vs 9.5.3, all decided: unknown keys rejected (D-0203); errors are real `Error`s, type-only export (D-0210); `map` = Source Map v3 (D-0211); `transform` off main thread (D-0209); Node >= 20 + 5 triples (D-0213); `NormalizedTransformOptions` dropped (D-0214). Not diffs: instrumentation kept (D-0205), apiVersion float floor (D-0206), name/namespace optional (D-0207), `undefined` semantics per upstream (D-0208), deprecation warnings kept (D-0212). Open: package name (Tariq, D-0202).

## 3. Owner split (per file / function)
| path / item | owner | basis |
|---|---|---|
| `crates/lwc-compiler-napi/{package.json,index.js,index.d.ts (hand-written parts),npm/**,tests/loader_guard.rs,.gitignore}`, `build.rs`, its `Cargo.toml` | ship | D-0199 |
| `crates/lwc-compiler-napi/src/lib.rs`: exports, `prepare`, `compile`, `CompileTask`, replay envelope, serde structs (field list generated) | ship | D-0200 |
| `crates/lwc-compiler-napi/src/errors.rs` (absorbs aggregate.rs, single_error.rs, error_urls.rs) | js-style | D-0200 |
| AB-0008 option schema + generator (serde field list + option d.ts) | harness | AB-0008 |
| `crates/lwc-compiler` options.rs (normalize → `NormalizedOptions` + `Vec<DeprecationId>`), dispatch (`.mts`, LWC1005), `CompileOutcome.events` plumbing | harness | D-0199, AB-0008/0009 |
| `lwc_shared`: `InstrumentationEvent`, `DeprecationId` + upstream warning texts, INVALID_SOURCE/INVALID_ID error-info, `meta::LWC_VERSION` | js-style | D-0199 error model, AB-0001/0010 |
| instrumentation event emission at template codegen sites (upstream template-compiler codegen/index.ts) | template | owns lwc-template |
| `harness/**`, `scripts/*.sh` (board.sh, official-board.sh, land.sh, install-napi.sh, golden-gate.sh) | harness | PLAN §3 |
| `crates/lwc-template/Cargo.toml` parse5 git dep | template | D-0201/D-0204 |
| `.github/workflows/rust-*.yml`, `rust-toolchain.toml`, `ci/clippy-baseline.txt`, glibc smoke | ship (reaches GitHub via the existing verified push; ship never pushes) | D-0202 |

## 4. Build / CI plan
- Toolchain: `rust-toolchain.toml` `channel = "1.99.1"`, `components = ["rustfmt","clippy"]` (edition 2024 needs ≥1.85). Targets are added per CI job.
- parse5-rust: git dep `rev = e3520ba` (full sha per D-0204) (D-0201 as amended by D-0204; must equal publish-official.sh `OFFICIAL_PARSE5_COMMIT`). Template edits the Cargo.toml.
  - Box override: `[patch."https://github.com/tariqrafique/parse5-rust"] parse5 = { path = "/workspace/wt/parse5-rust/crates/parse5" }` in `/workspace/wt/.cargo/config.toml`.
  - **PROPOSE-TO-HARNESS P1**: fail-closed check in board.sh/official-board.sh/land.sh that `git -C /workspace/wt/parse5-rust rev-parse HEAD` == the rev in Cargo.lock's parse5 `source` (`…#<rev>`), else abort. D-0204 notes trees are byte-identical across e3520ba/2826427/88b0844, so the check should compare tree hash (`rev-parse HEAD^{tree}` vs `rev-parse <lockrev>^{tree}`) to allow that equivalence while still failing closed on any content difference.
  - Clean-checkout CI is red until template lands the git dep.
- Cross targets:
  - linux-x64-gnu + linux-arm64-gnu: `cargo zigbuild --target <t>.2.17` on ubuntu-latest (zig pinned via `mlugg/setup-zig`).
  - darwin: `macos-14` native arm64, with `--target x86_64-apple-darwin` for x64.
  - win32-x64-msvc: `windows-latest`.
- CI (GitHub Actions on tariqrafique/lwc branch `rust-compiler`, D-0202). Workflow placement within that branch to be confirmed with harness/Migrate.
  1. `rust-check` (ubuntu):
     - Hard-fail: `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo test --workspace`.
     - Clippy shrink-only ratchet (AB-D0005 #6): `cargo clippy --workspace --all-targets --message-format=json` → count `"level":"warning"|"error"` compiler-messages → fail if count > `ci/clippy-baseline.txt` (single integer, committed; first value measured in the S5 landing, not guessed here). If count < baseline, the job fails with "lower the baseline to N" so the ratchet only moves down.
  2. `napi-build` matrix (5 triples): `napi build --platform --release --target <t> --js false` → artifact `lwc-compiler.<triple>.node`.
  3. `napi-smoke` matrix: install main + triple tarballs in a temp dir, run the §5 S1 (b) fixtures. Plus the glibc jobs below.
  4. `napi-pack`: `napi artifacts`, `npm pack` ×6 → artifacts. No publish (D-0202).
  Official boards stay box-only (WORKFLOW.md).

### glibc floor (AB-D0005 #8): honest statement
- **Facts:**
  - Official Node ≥18 linux binaries require glibc ≥2.28, so official Node 20 does not run on glibc 2.17.
  - unofficial-builds.nodejs.org publishes `linux-x64-glibc-217` builds (v20.1.0…v20.20.2, v22, v24 in its index.json today). There is none for arm64.
- **Proof jobs:**
  - (a) Static, every linux triple: `objdump -T lwc-compiler.<triple>.node | grep -o 'GLIBC_[0-9.]*' | sort -V | tail -1` must be ≤ the target floor (2.17 with the `.2.17` zig suffix). This bounds what the `.node` itself needs, independent of Node.
  - (b) Runtime, x64: container `quay.io/pypa/manylinux2014_x86_64` (CentOS 7, glibc 2.17) + Node `v20.20.2-linux-x64-glibc-217` (unofficial build, sha256-checked against its SHASUMS256.txt) runs S1 (b) fixtures through the package loader.
  - (c) Runtime, arm64: `ubuntu-24.04-arm` runner, official Node 20 inside `quay.io/pypa/manylinux_2_28_aarch64` (glibc 2.28) runs the same fixtures.
- **Honest floor:**
  - linux-x64-gnu `.node` = glibc **2.17** (proven by (a)+(b), but only on unofficial Node).
  - Supported floor for users on official Node 20 = glibc **2.28**, on x64 and arm64, because Node itself needs 2.28.
  - linux-arm64-gnu = **2.28** (runtime-proven by (c); (a) shows the `.node` needs ≤2.17, but no 2.17 Node exists to prove it at runtime).
  - Documented floor in package.json/README: **glibc ≥ 2.28** (what we can prove with supported Node); x64 2.17 noted as "works with unofficial glibc-217 Node, CI-tested".

## 5. Slices and prove-it-works
Numbers of record come only from an official board (PLAN §5). Live checks are pass/fail smoke.
| slice | change | depends on | checks |
|---|---|---|---|
| S1 | **Untyped linux-x64 package**: package.json, `npm/linux-x64-gnu/`, ESM loader (2-step order + replay `settle` with empty replay/warnOnce until S3), untyped index.d.ts, lib.rs export rename `transform`→`transformSync` (raw envelope). Harness H1-H6 land in the same window | Arch APPROVE of this plan | (a) `npx @napi-rs/cli@2.18.4 build --platform --release --js false`; `npm pack` main + linux-x64-gnu; `npm i` both tgz in a mktemp dir. (b) `node --input-type=module -e "import {transformSync} from 'lwc-compiler-rs'; const r=transformSync('<template><p>{x}</p></template>','foo.html',{name:'foo',namespace:'x'}); if(typeof r.code!=='string'||!r.code.length) process.exit(1); console.log('ok')"` plus `.js` and `.css` fixtures. (c) **Renamed binary only**: move `lwc-compiler.linux-x64-gnu.node` aside in both sibling and package locations, so the import throws the §1 message (exit≠0, grep "no native binary for linux-x64-gnu"). (d) `rg -n "module\.exports\|require\(" index.js` matches only `createRequire`; G1, G2 pass. (e) Official board with harness importing `.pins/<sha12>/index.js`: every row equal to the previous official board (napi field changes to pin sha; counts must not) |
| S2 | Generated option d.ts + serde field list (from AB-0008 schema); hand-written non-option types | **AB-0008 landed** | `tsc --noEmit --strict` fixture importing every exported type and value; seam test: runtime `TransformResult` / warnings / thrown errors have exactly the hand-written fields; option-gate row unchanged; regenerating produces no diff (CI) |
| S3 | `prepare`/`compile` split; AsyncTask `transform`; errors.rs wiring (real Errors); INVALID_SOURCE/INVALID_ID from lwc-shared; replay of instrumentation + warnOnce | js-style J1-J3; harness options normalize returning deprecation ids (AB-0008); lwc-compiler events plumbing | Seam tests: (i) **`await transform(s,f,o)` deep-equals `transformSync(s,f,o)` for every js-api row input**, both results and errors (compare `{name,message,code,filename,location,level,url,errors}`), gate G5. (ii) Thrown errors `instanceof Error` / `AggregateError`, with fields equal to 9.5.3 goldens. (iii) Arg errors throw synchronously from both exports. (iv) Instrumentation callbacks receive the same sequence as 9.5.3 on the template fixtures that log. (v) Deprecation warning printed once across 2 calls (mirrors options.spec.ts:14-52). Official board: js-api, template-api, style-api, ssr_render_api, template-options unchanged |
| S4 | `version` export | **AB-0010** | `version === '9.5.3'` from lwc_shared::meta; G-check no other version literal in napi |
| S5 | CI workflows, rust-toolchain.toml, clippy baseline, glibc jobs (a)(c) for x64 | template lands parse5 git dep (D-0201/D-0204) | workflow green on `rust-compiler`; clippy baseline file committed with measured count |
| S6 | Other 4 triples + `napi-pack` tarball artifacts + arm64 glibc job | S5 | per-triple smoke green; 6 tarballs uploaded; no publish |
| S7+ | Consumer ports in `/workspace/wt/lwc-consumers` (rollup-plugin, fixtures move, barrel), dup-table lint, deletes | S3 (+S2 for typed consumers) | per ship-survey.md §6 S7-S11 and CUTOVER.md |

## 6. Subtractions (harness-owned files: ship proposes, harness lands, see §7)
| today | replaced by |
|---|---|
| `scripts/install-napi.sh` (cargo build + atomic mv to `harness/lwc-compiler-napi.node`) | napi CLI build into `crates/lwc-compiler-napi/` + pin step (H2) |
| `harness/lwc-compiler-napi.node`, backend.ts:58-61 candidate list incl. `LWC_COMPILER_NAPI_PATH` | `import(pathToFileURL('.pins/<sha12>/index.js'))` (H1) |
| `harness/board.sh:27,34` (`.node` pin + env export) | pin dir `.pins/<sha12>/`; the harness gets the pin path by its own internal config (harness decides; it is not read by the product loader) (H2) |
| `scripts/official-board.sh:69-70,98` (`.so`→`.node` copy) | build package in snapshot → pin dir (H3) |
| `scripts/land.sh:78` live rebuild | same, via new build + pin (H3) |
| `harness/scripts/golden-gate.sh:22` (`unset LWC_COMPILER_NAPI_PATH`) | delete the line (H4) |
| README.md:38,41,89-90; CSR_RENDER.md:17-18; SSR_RENDER.md:12-14; dump-ssr.mjs:7; template-diag-diff.mjs:19,399 | pin-dir import (H5) |
| `napi:` field in ssr.render.test.ts:95, csr.render.test.ts:235 reports; OFFICIAL_BOARD.txt `napi_sha256_12` | pin sha12 (H6) |
| crate-local aggregate.rs, single_error.rs, error_urls.rs | errors.rs + lwc-shared error-info (J1) |

## 7. Work owed by others
**HARNESS-LANDS** (harness edits its own files; ship supplies the package and a patch sketch on request):
- H1 `harness/src/backend.ts:54-67`: drop the candidate list and `LWC_COMPILER_NAPI_PATH` (:58); import the pinned package `index.js`; call `transformSync` (rename from `transform`, :135); stop rethrowing `r.error`; delete RUST_OPTION_KEYS/NESTED_SHAPES once AB-0008 generates the schema.
- H2 `harness/board.sh:27,34`: build + pin the package dir to `.pins/<sha12>/` (read-only, atomic); stop exporting the env var.
- H3 `scripts/official-board.sh:69-70,98`, `scripts/land.sh:78`, `scripts/install-napi.sh` (retire): package build + pin.
- H4 `harness/scripts/golden-gate.sh:22`: delete `unset LWC_COMPILER_NAPI_PATH`.
- H5 docs/tools: harness/README.md:38,41,89-90, CSR_RENDER.md:17-18, SSR_RENDER.md:12-14, scripts/dump-ssr.mjs:7, scripts/template-diag-diff.mjs:19,399.
- H6 report fields: `napi` in ssr.render.test.ts:95 / csr.render.test.ts:235 reports, board.txt header, OFFICIAL_BOARD.txt `napi_sha256_12` := pin sha12 (keep `.node` sha as an extra field).
- H7 AB-0008: schema + generator for serde field list and option d.ts; `normalize` returns `NormalizedOptions` + deprecation ids; undefined-per-field row (D-0208).
**PROPOSE-TO-HARNESS** (harness decides):
- P1 parse5 [patch] fail-closed check: local checkout HEAD == Cargo.lock rev (tree-hash equality per D-0204) in board.sh / official-board.sh / land.sh.
- P2 gate G1 `rg -n LWC_COMPILER_NAPI_PATH` over the Rust repo == 0 as a board/land check after H1-H5.
**js-style:**
- J1 `crates/lwc-compiler-napi/src/errors.rs`: real Error/AggregateError construction with upstream fields (D-0200, D-0210).
- J2 INVALID_SOURCE / INVALID_ID (and all compiler.ts codes) in lwc-shared generated error-info; sole writer.
- J3 `lwc_shared` types `InstrumentationEvent`, `DeprecationId` + upstream deprecation texts; `meta::LWC_VERSION` (AB-0010).
**template:** parse5 git dep in crates/lwc-template/Cargo.toml (D-0201/D-0204); instrumentation emission at codegen sites (with J3 types).

## 8. Gates (from AB-D0005)
- G1 `rg -n LWC_COMPILER_NAPI_PATH` (Rust repo, after H1-H5) == 0. Owner: harness check (P2); ship's files never contain it.
- G2 `crates/lwc-compiler-napi/tests/loader_guard.rs` (ship): reads `index.js`, fails if it contains any option key as a string literal, identifier property access or destructure. Key set = the AB-0008 schema's generated key list (top-level + nested + `instrumentation`, `log`, `incrementCounter`); until AB-0008 lands, the option-gate inventory (61 fields) plus those 3. Also fails on `process.env`.
- G3 clippy count ≤ `ci/clippy-baseline.txt`, shrink-only (CI job 1).
- G4 parse5 HEAD == Cargo.lock rev, fail-closed (P1, harness).
- G5 seam test: `await transform` deep-equals `transformSync` (results and errors) across js-api rows (S3).
