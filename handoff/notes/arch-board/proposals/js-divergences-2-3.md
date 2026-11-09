# js divergences 2–3: AB-L0068 / AB-L0063 round 2 (2026-11-09 00:20 PST)

Both rebased onto main **dbbb940**. Local gates, PROVISIONAL. lwc-js only on div 3; div 2 also pins one D-0248 case in napi `errors.rs` (JS-owned per D-0227/D-0229). `bot/js` (898e7ca) untouched.

| branch | head | base | pid (`scripts/review-patch-id.sh`) |
|---|---|---|---|
| `bot/js-div3` | **0dd98df** | dbbb940 | **e817872d342eb044136c18807d63acf489763497** |
| `bot/js-div2` | **e1035f5** | dbbb940 | **4a6118f02f1d34e69f9430e76fee6b2cf175c0a8** |

## Div 3: `bot/js-div3` 0dd98df, pid e817872d (AB-L0068)

`git range-diff a051a35..d9bf476 main..bot/js-div3`: commit 1 (`337b306` = `36ddcab`) is `=` as `3094d50`. Only commit 2 changed.

`Origin::map` no longer `debug_assert`s then returns `src.len()` or `0` (an in-bounds wrong position in release). After the piece walk, an end at 0 returns 0; a start at the output end `assert!`s and returns `src.len()`; anything else panics in every profile (same idea as lib.rs `expect`). The `should_panic` test has no `#[cfg(debug_assertions)]`; `cargo test --release -p lwc-js --lib origin` runs it (3/3).

## Div 2: `bot/js-div2` e1035f5, pid 4a6118f0 (AB-L0063 r2; D-0245, D-0248)

