# CSR DOM render-parity suite

`test/csr.render.test.ts` (helpers in `src/csrRender.ts`, rollup core shared with the SSR suite via
`src/ssrRender.ts` `bundleLwc` / `fixturePluginOptions`) compiles each fixture through the CompilerBackend
(`targetSSR: false`), links it with rollup, renders it client-side with the built `@lwc/engine-dom` dist under
jsdom (`createElement('fixture-test', { is })`, props, `document.body.appendChild`, three macrotask flushes), and
serializes the DOM byte-exact. Shadow roots (open or closed, captured from `attachShadow`) are inlined as the first
child `<template shadowrootmode="open|closed"[ shadowrootdelegatesfocus]>`; `<style>` nodes the engine adds to
`document.head` (light-DOM / global styles) are recorded separately as `head`. The result is compared to goldens
recorded from the JS backend.

Run from harness/ with `PATH=$LWC_ORACLE/node_modules/.bin:$PATH`:

    CORPUS=ssr|template|all npx vitest run test/csr.render.test.ts --pool=threads --testTimeout=5000 --reporter=dot
    # record goldens (JS backend only; refuses under rust)
    UPDATE_GOLDENS=1 npx vitest run test/csr.render.test.ts --pool=threads --testTimeout=5000 --reporter=dot
    # rust: use a pinned binary (never overwrite lwc-compiler-napi.node; pin = copy to tmp, then rename)
    LWC_COMPILER_BACKEND=rust LWC_COMPILER_NAPI_PATH=$PWD/.pins/lwc-compiler-napi.<sha12>.node CORPUS=ssr npx vitest run test/csr.render.test.ts ...
    # failure clusters for a run
    node scripts/csr-render-clusters.mjs .csr-render-out/rust-ssr/report-ssr.json [cluster] [n]

Env vars:
- CORPUS: `ssr` (A), `template` (B) or `all` (default). Goldens and results are identical whichever you pick.
- UPDATE_GOLDENS=1: re-record `goldens/csr-render/{ssr-fixtures,template-fixtures}.json` (JS backend only).

Corpora:
- A, ssr-fixtures (387): `@lwc/ssr-compiler/src/__tests__/fixtures/**/config.json`, with `entry`, `props` and
  `experimentalComplexExpressions` from config.json and the same plugin options as upstream fixtures.spec.ts
  (enableDynamicComponents, enableLwcOn, experimentalDynamicDirective, modules dir, dynamicImports loader), minus
  ssrMode. Compile errors throw on any rollup warning except CIRCULAR_DEPENDENCY, as upstream does.
- B, template-fixtures (567): every `@lwc/template-compiler/src/__tests__/fixtures/**/actual.html`, compiled with
  exactly the template.behavior call `transform(src, '<name>.html', { namespace: 'x', name, ...config.json })` as the
  template of `x/<name>`, a minimal `LightningElement` (`static renderMode = 'light'` when the root template has
  `lwc:render-mode="light"`). Any other `ns/name` it imports is an empty stub component. Template warnings are
  recorded as `warnings` rather than failing the bundle. Render errors from props the minimal component doesn't
  define (for example `Cannot read properties of undefined`) are expected, deterministic data.

Golden entry per fixture: `{ status: ok | compile-error | render-error | excluded, html, head?, error?, errors?,
logs?, warnings?, reason?, note? }`. `error` is the compile (bundle) error message. `errors` holds thrown
`createElement` / mount errors, window `error` events and unhandled rejections during the fixture. `logs` holds the
engine's console output. Nothing is skipped: unrenderable fixtures record their error text. Error stacks are dropped,
the scratch dir becomes `<out>`, and engine-dom's global VM counter in messages (`[object:vm X (#)]`) is normalized,
because it depends on how many components rendered earlier in the run.

Per-run output: `.csr-render-out/<backend>-<CORPUS>/report-<corpus>.json` (one row per fixture, with got/want for
mismatches; `ssrExpectedMatch` says whether the CSR DOM, run through formatHTML, equals upstream's SSR expected.html.
That is a diagnostic only, 225/387 on JS). The directory is wiped on every run, so trust it only after a full
(non `-t`) run. The bundles are written there too, so vite inlines them with the test's engine-dom instance.

Places this does not mirror upstream:
- No upstream CSR fixture runner exists. The rendering protocol (tag `fixture-test`, macrotask flushes, serializer)
  is this harness's own. Native shadow only (no synthetic-shadow polyfill), dev-mode engine-dom dist, jsdom 29
  (no constructable stylesheets, so styles show up as `<style>` elements, and no `ShadowRoot#delegatesFocus`, which
  is taken from the attachShadow init).
- Upstream `.skip` markers are honored as `excluded` (scope-token, wire/errors/throws-when-colliding-prop-then-method).
  Several SSR fixtures assert SSR-only behavior (`renderedCallback should not be reachable in SSR`, slotchange,
  rehydration) and fail on CSR in the JS oracle too. That is recorded as render-error, not skipped.
  barrel-lwc-exports/* render-error on JS as well (the SSR suite skips them for a dist `unwrap$1` rename).
- Template corpus: the `experimentalComputedMemberExpression` config key (expression/computed) is dropped with a
  `note`. It is not an @lwc/compiler TransformOption, so transformSync ignores it and the rust seam would reject it.
  That fixture therefore records a compile error.
- `@lwc/ssr-runtime` imports (one barrel fixture) resolve to its dist. `lwc` resolves to the single engine-dom dist instance the
  test imports, because one customElements registry rules out per-fixture engine instances.

Suggested board.sh rows: `csr-render` → `CORPUS=ssr npx vitest run test/csr.render.test.ts --pool=threads
--testTimeout=5000 --reporter=dot`, `csr-render-template` → the same with `CORPUS=template`.
