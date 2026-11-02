# LWC Rust cutover checklist (delete JS compiler packages)

ESM-only. **AMD: N/A.** Source maps out of Phase-0 parity (style map = ""). Warnings: match code/level only.

## Preconditions
- [ ] Zero parity shims in crates/: board `shim-check` (harness/scripts/shim-gate.mjs; words shim/compat/workaround/hack/mask*/rescan; allowlist harness/shim-allowlist.tsv requires a reason) must read `SHIM(template-gap:`=0, `SHIM(`|`TODO(parity`=0, and untagged `shim` mentions=0 (untagged count is board-GATING now; tagged shims allowed until cutover). Known today: crates/lwc-ssr/src/compile_template/ir_compat.rs (tagged: see "SSR template-gap shims" below) rewrites Template IR + re-scans source for SSR; must be deleted once lwc-template IR parity lands.
- [ ] Crate-surface review before ANY gate closes: diff each crate's modules + `pub`/`pub(crate)` fns against the upstream package's module list (e.g. lwc-ssr vs @lwc/ssr-compiler/src, lwc-template vs @lwc/template-compiler/src); every Rust module with no upstream counterpart is flagged and must be justified in decisions.tsv or deleted. (Grep gates are dodgeable; this is the structural check.)
- [ ] Consumers migrated to Rust CLI/API (`lwc-compiler` crate behind `CompilerBackend`); the `@lwc/compiler` name dies at cutover.
- [ ] Gates green:
  - [x] style byte fixtures 69/69 (PostCSS-Rust port gate) — GREEN 2026-11-01 via PostCSS Rust
  - [x] Phase 0 behavioral style suite 70/70 with `LWC_COMPILER_BACKEND=rust` (harness/) — GREEN 2026-11-01
  - [x] Phase 0 behavioral JS suite 174/174 with `LWC_COMPILER_BACKEND=rust` (babel-plugin-component / lwc-js) — GREEN 2026-11-01
  - DIAGNOSTIC ONLY — template compile-observation suite on rust (520/568 @ napi af4f460a07bf (src 8a853a63374e) official): module eval w/ stubs, no DOM render
  - [ ] CSR DOM render parity on rust via @lwc/engine-dom + jsdom (`test/csr.render.test.ts`, JS-oracle goldens/csr-render) — the template/CSR gate:
    - [ ] corpus ssr fixtures (row csr-render): 390 tests: 217 @ af4f460a07bf (src 8a853a63374e) official
    - [ ] corpus template-compiler fixtures (row csr-render-template): 381/570 tests @ af4f460a07bf (src 8a853a63374e) official
  - DIAGNOSTIC ONLY (non-gating, Migrate 2026-11-01) — ssr compile-observation suite on rust, every JS-shipped `CompilationMode` (@lwc/shared: sync | async | asyncYield), one board row each (`SSR_MODE=<mode>`, goldens/ssr[-<mode>]):
    - [ ] sync 388/388 @ napi 6784606568ec (clean rebuild) — green; not ticked until all modes are
    - [ ] async 16/388 — lwc-ssr ignores `CompileOptions.ssr_mode` (always emits sync)
    - [ ] asyncYield 16/388 — same
  - [ ] ssr RENDER-HTML parity on rust (`test/ssr.render.test.ts`, rows ssr-render[-<mode>]; byte-exact HTML via ssr-runtime vs JS-oracle goldens/ssr-render/<mode>) — the ONLY SSR gate; must hold 384/384 in all 3 modes WITH crates/lwc-ssr/src/compile_template/ir_compat.rs DELETED (Migrate 2026-11-01):
    - [ ] sync 383/384 @ af4f460a07bf (src 8a853a63374e) official — REGRESSED: attribute-global-html/tabindex (was 384 @ 315a); ir_compat still present → not closed
    - [ ] async 384/384 @ af4f460a07bf (src 8a853a63374e) (weak signal: oracle renders [object Promise])
    - [ ] asyncYield 383/384 @ af4f460a07bf (src 8a853a63374e) (tabindex regression) ( ssrMode verified reaching codegen: async function* + yield in template, setStaticInternals(...,"asyncYield",...) in component)
  - [ ] `cargo check && cargo test` in /workspace/lwc-compiler-rust
