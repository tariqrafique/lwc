# LWC compiler migration harness (Phase 0)

Behavioral parity harness for the `CompilerBackend` seam
(`transform(src, filename, options) -> { code, map? }` or throws). JS reference: `@lwc/compiler` `transformSync`.
Rust mirror: `crates/lwc-compiler` (`CompilerBackend` trait + `dispatch`).

**This is NOT the 69/69 style byte gate.** Byte fixtures (`expected.js`) belong to the PostCSS-Rust port.
Here we evaluate the emitted ESM and compare *behavior*:
- flatten `export default` (arrays/imports stubbed), check `$scoped$` / `$nativeOnly$` flags
- call each `stylesheet(token, useActualHostSelector, useNativeDirPseudoclass)` for
  `x|true|false`, `x|false|true`, `x|false|false`, `x|true|true`, `undefined|true|false`
- compare the evaluated CSS strings to goldens in `goldens/style/*.json` (recorded from the JS backend)
- error fixtures (non-empty `error.json`): must throw; line checked when available; name/line/column stored in golden
- style `map` must be `""` (JS `{mappings:""}` is normalized to `""`)

## Run
```
# prerequisites: /workspace/lwc has node_modules and @lwc/compiler built
cd /workspace/lwc && npx nx run @lwc/compiler:build
cd /workspace/lwc-compiler-rust/harness
npm run link:lwc            # symlinks @lwc/compiler + vitest (equivalent of the file: dep; yarn not on box)
npx vitest run              # or: yarn test
UPDATE_GOLDENS=1 npx vitest run   # re-record baseline — JS backend ONLY
```

## Goldens policy (Migrate-approved)
- Style 70/70 JS behavioral baseline is **accepted**. Goldens are the JS oracle.
- **Do not** `UPDATE_GOLDENS=1` with `LWC_COMPILER_BACKEND=rust` until a napi/wasm binding of `lwc_compiler::dispatch` exists.
- After binding: run rust against the existing JS goldens; only re-record if Migrate explicitly changes the oracle.

## Backends
`LWC_COMPILER_BACKEND=js` (default) | `rust`.

**Rust (napi-rs facade):** crate `crates/lwc-compiler-napi` → `lwc_compiler::dispatch`.
```
source "$HOME/.cargo/env"
cd /workspace/lwc-compiler-rust && cargo build -p lwc-compiler-napi --release
cp target/release/liblwc_compiler_napi.so harness/lwc-compiler-napi.node
cd harness && LWC_COMPILER_BACKEND=rust npx vitest run test/style.behavior.test.ts
```
Override addon path with `LWC_COMPILER_NAPI_PATH`. Style behavioral **70/70** green on rust against JS goldens (no UPDATE_GOLDENS). Template/js/ssr on rust wait on those crates.

## Template suite (Phase 0d v1) — `test/template.behavior.test.ts`
Walks 567 `@lwc/template-compiler` fixtures (`**/actual.html`, + `config.json`), compiles through the backend as
`<dir>.html` with `{namespace:'x', name:<dir>, ...config}` (same as upstream fixtures.spec). Asserts:
- outcome: fixture `metadata.json` has a level ≤1 diagnostic (Fatal/Error) ⇒ transform must throw; error `code` must match the first error code
- success: `warnings` **codes + levels** (multiset) equal the fixture's `metadata.json` (no message text)
- `src/evalTemplate.ts` evaluates the ESM with stubbed imports (`lwc` named imports recorded, `registerTemplate` = identity, css → undefined, components → stub);
  default export must be a function and registered; golden records imports, lwc imports, `stylesheetToken`, `cssScopeTokens`
- goldens: `goldens/template/*.json` (JS oracle). `UPDATE_GOLDENS=1` with `LWC_COMPILER_BACKEND=rust` throws at load.
- seam gaps (`SEAM_GAPS` in the test): `expression/computed` — `experimentalComputedMemberExpression` isn't forwarded by `transformSync`, so it throws via the seam.

Not covered yet: **0d.1** — render through `@lwc/engine-dom` (jsdom) and compare DOM/vnode output. Next: js / ssr suites.

