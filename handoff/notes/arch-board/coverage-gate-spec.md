# coverage-gate spec v2 (Arch Board → Harness)

Status: spec for Harness to build. Arch Board owns `arch-board/COVERAGE.tsv`; Harness owns the gate; Migrate owns
`lwc-rust-migrate/decisions.tsv` and the report-only → strict flip.
Exit criterion it enforces: PLAN.md §4 "Coverage gate green".

v2 changes from v1: one source of truth for row status (cells cite `row:<id>` only; the gate derives status from
board.sh); NA is structural (an `inputs` column plus a required input kind per stage, checked mechanically); cells cite
`dec:<id>` against a stable id column that Migrate must add; a cell is covered only if it has a gating row, and a cell
with only diagnostic rows is `diagnostic-only`; report-only until Migrate flips to `--strict`.

## Matrix format (`arch-board/COVERAGE.tsv`)

Header, tab-separated, exactly:

```
source  inputs  style  template_codegen  template_diagnostics  js_transform  whole_component  ssr_render  csr_render  semantic_diff  notes
```

`semantic_diff` is always the last stage column, right before `notes`. Adding a stage or an input kind means changing
this spec and the gate together.

### Input kinds (closed vocabulary)

`inputs` is a comma-separated, non-empty set drawn from exactly these kinds. It lists what the source contains as
compiler input. Expected outputs (expected.js, expected.html), test drivers, tooling scripts and node_modules stubs do
not count.

| kind | the source contains |
|---|---|
| `css` | a stylesheet the LWC style compiler is meant to accept (a `.css` file whose content is CSS, or an inline CSS snippet). `.scss` does not count, and neither does a `.css` file that holds JS (module-resolver). |
| `html` | an LWC template: a `.html` file with a `<template>` root, an inline template snippet, or a template expression taken from one (bind_corpus). |
| `js` | a JS or TS module meant as LWC compiler input: a component class, a module in an LWC module tree, or an inline JS snippet. `.ts` counts as `js`. |
| `component` | an LWC component bundle: a directory `<name>/` holding `<name>.js` or `<name>.ts` whose default export extends `LightningElement`, with or without a `<name>.html` or `<name>.css` beside it. A single-file class snippet is `js`, not `component`. A template wrapped in a harness-made stub component is not a component of the source. `component` implies `js`. |

### Required input kind per stage

| stage | requires (any of) | why |
|---|---|---|
| `style` | `css` | style compiler input |
| `template_codegen` | `html` | template compiler input |
| `template_diagnostics` | `html` | template compiler input |
| `js_transform` | `js` | babel-plugin-component / SSR JS input |
| `whole_component` | `component` | compiling js + template + css of one real component together |
| `ssr_render` | `component` | something to render on the server. The oracle is the JS backend, so a shipped expected output is not required. |
| `csr_render` | `component` | same as ssr_render, on engine-dom |
| `semantic_diff` | `css`, `html`, `js` | any compiled output can be diffed, so this stage is never NA for a real source |

### Cell grammar

```
cell := rowlist | "dec:" ID | "NA" | "GAP"
rowlist := "row:" ROWID ( ";" "row:" ROWID )*
ROWID := [A-Za-z0-9_-]+          (a board.sh ROWS id)
ID    := [A-Za-z0-9_.:-]+        (a decisions.tsv id)
```

There is no whitespace, no empty entry, and no status suffix. `row:x:gating` and `row:x:diagnostic` are malformed in
v2. Rows and `dec:` are not mixed in one cell, and neither is mixed with `NA` or `GAP`.

### Cell kinds (derived, never written)

- **covered**: a rowlist with at least one row that is gating at run time (in ROWS, not in DIAG, and once AB-0006
  lands, not in a non-gating ratchet state), or a `dec:` that resolves.
- **diagnostic-only**: a rowlist whose rows are all diagnostic. This is not covered. It counts as a GAP in `--strict`.
- **GAP**: written `GAP`. Every GAP names its owner in `notes` (`GAP owner: <stage>=<owner> (<why>)`).
- **NA**: written `NA`. Legal only when the source's `inputs` lack every kind the stage requires.

Status comes only from board.sh at run time. The matrix never records whether a row is gating, so a row that moves
between ROWS and DIAG (or into the AB-0006 ratchet file) changes the gate's report with no edit to COVERAGE.tsv.

## Shape (mirror option-gate)

- File: `harness/scripts/coverage-gate.mjs`, next to `option-gate.mjs`. One file, no dependencies, Node built-ins only.
- Called from `harness/board.sh` right after option-gate, the same way:

  ```sh
  covgate=0; COVGATE_DIAG="$DIAG" COVGATE_RATCHET="${RATCHET_FILE:-}" node "$here/scripts/coverage-gate.mjs" ${COVGATE_STRICT:+--strict} "${ROWS[*]}" || covgate=1
  ```

  `ROWS` and `DIAG` are the arrays board.sh already defines. `COVGATE_RATCHET` stays empty until AB-0006 adds the
  ratchet file. When it is set, the gate reads row status from that file instead of `COVGATE_DIAG`. A nonzero exit
  prints `!! BOARD FAILED: coverage gate` and fails the board, the same as option-gate.