- [ ] **Move engine-server fixture tests' fixtures out of `@lwc/ssr-compiler`** (engine-server reads fixtures from inside ssr-compiler) BEFORE deleting it.

## Packages that **stay** (do not delete)
- [ ] `@lwc/ssr-runtime` stays — JS ABI frozen; SSR goldens keep calling it
- [ ] `@lwc/engine-dom` stays
- [ ] `@lwc/engine-core` stays
- [ ] Also keep: `@lwc/errors`, `@lwc/shared`, `@lwc/features`, `@lwc/engine-server` until its own deprecation (not part of this compiler cut)

## Remove / stop publishing
- [ ] @lwc/compiler
- [ ] @lwc/style-compiler
- [ ] @lwc/template-compiler
- [ ] @lwc/babel-plugin-component
- [ ] @lwc/ssr-compiler
- [ ] @lwc/rollup-plugin: re-point to Rust backend (or remove) — it imports @lwc/compiler / ssr-compiler
- [ ] `lwc` barrel package: drop re-exports of the compiler packages
- [ ] Remove from workspace config, nx project graph, release/publish scripts, tsconfig references, CODEOWNERS

## Verify
```
cd /workspace/lwc-compiler-rust && cargo check && cargo test
cd harness && LWC_COMPILER_BACKEND=rust npx vitest run
cd /workspace/lwc && grep -rn "@lwc/\(compiler\|style-compiler\|template-compiler\|babel-plugin-component\|ssr-compiler\)" packages --include=package.json --include=*.ts -l   # expect none
cd /workspace/lwc && yarn build && yarn test
```

