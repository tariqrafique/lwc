# lwc-js architecture (for LWC Migrate review)

Owner: LWC JS Rust. Status: **proposed — await Migrate lock on oxc pick + phased order.**
SoT: `/workspace/lwc` + `/workspace/lwc-compiler-rust`. Crate: `crates/lwc-js`. Facade: `crates/lwc-compiler`.
Parity: **runtime semantics + new goldens** (not Babel bytes). Phase 0 behavioral harness: Harness Cutover.

## Current JS surface (map)

Entry: `@lwc/compiler` `scriptTransform` → `packages/@lwc/compiler/src/transformers/javascript.ts`

Babel plugin order today:
1. `LwcPrivateMethodTransform` (if `enablePrivateMethods`) — `#method` → `__lwc_component_class_internal_private_*`
2. `@lwc/babel-plugin-component` (`LwcClassTransform`)
3. `@babel/plugin-transform-class-properties` `{ loose: true }`
4. `LwcReversePrivateMethodTransform` (if `enablePrivateMethods`)
5. `@babel/plugin-transform-object-rest-spread` unless `APIFeature.DISABLE_OBJECT_REST_SPREAD_TRANSFORMATION` (`apiVersion >= V60`)
6. If `enableLightningWebSecurityTransforms` (LWS): `@locker/babel-plugin-transform-unforgeables` + async-to-generator + async-generator-functions
7. Parser `errorRecovery: experimentalErrorRecoveryMode`; after transform, aggregate `metadata.lwcErrors` → `CompilerAggregateError`

`LwcClassTransform` (single Program enter/exit + visitors):
- Program enter: validate LWC decorator imports; `scopeCssImports` (`?scoped=true`)
- Program exit: remove imported decorator specifiers (`api`/`track`/`wire`)
- Import: dynamic import rewrite
- Class: `@api` / `@track` / `@wire` validate+transform → `registerDecorators(...)`
- ClassBody: compiler version stamp
- ExportDefaultDeclaration: wrap with `registerComponent(cmp, { tmpl, sel, ... })`

Fixture corpus (reference, not byte gate): `packages/@lwc/babel-plugin-component/src/__tests__/fixtures` — **173** `actual.js` across:
wire 37 · private-methods 28 · api 28 · decorators 21 · component 15 · error-recovery 12 · dedupe-imports 8 · implicit-explicit 7 · observed-fields 6 · track 5 · dynamic-imports 5 · import-scoped-stylesheet 1

Plus `@lwc/compiler` `transform-javascript.spec.ts` for option matrix (apiVersion 59/60 rest-spread, LWS, private methods, error recovery, decorator errors).

### Option matrix (JS-relevant)

| Option | Effect |
|---|---|
| `namespace` + `name` | custom element tag for `registerComponent` |
| `apiVersion` | rest/spread disable @V60+; `SKIP_UNNECESSARY_REGISTER_DECORATORS` @V60+; decorator-outside-LE error messaging |
| `enablePrivateMethods` | private method sandwich around class-properties |
| `enableLightningWebSecurityTransforms` | LWS unforgeables + async lowers |
| `experimentalErrorRecoveryMode` | tolerant parse + collect `lwcErrors` → aggregate throw |
| `isExplicitImport` | component/register behavior |
| `dynamicImports.{loader,strictSpecifier}` | dynamic import rewrite |
| `enableSyntheticElementInternals` | registerComponent metadata |
| `componentFeatureFlagModulePath` | optional default import into registerComponent |
| `outputConfig.sourcemap` | map emit |
| `instrumentation` | error/perf hooks |

## oxc vs swc ranking

Legend: ✅ fit · 🟡 doable/custom · ❌ gap. Custom LWC transforms are ours either way.

| Need | oxc | swc |
|---|---|---|
| Parse decorators-before-export + class fields + private methods | ✅ | ✅ |
| Custom VisitMut / Traverse for LWC AST rewrites | ✅ VisitMut + Traverse | ✅ Visit/Fold |
| class-properties loose lower | ✅ ES lowering / assumptions | ✅ transforms |
| object rest/spread (apiVersion-gated) | ✅ target/lowering | ✅ |
| async / async-generator lower (LWS) | ✅ | ✅ |
| `@locker` unforgeables | 🟡 port ourselves | 🟡 port ourselves |
| Error recovery / diagnostics aggregation | 🟡 map to CompileError; oxc tolerant parse TBD vs Babel recovery | 🟡 same |
| Speed / memory / package size | ✅ ~4× transformer vs swc | baseline |
| Babel-modeled transform tests upstream | ✅ imports Babel transformer tests | 🟡 own suite |
| Plugin ecosystem for third-party Babel plugins | ❌ N/A (full-Rust; we own transforms) | 🟡 Wasm plugins — unused |
| Arena AST / single-pass compose | ✅ design goal | multi-pass Fold common |
| Alignment with PROGRAM.md default | ✅ lean oxc | alternate |