- Usage: `node scripts/coverage-gate.mjs [--strict] "<space-separated board rows>"` or `--self-test`.
- Output: one summary line, then one indented line per finding (option-gate format):

  ```
  coverage-gate: notes <commit12> report-only: 24 sources x 8 stages = 192 cells, 20 covered (declared gating), 11 diagnostic-only, 85 gap, 76 na, 0 dec, 0 invalid
    GAP corpus:lbc x js_transform owner=LWC JS Rust
    DIAG-ONLY corpus:lbc x template_diagnostics rows=template-diag,template-diag-napi
    UNKNOWN-ROW upstream:@lwc/foo x style -> row:bar
  ```

  board.sh appends the summary line to board.txt next to the `option-gate:` line, so a published board shows the
  coverage state and the notes commit it was checked against.

## Inputs: read at the notes commit the board records, never a working-tree copy

- `NOTES_COMMIT = $OFFICIAL_NOTES_COMMIT` when set (official-board.sh exports it). Otherwise it is
  `git -C /workspace/notes rev-parse HEAD`. official-board.sh commits the notes repo and records `notes_commit` in
  OFFICIAL_BOARD.txt before the run, so the gate checks the same commit the board publishes.
- Matrix: `git -C /workspace/notes show $NOTES_COMMIT:arch-board/COVERAGE.tsv`.
- Decisions: `git -C /workspace/notes show $NOTES_COMMIT:lwc-rust-migrate/decisions.tsv`, at the same commit.
- Never read the working-tree files, and never read the frozen snapshot: official-board.sh archives only
  `lwc-rust-migrate/`, so `arch-board/` is not in it.

## Checks

Failures (each finding is one line, and any failure exits 1 in every mode):

| kind | rule |
|---|---|
| `BAD-HEADER` | header is not exactly the v2 header above |
| `BAD-LINE` / `DUP-SOURCE` / `NO-EVIDENCE` | a data line does not have 11 fields; `source` is empty or repeated; `notes` is empty |
| `BAD-INPUTS` | `inputs` is empty, holds a kind outside the vocabulary, or has `component` without `js` |
| `MALFORMED` | a cell does not match the grammar (this includes a v1 `:gating`/`:diagnostic` suffix and a blank cell) |
| `UNKNOWN-ROW` | `row:<id>` where `<id>` is not in argv ROWS |
| `ORPHAN-ROW` | an id in ROWS that no cell cites, so a board row exists with no declared coverage |
| `NA-ILLEGAL` | `NA` on a source whose `inputs` contain a kind the stage requires |
| `GAP-MUST-BE-NA` | `GAP` on a source whose `inputs` lack every kind the stage requires |
| `CITE-MUST-BE-NA` | a rowlist or `dec:` on a source whose `inputs` lack every kind the stage requires. A row cannot cover a stage for input the source does not have, so the cell must be `NA`. |
| `DEC-UNRESOLVED` | `dec:<id>` where no row of decisions.tsv at NOTES_COMMIT has exactly `id == <id>`. This also fires on every `dec:` cell while decisions.tsv has no `id` column. |
| `DEC-ID-INVALID` | decisions.tsv at NOTES_COMMIT has an `id` that is missing, does not match `^D-[0-9]{4}$`, or appears on more than one row (union merges can duplicate ids). Fails the gate whether or not any cell cites it. |

**One validator (AB-L0030).** The decisions.tsv registry checks (DEC-ID-INVALID, DEC-UNRESOLVED, DEC-NOT-LOCKED) are implemented once, in `tools/semdiff` (`semdiff check-registry --registry <path> [--require <id>...]`, exit 0/1/2). `coverage-gate.mjs` calls that command for every `dec:` cell and never re-parses decisions.tsv itself.

The four input-kind kinds (`NA-ILLEGAL`, `GAP-MUST-BE-NA`, `CITE-MUST-BE-NA`, `BAD-INPUTS`) are the mechanical form of
"NA is structural": NA comes from the inputs column, not from a judgment call per cell. The gate does not scan
source trees. `inputs` is Arch Board's assertion, and each source's `notes` carries an `inputs:` clause with the file
evidence. A future `--audit-inputs` mode could recount the kinds from the oracle tree, but it is not part of this spec.

Report lines (exit 0 in report-only mode; failures in `--strict`):

| kind | rule |
|---|---|
| `GAP` | each `GAP` cell, with the owner parsed from notes |
| `DIAG-ONLY` | each rowlist whose rows are all diagnostic |

Mode:

- **report-only** (the default until Migrate flips it): prints everything and exits 1 only on the failure kinds
  above. GAP and DIAG-ONLY never fail.
