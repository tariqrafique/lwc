# state/arch-board.md
Updated: 2026-11-09 by LWC Arch Board. Mode: **milestone-only (D-0276)**. Idle until Migrate sends a milestone: a gate flip, an ownership or direction change, or cutover. Per-landing reviews are retired, and every open per-landing row is CLOSED in reviews.tsv under D-0276.

## Carry-forward conditions for LWC Worker (from closed reviews)
1. Land gate (AB-L0079, landed as 8f23d3c):
   - forbid `eval` in production-path scripts (land.sh, board.sh, official-board.sh, cargo-locked.sh, board-rows.sh, non-test main_run targets); an eval-wrapped `cargo_locked` redefinition still passes;
   - scan cargo spawns from .mjs/.py, and lock `tools/style-diff/run.mjs:99` and `harness/scripts/template-diag-diff.mjs:375`.
2. Official semantic-diff row (AB-L0078):
   - do not land the vendored `tools/semdiff/board/{registry.tsv,baseline.json}` (semdiff 5c271eb);
   - the row runs `semdiff run --gate strict` with no baseline (the official baseline is all-equivalent, 16795/0/0/0/0);
   - `--registry` comes from notes `lwc-rust-migrate/decisions.tsv` at the NOTES_COMMIT the board records;
   - the row asserts the manifest case count and corpus;
   - a ratchet baseline returns only for a corpus with accepted debt, as its own id+bucket+totals schema design.
3. SSR exec leg SD-2b (AB-D0007-r2, design approved; doc at notes 025d5ba):
   - one pair emitter, the harness `bundleFixture` path (like H-1 TDD_EMIT_PAIRS), with no semdiff-side bundler;
   - a manifest completeness check (fixtures with config.json == pairs + named exclusions, else exit 2); today it is 387 vs 359;
   - re-measure async and asyncYield on the emitter;
   - no exec-equal class; comments reuse SD-2a's decide path; engine-server cross-check deleted.
4. Errors (AB-L0080 follow-up):
   - `CompileError` has two position sources (diagnostics[].location and bare line/column for the raw CssSyntaxError path); make raw producer position one typed variant;
   - delete the dead `e.line/e.column` fallback in `harness/test/style{,-api}.behavior.test.ts`.
5. CI (AB-L0059 owed): SHA-pinned actions; multiset clippy baseline.
6. SD-2c (AB-D0006): build stays deferred (D-0270).
7. AB-D0004: the C1-C5 `cluster.mjs` check runs on the first official board after S4 (D-0271).
8. Nit (AB-L0058): `Gen.cur_cr` becomes a local.
9. AB-M0002-T1/S3 (post-land, D-0276; landed as table 6628018, slice 05aee74):
   (a) The unlocked scan must match napi/npx/npm the way it matches cargo. It currently misses `$(napi build)`, `env|exec|command napi build`, and echo-wrapped forms; the napi-pack locked exemption stays as it is.
   (b) Plant source must not dodge the scanner by splitting phrases (6628018: `na'pi' bu'ild'`, `'c'argo bu'ild'`). Move the plant bodies to data files with a path-exact exemption.
   (c) log_box_configs reads MAIN's working-tree table through main_data. Read it with git show ${base} like the stale check.

## Standing rules (still apply to all work; see ARCHITECTURE.md)
- No pass re-scans emitted JS. Complex-mode codegen goes only through the AB-D0004 typed oxc AST, so don't extend the hand member_chain.
- Whitespace-only parity is not a goal (D-0196). D-0224 reproduces astring bugs. D-0227 OWNERS is the single ownership source.
- Per-node codegen facts come from the node or a typed parameter, never a Gen field (AB-L0058). slot_parent stays a typed parameter on oxc (AB-L0057).
- One equivalence authority (D-0219): exact bytes, or a semdiff verdict in {STATIC_EQUAL, REWRITE_EQUAL}.
- Cargo.lock parse5 source is a git rev that exists (D-0238/D-0273). All cargo runs `--locked`.

## Notes
- Board files: arch-board/{reviews.tsv, ARCHITECTURE.md, COVERAGE.tsv, proposals/}. Commit with `bin/notes-commit arch-board`.
- Routines: none.

