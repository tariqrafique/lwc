# LWC JS compiler path map (for the Rust port)
Source: salesforce/lwc @ b9e8007. Written 2026-11-01. Read-only research; the lwc repo was not modified.
Contract: parity means the same runtime behavior plus NEW goldens, not byte-identical Babel output. Crate: crates/lwc-js. Facade: crates/lwc-compiler. Workspace: /workspace/lwc-compiler-rust. Phase 0 harness: Harness Cutover. oxc vs swc is NOT locked; we lean oxc.

## A. Entry call graph
- `@lwc/compiler` `transform()` / `transformSync()` (packages/@lwc/compiler/src/transformers/transformer.ts:40,76)
  -> `normalizeOptions` (src/options.ts; defaults at ~L29-39; `apiVersion = getAPIVersionFromNumber()` at L239; dynamicImports defaults merged at L234)
  -> `transformFile()` (transformer.ts:91), which dispatches on file extension: .js/.ts/.jsx/.tsx/.mjs/.mts -> `scriptTransform` (transformers/javascript.ts)
  -> `babel.transformSync(code, {babelrc:false, configFile:false, compact:false, sourceMaps, parserOpts:{errorRecovery}, plugins})`
  - Plugin order:
    1. `LwcPrivateMethodTransform` (only if enablePrivateMethods) — babel-plugin-component/src/private-method-transform.ts
    2. `LwcClassTransform` (babel-plugin-component/src/index.ts). Its parser plugins: classProperties and decorators{decoratorsBeforeExport:true}. Its visitors:
       - Program.enter: `getEngineImportSpecifiers`, `validateImportedLwcDecoratorUsage`, `scopeCssImports`
       - `Import` -> dynamic-imports.ts
       - `Class` -> decorators/index.ts (api/wire/track)
       - `ClassBody` -> compiler-version-number.ts
       - `ExportDefaultDeclaration` -> component.ts (registerComponent)
       - Program.exit: `removeImportedDecoratorSpecifiers`
    3. `@babel/plugin-transform-class-properties` {loose:true}
    4. `LwcReversePrivateMethodTransform` (only if enablePrivateMethods)
    5. `@babel/plugin-transform-object-rest-spread` (only if apiVersion < 60)
    6. LWS only: `@locker/babel-plugin-transform-unforgeables`, `transform-async-to-generator`, `transform-async-generator-functions`
  - Afterwards: in error-recovery mode, `metadata.lwcErrors` is turned into a `CompilerAggregateError`. Any other exception goes through `normalizeToCompilerError(JS_TRANSFORMER_ERROR)`. If the message contains "Decorators are not enabled" plus track/api/wire, the error becomes `JS_TRANSFORMER_DECORATOR_ERROR`.
- The JS path never calls the template or style compilers. It only emits imports: `./<basename>.html` and `*.scoped.css?scoped=true`.
- ssr-compiler has its own JS pipeline. It imports only the `LwcBabelPluginOptions` type and copies some of this behavior (wire validation, stylesheets).

