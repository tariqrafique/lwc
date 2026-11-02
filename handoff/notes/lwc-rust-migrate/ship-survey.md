# SHIP-0 survey (read-only)

Written 2026-11-01 ~23:20 PST by ship. Rust: `/workspace/wt/ship` @ ee6ebb1 (bot/ship == main at dispatch; main now 6ad019e). Oracle `/workspace/lwc` @ b9e8007, clean, not written.
Owners per D-0199 / PLAN §3: **ship** = napi packaging files (package.json, loader, d.ts, build.rs), binaries, publish, CI, consumer moves, JS deletes, dup-table lint. **harness** = `crates/lwc-compiler` options + dispatch. **js-style** = napi `lib.rs` error surfacing. Line refs are `crates/...` in the Rust tree, `packages/...` in the oracle.

## 1. napi crate today (`crates/lwc-compiler-napi`)
- Cargo.toml: `crate-type = ["cdylib"]`; `napi = "2"` (default-features off, `napi4`, `serde-json`) → lock 2.16.17; `napi-derive = "2"` → 2.16.13; `napi-build = "2"`; edition 2024; version 0.1.0.
- build.rs: `napi_build::setup()` (1 line).
- Files: src/lib.rs (479), aggregate.rs (200), single_error.rs (161), error_urls.rs (12); tests/fixtures/{aggregate,single-error}-shipped.json.
- Exported JS fns (`#[napi]`): exactly ONE: `transform(src: String, filename: String, options: serde_json::Value) -> Result<JsCompileResult>` lib.rs:451-479. Sync. No `transformSync`, no async `transform`, no `version`.
- Options: `JsCompileOptions` lib.rs:15-42, taken as JSON then serde. `deny_unknown_fields` on every struct: lib.rs:16 (top), :46 StylesheetConfig, :52 CustomProperties, :58 Resolution (tagged enum), :65 DynamicImports, :73 OutputConfig, :103 CustomRendererConfig, :110 CustomRendererElement. Unknown key → thrown `InvalidArg "invalid compile options: …"` (lib.rs:300). LWC validation errors (e.g. LWC1021 sourcemap) → returned `{error}`.
- `name`/`namespace` are required `String` (lib.rs:18-19). JS treats them as optional (options.ts:149-151 OptionalTransformKeys).
- `ssrMode` invalid → thrown InvalidArg (lib.rs:303-306).
- Result `JsCompileResult` lib.rs:211-221: `{code, map: JSON|null, cssScopeTokens?, warnings?, error?}`. Compile errors are RETURNED as `error` (lib.rs:457+), not thrown. Harness re-throws (harness/src/backend.ts:135+).
- Harness load: `harness/src/backend.ts:54-67` `loadNative()` via `createRequire`, candidates `$LWC_COMPILER_NAPI_PATH`, `harness/lwc-compiler-napi.node`, `target/{release,debug}/liblwc_compiler_napi.so`. Allowlist `RUST_OPTION_KEYS` backend.ts:72-78 + `NESTED_SHAPES` :81-86; `instrumentation` non-null → throw (:100-103); `RustCompilerBackend` :112+ strips null/undefined.
- Build/install: `scripts/install-napi.sh` → `cargo build -p lwc-compiler-napi --release`, atomic copy to `harness/lwc-compiler-napi.node`. Linux `.so` path hardcoded (install-napi.sh:9).
- package.json / index.js / index.d.ts / `@napi-rs/cli` config / `.npmignore`: **none** anywhere in the Rust repo (only `harness/package.json`, private test harness).