## JS suite (Phase 0d v1) — `test/js.behavior.test.ts`
Walks 173 `@lwc/babel-plugin-component` fixtures (`**/actual.js` + `config.json`; richest JS corpus — `@lwc/compiler`
has only `options.spec.ts`). Compiles via the backend as `test.js` with `{namespace:'lwc', name:'test', enablePrivateMethods:true, ...config}`
(same base as upstream index.spec, which always runs the private-method transforms); `parserOpts.errorRecovery` → `experimentalErrorRecoveryMode`.
Asserts:
- outcome: non-empty `error.json` or `lwcErrors.json` ⇒ must throw; every `LWCxxxx` code in the fixture must appear in the thrown error (code / aggregate `errors[]` / message)
- success: `src/evalJs.ts` evaluates the ESM (lowered to CJS by `@babel/plugin-transform-modules-commonjs` as a harness-side eval aid only) with stubs:
  `lwc` → `LightningElement` base + recording `registerComponent` / `registerDecorators`; other imports → recording proxies; undeclared free identifiers (e.g. `mixin`) → identity.
  Golden records imports, lwc imports, export names+kinds, default export (class? extends LightningElement? registered? `sel`, apiVersion type, tmpl, feature flag, synthetic internals),
  `registerDecorators` metadata shape, and warning code+level (transformSync currently returns none for JS).
- goldens: `goldens/js/*.json` (JS oracle). `UPDATE_GOLDENS=1` with `LWC_COMPILER_BACKEND=rust` throws at load.
- seam gaps (`SEAM_GAPS`): `dynamic-imports/passthough-with-no-config` — transformSync defaults `dynamicImports.strictSpecifier=true` (plugin default off) ⇒ LWC1121 throw via the seam.

Not covered yet: **0d.2** — engine event smoke: instantiate the registered class via `@lwc/engine-dom` (jsdom), mount, and check
public props / `@wire` / `@track` reactivity and lifecycle callbacks.

## SSR suite (Phase 0d v1) — `test/ssr.behavior.test.ts`
Walks 387 `@lwc/ssr-compiler` fixtures (`**/config.json`; each has `modules/<ns>/<name>/*.{js,html,css}`, 1263 module files).
Every module file is compiled individually through the backend with `{targetSSR:true, ssrMode:'sync', namespace, name,
enableDynamicComponents, enableLwcOn, experimentalDynamicDirective, dynamicImports:{strictSpecifier:false}}` (+ `experimentalComplexExpressions`
from config) — the per-file equivalent of upstream `fixtures.spec.ts`'s rollup options. Asserts:
- outcome: fixture error file (`ssrFiles.error` ?? `error.txt`) starting `LWCxxxx:` ⇒ some module must throw with that code;
  a non-coded error text that matches a thrown compile message (inner-outer-html) ⇒ accepted compile error; otherwise every module must compile.
  Remaining non-empty error files are render-time errors (golden flag `renderErrorDeferred`, 0d.3).
- success: `src/evalSsr.ts` evaluates the ESM (CJS-lowered, harness-only) with `@lwc/ssr-runtime` stubbed (LightningElement base,
  recording `setStaticInternals` / `registerTemplate`), css → undefined, other imports → stub proxies. Golden records imports, touched runtime
  names, export kinds, default kind, `extends LightningElement`, static internals (tagName, public props, ssrMode, has tmpl),
  template registered + `stylesheetScopeToken` / `hasScopedStylesheets`, and warning code+level. `.html` must export a registered function.
- goldens: `goldens/ssr/*.json` (JS oracle). `UPDATE_GOLDENS=1` with `LWC_COMPILER_BACKEND=rust` throws at load. `SEAM_GAPS`: none.

Not covered yet: **0d.3** — HTML render compare: needs module resolution/bundling (rollup + `@lwc/rollup-plugin` or a harness resolver over
backend output) then `serverSideRenderComponent` + `formatHTML` vs `expected.html` / `expected-ssr.html`, including the 18 render-time error fixtures.

## Shim registry (tagged SHIM tracking)
Every `SHIM(<tag>)` anywhere in the repo (`git ls-files`, not just `crates/**/*.rs`) must have
a matching row in `shim-registry.tsv` (`path` repo-relative, exact tag text, owner, expiry,
reason). A row whose path+tag no longer occurs is `STALE` and fails the gate — delete it in
the same commit that removes the shim. `TODO(parity` tags stay count-only (cutover blockers)
and are not registered. The untagged-word scan stays crates-only.

**Expiry is not machine-evaluated.** `shim-gate.mjs` records it; enforcement happens at the
named stage's review (`S<n>`, an `AB-...` id, or a `D-NNNN` decision). Owner must be a lane
from **MAIN's** `OWNERS` file (land.sh passes `SHIM_OWNERS=$MAIN/OWNERS`); a branch that adds
a lane or deletes `OWNERS` has no effect. Missing, empty, or malformed MAIN `OWNERS` exits 2.