## B. Transform inventory
| # | Transform | AST effect | Gate |
|---|---|---|---|
| 1 | Private-method forward | Phase 1 collects `#m` methods (kind 'method' only). Phase 2 renames them to `__lwc_component_class_internal_private_m` and rewrites `this.#m` member expressions. A decorator on a private method is an error (DECORATOR_ON_PRIVATE_METHOD). The renamed set is stored in file metadata `__lwcTransformedPrivateMethods`. | enablePrivateMethods |
| 2 | Decorator import validation | Every reference to an api/track/wire import from 'lwc' must be a decorator (IS_NOT_DECORATOR) on a class property or method (IS_NOT_CLASS_PROPERTY_OR_CLASS_METHOD). | always |
| 3 | Scoped CSS import | `import "x.scoped.css"` becomes `"x.scoped.css?scoped=true"` (top-level ImportDeclarations only). | always |
| 4 | Dynamic import | `import(x)` becomes `load(x)`, with `load` imported from `dynamicImports.loader`. With `strictSpecifier`, a non-string-literal source is an error (INVALID_DYNAMIC_IMPORT_SOURCE_STRICT). Also bumps the DynamicImportTransform metric. | dynamicImports.loader / strictSpecifier |
| 5 | Decorators -> registerDecorators | For each Class: skip if the body is empty. Skip if there is no superclass and apiVersion >= 60. Collect decorators, then validate: the binding must be imported from 'lwc', plus api/wire/track rules. Build the meta object: `publicProps{name:{config: bitmask}}` (property 0, getter 1, setter 2, OR'd with the paired get/set), `publicMethods[]`, `wire{name:{adapter, dynamic[], config: function($cmp){...}, method?:1}}`, `track{name:1}`, `fields[]` (undecorated, non-static, non-computed fields). If the meta is empty, do nothing. Otherwise remove the decorators. A named class gets `registerDecorators(Cls, meta)` inserted after the class. An anonymous class or class expression is wrapped as `registerDecorators(class…, meta)`. | apiVersion (SKIP_UNNECESSARY_REGISTER_DECORATORS, v60) |
| 5w | Wire config codegen | Literal values that start with `$` are reactive: they go into `dynamic`. A dotted path becomes `let vN=$cmp.a; vN!=null && (vN=vN.b)!=null ? vN.c : undefined`. If a segment is not a valid ES3 identifier, it uses computed string access. Static config properties are copied as-is. Spread and methods are ignored (see the config-ignores-* fixtures). | — |
| 6 | Compiler version comment | Adds `/*LWC compiler vX*/` as a trailing comment on the last class-body member, or as an inner comment if the body is empty. Only for classes with a superclass. | always |
| 7 | registerComponent | Only when `isExplicitImport` is false and the default export is an identifier (not undefined/null), call, class declaration, or conditional. Imports `registerComponent` from 'lwc' and the default template `./<basename>.html` (as `_tmpl`). If the flag is set, also default-imports the feature-flag module. A named class is hoisted. Emits `const __lwc_component_class_internal = registerComponent(X,{tmpl, sel:"ns-name", apiVersion:N, [componentFeatureFlag:{value:Boolean(id), path}], [enableSyntheticElementInternals:true], [enablePrivateMethods:true]}); export default __lwc_component_class_internal;` | isExplicitImport, componentFeatureFlagModulePath, enableSyntheticElementInternals, enablePrivateMethods, namespace/name, apiVersion |
| 8 | Remove decorator imports | Removes the api/track/wire specifiers from 'lwc', and the whole import declaration if it ends up empty. | always |
| 9 | Class properties (Babel, loose) | Fields become `this.x = init` (or `void 0`) inside a constructor, with `constructor(...args){super(...args)}` synthesized if needed. | always |
| 10 | Private-method reverse | Turns the renamed methods back into `#m`. If a user wrote a name with the reserved prefix, throws PRIVATE_METHOD_NAME_COLLISION. Throws a count-mismatch error if not every forward rename was reversed. | enablePrivateMethods |
| 11 | Object rest/spread lowering | Babel's default lowering (emits helpers). | apiVersion < 60 |
| 12 | LWS unforgeables, async->generator, async generators | Locker plugin plus Babel's async lowering (emits helpers). | enableLightningWebSecurityTransforms |

When `enablePrivateMethods` is off, private methods are rejected (the parser and class-properties path errors out; see transform-javascript.spec.ts:245).
Error handling: `utils.handleError` throws, or in recovery mode (`parserOpts.errorRecovery`) pushes the error into `file.metadata.lwcErrors`. `instrumentation` collects metrics.

## C. Option matrix
| Option | Effect on JS output |
|---|---|
| namespace, name | `sel: "<ns>-<name>"` via `generateCustomElementTagName` |
| apiVersion | Normalized `apiVersion: N` in registerComponent. Version 60 or later: no rest/spread lowering, and no registerDecorators for classes without a superclass. |
| isExplicitImport | true: no registerComponent and no template import |
| enablePrivateMethods | Wraps the pipeline with the forward/reverse passes. `#methods` survive to the output. Adds `enablePrivateMethods:true`. Off: `#methods` are rejected. |
| enableLightningWebSecurityTransforms | Adds unforgeables plus async and async-generator lowering |
| experimentalErrorRecoveryMode | Babel parser errorRecovery. Errors are collected into a CompilerAggregateError. |
| dynamicImports.{loader,strictSpecifier} | `import()` becomes `load()`, plus strict source validation |
| enableSyntheticElementInternals | `enableSyntheticElementInternals:true` in registerComponent |
| componentFeatureFlagModulePath | Default import plus a `componentFeatureFlag:{value,path}` property |
| instrumentation | Metrics only, no change to output |
| outputConfig.sourcemap (true / 'inline') | Source map output |
| filename | Template import name, extension dispatch, error locations |

## D. Parity surfaces
- packages/@lwc/babel-plugin-component/src/__tests__/fixtures/** has 173 cases in 12 groups: api-decorator, component, decorators, dedupe-imports, dynamic-imports, error-recovery-mode, implicit-explicit, import-scoped-stylesheet, observed-fields, private-methods, track-decorator, wire-decorator. Each case has actual.js, expected.js, and error.json; there are also 30 config.json and 12 lwcErrors.json files. These are **string goldens** of the plugin plus class-properties, private passes on. They do not include rest-spread or LWS. Normalization: LWC version -> X.X.X, latest apiVersion -> 9999999. Driven by index.spec.ts via testFixtureDir.
- babel-plugin-component/src/__tests__/private-method-transform.spec.ts and warnings.spec.ts: unit tests (string and error).
- compiler/src/transformers/__tests__/transform-javascript.spec.ts: rest-spread by apiVersion, LWS on/off, instrumentation metrics, unnecessary registerDecorators (v59 vs latest), enablePrivateMethods, sourcemaps, extensions, error recovery (aggregate errors). These use substring/regex checks. Also transform.spec.ts and compiler/src/__tests__/options.spec.ts (option validation).
- Runtime consumers whose behavior must be preserved: engine-core/src/framework/component.ts:44 `registerComponent`, and decorators/register.ts:177 `registerDecorators` (meta shape: publicProps.config bitmask, publicMethods, wire{adapter,config,dynamic,method}, track, fields). The integration-wtr / integration-wdio / engine-server fixtures compile real components end to end and are the **runtime** parity surface.
- Under the new contract: reuse the 173 inputs, error.json, and lwcErrors.json as inputs and expected errors. Regenerate expected.js as new oxc goldens. Judge equivalence by runtime behavior (Harness Cutover).

## E. Prior decisions
- decisions.tsv: full Rust; consumers move to Rust; AMD dropped (ESM only); style via cssparser. Harness Cutover owns the ESM parity harness, the CompilerBackend seam, and the JS deletion checklist.
- Contract update (2026-11-01): parity = runtime + new goldens. Crates crates/lwc-js and crates/lwc-compiler in /workspace/lwc-compiler-rust. oxc vs swc not locked (lean oxc). Architecture review before the large port.
- The repo has no oxc or swc transform experiments. @swc/core and @swc-node/register in the root package.json are test/build tooling only.
- notes/postcss-rust: copy its 3-doc pattern (surface -> survey -> parity matrix plus gate split).

## oxc vs swc ranking criteria (score each 1-5, weights in brackets)
1. [3] Decorators: parsing legacy/stage-3 `@x` with decoratorsBeforeExport. The transform must be able to read and remove decorators itself, without the built-in decorator lowering.
2. [3] Scope/binding API: decorator binding must resolve to an import from 'lwc', plus reference walking for validation (oxc_semantic vs swc hygiene/resolver).
3. [2] Class-properties lowering in loose `this.x=` constructor form, including the synthesized `super(...args)`. Must preserve `#private` fields and methods so the private-method round trip can be dropped.
4. [2] Object rest/spread lowering (needed only below v60), and how its helpers are emitted (inline vs import).
5. [2] Async and async-generator -> generator lowering for LWS. The Locker unforgeables pass must be ported by LWC in either case.
6. [3] Error recovery: a parser that keeps going after errors and returns multiple diagnostics with locations.
7. [2] AST building ergonomics (allocator vs Box) for registerDecorators, registerComponent, wire config codegen, and import injection with deduplicated names (`_registerComponent` style).
8. [2] Codegen: comment attachment (the version comment) and source maps (inline or separate).
9. [1] TS/JSX stripping for .ts/.tsx inputs. Throughput and WASM/napi distribution.
Expected result: oxc leads on 6, 2, and 9 (oxc_semantic, speed). swc leads on 3, 4, and 5 maturity (swc_ecma_transforms_compat). Decide with a spike that runs fixtures for criteria 1, 3, and 6.

## F. Recommended crate split (JS path only)
crates/lwc-js/
- `options.rs`: `JsTransformOptions`, mirrors table C. apiVersion is normalized via a port of `ApiFeature` in the shared crate.
- `parse.rs`: parser config (decorators, TS, recovery). Collects diagnostics.
- `decorators/{mod,api,track,wire,validate}.rs`: produces the meta model (a typed `DecoratorMeta` struct), then emits the registerDecorators call.
- `component.rs`: registerComponent plus template and feature-flag imports.
- `imports.rs`: scoped CSS rewrite, decorator-import removal, a named-import injector with uid naming.
- `dynamic_import.rs`, `version_comment.rs`.
- `lower.rs`: class properties (loose), rest/spread (gated), async (LWS). This is a backend adapter over oxc_transformer or swc compat.
- `lws/unforgeables.rs`: port of @locker/babel-plugin-transform-unforgeables. Needs its spec.
- `errors.rs`: port of the LWC error codes (DecoratorErrors, LWCClassErrors, TransformerErrors) plus an aggregate type.
- tests/fixtures: the 173 imported inputs plus new goldens (insta).
crates/lwc-compiler is the facade: extension dispatch plus the CompilerBackend seam.
Order: options/errors -> component and imports -> decorators -> lowering -> LWS -> recovery.
Do not port the private-method forward/reverse passes. They exist only to work around Babel class-properties. Keep `#m` native and keep the error codes.

## G. Open questions
1. Can the new goldens drop Babel helper names (`_registerComponent`, `_tmpl`, the `v1` wire temporaries)? Do downstream tools depend on `__lwc_component_class_internal` or on the version comment format?
2. Locker unforgeables: where is the source/spec, who owns it, and does it have to stay byte-compatible for LWS?
3. Are rest/spread lowering below v60 and LWS async lowering still required, or can we set a minimum apiVersion floor?
4. Error recovery: must the multi-error lists match the current lwcErrors.json exactly (codes and locations)?
5. Behavior with enablePrivateMethods=false: keep rejecting `#methods`?
6. Does ssr-compiler's JS path stay out of scope, or should it share lwc-js's decorator model?
7. Do we keep metrics/instrumentation? (Only DynamicImportTransform is counted.)
8. Should the dead `state.dynamicImports` tracking (TODO #3444) be dropped?
