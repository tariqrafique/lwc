# lwc-compiler-rust: target architecture (LWC Arch Board, 2026-11-01)

Rust main: `/workspace/lwc-compiler-rust` (main @ c6c0240 when this was written; crates/ unchanged since baseline f1513cf).
Oracle: `/workspace/lwc` @ b9e8007 (LWC 9.5.3). Official board: tariqrafique/lwc `rust-compiler` @7b3cdca.
Policy: ESM only. A breaking full-Rust exit is allowed, with goldens locking runtime semantics. No Babel byte-identity.
Divergences only as logged rows in `notes/lwc-rust-migrate/decisions.tsv`.

All module lists below come from `find crates/*/src -name '*.rs'` and `find packages/@lwc/<pkg>/src -name '*.ts'` (no `__tests__`).
Line counts are `wc -l`.

## Dependency direction (allowed)

```
lwc-shared  <-  lwc-template  <-  lwc-ssr
    ^   ^            ^              ^
    |   +-- lwc-js   |              |
    +------ lwc-style|              |
            lwc-compiler (dispatch + options)  <-  lwc-compiler-napi (seam)
```

Rules:
- `lwc-shared` depends on nothing in the workspace.
- `lwc-ssr` may use only `lwc-template`'s public API (parse/Config/IR/scope tokens), never `parser::*` or `codegen::*` internals.
- `lwc-js` and `lwc-style` never depend on `lwc-template` / `lwc-ssr`.
- Only `lwc-compiler` depends on several product crates. Only `lwc-compiler-napi` depends on napi/serde for options.

What the Cargo.toml files actually say ([dependencies]):

| crate | edition | workspace deps | external |
|---|---|---|---|
| lwc-shared | 2024 | none | none |
| lwc-template | 2024 | lwc-shared | parse5 (path dep outside the workspace), serde |
| lwc-ssr | 2024 | lwc-shared, lwc-template | oxc |
| lwc-js | 2024 | lwc-shared | oxc, oxc_sourcemap |
| lwc-style | **2021** | lwc-shared | cssparser 0.34 |
| lwc-compiler | 2024 | lwc-shared, lwc-style, lwc-template, lwc-ssr, lwc-js | none |
| lwc-compiler-napi | 2024 | lwc-compiler, lwc-shared | napi, napi-derive, serde, serde_json |

**There are no crate-level dependency violations.**
lwc-ssr reaches lwc-template only through public items: `lwc_template::Config` (lwc-ssr/src/lib.rs:72/89), `parse` (:94), `RootDirective` (:142), and `generate_scope_tokens` (:154, compile_template/mod.rs:256).

