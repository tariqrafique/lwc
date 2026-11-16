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

