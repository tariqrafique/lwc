# semdiff SD-2c — Function#toString observation channels (D-0263)

Status: design only (AB-D0006 rework, notes 32cf47b). No code until Arch
approves this document *and* the premise gate in §6. Addendum to
semdiff-SD-2a.md §11.4 / D-0196. SSR is out of scope except as fork F6.

D-0263 narrows the Function#toString channel that D-0196 treated as
unrestricted. `toString` / `String()` of a compiled function is OBSERVED only
where LWC or the compiled output itself reads it. Unlisted channels found later
default OBSERVED (F5) until ruled.

## 1. Channels (each is a rule)

Reasons, when a channel fires, are named `tostring-<channel>`
(`tostring-banner`, `tostring-engine-read`, `tostring-dom`, `tostring-output`).

| id | reason | channel | what is observed | oracle site (b9e8007) |
|---|---|---|---|---|
| R-TOSTRING-BANNER-1 | tostring-banner | version banner | only the text that `LWC_VERSION_COMMENT_REGEX` matches, on the function bodies the engine actually passes to `checkVersionMismatch` (template, stylesheet, or component constructor) — not the rest of those bodies, and not the module text | `packages/@lwc/engine-core/src/framework/check-version-mismatch.ts:39` (`func.toString().match(LWC_VERSION_COMMENT_REGEX)`); regex in `@lwc/shared` `LWC_VERSION_COMMENT_REGEX = /\/\*LWC compiler v([\d.]+)\*\/\s*}/` |
| R-TOSTRING-ENGINE-READ-1 | tostring-engine-read | other engine reads of a compiled function's string | an allowlisted call site, pinned to the oracle sha | enumerated under F2 |
| R-TOSTRING-DOM-1 | tostring-dom | function value stringified into the DOM or an attribute | a compiled function that becomes attribute text or a property on an element (known case: `lwc:external` arrow) | `packages/@lwc/engine-core/src/framework/template.ts:223` (`String(rawValue)` when serializing a static-part attribute); renderer `setProperty` of a function-valued attr on an external element |
| R-TOSTRING-OUTPUT-1 | tostring-output | coercion inside compiled output | `fn.toString()`, `String(fn)`, `` `${fn}` ``, or `fn + ''` that the compiled module itself evaluates | the pair's module AST |

These ids are placeholders. They are not added to `rt-1.toml` until this design
is approved and a later change implements them.

## 2. Detection

Branch coverage is not proven. A trap therefore misses every stringify that
runs only on a branch the pair never takes. The static detectors for
channels 3 (DOM / attribute) and 4 (in-output coercion) are the sound side:
they must be complete over the compiled-module AST. Any coercion whose operand
is not *provably* a non-function is NotProven, and NotProven means OBSERVED.
The trap is only a cross-check. A trap hit that the static side missed is a
detector defect and the run exits 2; it is not classified DIFFERENT.

Channel 1 is static over the function bodies the engine passes to
`checkVersionMismatch` (F1), comparing only the regex match. Channel 2 is the
pinned allowlist (F2), not a guess at runtime.

### 2.1 Static

- **Banner (F1, APPROVE).** Identify the functions the engine actually passes
  to `checkVersionMismatch` (the compiled template function, each compiled
  stylesheet, the component constructor). Run `LWC_VERSION_COMMENT_REGEX`
  against each function's `toString()` text — equivalently, against that
  function's source body — and compare the matches (including a match on one
  side only). Differences outside the match, and differences elsewhere in the
  module, are not channel 1.
- **Engine reads (F2, APPROVE + closure gate).** A checked-in allowlist file
  pins the oracle sha (the `ORACLE_COMMIT` the exec runner already records).
  A check enumerates every `toString` / `String(` site in
  `@lwc/engine-core` and `@lwc/engine-dom` at that pin and fails (exit 2) if a
  site is neither listed as a channel-2 read nor explicitly excluded (error
  messages about property names, `vm.toString` debug labels, renderer-factory
  documentation, and so on). SD-2c does not re-derive the list at compare time
  beyond running that gate when the oracle pin moves.
- **DOM / attribute (channel 3, sound).** Walk the compiled-module AST for
  `$api` / renderer calls that write an attribute or property (`sa` / `sp` /
  static parts / `setProperty`) whose value expression is not provably a
  non-function, plus any `lwc:external` element whose attribute or property is
  a function. Incomplete proof → NotProven → OBSERVED.
