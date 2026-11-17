# CSR DOM render-parity suite

`test/csr.render.test.ts` (helpers in `src/csrRender.ts`, rollup core shared with the SSR suite via
`src/ssrRender.ts` `bundleLwc` / `fixturePluginOptions`) compiles each fixture through the CompilerBackend
(`targetSSR: false`), links it with rollup, renders it client-side with the built `@lwc/engine-dom` dist under
jsdom (`createElement('fixture-test', { is })`, props, `document.body.appendChild`, three macrotask flushes), and
serializes the DOM byte-exact. Shadow roots (open or closed, captured from `attachShadow`) are inlined as the first
child `<template shadowrootmode="open|closed"[ shadowrootdelegatesfocus]>`; `<style>` nodes the engine adds to
`document.head` (light-DOM / global styles) are recorded separately as `head`. After that first-mount record the
runner applies a deterministic step list derived from the fixture (see below), flushing three macrotasks after
every step, then unmounts as before so `disconnectedCallback` still runs. The result is compared to goldens
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
- A, ssr-fixtures (392 = 387 oracle + 5 harness): `@lwc/ssr-compiler/src/__tests__/fixtures/**/config.json`, with `entry`, `props` and
  `experimentalComplexExpressions` from config.json and the same plugin options as upstream fixtures.spec.ts
  (enableDynamicComponents, enableLwcOn, experimentalDynamicDirective, modules dir, dynamicImports loader), minus
  ssrMode, plus `harness/fixtures/ssr/**/config.json` (rel paths under that root, prefixed `csr-steps/`). Compile
  errors throw on any rollup warning except CIRCULAR_DEPENDENCY, as upstream does.
- B, template-fixtures (568 = 567 oracle + 1 harness): every `@lwc/template-compiler/src/__tests__/fixtures/**/actual.html`,
  plus `harness/fixtures/template/**/actual.html`, compiled with exactly the template.behavior call
  `transform(src, '<name>.html', { namespace: 'x', name, ...config.json })` as the template of `x/<name>`, a minimal
  `LightningElement` (`static renderMode = 'light'` when the root template has `lwc:render-mode="light"`). A sibling
  `actual.css` (harness fixtures only; the oracle set has none) is the component stylesheet. Any other `ns/name` it
  imports is an empty stub component. Template warnings are recorded as `warnings` rather than failing the bundle.
  Render errors from props the minimal component doesn't define (for example `Cannot read properties of undefined`)
  are expected, deterministic data.

Golden entry per fixture: `{ status: ok | compile-error | render-error | excluded, html, head?, error?, errors?,
logs?, steps?, warnings?, reason?, note? }`. `error` is the compile (bundle) error message. `errors` holds thrown
`createElement` / mount errors, window `error` events and unhandled rejections during first mount and the unmount
that follows the steps (step failures stay on the step). `logs` holds console output from those same windows,
including `debug` and `trace`. `steps` is omitted when the fixture drives no step class. Nothing is skipped:
unrenderable fixtures record their error text. Error stacks are dropped, the scratch dir becomes `<out>`, and
engine-dom's global VM counter in messages (`[object:vm X (#)]`) is normalized, because it depends on how many
components rendered earlier in the run.

## Step protocol

`src/csrSteps.ts` (`analyzeFixture`, `installStepRuntime`, `actStep`, `observeStep`, `instrumentCsrBundle`) derives
the plan from the fixture's JS, HTML, and `config.json` props — for corpus B, from the virtual component source
`templateComponentSource` actually emits. Generation order is stable: prop, event, list, dynamic, slot, lifecycle.
A class with nothing to drive contributes no steps.

Each step observation is `{ id, class, html, head?, errors?, logs?, note?, handlers?, ids?, slotchange?, lifecycle? }`.
`html` / `head` use the same serializer as first mount. Throws are `name: message`.

1. **prop** — each root `@api` / `@track` field and each `config.json` prop key, except a field that is an
   `lwc:is` / `lwc:dynamic` binding. The value is a deterministic alternate (arrays of length ≥ 2 reverse; shorter
   arrays gain `'step'`; strings gain `·`; numbers `+ 1`; booleans flip; objects gain `__step`). Writes go to the
   component instance so `@track` setters run. Id `prop:<kind>:<name>`.
2. **event** — types from `onfoo={...}`, `on:foo={...}`, `lwc:on={'foo'}`, and `addEventListener('foo')`.
   `EventTarget.prototype.addEventListener` is wrapped (and `removeEventListener` maps the wrapper back) so a
   dispatch records whether that listener fired and whether it threw. Id `event:<type>`.
3. **list** — `for:each` / `iterator:<name>` bindings that are a root field or a config prop. Three steps: reorder
   (reverse, or append `'step-reorder'` when shorter than 2), insert (`'step-insert'` or a cloned last object),
   remove (drop the last item). Node identity is a monotonic stamp in a `WeakMap`, assigned inside the patched
   `document.createElement`, `document.createElementNS`, and `Node.prototype.cloneNode` (the clone and its
   descendants), reset at the start of each `render()`. The step's `ids` map is tree path → stamp (`s/` crosses a
   shadow root). Stamps are not written into attributes, so first-mount `html` does not change just to carry them.
4. **dynamic** — `lwc:is` / `lwc:dynamic` identifier. Phase A reassigns the current constructor; phase B assigns
   another root field whose initializer is an identifier (only when the source has one — a single-ctor corpus
   fixture does not invent a second); then `null`.
5. **slot** — when any template contains `<slot`. Light-DOM children of the host: add `slot-add`, remove it,
   replace `slot-old` with `slot-new`. `slotchange` fires (and throws) are counted from a harness listener on each
   `<slot>` plus the fixture's own listener wrapper.
6. **lifecycle** — when the source declares `constructor`, `connectedCallback`, `render`, `renderedCallback`,
   `disconnectedCallback`, or `errorCallback`. `registerComponent` in the bundle is routed through a tracer that
   wraps those methods (return values unchanged) and subclasses the component so `constructor` is recorded without
   renaming the class (`name` and statics are copied). The observation lists `{ hook, id }` in order (`c1`, `c2`,
   …). `disconnectedCallback` is appended from the existing unmount after the steps.

The bookkeeping test titles stay `finds 387 ssr fixtures` and `finds 567 template fixtures` (board case ids). They
assert both the oracle count and `expectCount`. Harness fixtures live only under `harness/fixtures/`.

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
