# In-repo harness (D-0280)

The harness (`harness/`, `scripts/board` gates, `tools/semdiff`) lives in this
repository and must run on any machine from a fresh clone. There is no GitHub
Actions workflow. Cloud-agent board runs on `agent/*` branches are advisory.
The official board on the box, against the local oracle `/workspace/lwc` @
`b9e8007f9810edb59596536b596f80ca6c0d3fab`, is the final gate before any
fast-forward onto `rust-compiler`. Only that board's numbers go to Tariq.

C8 (CUTOVER.md) now means: the same checks run on the box by the in-repo
harness as part of the official board. Proposed notes row (Worker cannot
notes-commit `lwc-rust-migrate/CUTOVER.md`; OWNERS are ship,migrate):

> C8 | Same checks run on the box by the in-repo harness as part of the
> official board (no GitHub Actions): fmt, clippy (shrink-only ratchet),
> `cargo check --workspace --locked`, test, napi build, ssr-compile-js-oracle
> gate against the packed napi (never `SSR_ORACLE_NAPI`, D-0226), tarball
> artifacts (no npm publish). Cloud-agent runs of `scripts/run-portable-board.sh`
> on `agent/*` branches are advisory. | D-0280;
> `scripts/official-board.sh` invokes `scripts/c8-checks.sh` and
> `harness/board.sh`. Deleted `.github/workflows/rust-ci.yml` (landed as
> eac1ae9). `cargo fmt --all --check` is reported but not gating until D-0235
> (known red on main 3ecff19). | [ ]

## Layout

| path | role |
|---|---|
| `harness/board.sh` | row producer (vitest suites, shim/option/golden-io gates, ratchet) |
| `harness/oracle.pin` | full 40-hex salesforce/lwc sha |
| `harness/scripts/oracle-root.mjs` (`.sh` wrappers, `.ts` re-export) | single resolver for `LWC_ORACLE`, `STYLE_CORPUS`, notes `decisions.tsv` |
| `scripts/fetch-oracle.sh` | clone/fetch the pin into a dest that is **never** `/workspace/lwc` |
| `scripts/fetch-style-corpus.sh` | fetch `lbc`, `lbc-recipes`, and `slds` (`@salesforce-ux/design-system@2.264.3`) from public origins; verify vs `external-inputs.lock` (hashes exclude `.git/`) |
| `scripts/board-footers.sh` | shared `style-parse-errors`, `csr-clusters[*]`, `style-corpus`, and `napi-css-errors` rows for official and portable boards |
| `harness/decisions.snapshot.tsv` | committed copy of notes `lwc-rust-migrate/decisions.tsv`; `scripts/refresh-decisions-snapshot.sh` refreshes it |
| `scripts/c8-checks.sh` | land-path-checks (incl. `cargo fmt --all --check`, D-0235, hard), clippy ratchet, cargo check/test `--locked`, napi-pack, ssr-oracle-gate |
| `scripts/run-board.sh` | single driver: golden-gate, C8, `harness/board.sh`, board-footers |
| `scripts/run-portable-board.sh` | cloud / fresh-clone entry: resolve inputs, then `run-board.sh` (does not publish) |
| `scripts/official-board.sh` | box official path: pin/verify inputs, archive, `run-board.sh`, publish |
| `scripts/{clippy-ratchet,ssr-oracle-gate,napi-pack}.sh`, `scripts/clippy-baseline.txt` | C8 helpers from the runner tree |

`.github/workflows/rust-ci.yml` is deleted. Nothing in the tree references it.

## Oracle fetch and pin

`scripts/fetch-oracle.sh [--build] [dest]`

- Pin: `harness/oracle.pin` = `b9e8007f9810edb59596536b596f80ca6c0d3fab`.
- Default dest: `${LWC_ORACLE:-<repo>/.oracle/lwc}` (`.oracle/` is gitignored).
- Refuses dest that `realpath -m`s to `/workspace/lwc` or anything under it,
  before any network. It never writes the box oracle.
- `git init` / `fetch --depth 1 https://github.com/salesforce/lwc <pin>` /
  detached checkout; require `HEAD == pin`.
- `--build`: `corepack yarn install --frozen-lockfile --ignore-scripts` and
  `yarn build`.
- `board.sh` requires a clean tree at that exact sha (`git status --porcelain
  --untracked-files=all`) and exports `LWC_ORACLE` / `LWC_ROOT`.

Static test imports use the `/workspace/lwc` Vite alias (remapped to
`$LWC_ORACLE`). `canonOraclePath` rewrites the real oracle prefix to the
`<ORACLE_ROOT>` token so a hardcoded box oracle path in compiler output is
not masked off-box. On the box, if `LWC_ORACLE` is unset and the box oracle
checkout exists, `oracle-root.mjs` uses that checkout.

## Box-only dependencies

