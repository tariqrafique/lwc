# semdiff SD-1B: classify codegen-only cases from the H-1 manifest

Status: result, 2026-11-01 ~23:30 PST, by semdiff. **All numbers in this doc are LIVE-TREE and NON-REPORTABLE.** They are not
board numbers. Only an official board run may quote them.

## Provenance (reproducibility proof)
- Generated from main **273d936** (bot/semdiff-sd1b == main at generation time).
  - The branch is now on main 6ad019e. 6ad019e touches only crates/lwc-ssr and harness/ratchets/ssr-compile-js-scanners.txt.
  - The H-1 driver (tools/template-diag-diff/driver) depends only on lwc-template + lwc-shared, so the corpus is unaffected.
- Engine: **crate** (the H-1 default: the Rust driver binary). No napi, so there is no napi hash.
  - parse5 was resolved to tree 504f706. That is the D-0204 pin e3520ba's tree, byte-identical to /workspace/parse5-rust 2826427.
- Oracle: /workspace/lwc @ **b9e8007**, clean.
- H-1 command: `TDD_EMIT_PAIRS=/tmp/sd1b/pairs node harness/scripts/template-diag-diff.mjs`. It was run twice, and pairs/ is identical across the runs.
  - Manifest sha256 **c465fbc0aa2b56aa3b01f0547d0c1d67de056764046ae8bcfc4d85313e1039bf**.
  - meta.jsonl sha256 927891eb46e87867dbd465fb527dbf3f680a92d12decc679054f32703a5436e7.
- semdiff command: `target/release/semdiff run --manifest … --registry notes/lwc-rust-migrate/decisions.tsv --gate none`, rt-0, observation model D-0196.
  - It was run twice. summary1.json and summary2.json are **byte-identical**: sha256 dc3ee534f1297f86e4f227147a91dda77a1f9c9d2c6695b0c2c007876f04d162.
  - Registry sha256 ed0592246337dac28c0e84e18d9f2e937d65a298e19dd5c50c06b3bb7fdfd698 (notes 3276993). Decisions map sha 4464fac3…, 0 entries.
  - Re-run against the registry at notes 6914d99 (sha 0cbc43f6…): identical totals, by_reason and per-case buckets.
- Clustering: `node tools/semdiff/scripts/cluster.mjs --summary s.json --manifest m.json --out dir [--banner-whatif dir]`, committed on bot/semdiff-sd1b 042412b.
  - It was run 3 times, and the outputs are byte-identical every time.
  - clusters.json c8320df3774af029c3c7402405b24b1a5108a81be7f9989dcc92f0d8be06f5f7
  - features.json 8871fe18c6b6b939d3a00739536dae7510b36cb5f5219545cf8cd3daa7e62a0c
  - cases.jsonl bd1abea6ae988c9f1192466650d2b9aaef0d720948680a59757e0610c17c0f48
  - 261 clusters and 132 features.
- The clustering script never assigns a verdict, and nothing below is called equivalent by eye. Every equivalence statement comes from a semdiff run.

## Totals (live-tree, non-reportable)
H-1 (crate engine on 273d936):

| Count | Cases |
|---|---|
| Total | 63224 |
| diag-exact | 63112 |
| fully identical | 37917 |
| codegen-only | **25195** |
| crash | 0 |
| oracle-error | 0 |
| diagMismatch | 112 |

**AB-0017 hole: `totals.codegenOnlyStrippedEqual` = 20272.** These are the codegen-only cases that the old banner- and whitespace-stripped hash would have passed.

semdiff rt-0 on the 25195 cases:

| Bucket | Cases |
|---|---|
| equivalent | 0 |
| different_decided | 0 |
| different_undecided | 0 |
| unclassified | **25195** |
| crash | 0 |

by_reason: comments-differ 20305, printed-differs 4647, token-spelling-differs 133, printer-equal-ast-differs 110.