## AB-0008/AB-0009 closure review (2026-11-11; main 5087f1b+60d8098, board ca2ab905b502 VALID)
- AB-0009: CLOSED. Dispatch mirrors transformer.ts:96-122, including .mts, with Path::extension (matches extname on 18 inputs). LWC1005 comes from the generated table, and the x.mts and x.txt tests exist.
- AB-0008: OPEN. These gaps block closure:
  G1. napi still validates. Move the null-options LWC1023 (lib.rs:267-271), the apiVersion type check (de_api_version :243-257) and the ssrMode enum reject into options.rs. The ssrMode reject has no upstream equivalent, so either drop it or log it as a decision. napi should deserialize and call validate once; today it validates twice, at lib.rs:49.
  G2. There is still more than one key source. csrRender.ts:30-37 TRANSFORM_OPTION_KEYS must read rust-option-keys.json. `instrumentation` is in upstream options.ts but not in OPTION_KEYS, so add it or log the divergence.
  G3. The nested key test only checks wire ⊆ schema. It must be equality.
  G4. Delete the dead code: the discarded `let _text = …message()` (options.rs:250,:262; under D-0212, don't compute it) and the unused `_filename` (napi lib.rs:265).
- Carry-forward (non-blocking): the template-diag driver clamp (main.rs:25) should call lwc-shared get_api_version_from_f64. The mts oracle test accepts an extra trailing newline; normalize it explicitly and name it under D-0196. The enable_synthetic_element_internals and enable_private_methods unwrap_or(false) defaults differ from upstream's undefined; confirm there is no observable runtime difference.

## AB-0008 recheck (2026-11-15; main acc5297+84d8a6b, board 78636d164fd6 VALID: option-gate 61/61, js 174/174)
- G2 (key list derived, instrumentation listed), G3 (nested equality, with a plant) and G4 (_text, _filename) are fixed. AB-0008 stays OPEN on these gaps:
  G1a. ssrMode: options.rs:302-303 `from_js(s).unwrap_or(DEFAULT_SSR_MODE)` turns an unknown value into sync. Upstream passes the mode through: anything other than sync or async keeps the asyncYield shape (ssr-compiler compile-js/index.ts:436, compile-template/index.ts:167) and emits the raw string. Pick one of two shapes: (i) mirror upstream, where the IR carries the raw mode and the SSR codegen branches exactly as upstream does; or (ii) a migrate-authored decision that rejects unknown modes in options.rs with a coded error. Either way, add an SSR oracle test for `bogus` and `null`. The only test now, validate_upstream.rs:135, is CSR-only.
  G1b. Double validation: napi transform (:726-735) validates once, then lwc_compiler::dispatch validates again via prepare_compile_options (lib.rs:49). Give dispatch a validated type (NormalizedTransformOptions) so a second validate can't type-check. Fix the stale comment at lib.rs:47-48.
  G2b. instrumentation is listed but napi drops it silently (`instrumentation: None`, :304), and only the harness throws. D-0205 is lwc-bots-authored, not migrate. Either reject a non-null value in options.rs with a coded error, or get a migrate decision and implement the replay.
  G4b. options.rs:267 discards the stylesheet-warn latch result with `let _ =`, and nothing reads it. Surface the warning or delete the latch.
- Accepted: a non-number apiVersion becomes latest, as upstream (napi NaN to latest). Carry-forwards are unchanged: the driver clamp, the mts trailing newline, and the unwrap_or(false) defaults.


## AB-0008 recheck r2 (2026-11-15; main da7aa28..a492147, board b52f488b4470 VALID: option-gate 61/61, js 174/174, template-diag-napi mismatches 0)
- Fixed: G1a (unknown or non-string ssrMode becomes AsyncYield and the raw value is emitted; the JS and HTML groups match the oracle; the SSR test is validate_upstream.rs:170), G1b (dispatch takes &NormalizedTransformOptions at lib.rs:55-62; the napi path validates once), G4b (options.rs:303-306 adds DeprecationId::StylesheetConfig, and index.js calls console.warn; text and count match the oracle).
- a492147, the clippy::manual_non_exhaustive allow: LEGITIMATE. The private `_validated: ()` field limits construction to the validator inside the crate, which #[non_exhaustive] cannot do because it only blocks other crates. Carry-forward: switch it to #[expect(..., reason=...)].
- AB-0008 stays OPEN on:
  - G2b (instrumentation is unreachable from JS):
    - The napi transform takes `options: serde_json::Value` (napi lib.rs:314/342). A real `instrumentation:{log(){},incrementCounter(){}}` throws InvalidArg at the boundary, so the replay in napi lib.rs:320,333-341 and index.js replayAndStrip never runs.
    - Tests pass only on non-function shapes: napi lib.rs:821 `{}`, validate_upstream.rs:284 on the Rust API, loader_guard.rs:234 by string grep. backend.ts:125-128 still rejects instrumentation, so the board never exercises it.
    - Metric parity is partial: Rust has no emit site for `lwc-spread-directive`. Upstream emits it at template-compiler codegen/index.ts:605; Rust has only the enum variant at generated.rs:12.
    - `instrumentation:{}` throws LWC1052 in the oracle, but Rust accepts it.
    - D-0205 is lwc-bots-authored. No migrate decision covers the replay.
    - Required, one of:
      - (i) Reject a non-undefined instrumentation at the napi seam with a coded error, delete the recorder and replayAndStrip, and add a decision row.
      - (ii) Make a migrate decision, then: index.js strips the callbacks before the serde boundary and replays them; emit lwc-spread-directive at the template codegen site; give `{}` LWC1052 parity; add a node test that passes real callbacks and asserts the metric sequence against the oracle; and delete the backend.ts:125-128 reject so the board runs it.
  - G1b-r (the validated type is not sealed):
    - All fields of NormalizedTransformOptions are pub (options.rs:197-227), so a validated value can be mutated after validation. Required: private fields with read accessors, or pub(crate).
    - validated_compile_options (options.rs:416) and prepare_compile_options (:440) are exported with zero callers. Required: delete them.
  - G1a-r (raw JS text in a pub field): `ssr_mode_emit` is raw JS text in pub CompileOptions (options.rs:208, lwc-shared lib.rs:311) that lwc-ssr compile_js/mod.rs:916 splices in. Required: a typed value (for example `enum SsrModeLiteral { Mode(SsrMode), Raw(serde_json::Value) }`) that codegen renders. Fold in `ssrMode:{}` on .js, which throws in the oracle but emits `{}` in Rust: match the throw or log it as a divergence.
- Carry-forward (not blocking):
  - `stylesheetConfig:null` and `outputConfig.minify:null` warn in the oracle but not in Rust, because null becomes None on the wire.
  - dispatch_type.rs, loader_guard.rs:234 and validate_upstream.rs:419-460 are source-text tests. Replace them with behavior tests when touched.
  - serde_json moved to runtime dependencies of lwc-compiler (fine).

## AB-0008 recheck r3 (2026-11-15; main 4b53e18 = d52bdba+2cb9072, board ae879939adbf VALID: js 174/174, option-gate 61/61, template-diag-napi mismatches 0, template-options 205/205)
- G1b-r: CLOSED.
  - Fields are private with read-only accessors (options.rs:211-352), and the validator at options.rs:482 is the only constructor.
  - Three compile_fail doctests cover changing a field, building a struct literal, and passing CompileOptions to dispatch.
  - The dead exports are deleted, and dispatch_type.rs is now a behavior test.
- G1a-r: CLOSED.
  - SsrModeLiteral (lwc-shared lib.rs:221-245) is rendered at lwc-ssr compile_js/mod.rs:930-951, and ssr_mode_emit is gone.
  - The matrix matches the oracle, including the `{}`/`[]`/`{a:1}` error.
  - The clippy allow is gone because the lint no longer fires (better than #[expect]). The clippy baseline went from 135 to 134.
- G2b(ii): the design is approved. Callbacks are stripped in wrap.js and replayed. The spread emit at lwc-template codegen/mod.rs:660-667 mirrors upstream codegen/index.ts:602-605. The metric sequence matches the oracle on 10 inputs, and the odd shapes match on code and message. AB-0008 stays OPEN on:
  - G2b-1 (no gate):
    - The 58 new `js-options: inst|ssrmode` cases never run on the board.
    - The filters at board.sh:515-516 only match `lws|explicit|sourcemap|epm|dyn` and `api`. board.txt:17 shows 302 passed and 1297 skipped, against 1239 skipped before.
    - Required: the board runs them. Better, the filter is derived from the describe list, or skipped js-options cases fail the board, so a new describe can't be silently skipped.
  - G2b-2 (unlogged divergence):
    - With a callback that throws, the oracle throws a coded LWC1052/LWC1007 during the compile. Rust throws a plain uncoded error after the compile succeeds, because the replay runs after the compile.
    - Required: a migrate-authored divergence row with inputs, or replay errors wrapped in the oracle's coded error. Inputs for the row: spread.html, and dynamic import with `incrementCounter(){throw}`.
  - G2b-3 (second copy):
    - harness/src/napiWrap.js is a byte copy of crates/lwc-compiler-napi/wrap.js, kept equal by a test at loader_guard.rs:270.
    - Required: the harness imports the crate's wrap.js. Delete the copy and the equality test.
- Note: D-0009 and D-0205 are both lwc-bots-authored (b4a69e6, a1d3e3a). The migrate authority for (ii) is PLAN.md notes 44977cf.
- Carry-forward (not blocking):
  - The harness has two cwd normalizations (instrumentationObserve.ts:25 strips the cwd; compareError.ts:63 maps it to `<cwd>/`). Unify them.
  - `expect(INST_FIXTURES.length).toBe(53)` is a count lock.
  - `stylesheetConfig:null` and `outputConfig.minify:null` warn in the oracle but not in Rust; `outputConfig:null` throws in the oracle but succeeds in Rust.
  - lwc-compiler-napi errors.rs:14 and :28 have clippy warnings.
  - The JS syntax-error column differs: 1:49 in the oracle, 1:51 in Rust (unrelated to AB-0008).

## AB-0008 recheck r4 (2026-11-16; main 45d13cf, board e0252ee27381 VALID: js-options 438+1239 skipped (1677), skip-union 0 skipped in every row, template-options 309, template-diag-napi mismatches 0, clippy 134/134)
- r3-1 (board coverage): CLOSED.
  - The filter is a complement (board.sh:516), and the skip-union gate is at board.sh:580-593.
  - Plants: a new describe runs; `it.skip` makes BOARD FAILED; an unprefixed describe makes BOARD FAILED.
- r3-3 (wrapper copy): CLOSED. backend.ts:10-14 requires the crate's wrap.js, and the copy and its equality test are gone.
- r3-2 (synchronous coded throws): CLOSED for CSR.
  - Callbacks run through napi_call_function on the calling thread (napi instrumentation.rs:126-140), and the replay is deleted.
  - Against the oracle: 10/10 metric sequences, 20/20 throw cases and 40/40 odd-shape cases match.
  - D-0293 (fd4d191) is migrate-authored.
- The three divergences in notes c187146 are AB-0008 scope: all three were introduced by AB-0008 commits (d52bdba, 0d4c9bd). AB-0008 stays OPEN until r5 lands these:
  - D1, deprecation warn order:
    - The oracle warns at validation, before the compile (@lwc/compiler options.ts:185-219). wrap.js:9-21 prints only after native transform returns, so the counter fires first.
    - Required: emit deprecations at the validation point, in upstream order. The Rust validator calls a warn sink synchronously before dispatch, through the same calling-thread mechanism as instrumentation. Delete the post-return printDeprecations.
    - Test: the warn-vs-counter call order against the oracle.
  - D2, targetSSR rethrow:
    - lwc-template parser/mod.rs:1808-1826 applies the template-compiler's compile-level catch inside parse_template. Upstream parse() (template-compiler index.ts:33-38) has no catch; only compile() catches it (:64-79).
    - Required: move the catch to the compile entry only, so lwc-ssr (lib.rs:94-112) gets the raw thrown value, as ssr-compiler compile-template/index.ts:83-95 does.
    - Test: a throwing counter under targetSSR with lwc:render-mode, using Error, string, undefined and `{}`.
  - D3, lossy thrown value:
    - napi instrumentation.rs:173-201 reduces any thrown value to {name,message} (lwc-shared instrumentation mod.rs:41-51), so lwcCode, code and level are lost.
    - Required: carry the thrown value's lwcCode, code and level. Port getCodeFromError (@lwc/errors utils.ts:88-95) and convertErrorToDiagnostic (errors.ts:200-215) once into lwc-shared, used by both template_catch (:69-71) and babel_catch (:93-94).
    - Harness: add coded throwing shapes (lwcCode, code, `level:0`), and run the throwing matrix under targetSSR as well.
  - Subtract in r5 (dead paths):
    - Delete JsCallbacks::log and the end() rethrow (instrumentation.rs:77-94, :258-278), which can't be reached.
    - Delete the error-dropping `let _ =` increment_counter (:237-239); production code goes through try_ only.
    - InstrumentationRecorder is a public type that only tests use (lwc-shared instrumentation/mod.rs:123-147). Delete it, or make it cfg(test) in the test crate.
- Carry-forward (not blocking):
  - The LWC1007 message uses an absolute path in the oracle but `foo.js` in Rust.
  - `stylesheetConfig:null` and `outputConfig.minify:null` warn in the oracle but not in Rust.
  - `outputConfig:null` throws in the oracle but succeeds in Rust.
  - The D-0205 row (decisions.tsv:206) still has the replay wording. It's superseded by D-0293; mark it amended.
  - The harness still has two cwd normalizations.

## AB-0008 recheck r5 (2026-11-16; main 8ab3753 = 0152aab+48c00ba, board 475fc6bc0c9a VALID: js-options skip-union 1687 cases, 0 skipped in every row; template-options 406; template-diag-napi mismatches 0; clippy 133/133)
- AB-0008: CLOSED.
  - D1: the warn sink runs at validation (wrap.js:10-24, napi lib.rs:1021-1041, JsWarn instrumentation.rs:336-368, options.rs:402-428). printDeprecations is deleted, and 7/7 normal cases match the oracle on text, count and order.
  - D2: the only catch is at compile (lwc-template lib.rs:76-97; parse_template at parser/mod.rs:1802-1814 has no catch). SSR rethrows the identical value, and 125/125 SSR cases match the oracle. CSR still matches: 10/10 metric sequences, 20/20 throws, 40/40 odd shapes.
  - D3: lwcCode, code and level are carried in CaughtError (instrumentation.rs:178-235). get_code_from_error and convert_error_to_diagnostic exist once, in lwc-shared errors/mod.rs:305-360, and template_catch and babel_catch both use them. 100/100 CSR cases match the oracle.
  - Deletes: all verified by rg.
- Carry-forward #10 (follow-ups from the AB-0008 closure, not gates; land on the next touch of these files):
  - a) A `console.warn` that throws: the oracle aborts at validation, but Rust stores the error in JsWarn::warn (instrumentation.rs:353-362), compiles anyway (the counter fires), and rethrows at lib.rs:1036-1041. Fix: JsWarn returns Err and the validator propagates it, so dispatch never runs. Add an oracle row for a throwing warn. Correct the false doc comment at instrumentation.rs:333-335.
  - b) Delete the unreachable rethrow-without-value branch at napi lib.rs:1048-1053.
  - c) Null edges: `stylesheetConfig:null` (options.rs:410 checks is_some) and `outputConfig.minify:null` should warn as in the oracle, and `outputConfig:null` should throw as in the oracle. The wire model needs to tell an explicit null apart from an omitted field, as ssrMode already does.
  - d) Pre-existing, from aee371c (2026-11-01): lwc-template lib.rs:101-104 treats Fatal-level parse warnings as errors, but upstream index.ts:67-69 checks only Error.
  - e) A non-numeric `level` maps to Log (errors/mod.rs:327-331). Check it against upstream when touched.
  - f) From earlier rounds: the LWC1007 message uses an absolute path in the oracle but a relative one in Rust; the harness has two cwd normalizations; the D-0205 row should be marked as amended by D-0293.
