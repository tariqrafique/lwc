# LWC → Rust compiler program

Owner: LWC Migrate. Tariq: full autonomy; consumers assumed on Rust; AMD out.

## Paths
- JS source of truth: `/workspace/lwc` (salesforce/lwc ~9.5.3, b9e8007)
- Rust workspace: `/workspace/lwc-compiler-rust`
  - `crates/lwc-compiler` — facade `transform` by extension / flags
  - `crates/lwc-style` — style (PostCSS Rust)
  - `crates/lwc-template` — template (Template Rust + parse5-rust)
  - `crates/lwc-ssr` — SSR (SSR Rust)
  - `crates/lwc-js` — component JS (JS Rust)
  - `crates/lwc-shared` — shared constants/errors
- parse5-rust: `/workspace/parse5-rust` (github.com/tariqrafique/parse5-rust)
- Style research: `/workspace/notes/postcss-rust/`
- Decision log: `/workspace/notes/lwc-rust-migrate/decisions.tsv`
- Phase 0 Page: https://cursor.com/canvas/store-fc170c82-5486-41f5-8389-5dec6c9db1f3/eed7c8b5-736c-4c6b-a894-d4496bf15662

## CompilerBackend seam (locked)
```
transform(src: &str, filename: &str, options: CompileOptions) -> Result<CompileResult, CompileError>
CompileResult { code: String, map: Option<String> }  // style keeps map == Some("") during port
```
JS reference: `@lwc/compiler` `transformSync` → style/template/javascript/ssr transformers.
Rust facade dispatches; each crate owns its pipeline. No long-lived JS shim.

## Gates
| Pipeline | Package port gate | Phase 0 migration harness |
|---|---|---|
| style | 69 fixtures byte-identical expected.js + error.json (version comment normalized) | eval stylesheet(token,host,dir) / inject + scoping tokens |
| template | expand later; prefer behavioral DOM/render goldens | CSR render observables |
| javascript | new goldens; not Babel bytes | runtime under engine + option matrix |
| SSR | HTML/runtime vs ssr-runtime | SSR HTML + hydrate flags |

Style pick: cssparser + LWC lossless tree; lightningcss later minify/oracle only.

## Order
1. Harness seam + style 69/69
2. Template (parse5-rust)
3. SSR (depends on template/JS IR — define IR in lwc-shared when needed; none locked yet)
4. JS (oxc preferred default unless survey proves swc; lock after JS bot ranking)
5. Delete JS @lwc/compiler packages

## Shared IR
Not locked yet. SSR must not invent a second template AST. Template bot proposes IR in `lwc-shared`; SSR consumes; Migrate ratifies. Until then: SSR surveys JS ssr-compiler only.

## Bot ownership
- PostCSS Rust → lwc-style
- LWC Template Rust → lwc-template
- LWC JS Rust → lwc-js
- LWC SSR Rust → lwc-ssr
- LWC Harness Cutover → harness + cutover checklist + facade tests
- LWC Migrate → program, contracts, conflicts

## JS path locks (2026-11-01)
- Parser/transformer: **oxc** (swc only if oxc fails errorRecovery or private-method sandwich).
- Keep private-method sandwich pass order around class-properties.
- New goldens: normalized codegen OK (not Babel bytes).
- LWS: fail-closed behind flag until unforgeables ported; don’t block core phases.
- Detail: `/workspace/notes/lwc-rust-migrate/js-architecture.md`

## IR location (2026-11-01)
- Shared template IR types live in `crates/lwc-shared` (Root, Element, … expressions, SourceLocation).
- `lwc-template` parses HTML → shared IR and does CSR codegen; may re-export for convenience.
- `lwc-ssr` consumes shared IR only — no second AST.
- Facade: `.html` + `target_ssr` → `lwc_ssr` (compileTemplateForSSR); `.html` without → `lwc_template`; `.js/.ts` + `target_ssr` → `lwc_ssr` component path.

## SSR locks (2026-11-01)
1. Sync SSR mode only for first Full-Rust cut; drop async/asyncYield + transmogrify until needed.
2. Template IR: stable types in `lwc-shared` with shape parity to TS IR; Template moves interim ast there.
3. `engine-server`: delete in Full-Rust break (with its fixtures); do not port.
4. Source maps: not required for SSR (`map` undefined / empty OK).
5. `ssr-runtime` stays JS for now; freeze ABI to its public `index.ts` export list.
6. Compiled-JS snapshots skipped; HTML/runtime goldens only.
