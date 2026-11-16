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

## Comments (D-0292)

Upstream prints only leading comments, re-indents multiline block comments, and
emits a single newline before a comment that the source separated with a blank
line. Babel's generator (`retainLines` off, comments on) does not:

- Trailing comments stay on their token (`a = 1; // trail`, `function f() {} // c`).
- Inner comments stay in the gap they came from: empty blocks, object and array
  tails, `x /* i */ = 1`, `x = /* i */ 1`, and a comment after the last statement.
- A block comment is the source text between its delimiters, tabs included.
  Re-indenting the interior is what moved the `layout` comment in
  rlmCollectionsConsole.
- A leading comment keeps one blank line when the source had one (`stmt;\n\n// c`).
  The extra line is read from the source span, not from `preceded_by_newline`
  (that flag is only "not on the same line").

`build_comments` stores trailing normal comments at `attached_to`. Statement
semicolons, function and class ends, blocks, objects, arrays, calls, and
assignments print the comment whose span falls in that gap. Class-field
comments are not a printer concern: `class_properties` moves them the way
Babel's `inheritPropComments` does, and this printer then prints the result.

Template and SSR expression printers rebuild user expressions that the
template validator has already rejected if they contain a comment (`LWC1197`),
so they never reprint a user comment. SSR component JS is edited in place
rather than reprinted through this crate.
