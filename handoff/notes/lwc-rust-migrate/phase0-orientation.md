# Phase 0 orientation — LWC Harness Cutover (file inventory focus)
Scope note: PROGRAM.md + Phase 0 canvas own the contract; this is the inventory to implement against. LWC 9.5.3 local checkout.

## A. JS entrypoints / delete-at-cutover
- `@lwc/compiler` `src/index.ts`: exports `transform` (async wrapper), `transformSync`, `version`; types `TransformResult`, `TransformOptions`, `NormalizedTransformOptions`, `StylesheetConfig`, `DynamicImportConfig`, `OutputConfig`, `CustomPropertiesResolution`.
- Dispatch `src/transformers/transformer.ts::transformFile` by `path.extname`:
  - `.html` → `targetSSR ? ssr-compiler.compileTemplateForSSR(src,file,opts,ssrMode) : template.ts` (→ `@lwc/template-compiler.compile`)
  - `.css` → `style.ts` (→ `@lwc/style-compiler.transform`; AggregateError → CompilerAggregateError)
  - `.js/.ts/.jsx/.tsx/.mjs/.mts` → `targetSSR ? compileComponentForSSR : javascript.ts` (babel.transformSync + `@lwc/babel-plugin-component` + class-props/obj-rest/async plugins + locker unforgeables)
  - else `NO_AVAILABLE_TRANSFORMER`.
- `TransformResult` (`transformers/shared.ts`): `{ code: string; map: BabelFileResult['map']; warnings?: CompilerDiagnostic[]; cssScopeTokens?: string[] }`.
- Options: `src/options.ts` (`TransformOptions` L93, `validateTransformOptions` L180, `targetSSR`, `ssrMode`). No module-format option: output is ESM already; nothing AMD here.
- Delete at cutover: `@lwc/compiler` internals (or whole pkg if Rust ships its own npm entry), `@lwc/template-compiler`, `@lwc/style-compiler`, `@lwc/babel-plugin-component`, `@lwc/ssr-compiler`. Keep: `@lwc/errors`, `@lwc/shared`, runtimes (engine-*, ssr-runtime), `@lwc/rollup-plugin`.
- Sole in-repo consumer: `@lwc/rollup-plugin/src/index.ts` L12 `import { transformSync } from '@lwc/compiler'`, call at L372 inside rollup `transform(src,id)` (L342); uses `{code,map,warnings}`. Integration-wtr (`configs/plugins/serve-{integration,hydration}.js`) and engine-server fixtures go through rollup-plugin.
- package.json deps on compiler pkgs: babel-plugin-component, compiler, integration-wtr, rollup-plugin, ssr-compiler, style-compiler, template-compiler. Also `scripts/release/version.js` names them.

## B. Seams
- No `CompilerBackend`, `generateCompiledModule`, or `moduleFormat` exist. Natural seam = `transformFile(src, filename, NormalizedTransformOptions) → TransformResult` (already per-kind switch). Recommend:
```ts
// packages/@lwc/compiler/src/backend.ts
export type FileKind = 'template'|'style'|'script';
export interface CompilerBackend {
  name: 'js'|'rust';
  compileTemplate(src, filename, o: NormalizedTransformOptions): TransformResult; // incl. SSR via o.targetSSR
  compileStyle(src, filename, o): TransformResult;
  compileScript(src, filename, o): TransformResult;
}
export function getBackend(kind: FileKind): CompilerBackend; // env LWC_COMPILER_BACKEND=js|rust[,style=rust...]
```
  transformer.ts keeps validation + extname dispatch, calls backend. Errors must stay `CompilerError`/`CompilerAggregateError` from `@lwc/errors` (Rust returns diagnostics, JS shim wraps).

## C. Behavioral parity — style specifics
- Fixtures: `packages/@lwc/style-compiler/src/__tests__/fixtures/` → **230 files total** = 71 `.css` (actual.css + a couple of imported css) + 69 `expected.js` + 69 `error.json` + 21 `config.json`. "69" = number of fixture cases (pattern `**/actual.css`, runner `index.spec.ts` via `testFixtureDir` from `@lwc/test-utils-lwc-internals`, `scripts/test-utils/test-fixture-dir.ts`, LWC_VERSION→X.X.X). ~230 is file count, not case count. Plus `inline.spec.ts` (1 test).
- expected.js shape: `function stylesheet(token, useActualHostSelector, useNativeDirPseudoclass) { …return "<css string concat>"; /*LWC compiler vX.X.X*/ } [stylesheet.$scoped$=true; stylesheet.$nativeOnly$=true] export default [stylesheet];` (empty src → `export default undefined`). Imports appear as `import x from "./a.css"` spread into array.
- Runtime invocation: `engine-core/src/framework/stylesheet.ts` L234 `stylesheet(scopeToken, useActualHostSelector, useNativeDirPseudoclass)`; `$nativeOnly$` in synthetic shadow → ignored (L225); `evaluateStylesheetsContent` L160 flattens nested arrays.
- Behavioral style harness assertion: import module (ESM, via data: URL / vm.SourceTextModule), assert default export is `undefined` or array (recursively flatten), each fn's `$scoped$`/`$nativeOnly$` flags, and `fn(token,host,dir)` string output for matrix token∈{undefined,'x-foo_bar'} × host∈{t,f} × dir∈{t,f}, compared JS vs Rust (normalize version comment). Errors: compare `{name/code, line, column}`; message text only if decided. Byte parity for style remains the 69 expected.js fixtures (PostCSS-Rust track).
- Template/script/SSR behavioral: reuse `engine-server/src/__tests__/fixtures.spec.ts` pattern (rollup + lwcRollupPlugin → ESM → `renderComponent` → HTML compare, fixtures in `ssr-compiler/src/__tests__/fixtures`) and template-compiler fixtures (34 dirs) evaluated as modules; assert export names, `default` kind, `cssScopeTokens`, warnings codes.

## D. Phase 0 deliverables
1. `packages/@lwc/compiler/src/backend.ts` (+ `backends/js.ts` wrapping current template/style/javascript/ssr), transformer.ts delegates. Zero behavior change; existing tests pass.
2. `packages/@lwc/compiler/src/__tests__/parity/` : `style.parity.spec.ts` (runs over style fixtures, matrix above), `harness.ts` (loadEsm, flatten, diff), later `template.parity.spec.ts`, `ssr.parity.spec.ts`. Skip when rust backend absent.
3. `/workspace/notes/lwc-rust-migrate/cutover-checklist.md`: remove 5 pkgs from workspaces/nx, rollup-plugin dep → Rust pkg, integration-wtr deps, release `scripts/release/version.js`, ARCHITECTURE.md diagrams, test-utils fixture roots referencing ssr-compiler fixtures (engine-server spec L127 path!) must be relocated before deletion.

## E. Open questions
- Is `map` (source maps) in parity scope? Warnings: code-only or full message?
- Rust delivered as napi package replacing `@lwc/compiler` name, or backend behind it?
- Keep `transform` async wrapper/`version` export?
- Where do ssr-compiler fixtures live after deleting the package (engine-server depends on them)?

## F. Key files
compiler/src/{index.ts,options.ts,transformers/{transformer,shared,style,template,javascript}.ts}; rollup-plugin/src/index.ts (L12,L342,L372); style-compiler/src/__tests__/{index.spec.ts,inline.spec.ts,fixtures/}; engine-core/src/framework/stylesheet.ts (L160,L225,L234); engine-server/src/__tests__/fixtures.spec.ts; scripts/test-utils/test-fixture-dir.ts; ARCHITECTURE.md; AGENTS.md; root package.json (`test`=vitest).