- **In-output coercion (channel 4, sound).** An oxc walk of the compiled
  module only (engine and observer files are not walked): `CallExpression`
  callee `toString` or `String`, a template-literal interpolation, or `+`
  that would `ToString` its operand, where the operand is not provably a
  non-function. Incomplete proof → NotProven → OBSERVED.

### 2.2 Observer instrumentation (cross-check only)

One trap, on `Function.prototype.toString` (F4). `String(fn)`, `` `${fn}` ``,
`fn + ''`, and `Function.prototype.toString.call(fn)` all reach it through
`OrdinaryToPrimitive`. There is no `String` wrapper and no per-function
`Symbol.toPrimitive` / own `toString` trap: own properties show up in
`ownKeys` and `Object.keys` and would change behaviour. A required negative
test: a function with its own `toString` must keep that own property (the
trap must not install one) and must stay DIFFERENT if that own `toString`'s
text differs, or OBSERVED via the static walk if the compiled module reads it.

The trap records `{fn_id, site}` when the callee is a function the run has
already indexed (`{"$":"fn","id":k}` from SD-2a §2.1), then calls the original
`toString` and returns its result unchanged. Banner matching in the engine
still sees the real `func.toString()` text.

`site` is the caller's exact module URL from a V8 structured stack
(`Error.captureStackTrace`), mapped to an engine file (channel 1 or 2) or to
the pair module (channel 4, or channel 3 when the engine file is the
attribute serializer / `setProperty` path). Anything else is `unknown`.

F3 (APPROVE): an `unknown` site matters only for a function whose text differs
across sides. Stringifying a function whose compiled text is identical cannot
observe a difference.

Do not snapshot `Function.prototype.toString` results into the trace as raw
source. The point of D-0263 is that most of that source is not a contract.

A `stringify` event the static detector did not predict, for a function whose
text differs, is a detector defect: exit 2, not DIFFERENT `tostring-*`.

## 3. Verdict rule

When traces are equal (SD-2a §11.1 / r4: `Reason::ExecEqual` and
`Class::EquivalentByExecution` are deleted; leftover equal traces are
UNCLASSIFIED `exec-uncovered` until a channel rule can produce
EQUIVALENT-BY-EXECUTION again):

1. Comments differ → DIFFERENT `exec-static-comments-differ` (unchanged).
2. Rewrite reason is `printed-differs`, `token-spelling-differs`, or
   `printer-equal-ast-differs`:
   - if any listed channel fired (static) on a difference the rewrite
     `first_diff` can see → DIFFERENT `tostring-<channel>`;
   - if a static detector is NotProven → OBSERVED → DIFFERENT
     `tostring-<channel>` for the detector that could not prove a
     non-function;
   - if no listed channel fired → EQUIVALENT-BY-EXECUTION
     (`branch_coverage: not-proven`). Coverage is not a soundness proof;
     the static walks are.
3. Coverage gaps with no rewrite difference → UNCLASSIFIED `exec-uncovered`.

A hoist-only pair with no banner difference, no engine/DOM/output channel,
and no NotProven static walk becomes EQUIVALENT-BY-EXECUTION under this rule.
Today's tree still reports it DIFFERENT, because this design is not code.

Printer-equal pairs never reach exec (unchanged).

Required negatives when implemented: `lwc:external` arrow stays DIFFERENT
(`tostring-dom`); a banner comment that differs; a compiled module that
itself coerces a function whose text differs; a function with its own
`toString` (F4); an unlisted engine read, constructed against the pin or
documented as absent (the F2 gate is the proof).

## 4. Forks (AB-D0006, notes 32cf47b)

- **F1 APPROVE.** Match `LWC_VERSION_COMMENT_REGEX` only, scoped to the
  function bodies the engine actually passes to `checkVersionMismatch`, not
  the module text.
- **F2 APPROVE** plus a closure gate: the allowlist file pins the oracle sha.
  A check enumerates every `toString` / `String(` site in `engine-core` and
  `engine-dom` at that pin and fails if a site is neither listed nor
  explicitly excluded.
- **F3 APPROVE.** An unknown site matters only for a fn whose text differs
  across sides.
- **F4 REWORK.** One trap, on `Function.prototype.toString`. `String(fn)`,
  `` `${fn}` ``, `fn + ''` and `.call` all reach it through
  `OrdinaryToPrimitive`. Drop the `String` wrapper and the per-fn
  `Symbol.toPrimitive` / `toString` traps. Negative test: a fn with its own
  `toString`. Site = caller's exact module URL from a V8 structured stack
  (`Error.captureStackTrace`), mapped to the engine files or the pair module;
  anything else is unknown.