**Why 0 equivalent:** the Rust lwc-template never emits the oracle's `/*LWC compiler v9.5.3*/` comment in `tmpl`. It is missing in 25195 of 25195 Rust files and present in every oracle file.
- The oracle adds it in template-compiler codegen/index.ts:715 (`trailingComments`).
- D-0196 observes comments. The engine reads this one through Function#toString in `checkVersionMismatch` (engine-core check-version-mismatch.ts, called from secure-template.ts:28).
- So it is a real difference, and no rt-1 rule may normalize it.

**Banner what-if (measurement only, not a verdict).** `cluster.mjs --banner-whatif` inserts the oracle's banner line into every Rust file (25195 of 25195 patched).
- Manifest sha 67031898…; summary sha a4f8db14….
- Result: equivalent **20305**, unclassified 4890, and every other bucket 0.
- The stripped-equal set (20272) is a strict subset of the what-if equivalents (20272 − equivalent = 0). So on this tree the AB-0017 hole admitted no case that semdiff finds different.
- The extra 33 differ only in quote spelling (`'foo'` vs `"foo"`), which D-0196 lists as unobservable.
- This does not close the hole in general: the stripped hash also deletes whitespace inside string literals, which is observable.

## Top clusters (key = reason | first_diff coords | sorted diff-shape features; every one also carries the missing banner)
"Feature" counts are cases that carry the feature anywhere (features.json). Calls are (a) rt-1 rule candidate, (b) exec-leg candidate, or (c) likely codegen bug + owner.

