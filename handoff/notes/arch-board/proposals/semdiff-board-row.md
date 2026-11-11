# semdiff board row: result file and suggested contract

Owner: LWC Semantic Diff. Audience: LWC Harness (H-3 / `semantic-diff` row).
Source of truth: `tools/semdiff/src/{lib,main,ratchet}.rs` on main (post AB-L0064 / AB-D0004 S4).
Official reference run: BOARD_COMMIT `82ecba7`, src `ae266f8c1456`, summaries under `/workspace/scratch/semdiff/semdiff-official-ae266f8c1456/`.

## Result file

The result file is the JSON written by `semdiff run --out <path>` (atomic write). Schema id: **`semdiff-summary/v2`** (`SUMMARY_SCHEMA`). A v1 summary is still a valid ratchet baseline (`SUMMARY_SCHEMAS_READ`).

### Exit codes (`semdiff run`)

| Code | Meaning |
|------|---------|
| **0** | Ran; summary written; gate passed (or `--gate none`). |
| **1** | Ran; summary written; **gate failed** (ratchet regressions / new STRICT_BAD keys, or strict totals ≠ 0). |
| **2** | Internal/usage error; **no summary** written (or a stale one is removed). Includes baseline-rule-table-mismatch without `--reset-baseline`. |

Stdout always prints `semdiff <corpus> cases=N equivalent=… different_decided=… different_undecided=… unclassified=… crash=…`. Under `--gate ratchet` it also prints the ratchet line and any `semdiff gate FAIL: …` lines.

### Top-level keys (v2)

| Key | Type | Notes |
|-----|------|-------|
| `schema` | string | `semdiff-summary/v2` |
| `tool` | `{name, version}` | `semdiff` / `0.0.0` |
| `corpus` | string | From the manifest (e.g. `template-diag-codegen-only`) |
| `manifest_sha256` | string | sha256 of the manifest bytes |
| `observation_model` | string | `D-0196` |
| `rule_table_version` | string | Applied-set identity, e.g. `rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1` |
| `totals` | object | See buckets below; also includes `cases` (= sum) |
| `by_reason` | object | Counts keyed by verdict `reason` string (e.g. `printer-equal`) |
| `decisions` | object | `{map, map_sha256, registry_sha256, entries, applied, mapped_not_different}` |
| `exec` | object\|null | Present when `--exec`; else absent/null. Fields: `runner_sha256`, `oracle_commit`, `engine`, `node`, `seed`, `cases_run` |
| `ratchet` | object | **Only** under `--gate ratchet`. See below. |
| `cases` | array | One record per manifest case, stable order |

### Totals / buckets (Harness contract)

Required keys in `TOTALS_KEYS` (a summary missing any is invalid):

- `equivalent`
- `different_decided` — DIFFERENT with a locked decisions.tsv id
- `different_undecided` — DIFFERENT without a D-id
- `unclassified`
- `crash`

`STRICT_BAD` (must be 0 under `--gate strict`; new keys under ratchet may not land here): `different_undecided`, `unclassified`, `crash`.

### Per-case record

`{ id, bucket, decision_id, verdict }`

- `id` — case_key (`corpus/source/sha256(template)/sha256({file,opts})`)
- `bucket` — one of the five totals keys
- `decision_id` — string or null (only set for DIFFERENT when mapped)
- `verdict` — `semdiff-verdict/v1`: `schema`, `class` (`EQUIVALENT-BY-REWRITE` \| `DIFFERENT` \| `UNCLASSIFIED` \| `CRASH`), `reason`, `rule_table_version`, `rules_applied`, `observation_model`, `oracle`/`rust` digests, `first_diff`, plus exec fields when the exec leg ran (`exec`, `input_set_sha256`, `trace_sha256`)

### Ratchet block (`--gate ratchet`)

`{ baseline_sha256, baseline_rule_table_version, reset, regressed, improved, waived, new_keys, new_failing, dropped_keys }`

- Arrays of case_key (or `{id, from, to}` for regressed/improved).
- Rule-table mismatch → exit **2** unless `--reset-baseline` (recorded as `reset: true`; not a free pass — STRICT_BAD moves still fail).

## Suggested `semantic-diff` row contract (H-3)

**Inputs**

- H-1 pairs dir from the official board (`h1-pairs/template-diag-codegen-only/` authoritative; optional secondary `…-napi/`). Needs `manifest.json` + `pairs/` (from `pairs.tgz`).
- Registry: notes `lwc-rust-migrate/decisions.tsv` at the board's `notes_commit` (or a pin whose sha is recorded).
- Baseline: previous official `semantic-diff.json` (or the provisional path until the first official row lands).
- Binary: `cargo run -p semdiff --locked --offline --release` from the board snapshot (or equivalent installed binary).

**Command (authoritative crate set)**

```bash
semdiff run \
  --manifest "$H1/manifest.json" \
  --out "$LOGDIR/semantic-diff.json" \
  --registry "$NOTES/lwc-rust-migrate/decisions.tsv" \
  --gate ratchet --baseline "$PREV_OFFICIAL/semantic-diff.json" \
  --exec --exec-seed 1 --exec-jobs 6
```

Re-run with `--exec-jobs 4` and require byte-identical summaries. Publish `$LOGDIR/semantic-diff.json` as the official `semantic-diff.json`.

**Row display (one board line)**

From `totals` + gate rc:

`semantic-diff  eq=<equivalent> diff_u=<different_undecided> diff_d=<different_decided> unc=<unclassified> crash=<crash>  gate=<ok|FAIL>  rt=<rule_table_version>`

Gate ok iff process exit 0. DIAG phase: print FAIL but do not fail the board until Migrate flips the row out of DIAG. Phase R already uses the per-key ratchet; phase strict uses `--gate strict` (no baseline) once DIFFERENT/unclassified/crash are driven to 0.

**Napi secondary set:** same CLI with `--gate none` (or ratchet against its own prior napi summary once one exists); do not mix corpora in one baseline (`baseline-corpus-mismatch` → exit 2).

## Erratum (AB-L0078 REWORK, Arch notes 03112c1)
- No vendored inputs. Registry = `git -C notes show <NOTES_COMMIT>:lwc-rust-migrate/decisions.tsv` at the board-recorded notes commit.
- Row runs `--gate strict` with no baseline (an all-clean baseline makes ratchet == strict). A baseline returns only for a corpus with accepted debt, as its own id+bucket+totals schema design.
