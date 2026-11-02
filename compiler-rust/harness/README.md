# LWC compiler migration harness (Phase 0)

Behavioral parity harness for the `CompilerBackend` seam
(`transform(src, filename, options) -> { code, map? }` or throws). JS reference: `@lwc/compiler` `transformSync`.
Rust mirror: `crates/lwc-compiler` (`CompilerBackend` trait + `dispatch`).

**This is NOT the 69/69 style byte gate.** Byte fixtures (`expected.js`) belong to the PostCSS-Rust port.
Here we evaluate the emitted ESM and compare *behavior*:
- flatten `export default` (arrays/imports stubbed), check `$scoped$` / `$nativeOnly$` flags
- call each `stylesheet(token, useActualHostSelector, useNativeDirPseudoclass)` for
  `x|true|false`, `x|false|true`, `x|false|false`, `x|true|true`, `undefined|true|false`
- compare the evaluated CSS strings to goldens in `goldens/style/*.json` (recorded from the JS backend)
- error fixtures (non-empty `error.json`): must throw; line checked when available; name/line/column stored in golden
- style `map` must be `""` (JS `{mappings:""}` is normalized to `""`)

## Run
```
# prerequisites: /workspace/lwc has node_modules and @lwc/compiler built
cd /workspace/lwc && npx nx run @lwc/compiler:build
cd /workspace/lwc-compiler-rust/harness
npm run link:lwc            # symlinks @lwc/compiler + vitest (equivalent of the file: dep; yarn not on box)
npx vitest run              # or: yarn test
UPDATE_GOLDENS=1 npx vitest run   # re-record baseline — JS backend ONLY
```

## Goldens policy (Migrate-approved)
- Style 70/70 JS behavioral baseline is **accepted**. Goldens are the JS oracle.
- **Do not** `UPDATE_GOLDENS=1` with `LWC_COMPILER_BACKEND=rust` until a napi/wasm binding of `lwc_compiler::dispatch` exists.
- After binding: run rust against the existing JS goldens; only re-record if Migrate explicitly changes the oracle.

## Backends
`LWC_COMPILER_BACKEND=js` (default) | `rust`.

**Rust (napi-rs facade):** crate `crates/lwc-compiler-napi` → `lwc_compiler::dispatch`.
```
source "$HOME/.cargo/env"
cd /workspace/lwc-compiler-rust && cargo build -p lwc-compiler-napi --release
cp target/release/liblwc_compiler_napi.so harness/lwc-compiler-napi.node
cd harness && LWC_COMPILER_BACKEND=rust npx vitest run test/style.behavior.test.ts
```
Override addon path with `LWC_COMPILER_NAPI_PATH`. Style behavioral **70/70** green on rust against JS goldens (no UPDATE_GOLDENS). Template/js/ssr on rust wait on those crates.

## Template suite (Phase 0d v1) — `test/template.behavior.test.ts`
Walks 567 `@lwc/template-compiler` fixtures (`**/actual.html`, + `config.json`), compiles through the backend as
`<dir>.html` with `{namespace:'x', name:<dir>, ...config}` (same as upstream fixtures.spec). Asserts:
- outcome: fixture `metadata.json` has a level ≤1 diagnostic (Fatal/Error) ⇒ transform must throw; error `code` must match the first error code
- success: `warnings` **codes + levels** (multiset) equal the fixture's `metadata.json` (no message text)
- `src/evalTemplate.ts` evaluates the ESM with stubbed imports (`lwc` named imports recorded, `registerTemplate` = identity, css → undefined, components → stub);
  default export must be a function and registered; golden records imports, lwc imports, `stylesheetToken`, `cssScopeTokens`
- goldens: `goldens/template/*.json` (JS oracle). `UPDATE_GOLDENS=1` with `LWC_COMPILER_BACKEND=rust` throws at load.
- seam gaps (`SEAM_GAPS` in the test): `expression/computed` — `experimentalComputedMemberExpression` isn't forwarded by `transformSync`, so it throws via the seam.

Not covered yet: **0d.1** — render through `@lwc/engine-dom` (jsdom) and compare DOM/vnode output. Next: js / ssr suites.

