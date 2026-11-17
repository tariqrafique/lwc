# LWC compiler option-coverage audit (harness × Rust seam)

Read-only audit, run Nov 1, 2026 (PST). No goldens, tests, or crates were changed.

- Upstream oracle: `/workspace/lwc` @ b9e8007. It has local uncommitted patches; see §3.
- Probes: `/tmp/cr*.mjs` and `/tmp/cfgscan.mjs`.
- Rust addon: `harness/lwc-compiler-napi.node`, built 19:22 PST.
- Counts come from rg or node scans of fixture `config.json` files.

## Suites and Rust results

Each suite was run with `LWC_COMPILER_BACKEND=rust`. All golden directories are fully populated, so nothing was written.

| Suite | Golden dir | Fixtures | Rust result |
|---|---|---|---|
| js | goldens/js | 173 | all pass |
| style | goldens/style | 69 | all pass |
| template | goldens/template | 567 | 157 fail |
| ssr, sync | goldens/ssr | 387 | 388/388 tests pass |
| ssr, async | goldens/ssr-async | 387 | 372 fail / 16 pass |
| ssr, asyncYield | goldens/ssr-asyncYield | 387 | 372 fail / 16 pass |

## 1. Prioritized gaps (most likely to hide real parity bugs first)

1. **`ssrMode` is GAP-UNFORWARDED inside the crate.** The harness and napi both forward it, but lwc-ssr never reads it.
   - `compile_js/mod.rs:1635` hard-codes `'sync'`.
   - The compile_template codegen is sync-only (`component.rs:1`, `slot.rs:1`, `mod.rs:1,17,48,215`).
   - Measured: async 372/388 fail and asyncYield 372/388 fail, against sync 388/388 pass.
   - The harness row exists (`ssr.behavior.test.ts:9-11`, `SSR_MODE` env). The work is in the crate.
   - Note that the upstream `fixtures.spec.ts:54` runs only sync. async/asyncYield are oracle-only goldens; upstream spec coverage of async is just `transmogrify.spec.ts:62-63`.

2. **`customRendererConfig` is GAP-UNFORWARDED at the seam. This is a silent false-pass risk.**
   - 14 template fixtures carry it (9 distinct configs).
   - It is not in `CompileOptions` (`backend.ts:6-17`), the Rust destructure (`backend.ts:69-80`), napi `JsCompileOptions` (`lib.rs:8-45`), or `lwc_shared::CompileOptions` (`lib.rs:79-106`).
   - lwc-template supports it (`config.rs:23`), but `lwc-compiler/src/lib.rs:55-72` fills it with `..Default::default()`, which is None.
   - Probe on `renderer-hooks/config/tagname-no-attributes`: "renderer" occurs 7× in JS-with-cfg output, 2× in JS-without-cfg output, and 2× in Rust-with-cfg output. Rust behaves as if the option were omitted.
   - Rust currently fails 13 of the 14 fixtures. `TemplateEval` records only imports, lwcImports, kind, registered, stylesheetToken and the stylesheet shape. Once the other template diffs are fixed, renderer-hook differences that do not change imports (for example `renderer:` keys on `api_element` data) could pass silently.
   - The spec also exercises it: `compiler/transform-html.spec.ts:86-111`.

3. **`experimentalDynamicDirective` has a default and false-path divergence, and no fixture covers the omitted case.**
   - `transformSync` derives `experimentalDynamicDirective ?? Boolean(dynamicImports)` (`compiler/src/transformers/template.ts:54`). Since `normalizeOptions` always materializes `dynamicImports`, the transformSync default is effectively **true**.
   - template-compiler's own default is false (`config.ts:428-442`). Rust maps None to false (`lwc-template/src/config.rs:81`).
   - Probe with `<x-foo lwc:dynamic={ctor}>`:

     | Value | JS result | Rust result |
     |---|---|---|
     | omitted | warnings `[1187]` | warnings `[]` |
     | `false` | throws LWC1128 | no error, no warnings |
     | `true` | `[1187]` | `[1187]` |

   - Fixture scan: all 17 template fixtures that use `lwc:dynamic` set the key explicitly. **0 fixtures exercise the omitted (transformSync-default) path.**
   - Rust fails 14/17 of these fixtures, including `directive-dynamic/invalid-config` (false).

