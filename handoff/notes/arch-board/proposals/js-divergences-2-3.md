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
