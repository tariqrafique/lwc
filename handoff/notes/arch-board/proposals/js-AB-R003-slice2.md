# AB-R003 slice 2/3: error surfacing owned by lwc-compiler, napi serialize-only

Branch `bot/js-slice2` @ 01b83c0, rebased on main 9c96ee4 (2026-11-08 22:54 PST). Not landed.
Patch-id (scripts/review-patch-id.sh, whole main..HEAD diff): `8ea94513ec121bcb4a8b2061983da184dcb52ea4`.
Supersedes 45ad87b / pid beb865a9 (REWORK, notes 530ec44 + e52cbc2).
Diff: 31 files, +1007 / -875. There is no `Arch-Review:` trailer yet. land.sh needs one, plus an APPROVE row with this pid,
and Cross-Lane trailers for Harness (crates/lwc-compiler) and Ship (napi lib.rs, serialize_only_gate.rs).

**Open before resubmit: Ship re-ack on lib.rs.** fd1ae14 is not byte-identical to the e0749a5 Ship OK'd (+4/-85).
It is +6/-86: item (3) changes the one `JsCompileError.filename` field type (the struct lives in lib.rs). Interdiff:

```diff
-    pub filename: Option<String>,
+    /// Same own-but-`undefined` shape as `location` (`Either::B`): shipped `generateCompilerError` with no origin.
+    pub filename: Option<Either<String, Undefined>>,
```

Oracle: /workspace/lwc @ b9e8007 (LWC 9.5.3), packages/@lwc/compiler/src/transformers.

## Commits (oldest first; `cargo check --workspace --all-targets` green at each, verified after rebase)

| sha | what |
|---|---|
| 02017c5 | lwc-style: `LWC_VERSION` comes from `lwc_shared::meta` (was `CARGO_PKG_VERSION`, "compiled with v0.1.0"). PENDING_VERSION_MAX 1→0. |
| 85e090c | napi: errors.rs, the one serializer of `TransformError` into `{ error }`. Not wired yet. |
| fd1ae14 | **napi lib.rs wiring for errors.rs (Ship review)**: lib.rs only, +6/-86 (e0749a5 + the `filename` field, see above). |
| db5cc3d | lwc-compiler facades normalize like upstream (style.ts:44-61, template.ts:72-90, javascript.ts:109-135, options.ts validateOptions); napi serialize-only; error_urls.rs, single_error.rs, aggregate.rs deleted; serialize_only_gate.rs (recursive, bans construction). |
| 781ca54 | lwc-js decorators.rs / private_methods.rs: `CompileError::generate_at(&DecoratorErrors::X, ..)`. error_literal_gate PENDING 2→0 (main already dropped the SSR entry). |
| 80b432c | lwc-js srcmap: uses `lwc_shared::location::LineIndex`. |
| 1e0c0fd | lwc-style serialize.rs: `lwc_shared::meta::version_comment()`. PENDING_VERSION_COMMENT_MAX 1→0. |
| 1269a67 | lwc-js LWC1121 via `generate_at(&LWCClassErrors::INVALID_DYNAMIC_IMPORT_SOURCE_STRICT, ..)`; `CompileError::lwc_at` deleted (byte offsets). |
| 01b83c0 | AB-X0013/AB-L0056 seam: crates/lwc-compiler/tests/seam_x0013.rs (facade) + napi errors.rs `lwc1121_seam_matches_live_oracle`, SSR and CSR, live 9.5.3 oracle. |

C5 ("lwc-js version via meta") needed no change: lwc-js has no local LWC_VERSION, and PENDING_VERSION was already empty for it.

## Where upstream throws (errors.rs header)
- **Single `CompilerError`**, from normalizeToCompilerError / CompilerError.from, at javascript.ts:121, template.ts:73, template.ts:85 and style.ts:60.
- **`CompilerAggregateError`** only at:
  - javascript.ts:129-133;
  - style.ts:47-58 (recovery mode);
  - template.ts:79-82 (recovery mode; the message is `""` there).
- **SSR errors** pass through as `Unnormalized`.
- **Byte-level spec:** tests/fixtures/{single-error,aggregate}-shipped.json.

## Allowlist ceilings
| gate | main 9c96ee4 | branch |
|---|---|---|
| error_literal_gate PENDING_MAX | 2 | 0 |
| PENDING_VERSION_MAX | 1 | 0 |
| PENDING_VERSION_COMMENT_MAX | 1 | 0 |
| lint-shared-tables CEILING | 23 (23/23) | 23 (23/23) |

