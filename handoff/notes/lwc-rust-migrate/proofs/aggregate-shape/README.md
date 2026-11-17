# Recovery AggregateError shape: shipped 9.5.3 vs napi (Phase A)

- `dump-shipped.mjs` → `shipped-shape.json`: every own property of shipped's thrown aggregate and its `errors[]` entries.
- `make-fixture.mjs` → `crates/lwc-compiler-napi/tests/fixtures/aggregate-shipped.json` (3 fixtures pinned for the seam test).
- `make-urls.mjs` → `crates/lwc-compiler-napi/src/error_urls.rs` (`errorInfo.url` per code; 39 of 190 codes have one).
- `napi-shape.mjs` → `napi-shape.out`: JS-visible keys of the installed napi error objects.

The shipped aggregate (prototype chain CompilerAggregateError → AggregateError → Error) has own keys
`message` and `errors` only. `name` comes from the prototype ('AggregateError'), and there is no own
`code`/`filename`/location, so `e.code === undefined`.
Each entry is a CompilerError: own `message`, `code`, `filename`, `location{line,column,start,length}`, `level`, `url`; `name` is 'Error' from the prototype.
- `message` is the unframed `LWC####: ...`.
- `filename` is Babel's `path.resolve(cwd, filename)`.
- `level` is always 1.
- `url` is the info url or ''.

Phase A napi: `errors[]` is identical to shipped. message/code are unchanged until `aggregate::AGGREGATE_EXACT_SHAPE` flips.