## JS suite (Phase 0d v1) — `test/js.behavior.test.ts`
Walks 173 `@lwc/babel-plugin-component` fixtures (`**/actual.js` + `config.json`; richest JS corpus — `@lwc/compiler`
has only `options.spec.ts`). Compiles via the backend as `test.js` with `{namespace:'lwc', name:'test', enablePrivateMethods:true, ...config}`
(same base as upstream index.spec, which always runs the private-method transforms); `parserOpts.errorRecovery` → `experimentalErrorRecoveryMode`.
Asserts:
- outcome: non-empty `error.json` or `lwcErrors.json` ⇒ must throw; every `LWCxxxx` code in the fixture must appear in the thrown error (code / aggregate `errors[]` / message)
- success: `src/evalJs.ts` evaluates the ESM (lowered to CJS by `@babel/plugin-transform-modules-commonjs` as a harness-side eval aid only) with stubs:
  `lwc` → `LightningElement` base + recording `registerComponent` / `registerDecorators`; other imports → recording proxies; undeclared free identifiers (e.g. `mixin`) → identity.
  Golden records imports, lwc imports, export names+kinds, default export (class? extends LightningElement? registered? `sel`, apiVersion type, tmpl, feature flag, synthetic internals),
  `registerDecorators` metadata shape, and warning code+level (transformSync currently returns none for JS).
- goldens: `goldens/js/*.json` (JS oracle). `UPDATE_GOLDENS=1` with `LWC_COMPILER_BACKEND=rust` throws at load.
- seam gaps (`SEAM_GAPS`): `dynamic-imports/passthough-with-no-config` — transformSync defaults `dynamicImports.strictSpecifier=true` (plugin default off) ⇒ LWC1121 throw via the seam.

Not covered yet: **0d.2** — engine event smoke: instantiate the registered class via `@lwc/engine-dom` (jsdom), mount, and check
public props / `@wire` / `@track` reactivity and lifecycle callbacks.

## SSR suite (Phase 0d v1) — `test/ssr.behavior.test.ts`
Walks 387 `@lwc/ssr-compiler` fixtures (`**/config.json`; each has `modules/<ns>/<name>/*.{js,html,css}`, 1263 module files).
Every module file is compiled individually through the backend with `{targetSSR:true, ssrMode:'sync', namespace, name,
enableDynamicComponents, enableLwcOn, experimentalDynamicDirective, dynamicImports:{strictSpecifier:false}}` (+ `experimentalComplexExpressions`
from config) — the per-file equivalent of upstream `fixtures.spec.ts`'s rollup options. Asserts:
- outcome: fixture error file (`ssrFiles.error` ?? `error.txt`) starting `LWCxxxx:` ⇒ some module must throw with that code;
  a non-coded error text that matches a thrown compile message (inner-outer-html) ⇒ accepted compile error; otherwise every module must compile.
  Remaining non-empty error files are render-time errors (golden flag `renderErrorDeferred`, 0d.3).
- success: `src/evalSsr.ts` evaluates the ESM (CJS-lowered, harness-only) with `@lwc/ssr-runtime` stubbed (LightningElement base,
  recording `setStaticInternals` / `registerTemplate`), css → undefined, other imports → stub proxies. Golden records imports, touched runtime
  names, export kinds, default kind, `extends LightningElement`, static internals (tagName, public props, ssrMode, has tmpl),
  template registered + `stylesheetScopeToken` / `hasScopedStylesheets`, and warning code+level. `.html` must export a registered function.
- goldens: `goldens/ssr/*.json` (JS oracle). `UPDATE_GOLDENS=1` with `LWC_COMPILER_BACKEND=rust` throws at load. `SEAM_GAPS`: none.

Not covered yet: **0d.3** — HTML render compare: needs module resolution/bundling (rollup + `@lwc/rollup-plugin` or a harness resolver over
backend output) then `serverSideRenderComponent` + `formatHTML` vs `expected.html` / `expected-ssr.html`, including the 18 render-time error fixtures.

## Official board (pinned binary)
- Install napi only via `../scripts/install-napi.sh` (tmp + `mv`, atomic). Never `cp` over `lwc-compiler-napi.node` — a live vitest fork may have it mmap'd (SIGBUS).
- Measure only via `./board.sh [suite...]`: snapshots the current `.node` to `.pins/lwc-compiler-napi.<sha12>.node` (read-only), runs every suite against that one file via `LWC_COMPILER_NAPI_PATH`, `--pool=threads`, and prints the hash. Board numbers without a hash are not official.