| dep | box default | portable handling |
|---|---|---|
| salesforce/lwc @ pin | `/workspace/lwc` | `LWC_ORACLE` or `scripts/fetch-oracle.sh`; fail-closed if neither |
| `style-corpus/lbc` | `/workspace/style-corpus/lbc` | `scripts/fetch-style-corpus.sh` downloads `lightning-base-components@1.28.19-alpha` from the public npm registry and extracts it to `lbc/package/`. Hash in `external-inputs.lock` must match (fail closed). `STYLE_CORPUS_SOURCE` is an optional override, never required. package.json says MIT; `LICENSE.txt` is Salesforce Terms of Use (we fetch, we do not vendor). |
| `style-corpus/lbc-recipes` | `/workspace/style-corpus/lbc-recipes` | fetch `https://github.com/salesforce/base-components-recipes.git` @ `9c5f95d4e0cd5434a2b8373405e4f6c7657c88e8`. Hash excludes `.git/` (`036116865208`). LICENSE file MIT; package.json `CC0-1.0`. |
| `style-corpus/slds` | `/workspace/style-corpus/slds` | `scripts/fetch-style-corpus.sh` downloads `@salesforce-ux/design-system@2.264.3` and extracts it to `slds/package/`. Hash in `external-inputs.lock` must match (fail closed). CSS is the compiler input; `.scss` is not. |
| notes `lwc-rust-migrate/decisions.tsv` | `/workspace/notes/...` | Committed `harness/decisions.snapshot.tsv` + `harness/decisions.snapshot.notes-commit`. `notes_decisions_path()` uses live `/workspace/notes/...` when present and the committed snapshot only when notes are absent. `official-board.sh` archives live notes, `cmp`s them against the snapshot, and reads `harness/decisions.snapshot.notes-commit` (fail closed on drift; skip if the boarded commit predates the file). Refresh with `scripts/refresh-decisions-snapshot.sh` on the box. |
| parse5-rust | `/workspace/parse5-rust` (official-board still archives a local clone) | Unset `PARSE5_DIR` means `/workspace/parse5-rust` and fails closed if absent. `run-portable-board.sh` sets `PARSE5_DIR=remote` to depth-1-fetch the lock rev from the public Cargo.toml URL. `land.sh` passes the box path explicitly. |
| `node_modules` / vitest | `harness/scripts/ensure-deps.sh` links into `$LWC_ORACLE` | same script; requires the oracle to be built (`compiler`, `engine-dom`, `engine-server`, `ssr-runtime` dist + vitest) |
| `/workspace/.cargo/config.toml` + parse5 url rewrite | required on the box (land / official cargo) | not required on a fresh clone: lock is a public git source |
| official store / `/workspace/push` | official-board publish | `run-portable-board.sh` never publishes |
| `land.sh` `MAIN=/workspace/lwc-compiler-rust` and `/workspace/.lwc-land.lock` | box-only path onto main | unchanged; cloud agents do not land |

## How official-board.sh invokes the in-repo harness

`scripts/official-board.sh` pins MAIN, NOTES, `LWC_ORACLE`, `PARSE5_DIR`, and
`EXT_INPUTS_ROOT` to the box defaults (an override is refused unless
`OFFICIAL_NO_PUBLISH=1`), archives the commit, verifies the src hash and
`external-inputs.lock` (format from the `# hash-format:` marker), then calls
`scripts/run-board.sh` — the same driver as the portable path. C8 helper
scripts come from the runner tree; cargo/clippy/pack run against the snapshot.
`run-board.sh` runs golden-gate, full C8, `harness/board.sh`, and
`board-footers.sh`.

## Cloud-run commands (Migrate C8 check)

From a fresh clone of this repo, on the same commit as the official board
being compared (here: `f7cc643`):

```bash
export LWC_ORACLE=$PWD/.oracle/lwc
export STYLE_CORPUS=$PWD/.oracle/style-corpus
# run-portable-board.sh sets PARSE5_DIR=remote.
# decisions come from /workspace/notes when present, else harness/decisions.snapshot.tsv.
scripts/fetch-oracle.sh --build
scripts/fetch-style-corpus.sh
scripts/run-portable-board.sh
```

`run-portable-board.sh` resolves inputs and calls `scripts/run-board.sh`
(the same function official-board uses) so the portable `board.txt` rows
come from one writer.
Compare the printed row lines to
`official/7ae940796f7b/board.txt` / `OFFICIAL_BOARD.txt`. Documented id
fields that will differ: `napi sha256`, `pinned=`, `oracle_repo=`, timestamps,
and option-gate's `[decisions <path>]`. Gating row pass/fail/skip counts and
the three footer rows must match.

## rust-ci.yml

Deleted. The S5 workflow copied onto `tariqrafique/lwc` `rust-compiler` is
retired (D-0280: no Actions, push routine must not copy workflows). C8 checks
moved into `scripts/c8-checks.sh` as invoked above.
