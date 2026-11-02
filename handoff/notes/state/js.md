# state/js.md
Updated: 2026-11-02 00:37 PST by js (LWC JS Rust). Read PLAN.md first; this file holds only this bot's working state.

## Current item
- AB-R003 + AB-0001 + AB-0010 + AB-R001 (DiagnosticLocation type): one error model in lwc-shared (typed CompilerError/CompilerAggregateError, normalize_to_compiler_error, single @lwc/errors error-info table + url, LWC_VERSION); lwc-compiler facades mirror style.ts:44-61 / template.ts:72-86 / javascript.ts:109-135; napi serialize-only; grep gates (napi: level_url|error_url|Origin::|AGGREGATE_; error-info literals outside lwc-shared). Status: slice 1 landed (48c44ae); slice 2 next.
- Slice order per PLAN @ faaa5e7: (1) SMALL lwc-shared-only: table type (module per upstream group), CompilerError/AggregateError enum, DiagnosticLocation + single offset->loc conversion, unframed ctor, meta::LWC_VERSION(_COMMENT), WHOLE 9.5.3 @lwc/errors table generated (sole writer, PLAN @ d44d60e), guard with shrink-only allowlist -> Arch review, land fast. (2) owners only redirect call sites to shared entries (diagnostics: messages.rs; ssr: literals). (3) mine: napi deletion + lwc-js migration + lwc-compiler facades.

## Owed REWORKs (ids)
- AB-R003: in progress, bot/js WIP.
- AB-0001: folded into AB-R003 change.
- AB-0010: folded into AB-R003 change (lwc-js half; lwc-style CARGO_PKG_VERSION removal too).
- AB-R001: DiagnosticLocation type, same change; postcss migrates after.
- AB-0011: delete tag_name.rs copy — not started.
- Divergence items from Migrate (21:30): (2) TS anon @api reject like shipped, (3) code frame from original source, (4) private-method check order across classes, (5) template recovery errors[] full. Commits 35da454, eb0dea7 on bot/js (unreviewed, not landed); (1) `a b` recovery leniency accepted -> decisions row owed. Paused behind AB-R003.
- SSR asks (21:58; SsrCompilerErrors in my slice 1 per d44d60e): #2 meta::LWC_VERSION(_COMMENT) + DecoratorErrors/INVALID_DYNAMIC_IMPORT_SOURCE_STRICT/RESERVED_IDENTIFIER_PREFIX + #3 unframed generateCompilerError ctor -> in AB-R003. #4 DISALLOWED/AMBIGUOUS_PROP_SET -> lwc_shared::html_attributes after AB-R003 (with AB-0011). #1 edits.rs move + #5 oxc workspace dep: OK'd to ssr to do themselves.
- customRendererConfig 1150/1151 pass-through in lwc-compiler: after template adds entries to shared table.

## Branch/worktree + last landed main commit
- Worktree: /workspace/wt/js. Slice 1 LANDED main 48c44ae (AB-L0020, pid 8ada1ed0) 22:20. Last landed main commit: 48c44ae. Next: slice 2 on bot/js, rebased on main 48c44ae (superseded 1230144 skipped); unlanded divergence commits a04e916 (div 2) and 45e0859 (div 3) are green and still need Arch review.
- Uncommitted: none.
- Last landing on main: frozen tree in e511537 (pre-git, no Arch-Review).

## Last trusted board
- Official src=dfa221a8867a commit=7dcfe44 published=2026-11-01 21:48 PST (PROVISIONAL per PLAN: AB-0003/AB-0007). Rows I move: js 174/174, js-eval 2/2, js-options 302/1541 + js-api 1239/1541, template-options 67/67.
- Provisional? yes (golden-gate hole).

## Dead ends (what was tried and why it failed, with evidence paths)
- Explicit `code: null` on aggregate: not shipped 9.5.3 (no own property). Evidence: notes/lwc-rust-migrate/proofs/aggregate-shape/.

