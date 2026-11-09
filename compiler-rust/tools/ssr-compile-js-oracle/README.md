# ssr-compile-js-oracle — `@lwc/ssr-compiler` vs Rust compile_js, per case

Compiles each case in `cases-props.mjs` (public properties, `@api`/`@track`) and `cases-wire.mjs` (`@wire`) with
upstream `compileComponentForSSR` (`$LWC_ROOT/packages/@lwc/ssr-compiler/dist`, default `/workspace/lwc`) and with
the Rust napi this tree built: `${CARGO_TARGET_DIR:-<repo>/target}/release/liblwc_compiler_napi.so`, the output of
`cargo build -p lwc-compiler-napi --release` (land.sh and `scripts/install-napi.sh` both run it). `--napi=<path>`
names another build output (dev use). The old `SSR_ORACLE_NAPI` env override is retired: if it is set at all, even
empty or next to `--napi=`, run.mjs exits 2 with no verdict, so an inherited env var can never choose the binary a
gate judges (D-0226). It compares a per-suite extract:

- props: the `__setStaticInternals` public-property array and the `__registerPublicProperties` arrays, taken as
  acorn AST source slices (so nested arrays are not truncated) and compared byte-exact (D-0219, D-0256), plus
  leftover decorators.
- wire: the `__setStaticInternals(...)` call as an acorn token stream (no other normalization), the class members, leftover `@wire`, and every
  `instance["…"]` chain verbatim (the `$` path segments' `JSON.stringify` form).
- members: constructor params, `super(...)` args and lifecycle members, as acorn token streams.
- markup (`cases-markup.mjs`: `__setStaticInternals`, the template import, `__registerPublicProperties`) and
  dynamic (`cases-dynamic.mjs`: `dynamicImports.strictSpecifier` / `loader`): the whole output as a token stream.
  A case may be `{ src, dynamicImports?, expectAbsent? }`; upstream gets transformSync's defaults (`strictSpecifier: true`).
  `expectAbsent` is a closed set: `props`, `internals`, `sigs`, `reg`. An unknown name is exit 2. A listed name means
  both compilers omit that node; if it is present on either side, the run is exit 2 (the declaration is stale or false)
  and never a plain diff or a silent match. A missing expected node on either side is also exit 2. More than one
  `__setStaticInternals` or `__registerPublicProperties` call is exit 2 (the match must be unique).

Token streams compare string literals by cooked value (quote and escape spelling is not a diff; D-0196). That
decoded-value compare is a temporary second equivalence site under D-0219 and comes out when SD-2b's SSR semdiff
leg lands (D-0257). Token types stay distinct: a template literal is not a string, `{foo}`, `{"foo"}` and
`{["foo"]}` are different key forms, and `{foo}` (shorthand) is not `{foo: foo}`.
- errors: the first message line and its line:column, plus `+start,length` when the location has them.
  (LWC1121 `level` / `url` are not on napi's SSR-origin error, so they are pinned by the lwc-ssr unit test.)

```sh
export PATH=$HOME/.n/bin:$PATH
cargo build -p lwc-compiler-napi --release
node tools/ssr-compile-js-oracle/run.mjs           # 0 clean; 1 a new diff, a stale entry, or a failed $MAIN
                                                   #   check; 2 no verdict (napi or upstream build missing or
                                                   #   unloadable, SSR_ORACLE_NAPI set, $MAIN or
                                                   #   known-diffs.txt unreadable)
node tools/ssr-compile-js-oracle/run.mjs --main=origin/main   # compare the list against another ref (default main)
node tools/ssr-compile-js-oracle/run.mjs --root=/path/to/branch-worktree --main=<sha>
                                                   # intended land.sh invocation once D-0254/D-0226 are wired
node tools/ssr-compile-js-oracle/run.mjs --print   # every case, upstream and ours
node tools/ssr-compile-js-oracle/run.mjs --napi=/path/to/liblwc_compiler_napi.so   # a different build (dev)
tools/ssr-compile-js-oracle/self-test.sh           # every exit path above, in a scratch git repo, plus a
                                                   #   mutant run per negative (rule off => must go green),
                                                   #   nested-array / missing-node / token-type pairs;
                                                   #   prints `N checks: N ok, 0 failed`
```

Arguments are a closed set: `--root=<tree>`, `--main=<ref>`, `--napi=<path>`, `--print`, each at most once. An
unknown flag or a positional (`--roots=x`), a duplicate, an empty or missing value (`--root=`, `--main=`,
`--napi=`, bare `--root`), or `--print=<x>` is exit 2 with a usage line. Defaults (own tree, `main`, the root's
built napi) apply only when the flag is absent, never to an empty value.

The last line names the napi it loaded (`napi sha256:<12> <path>`). Any cwd works; a run takes about a second.

`known-diffs.txt` lists the cases that differ today, each with its reason. It only shrinks: a case that starts
matching must be deleted from it in the same commit, and an entry is added only by the LWC SSR Rust bot (lwc-ssr owner), which owns the list.

`--root=<tree>` (absent: the tree the script sits in) is the tree under test, like shim-gate's `CRATES_ROOT`: its
`tools/ssr-compile-js-oracle/known-diffs.txt` and `cases-*.mjs`, and its napi
(`${CARGO_TARGET_DIR:-<root>/target}/release/…`, `CARGO_TARGET_DIR` relative to the root), are what is judged, and
`$MAIN` comes from `git show` in the root's repo. Once D-0254 and D-0226 are wired, land.sh is meant to run
**main's** copy of run.mjs with `--root=<branch worktree>`, so the rules that judge a branch are main's: a branch
that edits its own run.mjs to switch a rule off changes nothing (self-test `root-guard`). That is how this tool is
meant to be invoked then, not what land.sh does today. The suites are the root's `cases-<suite>.mjs` files; one with
no extractor in this script is exit 2.

The shrink-only rule is mechanical (D-0253). run.mjs reads `$MAIN` (`--main=<ref>`, default `main`) with
`git show`, never from the working tree: `$MAIN`'s `known-diffs.txt`, and its case ids, taken from every
`cases-*.mjs` on `$MAIN`. The branch's and `$MAIN`'s case ids come from the same function (`<suite>/<key>`, suite =
the file name after `cases-`). It
exits 1, naming the ids, unless both hold:

- every known-diff id on the branch is a known-diff id on `$MAIN` (a new entry fails, and so does a swap that
  deletes one entry and adds another, though the count is unchanged);
- every case id on `$MAIN` still exists on the branch (deleting a case together with its entry is not a shrink).

A duplicate line also fails. Removing entries, and adding cases that match, needs nothing else. A new entry can only
come from a change that lands on `main` through Arch review with the entry in it; a branch run cannot add one.
