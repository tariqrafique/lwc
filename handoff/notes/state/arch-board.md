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

## Standing rules (still apply to all work; see ARCHITECTURE.md)
- No pass re-scans emitted JS. Complex-mode codegen goes only through the AB-D0004 typed oxc AST, so don't extend the hand member_chain.
- Whitespace-only parity is not a goal (D-0196). D-0224 reproduces astring bugs. D-0227 OWNERS is the single ownership source.
- Per-node codegen facts come from the node or a typed parameter, never a Gen field (AB-L0058). slot_parent stays a typed parameter on oxc (AB-L0057).
- One equivalence authority (D-0219): exact bytes, or a semdiff verdict in {STATIC_EQUAL, REWRITE_EQUAL}.
- Cargo.lock parse5 source is a git rev that exists (D-0238/D-0273). All cargo runs `--locked`.

## Notes
- Board files: arch-board/{reviews.tsv, ARCHITECTURE.md, COVERAGE.tsv, proposals/}. Commit with `bin/notes-commit arch-board`.
- Routines: none.