| # | n | reason | Diff shape (besides the banner) | Example id (prefix) | Call |
|---|---|---|---|---|---|
| 1 | 20305 | comments-differ | banner only (33 of these also differ in quote spelling) | template-fixtures/9d31e9aa…/3612a6a7… | **(c) LWC Template Rust**: emit the banner (index.ts:715). Fixing it makes all 20305 equivalent (what-if run). |
| 2 | 1106 | printed-differs | `slotAssignment: "media"` → `attrs: {"slot": "media"}` | lbc-recipes mockTileHolder.html | **(c) LWC Template Rust**: the slot attribute lowering is missing (oracle index.ts:545-551; Rust codegen/mod.rs:311-327 has no `slot` branch). Feature: 1420 (→attrs) + 198 (dropped). |
| 3 | 511 | printed-differs | `classMap: {"x": true}` → `attrs: {"class": "x"}` | lbc-recipes inputLocation.html | **(c) LWC Template Rust**: the string `class`→classMap lowering is missing (index.ts:525-530). Feature: 997 + 424 + 69. |
| 4 | 419 | printed-differs | `attrs: {"name": "label-end"}` dropped from `api_slot` data | lbc lightning/textarea.html | **(c) LWC Template Rust**: Rust deliberately drops `<slot name>` from attrs (mod.rs:454 "carried by slot_name"); the oracle keeps it. 1018 of the 1132 `ids:-attrs` cases have a slot `name` attr. |
| 5 | 349 | printed-differs | classMap missing + `svg: true` missing | mutation svg/xlink/…literal-value-starting-with-hash | **(c) LWC Template Rust**: two root causes (#3 and #7). |
| 6 | 207 | printed-differs | key `"@:4"` → `4` (S→N) | wtr light-dom/scoped-slot/if-block mixedSlotChild.html | **(c) LWC Template Rust**: the slotted-vnode key prefix `@${slotParentName}:${key}` is missing (oracle codegen.ts:484-492). 582 of the 584 S→N cases are this. |
| 7 | 205 | printed-differs | `svg: true` missing from SVG element data | wtr directive-lwc-dom-manual svgWithLwcDomManual.html | **(c) LWC Template Rust**: index.ts:638-640 is not ported. Feature: 624. |
| 8 | 143 | printed-differs | slot name dropped + `@` key | — | **(c) LWC Template Rust** (#4 + #6). |
| 9 | 110 | printer-equal-ast-differs | Rust adds parens: `? ($ctx._sanitizedHtml$0 = …) :` | lbc formattedRichText.ssr.html | **(a) R-PAREN-1 candidate, needs a new D-id** (see below); the cheaper fix is **(c) LWC Template Rust** printing like the oracle's printer. |
| 10 | 99 | token-spelling-differs | numeric spelling `0b1001011` → oracle `75` | mutation expression-complex/valid/numeric-literal-binary | **(a) new R-NUM-SPELLING-1 candidate, needs a D-id**; alternative (c) LWC Template Rust printing the canonical decimal. A scripted re-lex finds 105 of 108 N→N cases value-equal and 3 not re-lexed. |
| 11 | 92 | printed-differs | stc renumber + slot-name stc dropped + `tmpl.slots` order | ssr-compiler fixtures scoped-slots/named-slot child.html | **(c) LWC Template Rust**: #4 plus unsorted `tmpl.slots` (oracle helpers.ts:98 `.sort()`, Rust mod.rs:1025 insertion order; 494 cases differ). The renumbering is downstream. |
| 12 | 92 | printed-differs | slot name dropped + `@` key + attr-list shape | — | **(c) LWC Template Rust** (#4 + #6). |
| 13 | 65 | printed-differs | slotAssignment + `@` key | — | **(c) LWC Template Rust** (#2 + #6). |
| 14 | 62 | printed-differs | slot `name` stc dropped (`attrs: stc3` gone) | wtr rendering/dynamic-slots lightChild.html | **(c) LWC Template Rust** (#4). |
| 15 | 60 | printed-differs | stc renumber + slot-name stc dropped | — | **(c) LWC Template Rust** (#4; the renumbering is downstream). |

Also outside the top 15, and high severity: clusters 30, 31 and 32 (33 cases each) plus a few small ones.
- Under `experimentalComplexExpressions`, Rust lowers template literals, tagged templates and object literals to **`undefined`**.
  - Example: mutation expression-complex/valid/template-literal.
  - The oracle emits `` `${$cmp.name}: ${$cmp.occupation}` ``; Rust emits `"attr": undefined` and `api_dynamic_text(undefined)`.
- Call: **(c) LWC Template Rust**. The value is dropped.
- Also: string `style` → styleDecls is missing (140 cases, index.ts:540-543) → **(c) LWC Template Rust**.

(b) exec-leg: no top-15 cluster calls for it. The stc hoist/renumber shape (`hoist-renumber`, 318 cases) never occurs alone: it always comes with the slot-name drop. Re-cluster after #4 is fixed before calling R-STC-INLINE-1 or the exec leg.

## rt-1 candidates ranked by cases covered (banner considered fixed; "sole" = cases where the rule's shape is the only remaining difference)
1. **R-PAREN-1**: covers 144 sole, touches 217.
   - Shapes: 181 are `cond ? (a = b) : c` (the innerHTML sanitize memo) and 36 are arrow params `(foo) => foo` vs `foo => foo`.
   - Blocker: D-0196 does not list parens, and both shapes sit inside `tmpl`, so Function#toString observes them. A rule needs a new D-id ruling, plus AB APPROVE, plus the SD-0 §4 negatives: `(a?.b).c`, `delete (x)`, `delete (0,a.b)`, a parenthesized directive prologue.
   - Recommend fixing the printer in LWC Template Rust instead.
2. **R-NUM-SPELLING-1 (new, not in SD-0 §4)**: covers 99 sole, touches 108.
   - Needs a D-id, since numeric spelling isn't listed in D-0196.
   - Negatives: `1n` vs `1`, `-0` vs `0`, values beyond 2^53, `{1: x}` vs `{"1": x}`, legacy octal `010`, and `1e21` printing.
   - Same recommendation: canonical printing in LWC Template Rust is simpler.
3. **R-STC-INLINE-1**: 0 sole, touches 318 (all downstream of #4).
4. **R-IMPORT-SPEC-ORDER-1 / R-API-DESTRUCT-ORDER-1**: 0. No case's features are only a reorder of import or `$api` destructure identifiers.
5. **R-STATIC-PROP-KEY-QUOTE-1**: 0 needed. Quote spelling is already unobservable in v0 (the 33 above). The S→N key cases are the `@` key bug, not quoting.

Even if the top two were approved (243 sole cases together), the bug fixes dominate. In priority order: banner (20305), slot/slotAssignment, classMap, svg, styleDecls, slot-name attr, `@` keys, `tmpl.slots` sort, complex-expression `undefined`.

## Blockers
- **LWC Template Rust**: the banner is missing, so semdiff can report no equivalence at all on this corpus until it is emitted. The data-bag lowerings listed above are missing too.
- Any rt-1 rule in this corpus needs a D-id first, because both candidates are toString-observable under D-0196.
- H-1 emitter (owner LWC Template Rust): no issues found. It is deterministic, 2 runs are identical, and the pairs agree with the report's stripped-equal count.
- Environment (LWC Harness owns /workspace/.cargo/config.toml, changed 23:17 PST): its `[patch."https://github.com/tariqrafique/parse5-rust"]` is unused, because main's lwc-template still has a path dep (`../../../parse5-rust`, which in a /workspace/wt worktree resolves to /workspace/wt/parse5-rust).
  - As a result, `cargo … --locked` fails in every worktree: cargo wants to add `[[patch.unused]]` to Cargo.lock.
  - I ran tests with `--offline` and restored Cargo.lock.
  - The trees match today (504f706), so there is no behaviour difference.

## Reproduce
```
cd /workspace/wt/semdiff-sd1b   # bot/semdiff-sd1b
TDD_EMIT_PAIRS=/tmp/sd1b/pairs node harness/scripts/template-diag-diff.mjs
target/release/semdiff run --manifest /tmp/sd1b/pairs/manifest.json --registry /workspace/notes/lwc-rust-migrate/decisions.tsv --out /tmp/sd1b/summary1.json --gate none   # x2, cmp
node tools/semdiff/scripts/cluster.mjs --summary /tmp/sd1b/summary1.json --manifest /tmp/sd1b/pairs/manifest.json --out /tmp/sd1b/cl1 --banner-whatif /tmp/sd1b/whatif1
target/release/semdiff run --manifest /tmp/sd1b/whatif1/manifest.json --registry … --out /tmp/sd1b/whatif-summary.json --gate none
```

## Re-cluster 2026-11-02 on main 9cc1c0a (after AB-L0048). LIVE-TREE, NON-REPORTABLE
### Provenance
- Tree: /workspace/wt/semdiff-sd1b detached at **9cc1c0a**. It uses the landed semdiff (rewrite leg, rt-0, summary v1) and cluster.mjs (d5c4a58). bot/semdiff is untouched.
- Oracle: /workspace/lwc @ b9e8007, clean.
- H-1: `TDD_EMIT_PAIRS=… node harness/scripts/template-diag-diff.mjs`, engine crate.
  - Run 3 times; pairs/ is identical across all 3. The third run used a private `TDD_DRIVER_TARGET`.
  - The driver binary sha256 is d7728b74… in both the shared and private target dirs.
  - Result: diag-exact 63112/63224, fully identical 46317, codegen-only **16795**, stripped-equal 15924, crash 0.
- Manifest sha256 **de43daea2686f585519c3e6f3cbadc736012b72bc5cd76ecfe2009e701a52ec5**; meta.jsonl 8a0f625f…. This is byte-identical to the SD-2a smoke manifest.
- `semdiff run --gate none --registry decisions.tsv` (sha 00d6143a…, notes fb87c49), run twice. The summary is **byte-identical**, sha256 **eb43500df44f04662c84425d098b8abab3642e48d3d6461f6f40eda21a8f8eec**.
- cluster.mjs was run twice and is byte-identical:
  - clusters.json a636bd16…
  - features.json 65db3f41…
  - cases.jsonl b2727f16…
  - 11 clusters and 21 features.

### Buckets (16795)
| Bucket | Cases |
|---|---|
| equivalent | **15959** (printer-equal) |
| unclassified | **836** (printed-differs 695, token-spelling-differs 141) |
| different_* | 0 |
| crash | 0 |

### Why "16795 remaining" does not mean #1-#8 are open (root cause, proven)
H-1's codegen-only bucket compares **bytes**; it is not a semdiff verdict. Every pair was checked mechanically:
- 15924 differ in whitespace only. That is exactly H-1's stripped-equal count. The rust printer wraps and indents differently from astring.
  - Example: oracle `api_slot("slot4", stc0, [api_slot(...)])` on one line; rust breaks it over several lines.
  - Example case: template-fixtures/9d31e9aa…/3612a6a7….
- 35 differ in whitespace plus quote spelling.
- All 15959 of these are printer-equal, so they are EQUIVALENT under D-0196.
- The hypotheses were checked and are **not** the cause:
  - Not a stale manifest: it was regenerated 3 times on 9cc1c0a and is identical.
  - Not a stale binary: a private target rebuild gives the same sha and an identical manifest. The rust output carries the fixes:
    - banner in 16795 of 16795;
    - classMap 1338, slotAssignment 1558, styleDecls 100 and svg 630 cases match the oracle.
  - Not fixes missing the paths: the SD-1B features for those fixes all went to 0 (table below).
- Hazard (not the cause here): the H-1 driver's default target `/tmp/template-diag-diff-target` is **shared by every worktree**, and the last build wins. Owner: LWC Template Rust (H-1). Set `TDD_DRIVER_TARGET` per worktree if runs overlap.

### SD-1B #1-#8 status (feature cases: SD-1B → now)
| SD-1B # | n then | Status | Evidence |
|---|---|---|---|
| 1 banner | 20305 | **collapsed** | comment-missing feature 25195→0 |
| 2 slotAssignment | 1106 | **collapsed** | 1420→0, 198→0 |
| 3 classMap | 511 | **collapsed** | 997/424/69→0 |
| 4 slot `name` attr | 419 | **collapsed** | `ids:-attrs\|+` 1132→0 |
| 5 classMap+svg | 349 | **collapsed** | — |
| 6 `@slot:N` key | 207 | **OPEN, unchanged** | S→N 584→**582**; all 582 now sole |
| 7 svg | 205 | **collapsed** (624→0) | NEW inverse regression, see N1 |
| 8 slot name + `@` key | 143 | **partial** | slot-name half fixed; `@` key half is in the 582 |

Also from SD-1B:
- #9 sanitizedHtml parens: **partial**, 217→36. All 181 sanitize-memo cases are fixed; the 36 arrow-param cases remain.
- #10 numeric spelling: **open**, 108→105.
- #11 tmpl.slots sort and the hoist-renumber shapes (318→0): **collapsed**.
- styleDecls: **collapsed**, 140→0.
- complex-expression `undefined`: **open**, 35+35+35.

### Clusters now (ranked; owner = LWC Template Rust unless noted)
| # | n | Shape | Minimal repro / example |
|---|---|---|---|
| 1 | 582 | slotted-vnode key prefix missing: oracle `"@slot4:1"`/`"@:2"`, rust `1`/`2` | `<template lwc:render-mode="light"><slot><p>x</p></slot></template>` → oracle `api_static_fragment($fragment1, "@:2")`, rust `…, 2)` (oracle codegen.ts:484-492) |
| 2 | 105 | numeric spelling `0b1001011` vs `75` | mutation expression-complex/valid/numeric-literal-binary. Printer fix per Arch (no rt-1 rule) |
| 3 | 36 | arrow param parens `(foo) => foo` vs `foo => foo` | mutation expression-complex/valid/arrowfn-scoped-vars. Printer fix per Arch |
| 4 | 35 | tagged template → `undefined` | expression-complex/valid/template-literal-tagged: oracle `` ($cmp.foo)`barzy-bazzy${$cmp.bizzy}` ``, rust `undefined` |
| 5 | 35 | object literal → `undefined` (oracle hoists `{obj:'literal'}` into stc0) | expression-complex/valid/object-expr |
| 6 | 35 | template literal → `undefined` | expression-complex/valid/template-literal |
| 7 | 2 | `api_flatten` dropped | `<template><div><!-- c --><template for:each={l} for:item="i"><p key={i.k}></p></template></div></template>`. The oracle wraps the children in `api_flatten([...])` because of the non-preserved comment sibling; rust doesn't. Probably runtime-equal, so an **exec-leg candidate**; it also changes the `$api` destructure. |
| 8 | 2 | computed property inside for:each: oracle bare `index`, rust `$cmp.index` | `<template><template for:each={arr} for:item="x"><p key={x.k}>{arr[index]}</p></template></template>` + experimentalComputedMemberExpression |
| 9 | 2 | **NEW**: rust adds `svg: true` to a custom element after an svg | `<template><svg></svg><x-foo k={c}></x-foo></template>` → rust `{props:{k:$cmp.c}, key:2, svg:true}`; the oracle has no svg. Not seen for `<div>`. Regression from e57db63 (namespace state leaks to the next custom element). |
| 10 | 1 | quoted complex key dropped (`api_key` missing) | `<template><template for:each={b} for:item="o"><div key="{x()}"><a onclick={h}>one</a></div></template></template>` + experimentalComplexExpressions → oracle `api_key(1, $cmp.x())`, rust `1` |
| 11 | 1 | same as #10 with `key="{x[0]}"` | — |

- New since SD-1B: **only #9** (feature 0→2).
- #7, #8, #10 and #11 already existed at 2/2/1/1 but were buried under the banner. They are flagged here because they are now separate clusters.
- No rt-1 rule is proposed (Arch: no rules for parens or numeric spelling).

### SD-2a exec smoke cross-check (the 8 exec-differs cases)
- The SD-2a smoke manifest == this manifest (cmp).
- 7 of 8 are in cluster #1 (`kinds:-S|+N`), so `@slot:N` namespacing is **NOT fixed on 9cc1c0a**. No commit touching it exists in crates/lwc-template on main or the bot/template* branches since 9cc1c0a.
- The 8th is cluster #6 (template literal → `undefined`).

## Re-cluster 2026-11-08 23:20 PST: main 9c96ee4 + bot/semdiff babaf01, with exec (live-tree, NON-REPORTABLE)
### Provenance
- Tree: /workspace/wt/semdiff = main **9c96ee4** + babaf01 (SD-2a §10; touches tools/semdiff only). Oracle /workspace/lwc @ b9e8007, clean.
- H-1: `TDD_DRIVER_TARGET=/workspace/.targets/tdd-semdiff TDD_EMIT_PAIRS=… node harness/scripts/template-diag-diff.mjs`. That is a
  private driver target, cold-built from this tree; the driver sha256 is d9d9384ab3a1….
  - Run twice; `diff -r pairs1 pairs2` is identical.
  - diag-exact 63115/63224, fully identical 46320, codegen-only **16795**, stripped-equal 16514, crash 0.
- Manifest sha256 de43daea2686f585519c3e6f3cbadc736012b72bc5cd76ecfe2009e701a52ec5. **Caveat:** the manifest only lists
  case ids and file paths, so its sha matches the 9cc1c0a manifest because the case set is the same, not because the
  content is. The content hash is sha256 of the sorted per-file `sha256sum` lines of pairs/, **95b1ed4eca7d323c…**. Earlier
  sections that call manifests "byte-identical" proved only that the case set was the same.
- `semdiff run --exec --gate none`, seed 1:
  - registry pinned to notes 3d19c38 decisions.tsv, sha da03fa65…;
  - jobs 6 on pairs1 and jobs 4 on pairs2 give **byte-identical** summaries, sha256
    **fa3c58c603f42274fd6502a8726b0db73ef2c784eac8816b5200ededd4b70f0e**;
  - wall time about 3m40s; exec.cases_run 246; runner_sha256 f13d2c07….
  - An earlier pair of runs against the live registry differed only in `decisions.registry_sha256`, because other
    bots added D-0241/D-0242 mid-run. Everything else matched, so the registry is now pinned.

### Buckets (16795)
| Bucket | Cases |
|---|---|
| equivalent | **16549** = printer-equal 16549 + exec-equal **0** |
| different_undecided | **246** (exec-static-spelling-differs 140, exec-differs 71, exec-throw-one-side 35) |
| different_decided | 0 |
| unclassified | **0** |
| crash | 0 |

Versus 9cc1c0a: 836 unclassified → 590 became printer-equal, and the remaining 246 are now all DIFFERENT with a
cause. No case is EXEC_EQUAL, so nothing in this corpus depends on branch coverage (still not-proven) or on the §10.6
limit.

### Clusters vs 9cc1c0a (owner LWC Template Rust for all)
| old # | n then | now | Status | Verdict now / evidence |
|---|---|---|---|---|
| 1 `@slot:N` key prefix (AB-L0057 f656c18) | 582 | 0 | **collapsed** | all 582 oracle files with an `"@…:N"` key are printer-equal, and every rust file now carries the key |
| 2 numeric spelling | 105 | 105 | **open, unchanged** | static-spelling numeric: 35 `75`/`0b1001011` (numeric-literal-binary), 35 `66`/`0x42` (-hex), 35 `34`/`0o42` (-octal). The oracle (astring) prints the value in decimal; rust keeps the source spelling. |
| 3 arrow param parens | 36 | 36 | **open, unchanged** | 35 static-spelling parens (oracle `foo => foo`, rust `(foo) => foo`, arrowfn-scoped-vars) plus **1 exec-differs**: under `lwc:external` the arrow is stringified into an attribute, so the DOM shows `foo="a,(foo) => foo,…"` vs `foo="a,foo => foo,…"`. Parens are observable at runtime, not just in theory. |
| 4 tagged template → `undefined` | 35 | 35 | **open** | exec-throw-one-side at mount(A): the oracle reads `$cmp.foo` and throws calling it as a tag; rust calls `$api.d(undefined)` and doesn't throw |
| 5 object literal → `undefined` | 35 | 35 | **open** | exec-differs mount(A): oracle `$api.d("[object Object]")`, rust `$api.d(undefined)` |
| 6 template literal → `undefined` | 35 | 35 | **open** | exec-differs mount(A): the oracle reads `$cmp.name`, rust `$api.d(undefined)` |
| 7 `api_flatten` dropped (3043276) | 2 | 0 | **collapsed** | by count: 836 − 246 = 590 = 582+2+2+2+1+1, and none of the 246 has this shape |
| 8 computed index in for:each (541745c) | 2 | 0 | **collapsed** | all 65 oracle files with `arr[index]` are printer-equal |
| 9 svg leak on a custom element (b3067d0) | 2 | 0 | **collapsed** | by count (as #7) |
| 10/11 quoted complex key (9c96ee4) | 1+1 | 0 | **collapsed** | both oracle `api_key(N, $cmp.x())` / `$cmp.x[0]` files are printer-equal |

- Partial: none. New: none; every one of the 246 falls in old clusters 2-6.
- 4-6 are the AB-D0004 typed-expression work (complex-expression codegen), and 2-3 are printer.rs (Arch AB-L0041: no
  rt-1 rule).
- Minimal repros are unchanged from the 9cc1c0a table above (mutation corpus `expression-complex/valid/{numeric-literal-binary,-hex,-octal,arrowfn-scoped-vars,template-literal-tagged,object-expr,template-literal}`,
  `experimentalComplexExpressions: true`).

## Re-cluster 2026-11-08 23:40 PST: main 6016fa4 + semdiff 37cc88b, with exec (live-tree, NON-REPORTABLE)
- Tree: detached worktree /workspace/wt/semdiff-6016fa4 = **6016fa4** + 37cc88b. That commit is the same patch as
  bot/semdiff 19818d7 on 6ddb2d3, pid e52f32eb8183f951427236f4f2eb580835b79ef0 on both bases. Oracle b9e8007, clean.
- H-1 (private `TDD_DRIVER_TARGET=/workspace/.targets/tdd-semdiff`, driver sha256 25d3b7fdb334…), run twice;
  `diff -r pairs1 pairs2` is identical.
  - diag-exact 63115/63224, codegen-only 16795, stripped-equal 16514, crash 0.
  - Manifest (ids and paths) de43daea…; pairs content **95b1ed4e…**. Both are the same as on 9c96ee4: the rust output
    for this corpus is byte-identical between 9c96ee4 and 6016fa4, although the driver binary changed (lwc-shared).
- `semdiff run --exec --gate none`, seed 1, registry pinned to notes 4a2b05c decisions.tsv (sha 44f920fd…):
  - jobs 6 on pairs1 and jobs 4 on pairs2 give **byte-identical** summaries, sha256 **364466232a2473d9294c7069cd596888b4abf18d931d0b3e097ee54d90b09fae**.
  - It differs from the 9c96ee4 summary fa3c58c6… only in `decisions.registry_sha256`.
  - runner_sha256 f13d2c07…, exec.cases_run 246.

| Bucket | Cases |
|---|---|
| equivalent | 16549 = printer-equal 16549 + exec-equal 0 |
| different_undecided | 246 |
| different_decided | 0 |
| unclassified | 0 |
| crash | 0 |

Per-case list (D-0244 routing): `arch-board/proposals/semdiff-cases-6016fa4.tsv`. It has 246 rows; its header lists
each cluster's minimal repro. Its pair files are under /workspace/scratch/h1-6016fa4/pairs1, so regenerate them with
H-1 to reproduce.

| cluster | n @9c96ee4 | n @6016fa4 | delta | owner | minimal repro (experimentalComplexExpressions) |
|---|---|---|---|---|---|
| C1 numeric spelling | 105 | 105 | 0 | LWC Template Rust (printer.rs) | `<x-pert attr={0b1001011}>`: oracle `75`, rust `0b1001011` (also 0x42/66, 0o42/34) |
| C2 arrow param parens | 36 | 36 | 0 | LWC Template Rust (printer.rs) | `<x-pert foo={[foo, foo => foo]}>`: oracle `foo => foo`, rust `(foo) => foo` (35 static + 1 exec-differs via lwc:external toString) |
| C3 tagged template → undefined (one-side throw) | 35 | 35 | 0 | LWC Template Rust (AB-D0004) | `<x-pert attr={foo`a${b}`}>`: the oracle calls `$cmp.foo` as a tag and throws; rust `undefined` |
| C4 object literal → undefined | 35 | 35 | 0 | LWC Template Rust (AB-D0004) | `<x-pert attr={{obj:'literal'}}>`: oracle hoists the literal, rust `undefined` |
| C5 template literal → undefined | 35 | 35 | 0 | LWC Template Rust (AB-D0004) | ``<x-pert attr={`${name}: ${occupation}`}>``: rust `undefined` |

No delta: nothing collapsed, nothing new, nothing partial. Main 9c96ee4..6016fa4 has no lwc-template change. **Caveat:**
these buckets come from the §10 checker before the AB-L0064 rework, which flagged false-pass holes in it. Nothing in
this corpus is exec-equal, so those holes cannot have hidden any case here.