- **F5 APPROVE.** Unlisted channel → fail-closed OBSERVED.
- **F6 APPROVE.** SSR / SD-2b must restate the sites; it does not inherit the
  CSR allowlist by default.

## 5. What this does not do

- It does not restore `Class::EquivalentByExecution` until Arch approves this
  design and a later change implements §3.
- It does not weaken R-TRAILING-COMMA-1 / R-ASI-SEMI-1 (D-0258 / D-0266).
- It does not change the observer's existing `$api` recording or the
  AB-D0003 (i)–(iii) conditions.

## 6. Premise gate (required before any code)

Question: of today's 246 non-equal H-1 pairs, how many would the conservative
static rule in §3 certify?

Method: throwaway script, not shipped code.
Path: `/workspace/scratch/sd2c-premise-gate/measure.py`
(sha256 `c4497fc74c34d4588e2b1d4f67fcfa170d9e496227c0c47cedf04f563064dcca`).
It reads 681aba0's summary (sha256 `008f7583e7ec56aa471f0079bd33ef75272e4391a977cdeb4bc116c7f9363081`,
the same bytes as `/workspace/scratch/h1-1dab7cd/s1.json`) and the pinned
pairs at `/workspace/scratch/h1-b87b3fb/pairs1`. Output sha256
`5093d39231bb50f68ac561c7b6c6cc6c69366711bc032eee3753f10087460f87`
(`/workspace/scratch/sd2c-premise-gate/out.json`).

The script is conservative in the sense of §2: channel 1 compares only
`LWC_VERSION_COMMENT_REGEX` captures across each pair; channel 3 flags a
function-valued attribute API or `lwc:external`; channel 4 flags `.toString(`,
`String(<non-literal>)`, or `${<non-literal>}` in the compiled module.
Equal traces (`static-observed-differs`) are the only candidates. Unequal
traces cannot certify.

Results, broken down by the stated clusters:

| slice | n in the 246 | equal traces (candidates) | would certify |
|---|---:|---:|---:|
| C1 numeric | 105 | 105 | **105** |
| C2 arrow parens | 36 | 35 | **35** |
| C2 `lwc:external` (the one exec-differs of the 36) | 1 | 0 | 0 |
| C3 tagged-template throw | 35 | 0 | 0 |
| C4–C5 exec-differs (object-expr / template-literal undefined) | 70 | 0 | 0 |
| **total** | **246** | **140** | **140** |

Reasons in the summary: static-observed-differs 140, exec-differs 71
(70 printed-differs C4–C5 + 1 token-spelling C2/`lwc:external`),
exec-throw-one-side 35 (C3). 681aba0 had 0 unclassified.

For every C1 pair and every equal-trace C2 pair the banners match, the
compiled modules contain no `.toString(` / `String(` / `${…}` of a
non-literal, and there is no function-valued attribute API. Under F1 the
numeric or arrow-paren spelling sits in the `tmpl` (or nested arrow) body
that `checkVersionMismatch` stringifies, but only the regex match is
observed, and that match does not contain the spelling. So the conservative
static rule would certify them. The `lwc:external` arrow is not a candidate
(traces diverge) and would stay DIFFERENT `tostring-dom` if traces ever
agreed.

Full-corpus estimate:

- 16549 printer-equal pairs already never reach exec and would never need
  SD-2c.
- Hoist-only pairs (`printed-differs` + equal traces): **0**. With 0
  unclassified, a hoist-only pair would have landed in the 140 as
  `printed-differs` + `static-observed-differs`; every one of those 140 is
  `token-spelling-differs` (C1 or C2). There are none today.

### Recommendation

The conservative rule does **not** certify roughly zero: it would certify
all 140 equal-trace pairs (C1 105 + C2 35). I do not recommend deferring
SD-2c on "no value" grounds if the goal is to let those spelling pairs
become EQUIVALENT-BY-EXECUTION under D-0263.

I do recommend stating the motive honestly: the original hoist-only example
has **no instances** in this corpus. The measured value is C1/C2 spelling
inside `tmpl` bodies that reach `checkVersionMismatch` only for the banner
regex. Those clusters are also Template-owned (AB-D0004 C1–C5); regenerating
H-1 pairs against a tree that has already landed S4 may shrink or remove
them independently of SD-2c. If Arch's only remaining interest was hoist-only
pairs, defer. If certifying the 140 C1/C2 spellings is in scope for D-0263,
the design is ready for a second review and then code.