**There are module-level ownership violations** (code that duplicates another crate's source of truth). See sweep AB-0001, AB-0010 and AB-0011:
- lwc-ssr re-implements `generate_custom_element_tag_name` and kebab→camel case.
- lwc-ssr carries its own VOID and BOOLEAN attribute tables.
- lwc-js and lwc-ssr each inline lwc-class error strings.
- lwc-js and lwc-style each define `LWC_VERSION`.
- lwc-compiler-napi owns option validation and normalization plus a separate error-URL table.

---

## lwc-shared  (mirrors @lwc/errors + @lwc/shared)

Actual modules: `lib.rs` (692), `api_version.rs` (93), `template_ir.rs` (164).

Upstream mirrored today:
- `api_version.rs` mirrors /workspace/lwc/packages/@lwc/shared/src/api-version.ts. Single source; lwc-template config.rs:54 only aliases it (decision row 157).
- `lib.rs` contains the following:
  - `LwcErrorInfo`, `CompilerDiagnostic::generate` (:554), `generate_error_message` and `DiagnosticLevel` mirror /workspace/lwc/packages/@lwc/errors/src/compiler/errors.ts and shared/types.ts.
  - **Only one** error-info entry lives here: `CSS_TRANSFORMER_ERROR` (:482).
  - `CompileOptions` (:208) is labelled the single source of option defaults (see AB-0008).
  - `not_implemented` (:310) is a dead fallback.
- `template_ir.rs` mirrors /workspace/lwc/packages/@lwc/template-compiler/src/shared/types.ts (IR). `ComplexExpression = String` (:36) is lossy (AB-0013).

Required modules (target):
- `errors/error_info/{compiler,lwc_class,ssr,template_transform}.rs`: generated from /workspace/lwc/packages/@lwc/errors/src/compiler/error-info/{compiler.ts (27 codes), lwc-class.ts (30), ssr.ts (1), template-transform.ts (132)}. Full `{code, message, level, strict_level, url}`. Named constants, as `DecoratorErrors.*` / `ParserDiagnostics.*` are upstream.
- `meta.rs`: `LWC_VERSION`, `LWC_VERSION_COMMENT`, `LWC_VERSION_COMMENT_REGEX`, from /workspace/lwc/packages/@lwc/shared/src/meta.ts.
- `void_elements.rs`: from /workspace/lwc/packages/@lwc/shared/src/void-elements.ts.
- `html_attributes.rs`: from /workspace/lwc/packages/@lwc/shared/src/html-attributes.ts. Covers boolean attrs, GLOBAL_HTML_ATTRIBUTES, SPECIAL_PROPERTY_ATTRIBUTE_MAPPING and kebab↔camel.
- `aria.rs`: from /workspace/lwc/packages/@lwc/shared/src/aria.ts.
- `custom_element.rs`: from /workspace/lwc/packages/@lwc/shared/src/custom-element.ts.
- `template_ir.rs`: unchanged, except `ComplexExpression` becomes a structured expression (AB-0013).

Invariants:
- Every `LWCxxxx` code and message used anywhere in the workspace resolves to a generated entry here.
- No crate other than lwc-shared contains a 4-digit LWC code literal next to a message literal.
- One `LWC_VERSION`.
- Tables shared by template, SSR and engine semantics exist once.

## lwc-template  (mirrors @lwc/template-compiler)

Actual modules:
- `lib.rs` (1162, of which a large part is unit tests)
- `config.rs` (163)
- `scope_tokens.rs` (66)
- `codegen/mod.rs` (962)
- `parser/{mod.rs (885), element_check.rs (419), expr_check.rs (655), validate.rs (324), messages.rs (176), tables.rs (185)}`

Upstream (/workspace/lwc/packages/@lwc/template-compiler/src/):
- `config.ts`, `index.ts`, `scopeTokens.ts`, `state.ts`
- `parser/{index.ts, parser.ts, attribute.ts, constants.ts, expression.ts, expression-complex/{index,types,validate}.ts, html.ts, parse5Errors.ts, utils/{html-element-attributes,html-elements,javascript,svg-elements}.ts}`
- `codegen/{codegen,expression,helpers,index,optimize,static-element,static-element-serializer}.ts`, `codegen/formatters/module.ts`
- `shared/{ast,constants,estree,naming,renderer-hooks,types,utils}.ts`

How the current files map:

| Rust today | Mirrors |
|---|---|
| config.rs | config.ts (legitimate) |
| scope_tokens.rs | scopeTokens.ts |
| parser/mod.rs | parser/index.ts lowering (plus tables that belong elsewhere) |
| parser/element_check.rs | the `ctx.throw*` checks of parser/index.ts `parseElement`, run as a **separate pre-pass** |
| parser/validate.rs | warn/throw call sites of parser/index.ts |
| parser/expr_check.rs | parser/expression.ts + expression-complex/validate.ts, via a hand tokenizer instead of a JS parser |
| parser/messages.rs | errors/.../template-transform.ts (a copy; belongs in lwc-shared) |
| parser/tables.rs | parser/utils/*.ts + shared/constants.ts. Header says "Generated ... (tools: /tmp/gen.ts)"; /tmp/gen.ts does not exist |
| codegen/mod.rs | codegen/*.ts, in one file |

Required modules (target layout, one Rust module per upstream file):
- `parser/index.rs`: lowering, with the throw/warn call sites inline in upstream order. element_check.rs and validate.rs dissolve into it.
- `parser/attribute.rs`: attributeToPropertyName (now parser/mod.rs:284), normalizeAttributeValue, isAttribute and isValidHTMLAttribute. It also takes the attribute predicates now at codegen/mod.rs:561/566/572.
- `parser/constants.rs`: VALID_IF_MODIFIERS, DISALLOWED_MATHML_TAGS, DISALLOWED_HTML_TAGS, EVENT_HANDLER_NAME_RE, ATTRS_PROPS_TRANFORMS, KNOWN_HTML_AND_SVG_ELEMENTS.
- `parser/parse5_errors.rs`: PARSE5_ERROR_ON / PARSE5_WARN_OLDER (now parser/mod.rs:848/866).
- `parser/expression.rs` (+ `parser/expression_complex/validate.rs`): one `is_potential_expression` (today it exists twice, at validate.rs:87 and element_check.rs:55).
- `parser/utils/{html_elements,svg_elements,html_element_attributes}.rs`: split from tables.rs. The generator gets committed under `tools/`.
- `shared/types.rs` (ElementDirectiveName, RootDirectiveName) and `shared/constants.rs` (DASHED_TAGNAME_ELEMENT_SET).
- `codegen/{codegen,expression,helpers,optimize,static_element,static_element_serializer,formatters/module}.rs`.

Invariants:
- Diagnostics are emitted in one traversal, in upstream `parseElement` order. No second walk re-derives the same element facts.
- Every table is either generated from upstream (with a committed generator) or imported from lwc-shared.
- No message strings are defined here.
- The IR handed to lwc-ssr is the only contract (`tests/ssr_ir_contracts.rs`).

## lwc-ssr  (mirrors @lwc/ssr-compiler)

Actual modules:
- `lib.rs` (224), `runtime_abi.rs` (42)
- `compile_js/{mod.rs (1802), class_rewrite.rs (218), dynamic_imports.rs (143)}`
- `compile_template/{mod.rs (422), element.rs (255), component.rs (321), slot.rs (58), bind_expr.rs (124)}`

Upstream (/workspace/lwc/packages/@lwc/ssr-compiler/src/):
- `compile-js/{index,generate-markup,lwc-import,register-base-class-props,remove-decorator-import,stylesheet-scope-token,stylesheets,errors,types}.ts`
- `compile-js/decorators/{index,track,wire}.ts`, `compile-js/decorators/api/{index,validate}.ts`
- `compile-template/{index,context,expression,ir-to-es,shared,adjacent-text-nodes,types}.ts`
- `compile-template/transformers/{comment,element,for-each,for-of,legacyIf,lwcIf,slot,text}.ts`, `transformers/component/{component,lwc-component,slotted-content,index}.ts`
- `estemplate.ts`, `estree/{builders,validators}.ts`, `imports.ts`, `transmogrify.ts`, `shared.ts`, `index.ts`

Divergence:
- `compile_js/mod.rs` is a line/text scanner over `&str` (doc :1-3 "Minimal sync compileComponentForSSR ... strips decorator lines"). It has 129 `.lines()/find/contains/starts_with/strip_prefix` hits in compile_js/. See AB-0002.
- oxc is used only in class_rewrite.rs, dynamic_imports.rs and compile_template/bind_expr.rs.
- `compile_template/element.rs:25-28,47` re-declares boolean/void tables.

Required modules:
- `compile_js/{mod (index), decorators/{mod,api/{mod,validate},track,wire}, generate_markup, lwc_import, register_base_class_props, remove_decorator_import, stylesheet_scope_token, stylesheets}.rs`, all over an oxc AST.
- `compile_template/transformers/*` per upstream file.
- `transmogrify.rs` for the async/sync/asyncYield modes.

Invariants:
- No source-text scanning of JS.
- Errors come from lwc-shared error_info (lwc_class / ssr).
- Template expressions arrive structured from the IR; there is no re-parse with a silent fallback (bind_expr.rs:73).

## lwc-js  (mirrors @lwc/babel-plugin-component; oxc replaces Babel)

Actual modules:
- `lib.rs`, `parse.rs`, `codegen.rs`, `edits.rs`, `helpers.rs`, `srcmap.rs`, `constants.rs`, `tag_name.rs`
- `transforms/{mod, component, decorators (594), class_properties, private_fields, private_methods, dynamic_imports, hoist, lower, scoped_css, unforgeables, version_stamp}.rs`

Upstream (/workspace/lwc/packages/@lwc/babel-plugin-component/src/):
- `index.ts`, `component.ts`, `constants.ts`, `utils.ts`, `types.ts`
- `compiler-version-number.ts`, `dynamic-imports.ts`, `scope-css-imports.ts`
- `private-method-transform.ts`, `reverse-private-method-transform.ts`
- `decorators/{index,types}.ts`, `decorators/api/{index,shared,transform,validate}.ts`, `decorators/track/index.ts`, `decorators/wire/{index,shared,transform,validate}.ts`

Mapping is reasonable at file level. Divergences:
- `transforms/decorators.rs` is one file where upstream splits api/wire/track × transform/validate. It inlines `derr(code, "message")` for codes 1092-1132, 1199 and 1200 (:104-519).
- `constants.rs:14` pins `LWC_VERSION = "9.5.3"`.
- `tag_name.rs:2` duplicates @lwc/shared custom-element.ts.

Required:
- `transforms/decorators/{mod,api/{transform,validate},wire/{transform,validate},track}.rs`
- errors from lwc-shared, version from `lwc_shared::meta`, tag name from `lwc_shared::custom_element`.

Invariants: no message literals; one version source.

## lwc-style  (mirrors @lwc/style-compiler)

Actual modules: `lib.rs`, `ast.rs`, `parser.rs` (cssparser), `syntax_check.rs` (393), `scoping.rs` (290), `selector.rs` (209), `serialize.rs` (211), `consts.rs`, `error.rs`.

Upstream (/workspace/lwc/packages/@lwc/style-compiler/src/):
- `index.ts`, `transform.ts`, `serialize.ts`, `postcss-lwc-plugin.ts`
- `css-import/transform.ts`, `dir-pseudo-class/transform.ts`, `no-id-selectors/validate.ts`, `scope-at-rules/transform.ts`, `selector-scoping/{transform,validate}.ts`
- `utils/{dir-pseudoclass,error-recovery,message,rtl,selector-parser,selectors-scoping}.ts`

Divergences:
- `scoping.rs` folds postcss-lwc-plugin.ts plus five plugin dirs into one file.
- There are two CSS front-ends: the cssparser tree (parser.rs) and a PostCSS tokenizer/parser replica (`syntax_check.rs:1-13`) whose only job is to decide whether postcss would throw.
- `consts.rs:3` `LWC_VERSION = env!("CARGO_PKG_VERSION")`, which is the crate version, not LWC's.
- Edition 2021.

Required:
- `transforms/{css_import, dir_pseudo_class, no_id_selectors, scope_at_rules, selector_scoping/{transform,validate}}.rs` under `postcss_lwc_plugin.rs`.
- Version from `lwc_shared::meta`.
- Edition 2024.

Invariant: the serialized version comment equals `LWC_VERSION_COMMENT` from shared/meta.ts.

## lwc-compiler  (mirrors @lwc/compiler)

Actual modules: `lib.rs` (312) only.

Upstream (/workspace/lwc/packages/@lwc/compiler/src/): `index.ts`, `options.ts`, `transformers/{transformer,javascript,style,template,shared}.ts`.

Divergences:
- There is no `options.rs`. validateOptions / normalizeOptions live in lwc-compiler-napi `to_compile_options` (lib.rs:295).
- `dispatch` (lib.rs:28-50) uses `rsplit('.')` and omits `.mts`. An unknown extension returns an uncoded error instead of TransformerErrors.NO_AVAILABLE_TRANSFORMER (1005).

Required:
- `options.rs`: TransformOptions → validate → NormalizedTransformOptions, mirroring options.ts:28-249.
- `transformers/{mod (transformer.ts), javascript, style, template}.rs`.

Invariants:
- Every Rust-API caller gets the same validation as napi callers.
- Option defaults are defined once.

## lwc-compiler-napi  (the seam; no upstream package)

Actual modules: `lib.rs` (479), `aggregate.rs` (200), `single_error.rs` (161), `error_urls.rs` (12).

It should own exactly two things:
- serde deserialization of the JS object (with `deny_unknown_fields`, decision row 11);
- the JS error/diagnostic shape (aggregate.rs, single_error.rs).

Divergences:
- It owns option normalization (lib.rs:295-345).
- It has a second option schema (lib.rs:13-16, "Keep in sync with RUST_OPTION_KEYS in harness/src/backend.ts", the third schema at backend.ts:72).
- It has its own `CODES_WITH_URL` table (error_urls.rs:4) and code consts (single_error.rs:41-43).

Invariants:
- No defaults and no validation rules here.
- Accepted keys and the harness `RUST_OPTION_KEYS` are generated from one schema.

## Harness (not a crate; gates)

- `harness/scripts/golden-gate.sh`: re-records goldens with the JS oracle and diffs them.
- `harness/board.sh`: behavioural board.
- `harness/scripts/{shim-gate,option-gate}.mjs`.
- `scripts/land.sh` (+ `land-classify.sh`, `review-patch-id.sh`, `test-land-classify.sh`).

Invariant: a gate never passes on an error it swallowed. See AB-0003, AB-0005, AB-0006 and AB-0007.

## Board standing rules (Migrate routing, 2026-11-01)
- REJECT any SSR JS parity change built on the lwc-ssr compile_js line-scanner (AB-0002). SSR's oxc rebuild needs an approved design review before the bulk code lands.
- AB-0001 + AB-0010 (one lwc-shared error-info/version model) are owned by LWC JS Rust, folded into AB-R003. Until they land, REJECT diagnostics changes that edit error-info tables or code/message literals.
- Trivial paths: docs/* only (AB-0004). Re-enabling a + glob requires the classifier hardening in land-gate-proposal.md.
- Harness gate order: AB-0003 golden-gate, AB-0007 missing-golden write, then AB-0005/0006, then a fresh board diffed against dfa221a, then AB-0008 options.

### Semantic Diff rules (Migrate, 2026-11-01)
- LWC Semantic Diff (45d9257f) gives deterministic semantic-equivalence verdicts for output diffs. No LLM judgement.
- Every canonicalization rule needs a written safety proof plus negative tests, i.e. inputs where the rule must NOT equate.
- REJECT any rule that reorders code with observable effects unless purity is proven.
- COVERAGE.tsv gets a semantic_diff column: the classification per input source.

### One definition for @lwc/shared helpers
- JS whitespace/trim, parseStyleText/normalizeStyleAttributeValue and htmlEscape live once in lwc-shared (AB-L0022). Crate-local copies diverge: SSR's js_ws accepted U+0085, but JS does not.

## 2026-11-01 late: invariants added
- One JS string-literal printer: `lwc_shared::js_literal::json_string` (JSON.stringify over UTF-16 units, the astring raw-less Literal path). Debug formatting (`:?}`) in emit code is banned by lwc-ssr/tests/debug_format_gate.rs, which covers lwc-ssr src and lwc-template codegen. Template's js_string moves onto it under AB-X0016.
- One JS whitespace definition: `lwc_shared::js::{js_ws,js_trim}`; enforced by lwc-shared/tests/single_definition.rs.
- AB-D0004: complex expressions are a borrowed oxc AST in the IR; there is one printer (lwc-template printer::print_expr, an astring port) for CSR and SSR (D-0221: SSR takes S5). Upstream astring bugs are reproduced faithfully (D-0224).
- Dependency changes must carry their Cargo.lock in the reviewed diff; a --locked gate is coming (D-0223).
- Path ownership: `OWNERS` on main is the single source (D-0227); PLAN §3 lists roles only. Lane-crossing needs a `Cross-Lane: <bot>` trailer backed by a recorded agreement, never an asserted one. bot/hygiene (AB-L0049) is re-reviewed when Harness resubmits it.
- reviews.tsv patch_id holds exactly one bare 40-hex pid (the one land.sh will compute). Never annotate it; put own/cumulative notes in the files column or the shape text.

### Invariant: codegen per-node facts live on the node (AB-L0058, 2026-11-02)
Template codegen reads per-element facts (namespace, renderer need, slot parent, key) from the IR node, through ElementView or a typed parameter. `Gen` holds only template-wide state, such as counters, registries and config. A `Gen` field that one element writes and a later element reads is the svg:true leak class of bug (SD-1B #9) and gets rejected in review.

### Logged divergence: attribute normalization is a child of element_check (AB-X0017, 2026-11-08)
Upstream `parser/attribute.ts` is a sibling module whose `normalizeAttributeValue` is called from `parser/index.ts` (getTemplateAttribute). In Rust the file stays at `parser/attribute.rs`, but it is mounted as `parser::element_check::attribute` with `#[path]`, and `normalize_attribute_value` is `pub(super)`. That makes element_check's attribute pre-pass the only caller the compiler allows (E0603 anywhere else). The `pub(in …)` alternative does not compile on a sibling module (E0742). Invariant: attribute values are normalized exactly once, and lowering reads `CheckedAttr { attr, value }` pairs. Never re-normalize or look up attributes by pointer.

### Invariant: dependency allowlist is cargo metadata (AB-L0055, 2026-11-08)
`lwc-shared/tests/dependency_allowlist.rs` reads `cargo metadata`, covering normal, build, target-cfg and renamed dependencies. No TOML text scanners.

### Invariant: JS syntax errors come from oxc's diagnostic (D-0248 restating D-0155, AB-L0063, 2026-11-08)
For JS syntax errors, the LWC1007 code, the filename, the SyntaxError class and the line/column convention match upstream. The message text and the exact position come from oxc's diagnostic (its message and label span). No code in lwc-js may compare oxc message text or rebuild @babel/parser messages; any proposal that matches on oxc text is permanently rejected. Positions come only from AST or diagnostic spans, never from scanning source text.

### Ruling: semdiff F9 observation (AB-L0064, 2026-11-08, applying D-0196's default)
These are OBSERVED, not quote/escape spelling, because each one is a different AST node kind or flag and Function#toString shows it:
- property-key form: Identifier vs StringLiteral vs computed;
- shorthand vs longhand properties;
- TemplateLiteral vs StringLiteral.

semdiff must never certify EXEC_EQUAL while a spelling-sensitive token sits unexamined in an unaligned or moved run.