## Gates: main 9c96ee4 vs branch 01b83c0 (2026-11-08 22:46-22:54 PST, local, PROVISIONAL)
Main measured in a detached worktree /workspace/wt/js-mainbase @ 9c96ee4 with its own install-napi.sh build (napi 5e40a5a01fb4);
branch napi 6c41ae1c32b0. Both boards: oracle b9e8007 clean.
| gate | main | branch |
|---|---|---|
| cargo test --workspace | 447 pass / 0 fail | 449 pass / 0 fail |
| cargo check --workspace --all-targets | green | green at each of the 9 commits |
| lwc-style fixtures | 136/136 | 136/136 |
| style-diff | 3150 jobs, 0 fail | 3150 jobs, 0 fail |
| js | 174 | 174 |
| js-options | 302 | 302 |
| js-api | 1239 | 1239 |
| template | 568 | 568 |
| template-options | 67 | 67 |
| style | 70 | 70 |
| style-api | 829 | 829 |
| ssr | 388 | 388 |
| csr-render | 389/390 | **390/390** |
| csr-render-template | 567/570 | 567/570 (same 3: expression-complex/valid/{object-expr,template-literal,template-literal-tagged}) |
| template-diag (crate) | 63224 / clean 63115 / 109 mism | identical report (every section but meta) |
| shim-gate | untagged 0, scanners 9=9 | same |
| option gate | 61/61 mapped, 0 pending | same |
| lint-shared-tables | ok 23/23 | ok 23/23 |

- **csr-render:** main's one failure is attribute-class/with-scoped-styles-only-in-child/dynamic, caused by the extra "stylesheet was compiled with v0.1.0" log. That log appears 0 times on the branch (1 time on main).
- **Test count:** 12 napi unit tests go with single_error.rs (3), aggregate.rs (8) and a lib.rs test (1). 14 are added: errors.rs 5 (incl. lwc1121_seam_matches_live_oracle), serialize_only_gate 3, lwc-compiler 3 unit (template_levels_match_shipped, options_error_is_generate_compiler_error, template_warning_reaches_compile_result), seam_x0013 2, resolve_matches_node_path_resolve 1. No lwc-compiler test was removed.

## Behavior changes (napi dump: 33656 runs, 8013 errors, 784 differ from main, all toward 9.5.3)
- **530 template recovery aggregate entries** now carry `filename`, as CompilerError.from does at template.ts:79-82. Nothing else changed in those rows.
- **250 style single errors** lose their own `name: "Error"`. The shipped ownKeys have no `name`.
- **4 LWC1121 CSR errors (3 fail-fast, 1 recovery)** in non-ASCII sources: `location.start` changes from 27 to 24, i.e. from bytes to UTF-16, matching upstream.

