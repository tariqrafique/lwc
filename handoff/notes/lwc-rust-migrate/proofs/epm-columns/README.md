# enablePrivateMethods error positions (shipped 9.5.3 vs Rust lwc-js)

Cases (`cases.json`) put an LWC error on a line that also contains a renamed `#m`:
- `before`: `#m` comes before the error on the line;
- `after`: `#m` comes after the error;
- `use_before`: a `this.#m()` use appears before the error;
- `multiline`: a get/set pair spanning lines that also contain `#m`;
- `deco_misuse`: decorator misuse on a line that also has `#m`.

All cases run with `enablePrivateMethods: true`. Run `node shipped.mjs > shipped.json` (oracle `/workspace/oracle953`),
then `node rust.mjs > rust.json` (`target/debug/examples/errdump epm`), then `node compare.mjs`.

**Before the fix**, Rust reported offsets in the renamed (`__lwc_component_class_internal_private_m`) text:
| case | shipped start | Rust start |
|---|---|---|
| before | 126 | 164 |
| use_before | 143 | 219 |
| multiline | 168 (column 32) | 282 (column 70) |
| deco_misuse | 118 | 194 |

The frame was cut from the renamed source. Only `after` matched.

**Fix** (`crates/lwc-js/src/lib.rs`): the private-method rename's pieces are kept, and `unrename_locations` maps every
diagnostic's start and end back through them onto the original source. Line and column are then recomputed with
`lwc_shared::babel_line_col`, and the code frame is cut from the original source.

**After the fix:** all 5 cases match on code, `location.{line,column,start,length}` and message+frame.
The Rust test is `crates/lwc-js/tests/error_shapes.rs::epm_error_locations_use_original_source`.