- **`--strict`** (board.sh passes it when `COVGATE_STRICT=1`; Migrate owns the flip): GAP and DIAG-ONLY also fail.
  This is PLAN §4 "Coverage gate green".

The gate never repairs or rewrites the matrix. Summary counts come from the same pass as the findings.

## decisions.tsv `id` column (Migrate action, required before any `dec:` cell)

decisions.tsv has no id column today (header `ts phase decision why evidence result`). Arch Board does not edit it.
Migrate must add a stable `id` column (one unique, never-reused value per decision, for example `D0001`; ids are not
renumbered when lines move). The gate resolves `dec:<id>` by exact match on that column, never by substring.
The column exists (Migrate, notes b2ff49e): ids are `D-NNNN`, next = max+1, never renumbered. COVERAGE.tsv cites dec:D-0028 (engine-server ssr_render) and dec:D-0194 (module-resolver js_transform). NA does not
need a decision, so there are no "not applicable" decisions to log. A `dec:` cell is for an in-scope input that Migrate
rules out of scope, for example engine-server fixtures.spec x ssr_render if "delete engine-server" (decisions.tsv
line 29) is given an id.

## Self-test (`--self-test`)

`harness/scripts/coverage-gate.fixtures/` holds one tiny ROWS/DIAG/COVERAGE/decisions set per case, and
`node coverage-gate.mjs --self-test` runs them all and asserts the exact finding kinds. Each failure kind needs one
negative case (it must fire) and one positive case (the nearest legal matrix, where it must not fire):

| kind | negative (fires) | positive (silent) |
|---|---|---|
| UNKNOWN-ROW | `row:nope` with ROWS=`a` | `row:a` with ROWS=`a` |
| ORPHAN-ROW | ROWS=`a b`, only `row:a` cited | ROWS=`a b`, both cited |
| NA-ILLEGAL | inputs=`css`, style=`NA` | inputs=`html`, style=`NA` |
| GAP-MUST-BE-NA | inputs=`css`, js_transform=`GAP` | inputs=`js`, js_transform=`GAP` |
| CITE-MUST-BE-NA | inputs=`html`, whole_component=`row:a` | inputs=`css,html,js,component`, whole_component=`row:a` |
| DEC-UNRESOLVED | `dec:D9` with decisions ids `D1`; also `dec:D1` against a decisions.tsv with no `id` column | `dec:D1` with id column holding `D1` |
| GAP in --strict | one `GAP`, `--strict` → exit 1 | same matrix, report-only → exit 0 with a `GAP` line |
| DIAG-ONLY in --strict | `row:d` with DIAG=`d`, `--strict` → exit 1 | `row:d;row:a` (a gating) → covered, exit 0 |
| MALFORMED | `row:a:gating` (v1 suffix) | `row:a` |
| BAD-INPUTS | inputs=`ssr-fixture` | inputs=`js` |
| BAD-HEADER / BAD-LINE | v1 header (no `inputs`) / 10-field line | v2 header / 11-field line |

The self-test also checks status derivation: the same `row:a` cell is covered with DIAG empty and diagnostic-only
with DIAG=`a`, and the matrix bytes are identical in both cases.

## Known limits

- **"gating" is declared, not enforced.** board.sh:80 runs vitest with `|| true`. A non-DIAG row fails the board only
  when it prints no `Tests` summary, so a row with failing tests still "gates" in name only. Until AB-0006 (ratchet)
  lands, the gate and every report label covered cells **"declared gating"**. When AB-0006 lands, `COVGATE_RATCHET`
  points at the ratchet file and gating means "the ratchet fails the board on regression".
- `semantic_diff` is GAP in every applicable cell (owner LWC Semantic Diff) until a semantic-diff row is on the board.
  The reserved rows `semantic-diff` and `lbc` cannot be cited until board.sh lists them (otherwise UNKNOWN-ROW).
- Crate test sources (`crate:lwc-js/tests`, `crate:lwc-compiler-napi/tests/fixtures`, `crate:lwc-ssr/.../bind_corpus`,
  `crate:lwc-style/tests/parse-errors`) stay in the matrix as rows. Crate tests run only at land time (and napi only as
  an official-board cargo precondition with no row id), so they are GAP for board stages unless a board row covers them.
- Rows that do not really exercise a stage are not cited for it. template-diag/-napi hash whitespace-stripped codegen
  and count a codegen mismatch as a pass, so they are not template_codegen coverage. csr-render-template wraps a
  template in a harness stub component, so it is not whole_component or csr_render coverage of a source component.
  A cell that rests on one case (compiler/transformers-specs x style, one transform-css.spec `<` case) is GAP with a
  note.
- First run at the commit of this spec: 24 sources x 8 stages = 192 cells, of which 20 are covered (declared gating),
  11 diagnostic-only, 85 GAP, 76 NA, and 0 dec. Report-only stays green; `--strict` is red until the 96 GAP plus
  diagnostic-only cells are closed.