## Open questions for Migrate
- (resolved 22:09) lwc-compiler facade error surfacing = js; options/dispatch = harness (PLAN @ faaa5e7).

## 2026-11-01 23:10 PST: role change (now LWC JS Style)
- Owns crates/lwc-js, crates/lwc-style, and the lwc-shared error model. PostCSS Rust is retired; its state is in state/postcss.md.
- Order: AB-R003 slice 2, then AB-R001 (style), then 13 style GAPs, then js coverage GAPs (10, confirmed by Migrate; after slice 2).
- Sequencing decision: AB-R001 is effectively done on bot/postcss (5 commits f6d8665..78390ae, base ee6ebb1) and overlaps slice 2 in napi/lib.rs, lwc-compiler, errors/mod.rs and lwc-js parse.rs. So I rebase it first as bot/js-r001 (executor running 23:10), land it, then do slice 2 on top.
- Owed to Arch (AB-L0025): lwc-style serialize.rs:211 inline `/*LWC compiler v...*/` -> lwc_shared::meta::version_comment() and cap exactly_one_lwc_version_comment to 0 in the next lwc-style landing (after SSR S0's version_comment lands).
- AB-0011 step (d) is MINE per D-0199: once Template moves DISALLOWED/AMBIGUOUS_PROP_SET and tag_name to lwc_shared, repoint lwc-js and delete decorators.rs sets + tag_name.rs. Ship deletes only JS packages and owns the cutover lint. Sequencing R001 then slice 2 approved (PLAN 7f9ab31).
- decisions.tsv: col 1 is `id` D-NNNN; still owe the row for divergence (1) leniency.
- 23:20 AB-R001 carried to bot/js-r001 @ 33d78aa (5 commits on main 6ad019e), pid 4834775006c714707eb8e5cf06e36af479d49f4b, all gates identical to main (provisional local). Review doc postcss-rust/arch-review-AB-R001.md. Arch review requested; Template, SSR and Ship notified before land. Needs trailer `Arch-Review: AB-R001` on top commit at land time.
- D-0200: Ship owns napi lib.rs (#[napi] exports). Slice 2 puts all error construction/surfacing in crates/lwc-compiler-napi/src/errors.rs (mine), shaped like upstream @lwc/compiler; Ship wires it.
- Template AB-L0022: js_ws/js_trim live only in lwc_shared::js (single_definition.rs gate); never re-add a local copy in lwc-style.
- 23:19 Template Rust: R001 clear (merge-tree clean). Conditions: one-time single-writer exception for lwc-template; route any further lwc-template edits through Template; Template owns AB-X0009 fallback sites and verifying 1072/1075/1144 {0,0,0,0} + elem_loc parent fallback after land.
- 23:24 AB-R001 LANDED main 1d7f8d5 (pid 48347750, Arch-Review AB-R001), live napi c4517afd624b. Last landed main commit: 1d7f8d5.
- Slice 2 in progress: branch bot/js-slice2 from 1d7f8d5 (executor). Includes AB-X0013 (LWC1121 seam test, non-ASCII prefix) and lwc-style version_comment() with cap 0. The napi lib.rs wiring is a separate commit for Ship review.
- bot/js (div 2 d72afe5-ish / div 3 45e0859) conflicts with R001 in lwc-js lib.rs. Rebase aborted; bot/js still on the old base. Redo after slice 2.
- 23:30 SSR S2 blocked on lwc_shared::html_attributes (DISALLOWED/AMBIGUOUS_PROP_SET for 1110/1109). Template's AB-0011 design owns that module. Proposed a tiny pre-slice (just those 2 items, Template's signatures, plus lwc-js repoint/delete) to Template; waiting on A/B. SSR told.
- 23:33 html_attributes: Template's a1 (AB-L0031) creates it with is_disallowed_prop/ambiguous_prop and lands tonight; no pre-slice from me. My AB-0011 (d) (lwc-js decorators.rs repoint + delete sets) waits for Template's (b), per Arch's no-table-work-between-a1-and-(b) condition. SSR told the API.
- 23:40 Ship asks via Migrate:
  - J1 (errors.rs throws a real Error, aggregate only where 9.5.3 throws one, per D-0210) is folded into slice 2.
  - J2 (INVALID_SOURCE 1006 and INVALID_ID 1027) has been in the table since slice 1; nothing to do.
  - J3 (lwc_shared InstrumentationEvent + DeprecationId with upstream texts, per D-0205/D-0212) is next after slice 2.
- csr-render 389/390: the last failure is lwc-style consts.rs CARGO_PKG_VERSION stamping 0.1.0. Slice 2's first commit fixes it (meta::LWC_VERSION); csr-render is added to the gates.
- 23:43 Template a1 landed main 9cc1c0a (AB-L0048): lwc_shared::html_attributes + lwc_shared::js::{js_ws,js_trim,js_default_sort_cmp}; lwc-style parser.rs now uses shared js_ws (NEL no longer whitespace in check_missed_semicolon, fidelity fix). Slice 2's style gates on the rebased branch confirm it. AB-0011(d) still waits for Template (b) or an Arch carve-out.
- 23:59 Slice 2 ready: bot/js-slice2 @ 45ad87b on main 9cc1c0a, pid beb865a9311754a86ef655f5ade237527ac4f9a6, 9 commits. Gates equal to or better than main (csr-render 390/390). Review doc: arch-board/proposals/js-AB-R003-slice2.md. Arch review requested. Ship asked to OK lib.rs commit e0749a5.
- Known gaps: (a) template facade keeps Error+Fatal, upstream keeps Error only (follow-up); (b) lwc-ssr LWC1121 lacks level/url (SSR's, reported); (c) opts-1021 filename undefined not expressible (pre-existing); (d) napi returns {error} not throw until Ship wires J1; (e) stale lib.rs doc comments (Ship's).
- Notes commits now only via `bin/notes-commit js -m ... <files>`; js owns state/js.md, state/postcss.md, postcss-rust/*, error-callsite-form.md, proofs/js-divergences/*.
- 00:00 Ship OK on e0749a5 (lib.rs). Ship fixes the stale doc comments and wires the throw in S3. Slice 2 now waits only on Arch.
- 00:02 Slice 2 Arch REWORK (notes 530ec44): (1) options_error normalization moves napi -> lwc-compiler, gate bans constructing CompilerError/TransformError in napi; (2) template facade filters level==Error like template.ts, plus a Fatal oracle row (D-0224 says reproduce upstream); (3) filename uses Either<_,()> like location, delete the opts-1021 skip; (4) Ship ack already received 00:00. Executor reworking.
- 00:29 Renamed wip/sourcemap to bot/js-sourcemap. AB-L0056: SSR level/url plus seam_x0013 folded into the slice 2 rework. Cross-Lane agreements requested from Harness (lwc-compiler lib.rs, seam_x0013) and Ship (serialize_only_gate; e0749a5 already OK). bot/js-pre-reorder is a backup and won't land.
- 00:30 Harness agreed in principle to the Cross-Lane for slice 2, with conditions H1 (no option-semantics change; same reject set), H2 (existing lwc-compiler tests and the option gate unchanged), H3 (Error filter in the template facade only; warnings preserved) and H4 (seam_x0013 values come from the oracle, not hand-typed). Send Harness the final sha and pid for confirmation.
- 00:30 Ship agreed to the Cross-Lane on serialize_only_gate.rs. Its nit (walk src recursively) is being fixed in the rework. Any lib.rs change beyond e0749a5 needs Ship's sign-off on the diff first.
- 00:37 Arch added (5) no SSR None arm for level/url and (6) seam_x0013 pins LWC1121 end to end (notes e52cbc2). The APPROVE is gated on (1)-(3) and (5)-(6). Both are covered by the rework: single_error.rs is deleted and the seam test is being extended.
