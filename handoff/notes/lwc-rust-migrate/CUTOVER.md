# LWC Rust cutover checklist (delete JS compiler packages)

Updated: 2026-11-01 by ship (folded ship's CUTOVER.draft.md; Migrate approved).
Policy: **ESM only. No JS shim. No AMD. No CJS loader.** Unknown options rejected at the seam (D-0010, D-0203). Source maps out of Phase-0 parity (style map = ""). Warnings: match code/level only.
Inputs: PLAN §4 exit criteria, the previous preconditions of this file, ship-survey.md, ship-packaging-plan.md.
Status source: newest official board `official/ac1b61fb239a` (store-4280618e…/official/ac1b61fb239a/board.txt, published 2026-11-01 21:58 PST, commit c731fb601f40, napi acfc2e0095b1, oracle b9e8007 clean). **PROVISIONAL** until AB-0003/AB-0007 (PLAN §2). Row names as in board.txt. Update the status column only from a newer official board.
Legend: [x] met on that board · [ ] open · (—) no board row yet.

## A. Gates (board rows)
| # | item | proof (row / id) | status @ ac1b61fb239a |
|---|---|---|---|
| A1 | Golden-gate hole closed; fresh official board; earlier boards provisional | AB-0003, AB-0007 | [ ] open, all boards provisional |
| A2 | Board fails on row regressions; land.sh runs option-gate + ratchet + dependents | AB-0006, AB-0005 | [ ] open |
| A3 | style | `style` 70/70 | [x] |
| A4 | style API | `style-api` 829/829 | [x] |
| A5 | js | `js` 174/174, `js-eval` 2/2 | [x] |
| A6 | js API | `js-api` 1239 pass / 302 skip; `js-options` 302 pass / 1239 skip (complementary split of 1541) | [x] (skips are the other row's half) |
| A7 | template options/API | `template-options` 67/67, `template-api` 5671/5671 | [x] |
| A8 | csr-render 390/390 | `csr-render` 355/390 (35 fail; csr-clusters[ssr] slot-name 16, other 14, attr-values 4, attr-order 1) | [ ] template |
| A9 | csr-render-template 570/570 | `csr-render-template` 545/570 (25 fail; slot-name 11, attr-order 7, attr-values 4, other 3) | [ ] template |
| A10 | ssr-render ×3 hold after AB-0002 | `ssr-render`, `ssr-render-async`, `ssr-render-asyncYield` 384/389 each (5 skip) | [ ] AB-0002 not landed, so numbers don't count yet (PLAN §4) |
| A11 | ssr_render_api, ssr_dynamic_imports hold after AB-0002 | `ssr_render_api` 2710/2724 (14 skip); `ssr_dynamic_imports` 16/16 | [ ] same |
| A12 | Every skip justified in decisions.tsv or removed | ssr-render 5, ssr_render_api 14, `template-renderer` 554 skip (14 pass) | [ ] |
| A13 | Option inventory complete | `option-gate` 61 fields / 61 mapped / 0 pending / 0 invalid | [x] |
| A14 | Unknown-option rejection asserted by board row | (—) row to be added (harness, AB-0008) | [ ] |
| A15 | semantic-diff gate: DIFFERENT w/o D-id = 0, unclassified = 0 | (—) `semantic-diff` row (semdiff) | [ ] |
| A16 | Coverage gate: every COVERAGE.tsv cell → row or D-id | (—) coverage-gate (harness; report-only now) | [ ] |
| A17 | LBC whole-component parity, gating | (—) `lbc` row (harness; Arch design first) | [ ] |
| A18 | Shims 0 (tagged + untagged) | `shim-check` SHIM(template-gap:=0, SHIM(\|TODO(parity=0, untagged=0 | [x] (ir_compat deleted; 12 shim ids retired) |
| A19 | Diagnostics parity (non-gating today; decide gate or D-id) | `template-diag` 636 mismatches / 63224; `template-diag-napi` 1630 (1120 seam-reject, apiVersion u32 → AB-0008) | [ ] decision needed |

## B. Architecture / single sources
| # | item | proof | status |
|---|---|---|---|
| B1 | One option schema; apiVersion f64 floor; undefined-vs-omitted semantics; name/namespace optional | AB-0008 (harness) | [ ] |
| B2 | Dispatch `.mts` + LWC1005 NO_AVAILABLE_TRANSFORMER | AB-0009 (harness, `crates/lwc-compiler/src/lib.rs:35-50`) | [ ] |
| B3 | One error model | AB-R003, AB-0001, AB-R001 (js-style) | [ ] AB-R001 rebasing |
| B4 | One LWC_VERSION | AB-0010 | [ ] |
| B5 | Shared tables in lwc-shared; crate owners delete own copies | AB-0011, D-0199 | [ ] |
| B6 | **Duplicate-table lint** (ship-owned, D-0199): tool scans all crates for duplicated literal tables (`&[&str]`, phf, `matches!` lists) by content hash, plus copies of lwc-shared tables; report-only, then gating at cutover. Findings → crate owner (template / js-style / ssr), never fixed by ship. Seeds found in SHIP-0: `VOID` ×3 (lwc-template codegen/mod.rs:588, lwc-template parser/element_check.rs:27, lwc-ssr compile_template/element.rs:47; contents differ, e.g. `param`), `RESERVED` ×2 (lwc-js transforms/decorators.rs:140, lwc-template parser/expr_check.rs:27) | D-0199; lint row (—) | [ ] not built |
| B7 | Zero open REWORK in arch-board/reviews.tsv | PLAN §3 open list | [ ] |
| B8 | Crate-surface review (each crate's modules/pub fns vs upstream package; unmatched → D-id or delete) | CUTOVER.md precondition 2; arch-board | [ ] |
| B9 | `cargo check --workspace && cargo test --workspace` green on main | CUTOVER.md; CI job (S5) | [ ] no CI. Live-tree SHIP-0 run green (368 pass) is non-official |

## C. Packaging / API (ship unless noted; see ship-survey.md §1-2)
| # | item | proof | status |
|---|---|---|---|
| C1 | Packaging plan (`ship-packaging-plan.md`) APPROVED by Arch before any product-code change | reviews.tsv AB-D0005 APPROVE (plan ab0a49e, notes b8ba1df) | [x] |
| C2 | npm package name/scope decided (`@lwc/compiler` name dies); working name `lwc-compiler-rs` (ship-packaging-plan.md §1) | registry/scope pending Tariq (D-0202); no publish | [ ] |
| C3 | Main package + per-triple optionalDependencies; ESM `index.js` loader; `index.d.ts` | S1-S2; `npm pack` + `node -e import()` smoke per triple | [ ] partial: S1 landed main 95b85c3 (AB-L0043): pkg + loader + linux-x64-gnu, untyped d.ts; typed d.ts = S2 after AB-0008; other triples S6 |
| C4 | Exports: `transformSync`, `version`; async `transform` kept or dropped by D-id | S3 (AsyncTask D-0209); `version` after AB-0010; lib.rs = ship (D-0200) | [ ] only `transform` (sync) today |
| C5 | Errors thrown as JS Error/AggregateError with code/filename/location/level/url (no `{error}` field) | shape = upstream CompilerError/CompilerAggregateError; js-style supplies `napi/src/errors.rs`, ship wires (D-0200); seam test | [ ] returned today |
| C6 | `instrumentation` supported or dropped by D-id | D-0205: kept; Rust returns events, loader replays | [ ] missing (S3) |
| C7 | parse5 out-of-repo path dep → git dep pinned by rev = external-inputs `parse5_rust_commit`; local override via `[patch]` in /workspace/wt/.cargo/config.toml | D-0201 (template edits crates/lwc-template/Cargo.toml); clean-checkout CI build | [ ] |
| C8 | Same checks run on the box by the in-repo harness as part of the official board (no GitHub Actions): fmt, clippy (shrink-only ratchet), `cargo check --workspace --locked`, test, napi build, ssr-compile-js-oracle gate against the packed napi (never `SSR_ORACLE_NAPI`, D-0226), tarball artifacts (no npm publish). Cloud-agent runs of `scripts/run-portable-board.sh` on `agent/*` branches are advisory. | D-0280; `scripts/official-board.sh` invokes `scripts/c8-checks.sh` and `harness/board.sh`. Deleted `.github/workflows/rust-ci.yml` (landed as eac1ae9). `cargo fmt --all --check` is reported but not gating until D-0235 (known red on main 3ecff19). | [ ] |

## C2. Consumer-visible breaking changes vs @lwc/compiler 9.5.3 (all locked rows in decisions.tsv)
| diff | D-id |
|---|---|
| unknown option keys rejected (upstream spreads and ignores) | D-0203, D-0010 |
| `apiVersion` any JS number, floored | D-0206 |
| `transform` Promise resolves off main thread (napi AsyncTask); same result/errors as `transformSync` | D-0209 |
| errors thrown as real `Error` instances with upstream fields; `CompilerError` exported as type only | D-0210 |
| `TransformResult.map` is a Source Map v3 object, not Babel's type | D-0211 |
| Node >= 20; triples linux-x64-gnu, linux-arm64-gnu, darwin-x64, darwin-arm64, win32-x64-msvc only; Linux glibc >= 2.28 | D-0213, D-0220 |
| `NormalizedTransformOptions` type export dropped | D-0214 |
| package name changes from `@lwc/compiler` (working `lwc-compiler-rs`; not published) | pending Tariq |
| wtr drops non-upstream `dynamicImports.strict` at port | D-0203 |
| template `compile()` (Rust lwc-template, replacing `@lwc/template-compiler`) result no longer carries `root` (9.5.3 returned `{code, root, warnings}`); callers needing the IR call `parse()` | D-0230 |
Kept as upstream (not breaking): `instrumentation` (D-0205), optional `name`/`namespace` (D-0207), normalizeOptions incl. explicit `undefined` (D-0208), deprecation warnings once per process (D-0212).

## D. Consumers (ported in `/workspace/wt/lwc-consumers`, branch ship/consumers @ b9e8007, local only, never pushed, never read by board: D-0202; `/workspace/lwc` stays clean)
| # | item | replaced by | status |
|---|---|---|---|
| D1 | @lwc/rollup-plugin on napi `transformSync` (index.ts:12,17,372); rollup-plugin spec suite green | Rust napi transform | [ ] |
| D2 | rollup-plugin output format: ESM only vs current es+cjs (scripts/rollup/rollup.config.js:27) | D-id | [ ] |
| D3 | Move engine-server fixtures out of `@lwc/ssr-compiler/src/__tests__/fixtures` (engine-server fixtures.spec.ts:125) | move fixtures | [ ] |
| D4 | ssr-runtime styles.spec.ts:2 off `@lwc/style-compiler` | Rust napi transform (.css) | [ ] |
| D5 | integration-wtr serve-integration.js:28 `dynamicImports.strict` (unknown key: napi rejects) | consumer drops it at port; unknown options stay rejected (D-0203) | [ ] |
| D6 | wdio, integration-types, perf-benchmarks-components, playground, engine-server, ssr-runtime render tests via rollup-plugin | follows D1 | [ ] |
| D7 | `lwc` barrel: `./compiler` repoint; delete `./ssr-compiler`, `./style-compiler`, `./template-compiler`, `./babel-plugin-component` subpaths + deps (package.json:25-38,61-75) | delete (D-id: breaking) | [ ] |
| D8 | vitest.config.mjs project entries; docs (ARCHITECTURE.md, AGENTS.md, READMEs) | delete/update | [ ] |

## E. Delete + Verify + Rollback
- [ ] Packages that **stay**: `@lwc/ssr-runtime` (JS ABI frozen; SSR goldens call it), `@lwc/engine-dom`, `@lwc/engine-core`, `@lwc/errors`, `@lwc/shared`, `@lwc/features`, `@lwc/engine-server` (until its own deprecation).
- [ ] Remove / stop publishing in ONE revertable commit: `@lwc/compiler`, `@lwc/style-compiler`, `@lwc/template-compiler`, `@lwc/babel-plugin-component`, `@lwc/ssr-compiler`. Workspaces glob needs no edit; nx/tsconfig/CODEOWNERS have no per-package entries; release scripts glob packages (ship-survey.md §3).
- [ ] Duplicate-table lint (B6) = 0 findings.
- [ ] Run Verify:
```
cd /workspace/lwc-compiler-rust && cargo check && cargo test
cd harness && LWC_COMPILER_BACKEND=rust npx vitest run
cd <consumers clone> && grep -rn "@lwc/\(compiler\|style-compiler\|template-compiler\|babel-plugin-component\|ssr-compiler\)" packages --include=package.json --include=*.ts -l   # expect none
cd <consumers clone> && yarn build && yarn test
```

## Rollback
Deletion is one revertable commit; keep last JS versions published on npm (deprecate, don't unpublish).
Flip `LWC_COMPILER_BACKEND=js` / re-pin consumers to the last JS release; goldens remain valid.

## SSR template-gap shims

**All 12 ids retired (2026-11-01 8:55 PM PST). Total tags: 0.** `rg -c 'SHIM\(template-gap' crates/lwc-ssr` finds nothing, and board shim-check reports `SHIM(template-gap:=0 ... untagged=0`. `crates/lwc-ssr/src/compile_template/ir_compat.rs` (434 lines) is deleted, together with its `mod ir_compat` declaration and every call site. lwc-ssr now consumes `lwc_template::parse` IR directly: `compile_cx` walks `root` as given, and `transform_html` reads `preserve_comments` from `Root.directives`. There is no source rescan, for:each mask, config override or parse5 root filter. Template fixed every gap in lwc-template (napi 59fb85c01259; see `/workspace/notes/template-rust/shim-retire-status.md`). Board after the deletion (live tree; lwc-ssr tree sha f4f1ed19b782, crates sha 20b2bc696264, napi f456a3353b1f, 8:55 PM PST): ssr-render, ssr-render-async and ssr-render-asyncYield 384 + 5 skipped each; ssr, ssr-async and ssr-asyncYield 388; ssr_render_api 2710 passed + 14 skipped; ssr_dynamic_imports 16/16; untagged 0. The table below is kept as history; the IR contracts are in `/workspace/notes/lwc-ssr-rust/template-ir-contracts.md`.

| id | tags | sites (file:fn) | fixtures | root cause in lwc-template (file/line) | Template fix that retires it (IR contract) | status |
|---|---|---|---|---|---|---|
| `preserve-comments-directive` | 4 | lib.rs:transform_html (root_preserves, config override, preserve_comments), lib.rs:root_has_preserve_comments | comments-basic, comments-foreach, comments-if, comments-slot, comments-text-preserve, adjacent-text-nodes | parser/mod.rs:460 builds `Root { directives: vec![] }` (ROOT_DIRECTIVES at :491 is only validated); :256 keeps comments only when `cfg.preserve_html_comments` | emit RootDirective::PreserveComments(true) (and RenderMode) from root attrs; keep comments when the directive is set | retired |
| `lwc-directive-attrs` | 7 | ir_compat.rs:normalize_node (Element/ExternalComponent/Slot/Component/LwcComponent arms), as_expr, lift_directives | lwc-inner-html, inner-outer-html, barrel-lwc-exports, render-correct-tags (inner-html); lwc-dom-manual; lwc-on; lwc-dynamic; inner-outer-html (spread); scoped-slots, slot-forwarding, context-slotted, dynamic-components (slot-bind); directive-external | parser/mod.rs:310 builds `directives` from Key and Ref only; every other `lwc:*` stays in `attributes` via `lower_attr` (:184) | lift lwc:inner-html/dom/spread/on/dynamic/is/slot-bind out of attributes into ElementDirective on all element kinds | retired |
| `lwc-component-node` | 1 | ir_compat.rs:normalize_node (lwc:component block) | dynamic-components, dynamic-imports, attribute-class, read-only-props | parser/mod.rs:335 else-branch builds `ChildNode::Element` for any non-custom tag, `<lwc:component>` included (codegen/mod.rs:362 notes the gap) | lower <lwc:component> to ChildNode::LwcComponent (Is directive + component prop split) | retired |
| `component-prop-split` | 7 | ir_compat.rs:normalize_node (Component, LwcComponent arms), is_component_attribute, SPECIAL_ATTR_TO_PROP, ARIA_PROPS, attribute_to_property_name, resplit_component | attribute-component-aria, attribute-component-global-html, attribute-global-html/as-component-prop/*, attribute-global-html/tabindex, known-boolean-attributes/default-def-html-attributes/static-on-component, read-only-props, dynamic-components | parser/mod.rs:153 `keeps_as_attr` (role\|style\|class\|id\|slot\|key\|aria-*\|data-*) vs JS `isAttribute` for components (style\|class\|key\|slot\|exportparts\|data-*); :166 `split_props` also keeps any `:` name and names props with `kebab_to_camel` (:157) instead of `attributeToPropertyName` | component attrs = style\|class\|key\|slot\|exportparts\|data-*; all else Property named by attributeToPropertyName (role, id, aria-* become props; tabindex→tabIndex) | retired |
| `clean-text-node` | 4 | ir_compat.rs:normalize_children (reclean call), js_clean_text_node, split_mustache, reclean_texts | attribute-live-bindings, slots, slots-fallback, dynamic-slots, wire/deep-reference, imported-member | parser/mod.rs:62 `clean_text` trims the whole text node (used at :242) instead of babel `cleanTextNode` (per-line trim, non-last lines + " ", drop whitespace-only); each segment's `Text.raw` is the whole node (:252) | Text literals = babel cleanTextNode (per-line trim, inner newline→space, trailing space before siblings kept) | retired |
| `valueless-attr-bool` | 3 | ir_compat.rs:mark_valueless_attrs (source rescan); lib.rs:transform_html call (2 tags) | global-html-attributes, known-boolean-attributes/*, attribute-boolean, attribute-global-html/hidden | parser/mod.rs:184-191 `lower_attr` always makes `LiteralValue::String(a.value)`; parse5 gives `""` for a valueless attr so `hidden` and `hidden=""` are indistinguishable after parse | valueless attrs (`<input hidden>`) → LiteralValue::Bool(true); `attr=""` stays String("") | retired |
| `empty-ns-prefix` | 1 | ir_compat.rs:normalize_node (Element arm `:xmlns` strip) | svgs | parser/mod.rs:262 `Some(p) => format!("{p}:{}", a.name)` runs when parse5's foreign-attr prefix is `Some("")` | do not qualify foreign attrs with an empty prefix (`xmlns`, not `:xmlns`) | retired |
| `complex-expr-literal` | 3 | ir_compat.rs:lift_quoted_complex; lib.rs:transform_html call (2 tags) | attribute-dynamic-complex, text-interpolation-complex, scoped-slots/complex, lwc-if-else-complex | parser/mod.rs:99 `parse_binding` handles only Identifier/Member; `lower_attr` (:186) and text segments (:250) fall back to a string Literal, whereas lwc:if conditions (:211-227) already lower to `Expression::Complex` | complex `{expr}` in attrs/text → Expression::Complex(inner source) instead of a "{...}" string Literal | retired |
| `complex-for-each` | 11 | ir_compat.rs:mask_complex_for_each (doc + fn, 5 tags), unmask_complex_for_each; lib.rs:transform_html mask block + unmask call (6 tags) | for-each-block/complex | parser/mod.rs:128 `loop_directive`: `parse_binding(&a.value)?` returns None for a complex value, so no Loop; validate.rs:75-82 `template()` then pushes 1153 and drops the node | for:each with a complex value → ForEach{expression: Complex(..)}, no 1153 drop | retired |
| `ir-compat-walk` (driver) | 11 | ir_compat.rs: module doc, normalize_root, normalize_children, normalize_else, normalize_node, for_each_node, for_each_attr_value; compile_template/mod.rs: `mod ir_compat`, normalize_root call (2); lib.rs:transform_html `root.clone()` | all of the above | none (tree walkers that dispatch the shims above; `normalize_else` only recurses into lwc:elseif/lwc:else, since the else chain already matches JS) | retires once the 9 ids above are closed and ir_compat.rs is deleted | retired |
| `parse5-error-root` | 3 | lib.rs:transform_html (parse5_failed computation, root filter, comment) | parse5/unclosed-div (ssr_render_api, apiVersion >= 59) | parser returns `Some(root)` even when parseHTML produced Error-level 1058 (INVALID_HTML_SYNTAX) diagnostics; JS template-compiler parser/index.ts:147 returns `{warnings}` with no root, and ssr-compiler compile-template/index.ts:110 then throws | return `root: None` whenever any Error-level diagnostic comes out of HTML parsing (respecting TREAT_ALL_PARSE5_ERRORS_AS_ERRORS from v59) | retired |
| `expr-error-recovery` | 3 | ir_compat.rs:drop_errored_expr_nodes; lib.rs:transform_html call (2 tags) | *-complex fixtures in ssr_render_api below apiVersion 66 (attribute-dynamic-complex, lwc-if-else-complex, scoped-slots/complex, text-interpolation-complex) | below v66 (or without experimentalComplexExpressions) Template emits 1210 INVALID_NODE_CTE_API_VERSION but keeps the node: attrs/slot-bind stay a literal `"{...}"` String, lwc:if/elseif stay `Expression::Complex` | JS `withErrorRecovery` in parseChildren: drop the whole node whose expression failed validation (element, text, lwc:if template, slot with slot-bind); the following orphaned lwc:else/elseif is dropped too | retired |

## Official board procedure (2026-11-01 20:22)
- `scripts/official-board.sh` is the ONLY official path: frozen snapshot of Cargo.toml/Cargo.lock/crates + ../parse5-rust, fresh target dir (== cargo clean), cargo check --workspace --all-targets + seam crate tests must pass, napi built from snapshot, board.sh with SHIM_ROOT=snapshot, snapshot re-hashed after (mutation => INVALID). Report napi sha + src sha.
- `cargo clean && install-napi.sh && board.sh` on the live tree is NOT official: concurrent agent edits leaked into two runs (20:16, 20:18).
- VALID run (snapshot unchanged AND board rc 0, full row set) writes push artifacts: `/workspace/push/official-<srcsha>.tgz` + `official-<srcsha>.board.txt`. Only these may be pushed (Migrate 2026-11-01). parse5-rust is a build dep hashed into src sha but NOT in the tgz.
- Harness runs from the snapshot too (tests/goldens frozen); gates read snapshot crates via CRATES_ROOT.
- Golden integrity: `harness/scripts/golden-gate.sh` re-records all rows on the JS backend (oracle b9e8007) into a scratch copy and diffs goldens/ — wired into official-board.sh (abort before build on any diff).
- VALID runs are published to `/cursor/stores/store-4280618e-f4d8-4614-b1b6-bf88ea27f849/official/<src>/` (tgz, board.txt, OFFICIAL_BOARD.txt written last with sha256s) by scripts/publish-official.sh. Migrate pushes only from there.