4. **`enableLightningWebSecurityTransforms` (LWS) is GAP-UNFORWARDED.**
   - `lwc_shared` has the field and lwc-js uses it (`lwc-js/src/lib.rs:28`). backend.ts and napi drop it.
   - Probe with LWS true on an async method: JS emits `asyncToGenerator`; Rust does not.
   - 0 fixtures. Spec only: `transform-javascript.spec.ts:84-116`, plus the rollup compilerConfig spec. The harness's `ssrRender.ts:176` passes it to `backend.transform`, but RustCompilerBackend drops it there too.

5. **`isExplicitImport` is GAP-UNFORWARDED (latent).**
   - Probe with `isExplicitImport: true`: JS omits the implicit `.html` import; Rust keeps it.
   - The 7 js fixtures all use `false`, which is the default, so the harness can't see the drop. The field exists in `lwc_shared` and is used in `component.rs:22`.

6. **`outputConfig.sourcemap` is GAP-UNFORWARDED.**
   - Probe with `sourcemap: true`: JS returns a map; Rust returns none.
   - The style suite asserts `map ?? ''` is `''` (`style.behavior.test.ts:41`), and the ssr_render plugin forces `map:null`, so the harness pins it to false everywhere.
   - Spec: `'inline'` at `transform-javascript.spec.ts:269`, `true` at `:285`, invalid value → LWC1021 at `:298-311`.

7. **`enablePrivateMethods` is GAP-PINNED to true in the js suite** (`js.behavior.test.ts:44`).
   - The upstream babel spec always runs both private-method plugins but passes `enablePrivateMethods` undefined to the component plugin. Under it `component.ts:133` does **not** emit the `enablePrivateMethods: true` registerComponent key. With transformSync, `false`/omitted also skips both transforms (`compiler/src/transformers/javascript.ts:73,76`).
   - The `false`/omitted paths are never compared. 22 js fixtures contain `#method(` syntax.
   - `evalJs.ts:106-111` `registerOptions` does not record the key, so the pinned `true` key is unobserved too.

8. **Dynamic-import `strictSpecifier` default is muddled across the layers:**

   | Layer | Default `strictSpecifier` |
   |---|---|
   | Shipped `options.ts:44-49` | true |
   | Oracle, locally patched | false (`git diff`) |
   | napi (`lib.rs:114-116`) | false |
   | `lwc_shared::Default` (`lib.rs:126`) | true |

   - The js fixtures carry only explicit `strictSpecifier: true` (5).
   - The default path comes only from the `dynamic-imports/passthough-with-no-config` SEAM_BRIDGE (`js.behavior.test.ts:20-23`) and from ssr BASE `strictSpecifier:false` (`ssr.behavior.test.ts:23-26`).
   - Probe: omitted and `{}` both pass through on JS (patched oracle) and on Rust. They agree only because of the patch.
   - The ssr-compiler `dynamic-imports.spec.ts:19-23` matrix (strict {T,F} × loader {undefined,'myLoader'}) is not in the harness.

9. **`apiVersion` is COVERED for template (from cfg) but thin elsewhere.**
   - js: only 59×4. The spec also uses 60 (`transform-javascript.spec.ts:67-78`).
   - style and ssr: never varied. The ssr crate forwards it to the template (`lwc-ssr/src/lib.rs:72`).
   - The `null` value (1 template fixture) is deleted at the seam (`backend.ts:82`). That is fine because JS normalizes null the same way.
   - Rust `DEFAULT_API_VERSION=66` (`lwc-template/src/config.rs:62`) matches upstream HIGHEST (V66, `shared/src/api-version.ts:10-20`).