Env: `SHIM_REGISTRY` / `SHIM_ALLOWLIST` / `CRATES_ROOT` = branch inputs; `SHIM_OWNERS` /
`SHIM_REPO_ROOT` = MAIN's OWNERS path and the scanned tree. Internal errors (missing registry,
bad `CRATES_ROOT`, …) exit 2; `UNTRACKED` / `STALE` / `INVALID` exit 1.

## Official board (pinned binary)
- Install napi only via `../scripts/install-napi.sh` (tmp + `mv`, atomic). Never `cp` over `lwc-compiler-napi.node` — a live vitest fork may have it mmap'd (SIGBUS).
- Measure only via `./board.sh [suite...]`: snapshots the current `.node` to `.pins/lwc-compiler-napi.<sha12>.node` (read-only), runs every suite against that one file via `LWC_COMPILER_NAPI_PATH`, `--pool=threads`, and prints the hash. Board numbers without a hash are not official.

## Board ratchet (AB-0006): per-case baseline, equality gate, Baseline-Raise, Case-Set
- `harness/board-baseline.tsv` (`board-baseline/v2`) holds, per vitest row, a `counts` line (`passed failed skipped`) and
  the SET of passing, failing and skipped case ids (vitest fullName within the row's `-t` filter). Those three sets are
  the row's case universe (D-0255). template-diag* rows record only `crash oracle-error` (clean/mismatch from the
  closed enum {STATIC_EQUAL, REWRITE_EQUAL} are not in this file; D-0219). Parser and comparator:
  `harness/scripts/board-baseline.mjs` (one implementation; `--self-test`). Every number in the file gates.
- `board.sh` fails when any row's measured set differs from the committed one in EITHER direction. `board.sh check` is
  the same full run (no row arguments, `BOARD_RATCHET_UPDATE` refused). `--root=<tree>` runs that tree's tests, fixtures,
  crates and napi; the runner, obs writing, row list, comparator and pinned `vitest.board.config.ts` stay with this script. land.sh runs
  `$MAIN_TREE/harness/board.sh check --root=$wt` so the branch's `board.sh` is never the measurement producer.
  `BOARD_RATCHET_UPDATE=1 harness/board.sh` (full run, rust backend; refused on partial runs and on `check`, unset by
  official-board.sh) rewrites the file only if no row got worse.
- `land.sh` runs `scripts/baseline-ceiling.sh <repo> <pre-land-sha> <branch-sha>`: both sides are `git ls-tree` +
  `git show <sha>:harness/board-baseline.tsv` (a symlink / gitlink / missing-or-renamed branch blob is rc 2 and can
  never be accepted via a raise). A row is worse if it gains a failing or skipped id, loses passed, drops its total,
  drops ids from the failing/skipped sets without passed rising by at least as many, shrinks its case universe, or (for
  template-diag*) raises crash or oracle-error. A worse row lands only with `Baseline-Raise: <AB-id>` whose last
  `reviews.tsv` row is APPROVE with the landing's patch_id.
