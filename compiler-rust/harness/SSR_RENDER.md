# SSR render-HTML parity suite

`test/ssr.render.test.ts` (helpers in `src/ssrRender.ts`) ports upstream
`@lwc/ssr-compiler/src/__tests__/fixtures.spec.ts`. It compiles each fixture through
the CompilerBackend, links it with rollup, renders it with @lwc/ssr-runtime
`serverSideRenderComponent(..., SSR_MODE)` plus `formatHTML`, and compares the output
byte-for-byte with `expected.html` / `error.txt`.

Run from harness/ with `PATH=$LWC_ORACLE/node_modules/.bin:$PATH`:

    SSR_MODE=sync|async|asyncYield npx vitest run test/ssr.render.test.ts --pool=threads --testTimeout=5000 --reporter=dot
    # rust: pin the binary first (never overwrite lwc-compiler-napi.node)
    h=$(sha256sum lwc-compiler-napi.node|cut -c1-12); mkdir -p .pins; cp -n lwc-compiler-napi.node .pins/lwc-compiler-napi.$h.node
    LWC_COMPILER_BACKEND=rust LWC_COMPILER_NAPI_PATH=$PWD/.pins/lwc-compiler-napi.$h.node SSR_MODE=sync npx vitest run test/ssr.render.test.ts ...

Env vars:
- SSR_RUNTIME=dist (default; built dist) or src (upstream-style src aliasing).
- UPDATE_GOLDENS=1: re-record goldens/ssr-render/<mode>[.src].json. This works with the JS backend only.

Per-run output: `.ssr-render-out/<backend>-<mode>-<runtime>/report.json`, one row per
fixture. It is wiped on every run, so trust it only after a full (non `-t`) run.

Mode goldens hold only the cases where the upstream JS oracle can't match expected.html,
each with a reason:
- async: the runtime's setStaticInternals builds the yield generator only for asyncYield,
  so async renders "[object Promise]", and its render errors become unhandled rejections.
  The suite collects those rejections and checks each one against the expected error texts.
- asyncYield: dynamic-imports/basic and rehydration differ in microtask ordering.

Runtime rule (D-0242, `ssrRenderRuntime`): barrel-lwc-exports/{export-all,export-all-alias,export-named}
always bundle and render against the ssr-runtime TypeScript source. The dist build renames
`unwrap` to `unwrap$1`, which is a runtime-build artifact. Every other row uses `SSR_RUNTIME`
(default dist).

Upstream `.skip` markers stay skipped except two rows (`ssrUpstreamSkip`), which run on both
backends against JS-oracle output:
- scope-token (W-17972327 CI flake). sync and asyncYield match expected-ssr.html. async renders
  `[object Promise]` on both backends; that cell passes via the async golden and is logged on
  the board as `known-limitation: ssr-render-async scope-token (D-0242) both backends render [object Promise]`.
- wire/errors/throws-when-colliding-prop-then-method (engine-core dev/prod). SSR renders error-ssr.txt.

The same two rows run in ssr_render_api (7 apiVersion cells each).