10. **`instrumentation` is GAP-UNFORWARDED.** It is a callback object and can't cross napi as JSON. Spec only: `transform-javascript.spec.ts:136-153`, `transform-html.spec.ts:213`.

### Per-fixture config ignored by a suite

- **style**:
  - `customProperties.resolverModule` (8 fixtures) is remapped to `stylesheetConfig: {customProperties}` (`style.behavior.test.ts:31-39`). That shape is wrong: transformSync expects `resolution` (`compiler/src/transformers/style.ts:28-41`). It is also deprecated.
  - **Verified harmless.** style-compiler declares `resolverModule` (`transform.ts:21-23`) but never reads it. Compiling all 8 fixtures directly with and without it gives identical output (0/8 differ). Recommend removing the remap, or commenting that it is dead.
- **ssr** (`ssr.behavior.test.ts`): ignores `props`, `styleDedupe` and `ssrFiles.expected`/`error`. These are render-only. `ssr_render.behavior.test.ts:106` honors them, so this is OK.
- **template**: spreads the whole config, so nothing is ignored.
- **js**: maps `parserOpts.errorRecovery` to `experimentalErrorRecoveryMode` and spreads the rest, so nothing is ignored.

## 2. Option table

Legend:
- **COVERED**: harness varies it and Rust forwards it.
- **GAP-PINNED**: a single fixed value only.
- **GAP-UNFORWARDED**: dropped at or after the Rust seam.
- **N/A**: not applicable (reason given).

Seam column: forwarded through backend.ts → napi → lwc_shared?

