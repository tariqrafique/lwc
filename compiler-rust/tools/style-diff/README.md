# style-diff — JS ↔ Rust differential corpus for LWC style compilation

Compares `@lwc/style-compiler` (JS, `/workspace/lwc` dist) with `lwc_style::transform_with_config` (Rust)
over real component CSS from `@lwc/integration-{wtr,wdio,types}` (deduped by sha256, cap 200) plus the
style-compiler fixtures' `actual.css` as a smoke subset.

```sh
export PATH=$HOME/.n/bin:$PATH
node tools/style-diff/run.mjs      # from /workspace/lwc-compiler-rust; builds the Rust driver first
```

- Rust driver: `crates/lwc-style/examples/style_diff.rs` (JSONL jobs on stdin → JSONL results).
- Configs per file: default, scoped, dss, scoped+dss, erm, erm+scoped (`*.scoped.css` forces scoped; duplicate configs collapsed).
- Version comment normalized to `LWC compiler vX.X.X`. Byte-identical code = pass; both-error with identical
  {name,reason,line,column} (key order ignored) = pass. On byte mismatch the module is evaluated and
  `stylesheet(token, host, dir)` compared over 4 combos (reported as `semantic: equal|different`) — still a fail.
- Exit 1 on any failure. Optional `allowlist.json` (`[{file, config?, why}]`) — currently none.
- Env: `LWC_ROOT`, `STYLE_DIFF_CAP`, `STYLE_DIFF_OUT` (default `target/style-diff`), `STYLE_DIFF_NO_FIXTURES`, `STYLE_DIFF_NO_BUILD`.
- Full report: `target/style-diff/report.json`.
- Parse-error inputs (kind `parse-error`, every corpus mode; skip with `STYLE_DIFF_NO_PARSE_ERRORS=1`):
  `crates/lwc-style/tests/parse-errors/*/actual.css`, generated from the JS oracle by
  `node tools/style-diff/gen-parse-errors.mjs` (cases listed there). See
  `/workspace/notes/postcss-rust/07-error-parity.md`.
- Corpus mode `STYLE_DIFF_CORPUS=all|integration|slds` (default `all`). `slds` = SLDS + lightning-base-components CSS under
  `STYLE_CORPUS_ROOT` (default `/workspace/style-corpus`), cap `STYLE_DIFF_SLDS_CAP` (300), prioritizing @supports/@media/var(.
  See `/workspace/notes/postcss-rust/06-slds-differential-corpus.md`.