### Packaging gaps vs standard `@napi-rs/cli` multi-platform layout (owner tag)
| # | gap | owner |
|---|---|---|
| P1 | No main npm package (`package.json` with `name`, `napi.binaryName`, `napi.targets`, `type: module`, `exports`, `files`, `optionalDependencies` per triple) | ship |
| P2 | No per-triple packages (`npm/<triple>/package.json` with `os`/`cpu`/`libc`, `main: <bin>.<triple>.node`). Minimum: linux-x64-gnu, linux-arm64-gnu, darwin-x64, darwin-arm64, win32-x64-msvc; consider linux-x64-musl | ship |
| P3 | No loader (`index.js`): platform/arch/libc detect → local `.node` then `@scope/<pkg>-<triple>`. Must be ESM (`createRequire` for `.node`); no CJS/AMD output, no JS compat shim (PLAN §1) | ship |
| P4 | No `index.d.ts` (TransformOptions/TransformResult/CompilerDiagnostic types). napi v2 `#[napi(object)]` can emit types via `napi build --dts`, but options are `serde_json::Value` so they come out as `any`, so d.ts must be hand-written or generated from the one option schema (AB-0008) | ship (types); harness (schema AB-0008) |
| P5 | No `@napi-rs/cli` devDep / `napi build --platform --release` script; build.rs present (ok). install-napi.sh linux-only | ship |
| P6 | No CI (see §4); no cross-compile matrix, no artifact upload, no `napi prepublish`/`npm publish` job | ship |
| P7 | Path dep outside repo: `crates/lwc-template/Cargo.toml:9` `parse5 = { path = "../../../parse5-rust/crates/parse5" }` (repo github.com/tariqrafique/parse5-rust @ 2826427). CI/publish cannot build a clean checkout without it: needs git dep / submodule / vendored crate | ship proposes; template owns Cargo.toml of lwc-template, so Arch review needed |
| P8 | napi 2 vs 3: `@napi-rs/cli` 3 + napi 3 is current; staying on 2 is fine, but pick one before writing package layout | ship (Arch decision) |
| P9 | Errors returned as `{error}` instead of thrown JS `Error` with `code/filename/location/level/url`/AggregateError. Today the harness rebuilds them (backend.ts:135+). Shipping API must throw, and per PLAN §1 that cannot live in a JS shim, so it moves into napi lib.rs (napi v2 can't attach extra props to `napi::Error`, so build the JS Error object via `Env` / napi 3 `JsError`) | js-style (lib.rs error surfacing) |
| P10 | `version` export absent: needs one LWC_VERSION source (AB-0010) | js-style/harness (AB-0010); ship wires export |
| P11 | No async `transform` (deprecated upstream but exported) | ship (wrapper in napi, or decide to drop: needs D-id) |
| P12 | Host-panic behaviour over napi not asserted for all crates (not checked in this survey) | harness (board row) |

## 2. JS `@lwc/compiler` 9.5.3 public API vs napi
Source: `packages/@lwc/compiler/src/index.ts:7-21`, `transformers/transformer.ts`, `transformers/shared.ts`, `options.ts`.

### Exports
| export | kind | JS ref | napi | status |
|---|---|---|---|---|
| `transform(src, filename, options): Promise<TransformResult>` | fn (@deprecated) | transformer.ts:40-54 | — (napi `transform` is SYNC and has transformSync semantics) | **renamed/semantic clash**: napi `transform` = JS `transformSync` minus throw |
| `transformSync(src, filename, options): TransformResult` | fn | transformer.ts:76-84 | `transform` lib.rs:451 | **renamed**; errors returned not thrown (P9) |
| `version` | const = `process.env.LWC_VERSION` | index.ts:21 | — | **missing** |
| `TransformResult` | type | shared.ts:11-23 | JsCompileResult (+`error`) | type missing (no d.ts) |
| `TransformOptions` | type | options.ts:93-147 | JsCompileOptions (Rust only) | type missing |
| `NormalizedTransformOptions` | type | options.ts:167 | — | type missing |
| `StylesheetConfig` | type | options.ts:64 | JsStylesheetConfig | type missing |
| `CustomPropertiesResolution` | type | options.ts:58 | JsResolution | type missing |
| `DynamicImportConfig` | type | options.ts:84 | JsDynamicImports | type missing |
| `OutputConfig` | type | options.ts:70 | JsOutputConfig | type missing |
Argument validation: JS `invariant(isString(src))` LWC1? INVALID_SOURCE / INVALID_ID (transformer.ts:86-89) vs napi `String` arg coercion error (napi type error, not LWC code): **gap** (owner harness: facade validation).
Unknown ext: JS LWC NO_AVAILABLE_TRANSFORMER via `generateCompilerError` (transformer.ts:118) vs Rust plain `"unsupported file extension: …"` no code (`crates/lwc-compiler/src/lib.rs:43-50`): **gap** (AB-0009, harness).
Ext set: JS `.html .css .tsx .jsx .ts .js .mts .mjs` (transformer.ts:96-115); Rust `css html js ts mjs jsx tsx` (lib.rs:35-42): **`.mts` missing** (AB-0009, harness). Rust uses `rsplit('.')` (no-dot filename → whole name) vs `path.extname`.

### TransformOptions fields (top-level 24)
| field | JS type (options.ts line) | napi field (lib.rs line) | status |
|---|---|---|---|
| name | string, req in type, optional at runtime (95,150) | `name: String` (18) | present; **required in napi** (rollup-plugin may pass undefined) |
| namespace | string (97,151) | `namespace: String` (19) | present; **required in napi** |
| stylesheetConfig | StylesheetConfig, deprecated, console.warn (99,188-193) | `stylesheet_config` (41) | present (no deprecation warn) |
| dynamicImports | DynamicImportConfig (101) | `dynamic_imports` (36) | present |
| experimentalDynamicDirective | bool (104) | (33) | present |
| enableDynamicComponents | bool (106) | (31) | present |
| enableSyntheticElementInternals | bool (108) | (34) | present |
| experimentalComplexExpressions | bool (111) | (28) | present |
| outputConfig | OutputConfig (113) | `output_config` (39) | present |
| isExplicitImport | bool (115) | (38) | present |
| preserveHtmlComments | bool (117) | (29) | present |
| scopedStyles | bool (119) | (20) | present |
| enableStaticContentOptimization | bool (121) | (30) | present; **`undefined` ≠ omitted** in JS (spread overwrites default true; rollup-plugin index.ts:389-394 relies on it). JSON seam drops undefined → Rust default. Gap (harness: option schema AB-0008) |
| customRendererConfig | CustomRendererConfig (123) | (40) | present |
| enableLwcOn | bool (125) | (32) | present |
| disableSyntheticShadowSupport | bool (127) | (21) | present |
| enableLightningWebSecurityTransforms | bool (131) | (37) | present |
| instrumentation | InstrumentationObject {log, incrementCounter} callbacks (136; errors/src/compiler/instrumentation.ts:22-25) | — | **missing** (JS callbacks can't cross JSON; backend.ts:100-103 throws). Needs napi ThreadsafeFunction/JsFunction or D-id drop |
| apiVersion | number (138) | `api_version: Option<u32>` (22) | present; **u32 vs JS number (float)**: 1120 seam-rejects on template-diag-napi (AB-0008) |
| targetSSR | bool (139) | `target_ssr` rename "targetSSR" (23-24) | present |
| ssrMode | CompilationMode (140) | `ssr_mode: Option<String>` (25) | present (invalid → thrown, JS: no validation) |
| experimentalErrorRecoveryMode | bool (142) | (26) | present |
| componentFeatureFlagModulePath | string (144) | (35) | present |
| enablePrivateMethods | bool (146) | (27) | present |
Unknown top-level keys: JS silently accepts (`...options` spread, options.ts:241-248); napi rejects. Intended (PLAN §1 unknown-option rejection), but it breaks real callers (see §3 wtr `strict`). Needs a D-id stating the break.

### Nested fields
| path | JS | napi | status |
|---|---|---|---|
| stylesheetConfig.customProperties | obj (65) | JsCustomProperties (53) | present |
| …customProperties.resolution | `{type:'native'}` \| `{type:'module',name}` (58,66) | JsResolution (59-62) | present |
| …resolution.type / .name | | tag / `name` | present |
| dynamicImports.loader | string (85) | (67) | present |
| dynamicImports.strictSpecifier | bool (86) | (68) | present |
| outputConfig.sourcemap | bool \| 'inline' (76) | JsSourcemap (96-100) | present |
| outputConfig.minify | bool, deprecated, warns (81,213-218) | `serde_json::Value`, ignored (77) | present (no warn) |
| customRendererConfig.directives | string[] (renderer-hooks.ts:46) | Vec<String> (105) | present; **required** in both |
| customRendererConfig.elements | CustomRendererElementConfig[] (:40) | (106) | present |
| elements[].tagName | string (:19) | `tag_name` (112) | present |
| elements[].namespace | string? (:23) | (113) | present |
| elements[].attributes | string[]? (:29) | (114) | present |
| instrumentation.log / .incrementCounter | fn | — | missing |

### TransformResult fields
| field | JS (shared.ts) | napi | status |
|---|---|---|---|
| code | string (13) | `code` (213) | present |
| map | `BabelFileResult['map']` obj\|null (15) | `map: Value` obj\|null (216) | present (Phase-0: source maps out of parity, CUTOVER.md:3; harness source-map row pending) |
| warnings | CompilerDiagnostic[]? (17) | `Option<Vec<JsDiagnostic>>` (219) | present (napi omits when empty; JS emptiness per transformer to verify) |
| cssScopeTokens | string[]? (22) | (217-218) | present |
| (error) | n/a: JS throws | `error` (220) | **extra**: must become a throw (P9) |

## 3. Consumers of to-be-deleted packages (oracle, excl. the 5 packages)
rg over package.json + .ts/.js/.mjs, excl. node_modules/dist/CHANGELOG/md. 19 code/config sites in 12 consumers + root config.

| # | consumer | file:line | calls | test/prod | replaced by |
|---|---|---|---|---|---|
| C1 | @lwc/rollup-plugin | src/index.ts:12 `import { transformSync } from '@lwc/compiler'`; :17 types `StylesheetConfig, DynamicImportConfig`; package.json:54 dep | `transformSync(src, filename, {...})` index.ts:372 (in `transform` hook :342), once per file in rollup `transform` hook | **prod** (published) | Rust napi package `transform` (sync); types from napi d.ts |
| C2 | `lwc` barrel | compiler.js:7, compiler.d.ts:7, ssr-compiler.{js,d.ts}:7, style-compiler.{js,d.ts}:7, template-compiler.{js,d.ts}:7-8, babel-plugin-component.{js,d.ts}:7-8; package.json:25,26,34,36,38 deps; exports :61,62,71,73,75 | `export *` re-exports | prod | `./compiler` → re-export napi package (or drop); `./ssr-compiler`, `./style-compiler`, `./template-compiler`, `./babel-plugin-component` → **delete** subpaths (breaking, D-id) |
| C3 | @lwc/engine-server tests | src/__tests__/fixtures.spec.ts:125 reads `../../../ssr-compiler/src/__tests__/fixtures` | fixture corpus (via rollup-plugin C1 :11,:80) | test | **move fixtures** out of ssr-compiler (CUTOVER.md; PLAN §4) into engine-server (or shared test-utils) before delete |
| C4 | @lwc/engine-server tests | fixtures.spec.ts:11,15; html-serialization.spec.ts:15,50; package.json:55 devDep rollup-plugin | via C1 | test | indirect: follows C1 |
| C5 | @lwc/ssr-runtime tests | src/__tests__/styles.spec.ts:2 `import { transform } from '@lwc/style-compiler'` | style-compiler `transform` direct | test | Rust napi `transform` on `.css` (cssScopeTokens/code) |
| C6 | @lwc/ssr-runtime tests | render-component.spec.ts:5,34 (`targetSSR: true`) | via C1 | test | indirect |
| C7 | @lwc/integration-wtr | package.json:13 `@lwc/compiler` dep (no direct import found), :16 rollup-plugin; configs/plugins/serve-integration.js:4,23; serve-hydration.js:6,27 | via C1; serve-integration.js:26-29 passes `dynamicImports: { loader, strict: true }`: **`strict` is not a JS field** (silently ignored today; napi deny_unknown_fields will REJECT) | test | indirect; fix `strict`→`strictSpecifier` or drop (consumer edit in oracle repo at cutover) |
| C8 | @lwc/integration-wdio | scripts/build.js:10; package.json:22 | via C1 | test | indirect |
| C9 | @lwc/integration-types | src/playground/rollup.config.js:8,33; package.json:12 | via C1 | test | indirect |
| C10 | @lwc/perf-benchmarks-components | rollup.config.mjs:10,29-33 (`targetSSR` toggled) ; package.json:9 | via C1 | test/bench | indirect |
| C11 | playground | rollup.config.js:1,24; package.json:12 | via C1 | dev | indirect |
| C12 | root vitest | vitest.config.mjs:8,9,19,21,22 project list | test runner | config | delete entries |

rollup-plugin call pattern (C1, index.ts:342-409): parse `?scoped`/`?specifier` from id → `[namespace,name]` from specifier or last 2 dir segments (may be undefined → only `this.warn`, :357-368) → `getAPIVersionFromNumber(apiVersion)` (from @lwc/shared, stays) → `transformSync(src, filename, {name, namespace, outputConfig:{sourcemap}, stylesheetConfig, dynamicImports, experimentalDynamicDirective, enableDynamicComponents, enableSyntheticElementInternals, enableLwcOn, enableLightningWebSecurityTransforms, experimentalComplexExpressions, preserveHtmlComments, scopedStyles: scoped, disableSyntheticShadowSupport, apiVersion, enableStaticContentOptimization: ('enableStaticContentOptimization' in pluginOptions ? value : true), targetSSR, ssrMode, componentFeatureFlagModulePath, enablePrivateMethods})` → warnings mapped to RollupLog (:135 transformWarningToRollupLog, :403, uses `location.{line,column,start,length}`) → `{code, map}`.
SSR path: NO direct ssr-compiler import; SSR goes through `transformSync` with `targetSSR`/`ssrMode`, dispatched by @lwc/compiler transformer.ts:97-115 → `compileTemplateForSSR`/`compileComponentForSSR`. Rust equivalent: `lwc_compiler::dispatch` `target_ssr` arms (lib.rs:38,40-42).
Hazards for porting C1 to napi: (h1) `name`/`namespace` undefined → napi deserialize throw (JS: proceeds); (h2) `enableStaticContentOptimization: undefined` semantics lost over JSON; (h3) passes 21 keys incl. undefined ones: fine after null/undefined stripping, but stripping changes (h2); (h4) errors must throw (P9) since rollup surfaces thrown errors; (h5) rollup-plugin build is es+cjs (`scripts/rollup/rollup.config.js:27` formats `['es','cjs']`, package `files` include `dist/**/*.cjs`); ESM-only policy means a `.cjs` build of the napi loader is out (decide whether rollup-plugin keeps its cjs: D-id).

Root/workspace mentions:
- package.json `workspaces: ["packages/@lwc/*","packages/lwc","playground"]` (glob: deleting dirs suffices). Scripts `build` = `nx run-many --target=build` (no explicit list).
- nx.json: no package names (npm preset, infers graph from package.json).
- tsconfig.json: no project references.
- .github/CODEOWNERS: `* @salesforce/lwc-admin` only; no per-package entry.
- scripts/release/version.js:60 comment only; scripts/shared/packages.mjs globs `packages/@lwc/*/package.json` (auto); scripts/tasks/check-and-rewrite-package-json.js:22,128 comments re rollup-plugin default modules.
- Docs: ARCHITECTURE.md:13,30-37 (compiler diagrams), AGENTS.md:17, packages/lwc/README.md:81, engine-server/README.md:3, rollup-plugin/README.md:30-37: doc updates at delete.
- rg found no jest/vitest transformer importing @lwc/compiler; unit tests compile via rollup-plugin (C1).

## 4. CI
- Rust repo: **no CI** (`.github` absent in `/workspace/lwc-compiler-rust` and `/workspace/wt/ship`); **no `rust-toolchain.toml`**; no git remote on main. Box toolchain: cargo 1.99.1 / rustc 1.99.1. Edition 2024 (needs ≥1.85). Workspace members `crates/*`, `tools/semdiff` (Cargo.toml:3); `oxc = 0.156`.
- Oracle `.github/workflows` and Rust analogue needed:
  - unit.yml (ubuntu-22.04): yarn install, check-and-rewrite-package-json, license files, treeshake check, imports-declared, prettier, lint, bundlesize, test:types, `vitest run --coverage`, production tests, perf: analogue = `cargo fmt --check`, `cargo clippy`, `cargo check --workspace --all-targets`, `cargo test --workspace`, napi build + harness vitest smoke, d.ts typecheck (integration-types)
  - web-test-runner.yml: integration (shadow modes × env) + hydration (engine-server vs SSR v2): analogue = run wtr against rollup-plugin on the Rust napi (after C1 port)
  - webdriver.yml: wdio integration: same, after C1
  - release.yml (manual, ubuntu-latest): version bump, `yarn build`, publish: analogue = napi matrix build (5+ triples, cross via `napi build --target` / zig / native runners), upload artifacts, `napi prepublish -t npm`, publish per-triple then main
  - benchmark*.yml: perf: harness perf row (D-0198: harness adds perf)
- Live-tree run, **NON-OFFICIAL** (PLAN §5: not reportable as numbers of record), `/workspace/wt/ship` @ ee6ebb1, `CARGO_TARGET_DIR=/workspace/wt/ship/target`, 2026-11-01 ~23:08-23:10 PST:
  - `cargo check --workspace`: PASS (exit 0, 20s).
  - `cargo test --workspace --no-fail-fast`: PASS (exit 0), 34 result lines, 368 passed, 0 failed, 0 ignored. No failing tests.
  - Builds required `/workspace/wt/parse5-rust` (P7).

## 5. Rust transform entrypoint (`crates/lwc-compiler/src/lib.rs`, harness-owned)
- `pub trait CompilerBackend { fn transform(&self, src:&str, filename:&str, options:&CompileOptions) -> Result<CompileResult, CompileError> }` lib.rs:4-11; `RustBackend` impl lib.rs:13-26 → `dispatch`.
- `pub fn dispatch(src:&str, filename:&str, options:&CompileOptions) -> Result<CompileResult, CompileError>` lib.rs:28-52: ext via `rsplit('.')`: `css` → `lwc_style::transform`; `html` + `target_ssr` → `lwc_ssr::transform`; `html` → `transform_template` (lib.rs:55+); `js|ts|mjs|jsx|tsx` + `target_ssr` → `lwc_ssr::transform`; else `lwc_js::transform`; other → uncoded CompileError. Missing `.mts` + LWC1005 (AB-0009).
- napi calls `lwc_compiler::dispatch` directly (napi lib.rs:457), not the trait.

## 6. Slice plan (ship)
Each slice: one branch, `scripts/land.sh`, Arch-Review trailer unless trivial. Numbers only from official board.
| slice | scope | owner of touched files | prove-it-works |
|---|---|---|---|
| S0 | **Packaging plan → Arch review** (doc only, `notes/`): npm names (`@lwc/compiler` name dies per CUTOVER.md, so new name TBD, e.g. `@lwc/compiler-rs`?), triple list, napi 2 vs 3 (P8), ESM loader design, d.ts source (AB-0008 schema), parse5 dep strategy (P7), throw-vs-return (P9) split with js-style, CI matrix. No product-code change before APPROVE | ship (notes) | reviews.tsv APPROVE row |
| S1 | `crates/lwc-compiler-napi/package.json` + `npm/linux-x64-gnu/` + ESM `index.js` loader + `@napi-rs/cli` build script; install-napi.sh untouched (harness-owned) | ship | in a scratch dir: `npm pack` main + linux-x64 pkg, `npm i` both tarballs into a temp project, `node -e "import('pkg').then(m=>m.transform(...))"` returns code for .js/.html/.css; harness board unchanged (loader not on board path) |
| S2 | Hand-written or generated `index.d.ts` (TransformOptions/Result/CompilerDiagnostic, `transformSync`, `version`) | ship | `tsc --noEmit` on a fixture importing every exported type; d.ts keys == RUST_OPTION_KEYS == option-gate 61 fields (script diff) |
| S3 | API parity in napi: export `transformSync` (+ `transform` Promise or D-id to drop), `version` (after AB-0010), throw instead of `{error}` (P9) | ship (export names) + **js-style** (lib.rs error surfacing) + harness (backend.ts follows) | board js-api/template-api/style-api/ssr_render_api unchanged on official board; a new seam test asserting `transformSync` throws Error with code/filename/location |
| S4 | Option-semantics gaps: name/namespace optional, apiVersion float, undefined-vs-omitted (enableStaticContentOptimization), `.mts`, LWC1005, arg validation | **harness** (AB-0008/AB-0009, crates/lwc-compiler) | ship files tests only; option-gate + unknown-option row green |
| S5 | CI for Rust repo (`.github/workflows/rust.yml`: fmt, clippy, check, test; napi matrix build + artifact) + `rust-toolchain.toml` | ship; P7 parse5 resolved with template | workflow green on a push to a fork/branch (needs remote: Migrate/Tariq item) |
| S6 | Multi-platform binaries (darwin x64/arm64, linux arm64, win x64, musl?) + publish dry-run (`npm publish --dry-run` per package) | ship | artifacts load on each runner (`node -e` smoke per triple) |
| S7 | Port @lwc/rollup-plugin (C1) to napi `transformSync` in a scratch copy of oracle (oracle stays clean; work in a separate checkout) | ship | oracle rollup-plugin unit tests + engine-server fixtures + ssr-runtime render tests pass against napi; wtr integration/hydration run |
| S8 | Move engine-server fixtures out of ssr-compiler (C3); ssr-runtime styles.spec (C5) onto napi; fix wtr `strict` (C7) | ship | engine-server + ssr-runtime tests green without ssr-compiler/style-compiler installed |
| S9 | Duplicate-table lint (D-0199): content-hash scan of `&[&str]`/phf/match tables across crates + vs lwc-shared; report-only first, findings routed to crate owner | ship (tool under `tools/`?) | lint run lists known seeds (VOID ×3, RESERVED ×2, see CUTOVER.draft) then 0 at cutover |
| S10 | `lwc` barrel (C2): drop compiler subpaths / repoint `./compiler` | ship | `node -e "import('lwc/compiler')"` resolves napi; removed subpaths 404 (documented D-id) |
| S11 | Deletes: 5 packages, vitest.config.mjs entries, docs, duplicated JS tables; one revertable commit | ship | CUTOVER.md "Verify" block (grep expects none; `yarn build && yarn test`) |

### Ownership conflicts / flags
- `crates/lwc-compiler` (harness): `.mts`, LWC1005, option semantics (S4) are harness's. Ship must not edit.
- `crates/lwc-compiler-napi/src/lib.rs`: export surface vs error surfacing split; D-0199 gives lib.rs error surfacing to js-style, packaging files to ship. Adding `#[napi] transformSync`/`version` exports is in lib.rs → **needs explicit owner call** (propose: ship owns `#[napi]` export fns, js-style owns error_result/aggregate/single_error).
- `crates/lwc-template/Cargo.toml` parse5 path dep (P7): template-owned file.
- `harness/src/backend.ts` RUST_OPTION_KEYS mirrors napi struct: any napi option change touches harness.
- `scripts/install-napi.sh` hardcodes `.so` (harness-owned `scripts/*.sh`); S1 must not break it.
- Consumer moves (S7, S8, S10, S11) edit the oracle repo's packages: must happen in a separate checkout/fork; `/workspace/lwc` must stay clean at b9e8007 (board refuses dirty oracle, D-0010).
- Rust repo has no remote: CI (S5) and publish (S6) need a hosting decision (Migrate/Tariq).
