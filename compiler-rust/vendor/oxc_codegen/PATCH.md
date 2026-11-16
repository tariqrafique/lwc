# oxc_codegen 0.156.0, vendored

Pinned oxc is 0.156.0 (`compiler-rust/Cargo.toml`, `oxc_codegen` checksum
`88c1b6d7d9f1f5386534d88294903dadf00e72536e9fea6198e43450a52c41a5`).

`NumericLiteral`, `StringLiteral`, and `BigIntLiteral` each carry `raw: Option<Str>`
("None when this ast node is not constructed from the parser"). There is no
codegen option that prints it. `NumericLiteral` prints `raw` only under
`Context::TYPESCRIPT`. Otherwise numbers go through `print_non_negative_float`
(`oxc_ecmascript::with_number_literal`, the shortest spelling), strings are
re-quoted from the decoded value (`str.rs`), and BigInts print `value + "n"`.

This crate is that 0.156.0 source plus one option, `preserve_raw_literals`
(default `false`, so a caller that does not set it keeps upstream behavior):

- When the option is on, minify is off, and `raw` is present, number, string,
  and BigInt literals print that source text.
- Minify stays the shortest form. `lwc-js` unforgeables uses minify only to
  compare two expressions, not to emit them.
- Synthesized nodes (`raw: None`) keep the shortest number and the re-quoted
  string. Strings synthesized by LWC still go through `lwc_shared::js::string_literal`
  (D-0243) at the construction site; this printer is not a second quoter for them.

`lwc-js` sets the option on its module printer (`crates/lwc-js/src/codegen.rs`)
and on the unforgeables expression emission. The template and SSR expression
printers already print `raw` themselves and do not go through this crate.