**Rank: 1) oxc · 2) swc**

Pick **oxc**. Reasons: PROGRAM default; we own all LWC transforms anyway so SWC’s plugin ABI buys nothing; LWS locker plugin must be ported on either AST; oxc is faster and Babel-shaped for lowers we still need; semantic/new-golden gate removes Babel-printer lock-in that would otherwise favor copying Babel output.

Flip to swc only if oxc cannot: (a) parse with recovery equivalent to Babel `errorRecovery`, or (b) express the private-method sandwich relative to class-properties without multi-pass — both look doable with explicit passes.

## Proposed `lwc-js` API

```rust
// crates/lwc-js
pub fn transform(src: &str, filename: &str, options: JsOptions) -> Result<CompileResult, CompileError>;
// CompileResult { code, map } — same facade shape as PROGRAM CompilerBackend
```

`JsOptions` mirrors the JS-relevant subset of `NormalizedTransformOptions` (table above). Facade `lwc-compiler` dispatches `.js` here.

## Internal module split

```
crates/lwc-js/
  src/
    lib.rs              // transform()
    options.rs
    error.rs            // map oxc + LWC diagnostics → CompileError / aggregate
    parse.rs            // oxc Parser + source type + recovery flag
    codegen.rs          // oxc codegen + optional map
    transforms/
      mod.rs            // ordered pipeline
      private_methods.rs
      reverse_private_methods.rs
      component.rs      // registerComponent + template import
      decorators/
        mod.rs          // validate imports, registerDecorators emit, apiVersion skip
        api.rs
        track.rs
        wire.rs
      scoped_css.rs
      dynamic_imports.rs
      version_stamp.rs
      observed_fields.rs
      dedupe_imports.rs // if still required after oxc import helpers
      lws/
        unforgeables.rs // port of locker semantics
        async_lower.rs  // thin: invoke oxc async lowers
    lowers/
      class_properties.rs
      object_rest_spread.rs
  tests/                // NEW goldens (semantic / normalized AST or eval harness hooks)
```

Pass order mirrors JS (keep sandwich semantics):
`private_methods? → lwc_class → class_properties_loose → reverse_private? → rest_spread? → lws? → codegen`

## Golden / harness split

| Layer | Owner | Asserts |
|---|---|---|
| Package tests `crates/lwc-js` | JS Rust | New goldens derived from fixture *behaviors*: `registerComponent`/`registerDecorators` shape, metadata keys, private rename, scoped css query, dynamic import loader, error codes/positions under recovery. Prefer normalized AST or stable pretty-print — **not** Babel byte identity. |
| Phase 0 behavioral | Harness Cutover | Runtime under engine + **option matrix** (apiVersion × private × LWS × recovery). |
| Reference corpus | (read-only) | `babel-plugin-component` fixtures + `transform-javascript.spec.ts` as oracle while authoring new goldens. |

## Implementation phases (no large port until Migrate locks oxc)

0. **Scaffold** — replace stub `add()` with `transform` returning `todo!`; wire facade `.js` dispatch; depend on `oxc` (+ allocator/parser/codegen/transformer features).
1. **Parse + codegen round-trip** — identity transform; sourcemap plumbing.
2. **registerComponent + version stamp + scoped css + dynamic imports** — unlocks `component` / `dynamic-imports` / `import-scoped-stylesheet` fixture families as new goldens.
3. **Decorators** — validate + `@api`/`@track`/`@wire` + `registerDecorators` + apiVersion skip-unnecessary; wire/api/track/decorators fixtures.
4. **class-properties loose + private-method sandwich** — private-methods + observed-fields.
5. **apiVersion rest/spread gate**.
6. **Error recovery** — aggregate diagnostics parity with compiler errors.
7. **LWS** — unforgeables port + async lowers; matrix tests with Harness Cutover.
8. **Delete Babel path** — after Phase 0 harness green (Migrate/Harness).

## Non-goals
- style / template / SSR / AMD
- long-lived JS Babel shim
- Babel byte-identical output

## Ask of LWC Migrate
1. Lock **oxc** (or reject with reason).
2. Confirm pass order / private-method sandwich stays.
3. Confirm new golden style: normalized codegen OK vs must match a specific printer.
4. LWS unforgeables: port minimal subset now vs defer behind harness flag (recommend: stub flag fail-closed until port, don’t block phases 0–6).