| Option | JS default | Fixture values (count) / spec values | Harness per suite | Rust seam | Verdict | Proposed row (est. fixtures, golden key) |
|---|---|---|---|---|---|---|
| name / namespace | required | js: name camelCase×1, test×1; ns x×2. Template uses the fixture dir name | js `lwc`/`test`+cfg (:44); template `x`/name (:50); style default; ssr from path | yes | COVERED | — |
| apiVersion | HIGHEST (66) via `getAPIVersionFromNumber` (`options.ts:248`) | template 58×1 59×6 61×2 62×3 null×1; js 59×4; spec 58/59/60 | template, js from cfg; style, ssr never | yes | COVERED (template) / GAP-PINNED (style, ssr, js≠60) | js×{59,60,omitted} on rest-spread and decorator fixtures (~10, `js-api{N}/<rel>.json`) |
| scopedStyles | undefined (false) | style `scoped:true`×5 | style `!!scoped`; others omitted | yes | COVERED | — |
| disableSyntheticShadowSupport | false | template T×2 F×2; style T×6 | template, style cfg | yes | COVERED | — |
| experimentalErrorRecoveryMode | false | js `parserOpts.errorRecovery`×12; style T×5; spec erm true | js mapped; style cfg; template none | yes | COVERED (js, style); template never | template erm subset (spec `transform-html.spec.ts:228-231`, ~5, `template-erm/`) |
| enablePrivateMethods | undefined | spec T/F/omitted (`transform-javascript.spec.ts:227-254`); fixtures none | js pinned **true** | yes | GAP-PINNED | js×{omitted} over the 22 `#method` fixtures; record the registerComponent key in evalJs (`js-epm-off/<rel>.json`) |
| experimentalComputedMemberExpression | false (oracle patch adds it) | template T×1 | template cfg; bridge `expression/computed` | yes | COVERED (1) | — |
| experimentalComplexExpressions | false | template T×69; ssr T×5 | template, ssr, ssr_render cfg | yes | COVERED | — |
| preserveHtmlComments | false | template T×5 | template cfg; ssr never | yes | COVERED (template) / ssr never | ssr: none of the 387 set it; low priority |
| enableStaticContentOptimization | true | template F×29 T×19; spec undef/F/T | template cfg; ssr_render true | yes | COVERED | — |
| enableDynamicComponents | undefined (false) | template T×38 F×1 | template cfg; ssr pinned true | yes | COVERED / ssr GAP-PINNED (matches spec) | — |
| enableLwcOn | undefined (false) | template T×7 | template cfg; ssr pinned true | yes | COVERED / ssr pinned (matches spec) | — |
| experimentalDynamicDirective | transformSync: **true** via `Boolean(dynamicImports)`; template-compiler false; Rust None→false | template F×1 T×16; spec T/F (`transform-html.spec.ts:170-196`) | template cfg; ssr pinned true | yes, but default differs | GAP (default path never exercised) | template `lwc:dynamic` fixtures × {omitted} (17, `template-edd-default/<rel>.json`) |
| enableSyntheticElementInternals | undefined | js T×1 | js cfg | yes | COVERED (1) | — |
| componentFeatureFlagModulePath | `''` | js ×1 | js cfg | yes | COVERED (1) | — |
| dynamicImports.loader | `''` | js `@custom/loader`×2, null×3; ssr BASE `/virtual/custom-loader.js` | js cfg; ssr pinned | yes | COVERED | — |
| dynamicImports.strictSpecifier | shipped true / oracle-patched false | js T×5; ssr BASE false; ssr spec matrix | js cfg; ssr pinned false | yes (napi default false ≠ `lwc_shared` true) | GAP-PINNED (default) | js `dynamic-imports/*`×{omitted, strict F} (~8, `js-dyn-{mode}/<rel>.json`); port the ssr `dynamic-imports.spec` matrix (4 × 2 sources) |
| isExplicitImport | false | js F×7 | js cfg | **dropped** (backend.ts:69-80, napi) | GAP-UNFORWARDED (latent) | js×{true} on component fixtures (~20, `js-explicit/<rel>.json`) |
| enableLightningWebSecurityTransforms | false | spec T/F only | ssrRender passes it; backend drops it | **dropped** | GAP-UNFORWARDED | js×{LWS true} on async/generator fixtures (~10, `js-lws/<rel>.json`) |
| customRendererConfig | undefined | template 14 (9 distinct); spec | template cfg spread | **dropped** | GAP-UNFORWARDED | already in the template suite (14). Forward it, and add a renderer-hook signal to TemplateEval |
| targetSSR | false | ssr true | ssr, ssr_render true | yes | COVERED | — |
| ssrMode | `'sync'` (`shared/src/ssr.ts:9-11`) | fixtures none; spec sync, async (transmogrify) | ssr, ssr_render via `SSR_MODE` env | napi yes; **lwc-ssr ignores it** | GAP-UNFORWARDED (crate) | existing `ssr-async`/`ssr-asyncYield` rows (387 each) |
| instrumentation | undefined | spec only | never | dropped (callback) | N/A at seam: callback, not JSON | optional: napi callback or log-capture shim; spec-derived (2) |
| outputConfig.sourcemap | false (accepts `'inline'`) | spec true / `'inline'` / invalid | pinned false | **dropped** | GAP-UNFORWARDED | js×{true, `'inline'`} + LWC1021 error (~5, `js-sourcemap/<rel>.json`, keyed on map presence/shape) |
| outputConfig.minify | false | spec warns (`options.spec.ts:31-41`) | never | dropped | N/A (deprecated, warn-only) | — |
| stylesheetConfig.customProperties | `{resolution:{type:'native'}}` | style 8 (`resolverModule`) | style mis-remapped | dropped | N/A: `resolverModule` is dead in style-compiler (0/8 differ) | delete the remap |

## 3. Oracle caveat

`/workspace/lwc` has local edits (`git diff`):

- `options.ts`: `DEFAULT_DYNAMIC_IMPORT_CONFIG.strictSpecifier` is changed from true to **false**. `experimentalComputedMemberExpression` is added to `DEFAULT_OPTIONS` and `TransformOptions`.
- `transformers/template.ts`: forwards `experimentalComputedMemberExpression`.

The goldens therefore encode a non-shipped `strictSpecifier` default. Record this in the board, or key goldens on it if the patch is ever dropped.