## Known gaps (not fixed, out of scope)
- **napi returns `{ error }` instead of throwing** (D-0210, Ship S3).
- **Stale doc comments in lib.rs** (Ship's) still name single_error.rs at lines 166 and 168.
- Closed by this rework: opts-1021 filename (3), SSR LWC1121 level/url (5), template Error-only filter (2).
- Noted, not changed (H1 forbids dispatch changes): an unknown extension (including upstream-accepted `.mts`) is
  `Unnormalized` "unsupported file extension", where transformer.ts throws `generateCompilerError(NO_AVAILABLE_TRANSFORMER)`.
  Same as main; Harness's dispatch.

## Rework (notes 530ec44, e52cbc2)
| item | evidence |
|---|---|
| (1) options error built in lwc-compiler | `lwc_compiler::options_error` (lib.rs, cites options.ts:206-211 `validateOutputConfig` → errors.ts:77-100 `generateCompilerError`); napi errors.rs is `result(serialize(lwc_compiler::options_error(e)))`. serialize_only_gate.rs bans in napi src: `level_url/error_url/Origin::/AGGREGATE_`, `CompilerError/CompilerAggregateError/UnnormalizedError {` literals, any `::ctor(` on those and `TransformError`, `TransformError::{Compiler,Aggregate,Unnormalized}(` construction (match/let patterns exempt), `normalize_to_compiler_error`, `impl From<..> for` them. `gate_patterns` pins 12 caught + 9 allowed lines. Unit test `options_error_is_generate_compiler_error`: code 1021, level Error, filename/location None, url from the table. Through the built .node: own keys `message,code,filename,location,level,url`, `filename` own-undefined; 9.5.3 gives the same plus `stack`. |
| (2) template facade Error-only | `template_diagnostics` = template.ts:76 `filter(level === Error)` (thrown; :78-82 aggregate in recovery, :84-85 single) and :90 `filter(level === Warning)` (returned). Fatal oracle: no 9.5.3 input reaches a Fatal/Log template diagnostic: every @lwc/errors info is `level: Error` (190) or `Warning` (16) (grep errors/src), and template-compiler never passes `useStrictErrorOverride` (only parser.ts:365/399/435 + index.ts:77, all default). So `template_levels_match_shipped` runs 9.5.3 `transformSync` LIVE in node with `@lwc/template-compiler` wrapped (Module._load) to append `{code: 9999, level}` diagnostics, 7 sets × 2 modes, and compares line for line with `template_diagnostics`. 9.5.3: [Fatal]/[Log] → no throw, warnings []; [Warning] → warnings [it]; [Error] → throws it (recovery: aggregate); [Fatal,Error] → throws the Error only; [Fatal,Warning] / [Log,Warning,Fatal] → warnings [the Warning]. Mutation check: keeping Fatal with Error fails the test (Fatal is thrown as level 1). Missing oracle panics. |
| (3) filename own-but-undefined | `JsCompileError.filename: Option<Either<String, Undefined>>` (lib.rs, needs Ship re-ack, see top); `compiler_error` serializes `Some(e.filename.map_or(Either::B(()), Either::A))`. The `id != "opts-1021"` skip is deleted; `rg 'id != "' crates/` = 0 hits; single_errors_match_shipped passes every fixture case. |
| (4) Ship ack on e0749a5 | received 11-02 00:00; re-ack needed for the +2/-1 interdiff above. |
| (5) no None level/url for SSR | single_error.rs deleted (`ls crates/lwc-compiler-napi/src` = errors.rs lib.rs); the gate bans `level_url|Origin::` in napi. `ssr_js_error` (lwc-compiler): any coded lwc-ssr compile-js error → `TransformError::Compiler`, level from the info's diagnostic, url `lwc_shared::errors::url_of(code)` (the table), as compile-js/errors.ts:21-40 `generateError` → `generateCompilerError`. Only codeless SSR errors are `Unnormalized` (no level/url), as upstream's plain `new Error(..)` (compile-js/index.ts:215/264, stylesheets.ts:53; compile-template/index.ts:111 for all SSR html). |
| (6) LWC1121 end to end | crates/lwc-compiler/tests/seam_x0013.rs (facade, what napi serializes field by field) and napi errors.rs `lwc1121_seam_matches_live_oracle` (to_compile_options → errors::compile → serialize → `JsCompileError`), SSR and CSR, vs 9.5.3 live. Also run through the built .node (table below). |
| Ship nit: recursive walk | `scan()` walks src with a stack (nested dirs); `nested_module_files_are_scanned` writes `errors/inner/deep.rs` with a banned line in a temp tree and asserts both hits are reported at that path. |
| H1 option semantics | No change to `to_compile_options`, options types, defaults, dispatch or `deny_unknown_fields` (lwc-shared options untouched; napi lib.rs diff is error wiring + the filename type). Option gate 61/61 both. Reject set: 2301 inputs (every `compiler.*` inventory field × 23 values, 5 unknown keys top-level and nested, 3 missing name/namespace) × js/html/css, main vs branch .node: 1929 rejected on main, 0 differ in accept/reject or error code. js-options 302, template-options 67 on both. |
| H2 existing tests | Every lwc-compiler test on main passes on the branch. Moved for the `TransformError` type only, none loosened: recovery aggregate (`name == "AggregateError"` → `TransformError::Aggregate` match; message and codes kept, filename per entry added); `e.name == None` → `TransformError::Compiler` match; `thrown_template_failure_is_lwc1008` (helper `template_transformer_error` deleted, now a real invalid customRendererConfig throw: message, code 1008, location None kept, url "" and filename added); `empty_html_is_1072_at_0_0` (`(e.line, e.column) == (0,0)` and `[d] = diagnostics` → `e.location` {0,0,0,0} and `e.code`; napi derives line/column from location); `.message` in two panic texts → `{e:?}`. |
| H3 filter in the template facade only | `template_diagnostics` is called only from `transform_template`; style/js/ssr paths do not filter. `template_warning_reaches_compile_result`: LWC1149 (`key` off for:each) reaches `CompileResult.warnings` as level Warning, both modes (9.5.3: `[{code:1149, level:2}]`). Template board 568 and template-diag report identical. |
| H4 oracle-sourced seam values | seam_x0013 and lwc1121_seam_matches_live_oracle spawn node on `$LWC_ORACLE` (default /workspace/lwc), assert HEAD starts with b9e8007 and node exits 0; no skip path. With `LWC_ORACLE=/nonexistent` the tests panic. No value is typed into either test. |

Seam values (9.5.3 transformSync == branch .node, `const s = 'é😀'; import(foo)`, strictSpecifier):
| path | code | level | url | location {line,column,start,length} | main 9c96ee4 .node |
|---|---|---|---|---|---|
| SSR | 1121 | 1 | https://lwc.dev/guide/error_codes#lwc1121 | {1,17,17,11} | level, url missing |
| CSR | 1121 | 1 | https://lwc.dev/guide/error_codes#lwc1007 | {1,24,24,3} | start 27 (bytes) |