1. `first_reserved_ident` and `STRICT_RESERVED` deleted. `parse_source` (not recovering) runs `SemanticBuilder::with_check_syntax_error(true)` once after a clean parse. The 8 inputs that compiled on main and that 9.5.3 rejects with LWC1007 each have a test (`closes_*` in `tests/semantic_errors.rs`) asserting LWC1007, `SyntaxError: LWC1007: foo.js: ` and filename `foo.js`, plus the position (shipped's, or our D-0248 pin). Live 9.5.3 values are in `tests/fixtures/semantic-errors-shipped.json` (62 inputs × 2 modes; regenerator `proofs/js-divergences/dump-semantic-fixture.cjs`). No new errors on the 78 ts-parse fixtures (5 reserved-word-before-parse-error cases moved to `LOCATION_DIVERGENCES`) or on the compiler fixture board (js 174, js-api 1239, same as main).
2. Recovery mode leaves the semantic check off. 14 inputs where 9.5.3 still throws later are pinned `ACCEPTED_DIVERGENCES` under the placeholder **`D-PENDING-recovery-semantic`** (Migrate D-id owed):

   | id | src | 9.5.3 (recovery) |
   |---|---|---|
   | `dup-let-rec` | `let a; let a;` | TypeError LWC1007 Duplicate declaration "a" (location null) |
   | `dup-let-var-rec` | `let a; var a;` | same shape, "a" |
   | `dup-var-let-rec` | `var a; let a;` | same |
   | `dup-const-let-rec` | `const a = 1; let a;` | same |
   | `dup-class-let-rec` | `class A {} let A;` | Duplicate declaration "A" |
   | `dup-fn-let-rec` | `function f() {} let f;` | Duplicate declaration "f" |
   | `dup-let-fn-rec` | `let f; function f() {}` | same |
   | `dup-import-let-rec` | `import a from 'x'; let a;` | Duplicate declaration "a" |
   | `dup-block-let-rec` | `{ let a; let a; }` | same |
   | `dup-param-let-rec` | `function f(a) { let a; }` | same |
   | `dup-catch-let-rec` | `try {} catch (e) { let e; }` | Duplicate declaration "e" |
   | `lwc-comp-dup-rec` | LWC component wrapping `let a; let a;` | Duplicate declaration "a" |
   | `priv-dup-rec` | `class A { #x; #x; }` | SyntaxError Duplicate private field (location null) |
   | `delete-private-rec` | `class A { #x; m() { delete this.#x; } }` | Error "Internal Babel error: deleting private element" (location null) |

   We compile all 14 in recovery mode (and reject them in normal mode). Never classified by message text: each row is a fixture id. Ten more recovery inputs where oxc's *parser* rejects what Babel `errorRecovery` recovers are listed as **D-0245** (`dup-export`, `dup-default-export`, `new-target-top`, `return-top`, `await-top-id`, `getter-param`, `setter-noparam`, `abstract-decl`, `abstract-default`, `abstract-default-named`, each `-rec`). Two of those (`abstract-default{,-named}-rec`) additionally trip an internal re-parse after codegen because oxc left `abstract` in the printed JS; still a D-0245-shaped outcome divergence (we throw / 9.5.3 compiles or throws a different SyntaxError), not keyed on text.
3. `earlier_reserved_ident` deleted. Its 5 ts-parse cases (`ts-interface`, `ts-syntax`, `interface.ts`, `interface.tsx`, `interface.jsx`) are in `LOCATION_DIVERGENCES`. LWC1007 location parity on the 78-case fixture: **43/73** (was 48/73; 30 pinned, 2 non-1007 LWC1198 also match location). `locparity.cjs` vs napi `d6d78ec87e22`: outcome 77/78 (the miss is D-0245 `interface.ts-rec`); code 75/75; 1007 class+filename 73/73; location 45/75 both-throw = 43/73 LWC1007 + 2/2 LWC1198; message text 3/75. Arch probes: location 3/6, text 0/6 (unchanged).
4. The abstract guard is `default_export_abstract_class` only (`export default abstract class …`, the one form oxc's JS parser accepts). It runs only when not recovering. Message is LWC-owned: `'abstract' classes are TypeScript syntax; LWC compiles JavaScript only.` Class remains `SyntaxError`, facade code LWC1007, position at the end of `abstract` (1:23), matching 9.5.3's Missing-semicolon column.
5. `tests/no_oxc_message_match.rs` deleted. `tests/oxc_diagnostic_boundary.rs` is a token gate over every `.rs` under `crates/lwc-js/src` (recursive, comments/literals stripped, no stop at `#[cfg(test)]`). Argument: an `OxcDiagnostic` value can be used without naming the type, so naming it is not enough. Values come only from oxc's producers (`Parser::parse` / `parse_expression`, `SemanticBuilder::build`, `Transformer::build*`); other public oxc fns that return one live in private modules. The gate therefore: (R1) `OxcDiagnostic` / `Diagnostics` / `LabeledSpan` / `oxc::diagnostics` / `oxc_diagnostics` / `miette` named only inside `parse.rs::first_error`; (R2) those producers (and `Compiler`, `CompilerInterface`, `regular_expression`, crate names) named only in parse.rs, their `*Return` types nowhere; (R3) `.diagnostics` read only as a `first_error(..)` argument, accessors (`errors`, `warnings`, `into_vec`, `has_errors`, …) only inside `first_error`; (R4) parse.rs exports only `pub fn`s whose return types are on an allowlist (`SourceType`, `Result<Program, CompileError>`, `Scoping`, `Semantic`, `Result<(), CompileError>`), so no diagnostic value can leave it; (R5) the boundary pair `first_error` / `syntax_error` uses only an allowlisted identifier set and no `==`/`!=`/`&&`/`||` (the text is formatted, never inspected), and the rest of parse.rs never names `message`; (R6) no `include!` or `#[path]`. `lower.rs`, `unforgeables.rs` and `decorators.rs` go through `parse::{parse_with, scoping, semantic_with_nodes, transform}`. Residual outside the gate: after conversion the text lives in `CompileError.message` (a `String`); comparing that elsewhere is not an oxc diagnostic value.

   Planted negatives, every one failed the gate (baseline clean):
   - Arch's 7 shapes (`matches!(d.message.as_ref(),…)`, `.message().starts_with`, alias `m.contains`, multi-line `.contains`, `.to_lowercase().contains`, `format!(…d.message).contains`, `d.to_string().contains`) at 4 sites (inside `first_error`; elsewhere in parse.rs after reading `.diagnostics` directly; `lower.rs` running its own `Parser`; `decorators.rs` running `SemanticBuilder`) = 28 plants.
   - After an early `#[cfg(test)]` in parse.rs and in lib.rs (appended module names `OxcDiagnostic` and compares `.message`) = 2 plants.
   - 10 bypasses that never name the type in the "useful" way: parse.rs leaking `impl Iterator`, returning `ParserReturn`, exporting a `struct`, filtering the converted `CompileError.message`; `use oxc::diagnostics as dx`; glob-import + `Transformer`; `include!`; `#[path]`; the boundary fn calling `.len()` on the text; a second `fn first_error` in lib.rs.
6. D-0245 golden (`d0245_recovery_mode_ts_interface_is_lwc1007_not_ok`): asserts code 1007, filename `foo.ts`, `(line, column, index) = (2, 9, 54)`, and the `SyntaxError: LWC1007: foo.ts: ` prefix. It no longer pins oxc's full message text. napi `errors.rs` `js-parse-1007` (`let = ;`) is pinned the same way at (1, 6, 6) (D-0248; 9.5.3 is (1, 0, 0) "Unexpected reserved word 'let'.").

### Gates vs merge-base dbbb940 (2026-11-09 00:15–00:20 PST, local, PROVISIONAL)
napi via `scripts/install-napi.sh`: main `85409fe563ed`, div3 `5a51794b3f52`, div2 `d6d78ec87e22`. Oracle b9e8007 clean.
| gate | main dbbb940 | bot/js-div3 0dd98df | bot/js-div2 e1035f5 |
|---|---|---|---|
| cargo check --workspace --all-targets | green | green | green |
| cargo clippy --workspace --all-targets | exit 0, 0 errors | same | same |
| cargo test --workspace | 462 pass / 0 fail | 466 / 0 | 481 / 0 |
| lwc-style fixtures | 136/136 | 136/136 | 136/136 |
| style-diff | 3150 jobs, 0 fail | same | same |
| js | 174/174 | 174/174 | 174/174 |
| js-options | 302 | 302 | 302 |
| js-api | 1239 | 1239 | 1239 |
| template (diag) | 568/568 | 568/568 | 568/568 |
| style | 70/70 | 70/70 | 70/70 |
| style-api | 829/829 | 829/829 | 829/829 |
| ssr (diag) | 388/388 | 388/388 | 388/388 |
| csr-render | 390/390 | 390/390 | 390/390 |
| csr-render-template | 567/570 | 567/570 (same 3) | 567/570 (same 3) |
| shim gate | untagged 0, scanners 9=9 | same | same |
| option gate | 61/61 | 61/61 | 61/61 |
| golden-io-guard | ok | ok | ok |
| lint-shared-tables | ok 23/23 | ok 23/23 | ok 23/23 |

`cargo clippy --workspace --all-targets -- -D warnings` is not green on main either (pre-existing warnings in lwc-shared/lwc-js/napi). Clippy on the changed files introduced no errors.

---
# js divergences 2–3: resubmission after AB-L0063 (2026-11-08 23:50 PST)

The two are now separate branches, each based on main **a051a35**. Both rebase cleanly onto main d46d6de (lwc-template only) with the SAME pids (checked on scratch branches). lwc-js only.

| branch | head | commits | pid (`scripts/review-patch-id.sh`) |
|---|---|---|---|
| `bot/js-div3` | **d9bf476** | 337b306 (= 36ddcab, as approved in shape; own pid d3849106 confirmed) + d9bf476 (nit) | **5a945ec2736daea16ca7ad91d38e6490ad7fde54** |
| `bot/js-div2` | **84702f6** | 84702f6 (one commit, directly on main, independent of div 3) | **aa3eb7d8d201db842fad0530151f70205e85ec51** |

`bot/js` (898e7ca, held by /tmp/abjs) is untouched and superseded by these two.

## Div 3: `bot/js-div3` d9bf476, pid 5a945ec2
- **337b306** is the cherry-pick of 36ddcab. Alone over a051a35 its pid is `d38491068dff45e9cee53f2084124e4716070b30`, Arch's d3849106.
- **d9bf476** fixes the nit (AB-L0063) in `Origin::map` with an explicit mapping plus a `debug_assert`; no error type is added.
  - `edits::apply_mapped` tiles the input: piece *i* covers input `[in_start, pieces[i+1].in_start)`, and the last piece ends at `src.len()`. So a replaced run's end is the next piece's `in_start`, or `src.len()` when the run is last. That is the real meaning, not a guess.
  - The old `unwrap_or(p.in_start)` returned the run's own start for a trailing run, a zero-length span. That was a wrong mapping, not a harmless fallback.
  - An offset inside no piece is legitimate only at a span boundary: a start at the end of the output maps to `src.len()`, and an end at 0 maps to 0.
  - Any other offset is a span outside the renamed source, an internal bug, so it `debug_assert`s.
  - Why not an error type: every caller maps spans of the AST it just parsed from this same renamed source, so a bad offset can't come from user input. An `Err` would thread a never-taken path through decorators.rs and lib.rs.
  - Unit tests in origin.rs: mid-run, trailing run (the case the old fallback got wrong), start at EOF, end at 0, and `#[should_panic]` past the output.
- **Gates, local and provisional:**
  - `cargo check --workspace --all-targets` green at both commits;
  - lwc-js tests 76 pass / 0 fail;
  - js 174/174 and js-api 1239 on both main a051a35 (napi 09ba8afae23c) and the branch (napi b11ae3a76d3b);
  - shim gate untagged 0, option gate 61/61.
- **Div-3 oracle diff (t3, t3b, class-features-shipped.json; 69 inputs):** branch **65/69**, main 7/69. Same 4 out-of-scope misses as before: three item-4 `*-off` cases and `rename-before-cls-deco`.
- Why and upstream cites: unchanged; see "Why" → div 3 below.

## Div 2: `bot/js-div2` 84702f6, pid aa3eb7d8 (cites D-0245 and D-0248)
Based directly on main, not stacked on div 3: the two touch disjoint code (parse.rs/unforgeables.rs vs lib.rs/decorators.rs/origin.rs), and an independent pid can be reviewed and landed in either order.
- **Kept:** the plain-JS source type through the single `parse::source_type()` (parse.rs, unforgeables.rs), and the D-0245 golden.
- **Deleted from parse.rs:**
  - `babel_error` and `enum Babel` (the oxc-message → Babel-message table);
  - `matching_paren`, `innermost_open`, `ends_with("export")`, `next_tok`, `word_end`, `find_word`, `word_at`;
  - the dead `let _ = m`;
  - `missing_plugin_error`, which only the table reached (Babel prose too);
  - the `unwrap_or((0,0))` position.
- **New shape (D-0248, restates D-0155):**
  - `parse()` takes oxc's first error. Its message text goes through unchanged, and its FIRST label offset becomes the 1-based line / 0-based column, with a UTF-16 index via `LineIndex`, in the existing `parse_error`: `SyntaxError` class, `<file>: <text> (<l>:<c>)` + code frame, normalized by the facade to LWC1007.
  - No label → `CompileError::new("internal: oxc parse error without a label: …")`, not position 0.
  - A `debug_assert` checks that the label lies within the source.
- **Not deleted, flagged for Arch** (AST-based, not keyed on oxc text, and not in the delete list):
  - `abstract_class` (oxc accepts `abstract class` in JS; Babel rejects it as `abstract` + Missing semicolon);
  - main's pre-existing `first_reserved_ident` / `earlier_reserved_ident` (strict reserved words).
  - Both still emit Babel's text for errors oxc doesn't raise at all.
- **Tests:**
  - `tests/ts_parse.rs` compares outcome, code, the `SyntaxError: LWC1007: <file>: ` prefix and `{line,column,index}`. For LWC1007 it never compares the text: `const OXC_MESSAGE_DIVERGENCE = "D-0248"`, one line. Non-syntax errors (2 LWC1198 cases) still compare the whole message.
  - `ACCEPTED_DIVERGENCES = [("interface.ts-rec","D-0245")]`.
  - `LOCATION_DIVERGENCES` holds 25 cases, each with OUR pinned position. Each must exist in the fixture and still differ from shipped, and the parity count is asserted.
  - Orphan proof: `divergence_lists_reject_orphan_ids` shows `("no-such-case","D-9999")` is reported. Mutation checks:
    - adding it to `ACCEPTED_DIVERGENCES` fails `ts_and_jsx_parse_failures_match_shipped` ("ACCEPTED_DIVERGENCES orphan");
    - dropping a `LOCATION_DIVERGENCES` row fails with that case's location.
  - The D-0245 golden now pins oxc's text: `Expected a semicolon or an implicit semicolon after a statement, but found none (2:9)`, location {2, 9, 54}, code 1007, single error. It still asserts 9.5.3 = `ok`.
- **Grep gate `tests/no_oxc_message_match.rs`:**
  - Walks lwc-js/src recursively (non-test code). It bans `.message` / `msg` / `message` with `==`, `!=`, `starts_with`, `ends_with`, `contains`, `strip_prefix`, `strip_suffix`, `find`, `matches`, the comparison on either side, and `match … message … {`.
  - Negative check: 10 lines from the deleted table must hit, and 4 pass-through lines must not. Dropping the old b7fdd63 parse.rs into src/ makes the gate fail on all 12 table comparisons, which I checked.
- **Location parity (measured, not faked; live 9.5.3 vs napi f31921e40977):**
  - fixture = proofs t2+tsx+tsx2+tsx3, 78 cases: outcome 77/78 (D-0245), code 75/75, class+code+filename 73/73;
  - location {line,column,index}: **48/73** LWC1007 cases. One of the 48 is a class-features case whose location is null on both sides. Of the 72 with a location, the line is equal in all 72; the 25 misses are column/index;
  - message text: 7/73;
  - Arch probes: location **3/6**, text 0/6.
  - Details and examples: `proofs/js-divergences/D0248.md` (notes 5e215a8), `arch-probes.json`, `locparity.cjs`.
- **Nit:** `dump-ts-fixture.cjs` writes to `$LWC_JS_CRATE/tests/fixtures`. It fails loudly when the variable is unset or isn't an lwc-js crate. Regenerating gives a byte-identical fixture.

### Gates: main a051a35 vs `bot/js-div2` 84702f6 (2026-11-08 23:36–23:40 PST, local, PROVISIONAL)
napi: main 09ba8afae23c, branch f31921e40977, both via install-napi.sh. Oracle b9e8007 clean.
| gate | main | branch |
|---|---|---|
| cargo check --workspace --all-targets | green | green |
| cargo test --workspace | 449 pass / 0 fail | 454 pass / 0 fail (+ts_parse 3, +no_oxc_message_match 2) |
| lwc-style fixtures | 136/136 | 136/136 |
| style-diff | 3150 jobs, 0 fail | same |
| js | 174/174 | 174/174 |
| js-options | 302 | 302 |
| js-api | 1239 | 1239 |
| template (diag) | 568/568 | 568/568 |
| style | 70/70 | 70/70 |
| style-api | 829/829 | 829/829 |
| ssr (diag) | 388/388 | 388/388 |
| csr-render | 390/390 | 390/390 |
| csr-render-template | 567/570 | 567/570 (same 3) |
| shim gate | untagged 0, scanners 9=9 | same |
| option gate | 61/61 | 61/61 |
| golden-io-guard | ok | ok |
| lint-shared-tables | ok 23/23 | ok 23/23 |

---
# History (superseded submissions below)

# js divergences 2–3: lwc-js parses every script as plain JS; class-features code frames from the original source

**Current head (2026-11-08 23:25 PST): branch `bot/js-d0245` @ 36ddcab on main e9d5199**, 2 commits (b7fdd63 div 2 + D-0245, 36ddcab div 3), pid **1a901cd08c9325e30ad9fb234d90d1737c246c4f** (`scripts/review-patch-id.sh`, merge-base e9d5199). This supersedes the pid 4c107e0f submission below: same change, rebased, plus the D-0245 golden (see "D-0245" section). `bot/js` itself still points to 898e7ca (= pid 4c107e0f on e9d5199) because it is checked out in `/tmp/abjs`; it moves to 36ddcab once that worktree is released.

Previous: `bot/js` @ 0ec91ed on main 6016fa4, 2 commits, pid 4c107e0f4d8e71d0b16fa4457873a467b559b477.
Requested trailer at land: `Arch-Review: <id>`. Crates touched: `lwc-js` only (src + tests). No lwc-shared, lwc-compiler, napi, harness, board or other-bot files.
Written 2026-11-08 23:18 PST by js (LWC JS Style). All numbers are local and PROVISIONAL; official scores come from the published board.

| commit | what |
|---|---|
| 7670d82 | **div 2**: `parse.rs` always uses `SourceType::mjs()` (no TS/JSX from the extension). oxc JS-mode diagnostics are translated to Babel's message and position by class (`Missing semicolon.`, `Unexpected token[, expected ".."]`, `Unexpected reserved word 'enum'.`, `abstract class` → Missing semicolon, missing-plugin errors for jsx/flow). `unforgeables.rs` re-parses with the same source type. Pinned by `tests/ts_parse.rs` + `fixtures/ts-parse-shipped.json` (77 oracle cases). |
| 0ec91ed | **div 3**: `class_features_error` (decorators.rs) framed the renamed source (`__lwc_component_class_internal_private_m`). `lwc_js::Origin {src, pieces}` (new `lwc-js/src/origin.rs`) carries the pre-rename source and the forward-rename pieces through `transforms::run` → `decorators::run`, so frame offsets map back to the original `#m`. lib.rs `unrename_offset` becomes `Origin::map`. Pinned by `tests/class_features_frames.rs` + `fixtures/class-features-shipped.json` (29 oracle cases, enablePrivateMethods, normal + recovery). |

## Why (upstream, /workspace/lwc @ b9e8007)
- **div 2.** `@lwc/compiler/src/transformers/javascript.ts:72-77,93-107`: `babel.transformSync` with `babelrc:false, configFile:false`, plugins = private-method transform, lwc class plugin, class-properties, reverse transform; `parserOpts` sets only `errorRecovery`. `@lwc/babel-plugin-component/src/index.ts:40-43` `manipulateOptions` adds only the `classProperties` and `decorators` parser plugins. No typescript/jsx/flow plugin, and Babel never infers syntax from the filename, so `.ts/.tsx/.jsx` are plain JS: TS/JSX syntax is a Babel `SyntaxError` → LWC1007 (`javascript.ts:110-123` `normalizeToCompilerError`). JSX/flow syntax gets `@babel/parser`'s `missingPluginError` text. A TS-only anonymous `@api` class that main accepted or misreported now rejects like shipped (LWC1198 / LWC1007).
- **div 3.** `@babel/core@7.29.7 lib/transformation/file/file.js:159-176` `buildCodeFrameError` frames `this.code`, the ORIGINAL input. `babel-plugin-component/src/private-method-transform.ts:81-97` renames `#m` → `__lwc_component_class_internal_private_m` via `replaceWith`, and `utils.ts:168-174` `copyMethodMetadata` keeps the original `loc`. So `@babel/helper-create-class-features-plugin lib/features.js:114/117` (`Private methods in decorated classes…` / `Decorators are not enabled.…`) frame `#m` in the user's text.

## Rebase onto main (from bot/js 45e0859, base 48c44ae)
- Backups: `backup/js-45e0859` (original), `backup/js-806c7cd-on-c08fbf6` (first rebase, before main moved to 6016fa4).
- Nothing dropped: `git cherry main` showed both commits unlanded, and neither was on main in content.
- Conflicts were resolved in favor of main's error model (AB-R001 / slice 2):
  - **lib.rs:** kept main's `unrename_locations(JsError, ..)` / `unrename_diagnostics` (node_location + `LineIndex::source_location`, `JsError::{Thrown,LwcErrors}`); only the offset mapping is now `Origin::map`. The stale "Every transform error is `CompileError::lwc_at`" comment now says `generate_at`.
  - **decorators.rs:** kept main's `CompileError::generate_at(info, args, ..)` error closure and added the `origin` parameter.
  - **transforms/mod.rs:** kept main's `Result<_, JsError>` and added `origin`.
- **`Origin` placement.** Originally `Origin` was added to `lwc-shared/src/edits.rs`. That file was moved there under SSR's AB-D0001 and isn't clearly in my lane, so `Origin` now lives in `lwc-js/src/origin.rs` and edits.rs is untouched.
- **Semantic fixes needed on the new base, folded into 7670d82:**
  - `missing_plugin_error` built a pre-R001 `SourceLocation{start,end}`. It now sets Babel's Position, `DiagnosticLocation{line, column, index}` with a UTF-16 index via `LineIndex::utf16_offset`, exactly like main's `parse_error`. The oracle pins it: every jsx case's location matches.
  - `ts_parse.rs` and `class_features_frames.rs` asserted on `CompileError` fields that `lwc_js::transform` no longer returns (it returns `JsError` now). They now go through `lwc_compiler::dispatch` + `tests/support::Thrown`, the JS facade, because the fixtures record @lwc/compiler output (code 1007/1198, `SyntaxError: LWC….` message). No fixture value changed.
- The rebase onto 6016fa4 was clean. The branch doesn't touch `lwc-shared/tests/error_literal_gate.rs`; main's version stands.
- `cargo check --workspace --all-targets` is green at both commits. `cargo test -p lwc-js` at 7670d82: 73 pass / 0 fail.

## Evidence: live 9.5.3 `transformSync` vs napi
Tool: `proofs/js-divergences/cmp-lwc.cjs <napi.node> <cases.json…>`. The oracle is `require('/workspace/lwc/packages/@lwc/compiler').transformSync`, compared field by field: code, message (cwd stripped), level, url, location {line,column,index,start,length}, and `errors[]` for aggregates. napi .nodes, each built with `scripts/install-napi.sh` (or `cargo build --release` for the commit-1-only build):
- main 6016fa4 = **6c41ae1c32b0**, identical to the live napi from the slice 2 landing;
- commit 1 only (7670d82) = scratch build;
- branch 0ec91ed = **b5616c4da469**.

| input set | main | after 7670d82 | branch 0ec91ed |
|---|---|---|---|
| div 2: proofs t2, tsx, tsx2, tsx3 + fixtures/ts-parse-shipped.json (155) | 13 | 154 | **154** |
| div 3: proofs t3, t3b + fixtures/class-features-shipped.json (69) | 7 | 7 | **65** |

- **div 2:** 142 inputs fixed and 0 that matched before now differ, except one:
  - **`tsx.json:interface.ts-rec`** (`interface A { x: number }` in a `.ts` file, recovery mode). 9.5.3 compiles it (`ok`), because Babel `errorRecovery` recovers "Missing semicolon." The branch throws LWC1007 `Missing semicolon. (2:9)`. Main matched it only by accident, because it parsed `.ts` as TypeScript.
  - This is **divergence (1)**, Babel recovery-mode leniency, now decided as **D-0245** (option B, permanent; section below).
- **div 3:** 58 inputs fixed (all class-features frames, normal + recovery), 0 regressed. Example `anon-api-pm`, `export default class { #m() {} @api x; }`:
  - 9.5.3 frames `> 1 | export default class { #m() {} @api x; }`;
  - main frames `> 1 | export default class { __lwc_component_class_internal_private_m() {} @api x; }`;
  - the branch now equals 9.5.3 byte for byte.
- **4 div 3 inputs still differ on main and on the branch, all out of scope:**
  - `t3:two-cls-off`, `pm-before-cls-off`, `cls-deco-long-off`: "Class private methods are not enabled." without its code frame. This is **item 4** (private-method-not-enabled per-class order and frame), parked in `ITEM4-WIP.md`.
  - `t3b:rename-before-cls-deco`, non-recovery: 9.5.3 throws `Private name #a is not defined. (3:19)` (LWC1007) before any class-features error; we throw LWC1198 "Decorators are not enabled". This is a separate ordering divergence, not a frame one, and wasn't claimed by div 3.

## Gates: main 6016fa4 vs branch 0ec91ed (2026-11-08 23:07–23:13 PST, local, PROVISIONAL)
Main was measured in the detached worktree `/workspace/wt/js-mainbase` @ 6016fa4, each side with its own install-napi.sh build. Oracle b9e8007 clean (board.sh guard).
| gate | main | branch |
|---|---|---|
| cargo check --workspace --all-targets | green | green (each of the 2 commits) |
| cargo test --workspace | 448 pass / 0 fail | 450 pass / 0 fail (+ts_parse, +class_features_frames) |
| lwc-style fixtures | 136/136 | 136/136 |
| style-diff | 3150 jobs, 0 fail, 100% MATCH | same |
| js | 174/174 | 174/174 |
| js-options | 302 | 302 |
| js-api | 1239 | 1239 |
| template (diag) | 568/568 | 568/568 |
| style | 70/70 | 70/70 |
| style-api | 829/829 | 829/829 |
| ssr (diag) | 388/388 | 388/388 |
| csr-render | 390/390 | 390/390 |
| csr-render-template | 567/570 | 567/570 (same 3 failures) |
| shim gate | untagged 0, scanners 9=9 | same |
| option gate | 61/61 mapped, 0 pending | same |
| golden-io-guard | ok | ok |
| lint-shared-tables | ok 23/23 | ok 23/23 |

The board rows don't cover TS/JSX parse failures or private-method frames, so none move. The crate fixtures and the live oracle diff above are the evidence.

## Open / not in this change
- Divergence 4 (item 4, private-method-not-enabled order + frame) and the `rename-before-cls-deco` ordering case aren't addressed here.

## D-0245: recovery-mode leniency is a permanent accepted divergence (folded into b7fdd63)
Migrate decided D-0245 (option B): no Babel `errorRecovery` emulation. Entry: `proofs/js-divergences/D0245.md` (notes 715fa5f).
- **Input:** `import { LightningElement, api } from 'lwc';\ninterface A { x: number }\nexport default class Foo extends LightningElement {}`, `foo.ts`, experimentalErrorRecoveryMode.
- **9.5.3:** compiles (`ok`).
- **Ours:** CompilerError code **1007**, message `SyntaxError: LWC1007: foo.ts: Missing semicolon. (2:9)` + Babel code frame, location **{line 2, column 9, index 54}** (no start/length), level 1, url `…#lwc1007`.
- **Golden:** `ts_parse.rs::d0245_recovery_mode_ts_interface_is_lwc1007_not_ok` pins our exact code/message/location and asserts the recorded 9.5.3 value for the same input is `ok`, so they differ.
- **Fixture:** the case is now IN `ts-parse-shipped.json` (78 cases; regenerated from the live /workspace/lwc oracle; the other 77 are byte-identical) with `accepted_divergence: "D-0245"`.
  - `ts_parse.rs` `ACCEPTED_DIVERGENCES = [("interface.ts-rec", "D-0245")]` must agree with the fixture field, and that case must still differ from shipped (a stale entry fails). Every other case must match.
  - Mutation check: emptying the list fails `ts_and_jsx_parse_failures_match_shipped`.
- **Generator:** `dump-ts-fixture.cjs` no longer skips recovery-mode `ok` cases. It records listed ones and throws on any unlisted one.
- **Commit:** fixup autosquashed into the div 2 commit (now b7fdd63), whose body cites D-0245.
- **Gates on 36ddcab vs main e9d5199** (2026-11-08 23:22-23:24 PST, local, PROVISIONAL):
  - `cargo check --workspace --all-targets` green at b7fdd63 and 36ddcab;
  - lwc-js crate tests: 74 pass / 0 fail at b7fdd63, 75 / 0 at 36ddcab;
  - js 174/174, js-options 302, js-api 1239 (main e9d5199: same); shim gate untagged 0, option gate 61/61, golden-io-guard ok;
  - napi branch ad48b662ccf8, main 09ba8afae23c;
  - oracle diff with the new napi: div 2 154/156 (the 2 diffs are `interface.ts-rec` in tsx.json and in the fixture, both D-0245), div 3 65/69 (unchanged).