## Rollback
Deletion is one revertable commit; keep last JS versions published on npm (deprecate, don't unpublish).
Flip `LWC_COMPILER_BACKEND=js` / re-pin consumers to the last JS release; goldens remain valid.

## SSR template-gap shims

Every Template-IR workaround in `crates/lwc-ssr` is tagged `// SHIM(template-gap:<id>)`. The tag goes on the line above each site, and also inline on any line the board shim gate would flag. **Total tags: 58** (`rg -c 'SHIM\(template-gap:' crates/lwc-ssr` gives lib.rs 20, compile_template/ir_compat.rs 35, compile_template/mod.rs 3; board shim-check reports `SHIM(template-gap:=58 ... untagged=0`, 2026-11-01 8:23 PM PST).
Retirement gate: ssr-render, ssr-render-async and ssr-render-asyncYield each at 384 with `crates/lwc-ssr/src/compile_template/ir_compat.rs` deleted. Full per-id IR contracts are in `/workspace/notes/lwc-ssr-rust/template-ir-contracts.md`: input snippet, current Rust IR from a real `lwc_template::parse` run, the JS template-compiler IR, fixtures, and a drop-in `#[test]`. The tests also live in `template-ir-contracts.rs` there; all 9 compile against lwc-template today and fail as expected. All lwc-template paths below are `crates/lwc-template/src/`.

| id | tags | sites (file:fn) | fixtures | root cause in lwc-template (file/line) | Template fix that retires it (IR contract) | status |
|---|---|---|---|---|---|---|
| `preserve-comments-directive` | 4 | lib.rs:transform_html (root_preserves, config override, preserve_comments), lib.rs:root_has_preserve_comments | comments-basic, comments-foreach, comments-if, comments-slot, comments-text-preserve, adjacent-text-nodes | parser/mod.rs:460 builds `Root { directives: vec![] }` (ROOT_DIRECTIVES at :491 is only validated); :256 keeps comments only when `cfg.preserve_html_comments` | emit RootDirective::PreserveComments(true) (and RenderMode) from root attrs; keep comments when the directive is set | open |
| `lwc-directive-attrs` | 7 | ir_compat.rs:normalize_node (Element/ExternalComponent/Slot/Component/LwcComponent arms), as_expr, lift_directives | lwc-inner-html, inner-outer-html, barrel-lwc-exports, render-correct-tags (inner-html); lwc-dom-manual; lwc-on; lwc-dynamic; inner-outer-html (spread); scoped-slots, slot-forwarding, context-slotted, dynamic-components (slot-bind); directive-external | parser/mod.rs:310 builds `directives` from Key and Ref only; every other `lwc:*` stays in `attributes` via `lower_attr` (:184) | lift lwc:inner-html/dom/spread/on/dynamic/is/slot-bind out of attributes into ElementDirective on all element kinds | open |
| `lwc-component-node` | 1 | ir_compat.rs:normalize_node (lwc:component block) | dynamic-components, dynamic-imports, attribute-class, read-only-props | parser/mod.rs:335 else-branch builds `ChildNode::Element` for any non-custom tag, `<lwc:component>` included (codegen/mod.rs:362 notes the gap) | lower <lwc:component> to ChildNode::LwcComponent (Is directive + component prop split) | open |
| `component-prop-split` | 7 | ir_compat.rs:normalize_node (Component, LwcComponent arms), is_component_attribute, SPECIAL_ATTR_TO_PROP, ARIA_PROPS, attribute_to_property_name, resplit_component | attribute-component-aria, attribute-component-global-html, attribute-global-html/as-component-prop/*, attribute-global-html/tabindex, known-boolean-attributes/default-def-html-attributes/static-on-component, read-only-props, dynamic-components | parser/mod.rs:153 `keeps_as_attr` (role\|style\|class\|id\|slot\|key\|aria-*\|data-*) vs JS `isAttribute` for components (style\|class\|key\|slot\|exportparts\|data-*); :166 `split_props` also keeps any `:` name and names props with `kebab_to_camel` (:157) instead of `attributeToPropertyName` | component attrs = style\|class\|key\|slot\|exportparts\|data-*; all else Property named by attributeToPropertyName (role, id, aria-* become props; tabindex→tabIndex) | open |
| `clean-text-node` | 4 | ir_compat.rs:normalize_children (reclean call), js_clean_text_node, split_mustache, reclean_texts | attribute-live-bindings, slots, slots-fallback, dynamic-slots, wire/deep-reference, imported-member | parser/mod.rs:62 `clean_text` trims the whole text node (used at :242) instead of babel `cleanTextNode` (per-line trim, non-last lines + " ", drop whitespace-only); each segment's `Text.raw` is the whole node (:252) | Text literals = babel cleanTextNode (per-line trim, inner newline→space, trailing space before siblings kept) | open |
| `valueless-attr-bool` | 3 | ir_compat.rs:mark_valueless_attrs (source rescan); lib.rs:transform_html call (2 tags) | global-html-attributes, known-boolean-attributes/*, attribute-boolean, attribute-global-html/hidden | parser/mod.rs:184-191 `lower_attr` always makes `LiteralValue::String(a.value)`; parse5 gives `""` for a valueless attr so `hidden` and `hidden=""` are indistinguishable after parse | valueless attrs (`<input hidden>`) → LiteralValue::Bool(true); `attr=""` stays String("") | open |
| `empty-ns-prefix` | 1 | ir_compat.rs:normalize_node (Element arm `:xmlns` strip) | svgs | parser/mod.rs:262 `Some(p) => format!("{p}:{}", a.name)` runs when parse5's foreign-attr prefix is `Some("")` | do not qualify foreign attrs with an empty prefix (`xmlns`, not `:xmlns`) | open |
| `complex-expr-literal` | 3 | ir_compat.rs:lift_quoted_complex; lib.rs:transform_html call (2 tags) | attribute-dynamic-complex, text-interpolation-complex, scoped-slots/complex, lwc-if-else-complex | parser/mod.rs:99 `parse_binding` handles only Identifier/Member; `lower_attr` (:186) and text segments (:250) fall back to a string Literal, whereas lwc:if conditions (:211-227) already lower to `Expression::Complex` | complex `{expr}` in attrs/text → Expression::Complex(inner source) instead of a "{...}" string Literal | open |
| `complex-for-each` | 11 | ir_compat.rs:mask_complex_for_each (doc + fn, 5 tags), unmask_complex_for_each; lib.rs:transform_html mask block + unmask call (6 tags) | for-each-block/complex | parser/mod.rs:128 `loop_directive`: `parse_binding(&a.value)?` returns None for a complex value, so no Loop; validate.rs:75-82 `template()` then pushes 1153 and drops the node | for:each with a complex value → ForEach{expression: Complex(..)}, no 1153 drop | open |
| `ir-compat-walk` (driver) | 11 | ir_compat.rs: module doc, normalize_root, normalize_children, normalize_else, normalize_node, for_each_node, for_each_attr_value; compile_template/mod.rs: `mod ir_compat`, normalize_root call (2); lib.rs:transform_html `root.clone()` | all of the above | none (tree walkers that dispatch the shims above; `normalize_else` only recurses into lwc:elseif/lwc:else, since the else chain already matches JS) | retires once the 9 ids above are closed and ir_compat.rs is deleted | open |
| `parse5-error-root` | 3 | lib.rs:transform_html (parse5_failed computation, root filter, comment) | parse5/unclosed-div (ssr_render_api, apiVersion >= 59) | parser returns `Some(root)` even when parseHTML produced Error-level 1058 (INVALID_HTML_SYNTAX) diagnostics; JS template-compiler parser/index.ts:147 returns `{warnings}` with no root, and ssr-compiler compile-template/index.ts:110 then throws | return `root: None` whenever any Error-level diagnostic comes out of HTML parsing (respecting TREAT_ALL_PARSE5_ERRORS_AS_ERRORS from v59) | open |
| `expr-error-recovery` | 3 | ir_compat.rs:drop_errored_expr_nodes; lib.rs:transform_html call (2 tags) | *-complex fixtures in ssr_render_api below apiVersion 66 (attribute-dynamic-complex, lwc-if-else-complex, scoped-slots/complex, text-interpolation-complex) | below v66 (or without experimentalComplexExpressions) Template emits 1210 INVALID_NODE_CTE_API_VERSION but keeps the node: attrs/slot-bind stay a literal `"{...}"` String, lwc:if/elseif stay `Expression::Complex` | JS `withErrorRecovery` in parseChildren: drop the whole node whose expression failed validation (element, text, lwc:if template, slot with slot-bind); the following orphaned lwc:else/elseif is dropped too | open |

## Official board procedure (2026-11-01 20:22)
- `scripts/official-board.sh` is the ONLY official path: frozen snapshot of Cargo.toml/Cargo.lock/crates + ../parse5-rust, fresh target dir (== cargo clean), cargo check --workspace --all-targets + seam crate tests must pass, napi built from snapshot, board.sh with SHIM_ROOT=snapshot, snapshot re-hashed after (mutation => INVALID). Report napi sha + src sha.
- `cargo clean && install-napi.sh && board.sh` on the live tree is NOT official: concurrent agent edits leaked into two runs (20:16, 20:18).
- VALID run (snapshot unchanged AND board rc 0, full row set) writes push artifacts: `/workspace/push/official-<srcsha>.tgz` + `official-<srcsha>.board.txt`. Only these may be pushed (Migrate 2026-11-01). parse5-rust is a build dep hashed into src sha but NOT in the tgz.
- Harness runs from the snapshot too (tests/goldens frozen); gates read snapshot crates via CRATES_ROOT.
- Golden integrity: `harness/scripts/golden-gate.sh` re-records all rows on the JS backend (oracle b9e8007) into a scratch copy and diffs goldens/ (pending wiring into official-board.sh).