- A case-set change (an id that disappears from a row's passed+failed+skipped universe, or that becomes newly skipped)
  also needs a `Case-Set: D-NNNN` trailer. land.sh captures the notes HEAD sha once at the start of land and uses that
  sha for every `git show`, `blame` and pickaxe (never a later notes HEAD, never a working tree). Format: a
  tab-separated field that *starts with* `case-set:<row>:` followed by `|<id>`-separated tokens from the id charset
  (`[-A-Za-z0-9_ .:/=#@',()+[\]]`), running to the end of the field. Example:
  `case-set:js-eval: js-eval behavioral private-field-proxy#0|js-eval behavioral private-field-proxy#1`.
  `foocase-set:js-eval: x` (the token not at the start of a field) is refused with rc 2. A bare `case-set:<row>`
  token without a colon-and-list does not grant permission. The removed and newly skipped ids for that board row must
  be a subset of the ids listed in ONE such row (lists are not unioned across D-ids). If notes are unavailable, land
  exits rc 2 (not die/rc 1).
- D-0261 / notes `b0b4450` (`bin/d-id`): only migrate may mint or edit a `case-set:` row (`NOTES_COMMIT_BOT=migrate`,
  and `bin/d-id migrate ...`). land.sh additionally requires that the cited line was ADDED by a notes commit whose
  author is exactly `migrate <migrate@lwc-bots>` (`git log --reverse -G '^D-NNNN\t'`, line-anchored; a mention of the
  string in another row is not a mint) and that `git blame` of that line at the pinned notes sha reports the same
  author (a later rewrite by anyone else is refused; a missing author is refused).
- board.sh runs each vitest child under `setsid` and `env -u BOARD_LOG_DIR` (no board output path in the child
  environment), kills the process group after exit (rc 2 if any member survives), then copies stdout and the JSON
  report into a `mktemp -d` created after the child has exited (a path it never saw) and derives obs only there.
  Each obs file is copied into a private `mktemp` dir created after the child has exited (a path it never saw)
  and hashed there; `judge` and `update` read that private dir. `$logdir/obs` is only a published copy written
  from it. Both the private bytes and the published copy are re-checked against the derivation-time hashes
  before judge and before return, and `SHA256SUMS` is written from the variables (not by re-hashing files)
  so `judge-all` can verify them. A same-count ID swap written into `obs/` after `ids()` is refused.
  Alternating the published copy between the derivation-time bytes and a forged (green) claim cannot
  produce a green or forged verdict: `judge` reads the private dir, so the outcome is either an
  `obs sha256 mismatch` or the true ratchet result.
  Vitest is invoked with `--config <this-tree>/vitest.board.config.ts` (no plugins; closed `setupFiles` /
  `globalSetup` lists; `envDir` set so a branch `harness/.env` is not loaded). Branch edits to that config or to
  harness setup / globalSetup files trigger the land.sh self-test runner (`scripts/test-land-classify.sh`).
- Residual: branch test code is arbitrary code. A writer that starts a *new session* (`detached: true` / `setsid`)
  can escape the vitest process group; the pgid check is kept but cannot see that session. There is no `bwrap` or
  cgroup sandbox. `harness/test/**` and `harness/src/**` also trigger that land self-test runner.
- Case-Set lists: a trailing `|`, an empty or space-only list, or duplicate ids is rc 2 (tokenization does not use
  `IFS='|' read`, which drops a trailing empty field).
- After the napi release build and before the fast-forward, `land.sh` runs `$MAIN_TREE/harness/board.sh check --root=$wt`
  and ratchets that MEASURED run against the pre-land blob (D-0255). The committed baseline must equal the measurement.
- A pre-land commit that lacks `harness/board-baseline.tsv` is rc 2 (never raise-acceptable). The seed has
  landed; a missing pre-land baseline is no longer treated as NEW. `land.sh` does not grep `$MAIN`'s
  `baseline-ceiling.sh` for a leftover seed marker (D-0269): that probe refused every land, including the
  follow-up that deletes the marker, because only a land can change MAIN.
- `scripts/test-board-ratchet.sh` is the end-to-end proof (real `board.sh` on js-eval, `--root` mutation, F1–F6, the
  land.sh block verbatim, Case-Set subset + migrate authorship, and the ceiling EXIT-trap order).

- Updating `land.sh`'s main-chokepoint (`# >>> main-chokepoint` … `# <<< main-chokepoint`) is circular if you
  try to land a new body and a new `REGION_SHA256` in one commit: MAIN's checker pins the *current* body, so
  the new body fails `--check`, but changing only the table first also fails. A table-only land that rewrites
  `REGION_SHA256` to the not-yet-landed body fails the branch `--self-test` (that compares the *branch* table
  to the *branch* `land.sh`, which still has the old body). A table-only land that rewrites `REGION_SHA256` to
  a hash of the old body is a no-op. Use the two-step `NEXT_REGION_SHA256` path instead (or a D-id break-glass
  if you must land without the pin):
  1. Land a table-only commit that *adds* one `NEXT_REGION_SHA256` row whose digest is the sha256 of the new
     chokepoint body (markers excluded). Leave the body and `REGION_SHA256` unchanged. MAIN's checker accepts
     a body matching `REGION_SHA256` *or* `NEXT_REGION_SHA256`, so this land's body still matches `REGION`.
     The branch `--self-test` still matches `REGION` too.
  2. Land the new body together with a table edit that promotes `NEXT_REGION_SHA256` into `REGION_SHA256` and
     deletes the `NEXT` row. MAIN's table (from step 1) still has `REGION`=old and `NEXT`=new, so the new body
     matches `NEXT` and the land gate passes. The branch table now has `REGION`=new, so `--self-test` matches
     the new body.
  Do not leave a `NEXT_REGION_SHA256` row on main after step 2. The checker rejects more than one `NEXT` row.
  A body matching neither digest fails closed. A leftover NEXT whose digest equals this tree's body is an
  unpromoted step 2: `--self-test` and `--check` of the incoming table fail until that NEXT is promoted into
  `REGION_SHA256` and deleted. Step 1 (NEXT names a future body, so it differs from the current body) still
  passes. Production `--check` still reads MAIN's table, so a step-2 body may match MAIN's NEXT; the incoming
  table must not still carry that same NEXT.
