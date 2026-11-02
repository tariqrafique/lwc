//! Error shapes vs shipped 9.5.3 `transformSync(src, "/abs/dir/foo.js", { name: "foo", namespace: "x" })`
//! (oracle: /tmp/lwc953/run/errshape.mjs). Shipped thrown error: name `Error`, `code`, `location` {line 1-based,
//! column 0-based, start, length}; message = Babel first line + `\n` + `@babel/code-frame` frame, copied from the oracle.
use lwc_shared::{CompileError, CompileOptions};

const F: &str = "/abs/dir/foo.js";

fn err(src: &str, f: impl FnOnce(&mut CompileOptions)) -> CompileError {
    let mut o = CompileOptions::default();
    o.name = "foo".into();
    o.namespace = "x".into();
    f(&mut o);
    lwc_js::transform(src, F, &o).expect_err("should fail")
}

fn code(e: &CompileError) -> Option<u32> { e.diagnostics.first().and_then(|d| d.code) }

/// Shipped `location.start` / `location.length`.
fn start_len(e: &CompileError) -> Option<(Option<u32>, Option<u32>)> {
    e.diagnostics.first().and_then(|d| d.js_location()).map(|l| (l.start, l.length))
}

const API: &str = "import { LightningElement, api } from 'lwc';\nexport default class Foo extends LightningElement {\n    @api\n    get a() { return 1; }\n    @api\n    set a(v) {}\n}\n";
const WIRE: &str = "import { wire, LightningElement } from 'lwc';\nexport default class Test extends LightningElement {\n    @wire('not-an-identifier')\n    wiredFoo;\n}\n";
const DECO: &str = "import { LightningElement, track } from 'lwc';\nexport default class Foo extends LightningElement {\n  @track @api x;\n}\n";
const PRIV: &str = "import { LightningElement } from 'lwc';\nexport default class Foo extends LightningElement {\n  #m() {}\n}\n";

#[test]
fn api_error_babel_frame() {
    let e = err(API, |_| {});
    assert_eq!(e.message, format!("SyntaxError: {F}: LWC1112: @api get a and @api set a detected in class declaration. Only one of the two needs to be decorated with @api.\n  3 |     @api\n  4 |     get a() {{ return 1; }}\n> 5 |     @api\n    |     ^\n  6 |     set a(v) {{}}\n  7 | }}\n  8 |"));
    assert_eq!(start_len(&e), Some((Some(136), Some(20))));
    assert_eq!((code(&e), e.line, e.column, e.name.as_deref()), (Some(1112), Some(5), Some(4), None));
}

#[test]
fn wire_error_babel_frame() {
    let e = err(WIRE, |_| {});
    assert_eq!(e.message, format!("SyntaxError: {F}: LWC1097: @wire expects a function identifier as first parameter.\n  1 | import {{ wire, LightningElement }} from 'lwc';\n  2 | export default class Test extends LightningElement {{\n> 3 |     @wire('not-an-identifier')\n    |           ^^^^^^^^^^^^^^^^^^^\n  4 |     wiredFoo;\n  5 | }}\n  6 |"));
    assert_eq!(start_len(&e), Some((Some(109), Some(19))));
    assert_eq!((code(&e), e.line, e.column, e.name.as_deref()), (Some(1097), Some(3), Some(10), None));
}

#[test]
fn decorator_misuse_babel_frame() {
    let e = err(DECO, |_| {});
    assert_eq!(e.message, format!("SyntaxError: {F}: LWC1102: Invalid 'api' decorator usage. Supported decorators (api, wire, track) should be imported from \"lwc\"\n  1 | import {{ LightningElement, track }} from 'lwc';\n  2 | export default class Foo extends LightningElement {{\n> 3 |   @track @api x;\n    |   ^^^^^^^^^^^^^^\n  4 | }}\n  5 |"));
    assert_eq!(start_len(&e).map(|s| s.1), Some(Some(14)));
    assert_eq!((code(&e), e.line, e.column, e.name.as_deref()), (Some(1102), Some(3), Some(2), None));
}

#[test]
fn private_method_lwc1007_babel_internal_shape() {
    // Babel class-features error: code first, then filename; no location; same in recovery mode (not aggregated).
    for rec in [false, true] {
        let e = err(PRIV, |o| o.experimental_error_recovery_mode = rec);
        assert_eq!(e.message, format!("SyntaxError: LWC1007: {F}: Class private methods are not enabled. Please add `@babel/plugin-transform-private-methods` to your configuration."));
        assert_eq!((code(&e), e.line, e.column, e.name.as_deref()), (Some(1007), None, None, None));
    }
}

#[test]
fn recovery_mode_aggregates_lwc_errors_unframed() {
    // Shipped: CompilerAggregateError, message "Multiple errors occurred during compilation.", no location;
    // sub-errors keep the bare `LWC####: msg` text (no SyntaxError/filename prefix).
    let e = err(DECO, |o| o.experimental_error_recovery_mode = true);
    assert_eq!(e.name.as_deref(), Some("AggregateError"));
    assert_eq!((e.line, e.column), (None, None));
    let mut lines = e.message.lines();
    assert_eq!(lines.next(), Some("Multiple errors occurred during compilation."));
    assert_eq!(lines.next(), Some("LWC1102: Invalid 'api' decorator usage. Supported decorators (api, wire, track) should be imported from \"lwc\""));
    assert_eq!(lines.next(), Some("LWC1093: @api method or property cannot be used with @track"));
    let codes: Vec<_> = e.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![Some(1102), Some(1093)]);
}

#[test]
fn parse_error_babel_shape_and_location() {
    // Shipped: message = Babel parser message + `\n\n` + start-only code frame; location = Babel
    // Position {line: 2, column: 6, index: 33} (stored zero-width: start 33, length 0).
    let e = err("export default class Foo {\n  x = ;\n}\n", |_| {});
    assert_eq!(e.message, format!("SyntaxError: LWC1007: {F}: Unexpected token (2:6)\n\n  1 | export default class Foo {{\n> 2 |   x = ;\n    |       ^\n  3 | }}\n  4 |"));
    assert_eq!((code(&e), e.line, e.column), (Some(1007), Some(2), Some(6)));
    assert_eq!(start_len(&e), Some((Some(33), Some(0))));
}

#[test]
fn decorated_private_method_without_epm_is_lwc1103() {
    // Shipped (enablePrivateMethods omitted/false): LWC1103 at the member, before class-features' LWC1007.
    let src = "import { LightningElement, api } from 'lwc';\nexport default class Test extends LightningElement {\n    @api #decorated() {\n        return 1;\n    }\n}\n";
    let e = err(src, |_| {});
    assert_eq!(e.message, format!("SyntaxError: {F}: LWC1103: \"@api\" can only be applied on class properties\n  1 | import {{ LightningElement, api }} from 'lwc';\n  2 | export default class Test extends LightningElement {{\n> 3 |     @api #decorated() {{\n    |     ^\n  4 |         return 1;\n  5 |     }}\n  6 | }}"));
    assert_eq!((code(&e), e.line, e.column, start_len(&e)), (Some(1103), Some(3), Some(4), Some((Some(102), Some(43)))));
    let e = err(src, |o| o.enable_private_methods = true);
    assert_eq!((code(&e), start_len(&e)), (Some(1212), Some((Some(102), Some(43)))));
}

/// enablePrivateMethods: decorator errors on a line with a renamed `#m` report original-source positions
/// (shipped oracle: /workspace/notes/lwc-rust-migrate/proofs/epm-columns/shipped.json).
#[test]
fn epm_error_locations_use_original_source() {
    let cases: [(&str, &str, u32, u32, u32, u32, u32); 5] = [
        ("import { wire, LightningElement } from 'lwc';\nexport default class Test extends LightningElement {\n  #m() { return 1; } @wire('not-an-identifier') w;\n}\n", "SyntaxError: {F}: LWC1097: @wire expects a function identifier as first parameter.\n  1 | import { wire, LightningElement } from 'lwc';\n  2 | export default class Test extends LightningElement {\n> 3 |   #m() { return 1; } @wire('not-an-identifier') w;\n    |                            ^^^^^^^^^^^^^^^^^^^\n  4 | }\n  5 |", 1097, 3, 27, 126, 19),
        ("import { wire, LightningElement } from 'lwc';\nexport default class Test extends LightningElement {\n  @wire('not-an-identifier') w; #m() { return 1; }\n}\n", "SyntaxError: {F}: LWC1097: @wire expects a function identifier as first parameter.\n  1 | import { wire, LightningElement } from 'lwc';\n  2 | export default class Test extends LightningElement {\n> 3 |   @wire('not-an-identifier') w; #m() { return 1; }\n    |         ^^^^^^^^^^^^^^^^^^^\n  4 | }\n  5 |", 1097, 3, 8, 107, 19),
        ("import { wire, LightningElement } from 'lwc';\nexport default class Test extends LightningElement {\n  #m() { return 1; }\n  x = this.#m(); @wire('not-an-identifier') w;\n}\n", "SyntaxError: {F}: LWC1097: @wire expects a function identifier as first parameter.\n  2 | export default class Test extends LightningElement {\n  3 |   #m() { return 1; }\n> 4 |   x = this.#m(); @wire('not-an-identifier') w;\n    |                        ^^^^^^^^^^^^^^^^^^^\n  5 | }\n  6 |", 1097, 4, 23, 143, 19),
        ("import { api, LightningElement } from 'lwc';\nexport default class Test extends LightningElement {\n  #m() { return 1; } @api get a() {\n    return this.#m(); }\n  #n() {} @api set a(v) {\n    this.#n(); }\n}\n", "SyntaxError: {F}: LWC1112: @api get a and @api set a detected in class declaration. Only one of the two needs to be decorated with @api.\n  3 |   #m() { return 1; } @api get a() {\n  4 |     return this.#m(); }\n> 5 |   #n() {} @api set a(v) {\n    |           ^\n  6 |     this.#n(); }\n  7 | }\n  8 |", 1112, 5, 10, 168, 32),
        ("import { LightningElement, track } from 'lwc';\nexport default class Test extends LightningElement {\n  #m() {} #n() {} @track @api x;\n}\n", "SyntaxError: {F}: LWC1102: Invalid 'api' decorator usage. Supported decorators (api, wire, track) should be imported from \"lwc\"\n  1 | import { LightningElement, track } from 'lwc';\n  2 | export default class Test extends LightningElement {\n> 3 |   #m() {} #n() {} @track @api x;\n    |                   ^^^^^^^^^^^^^^\n  4 | }\n  5 |", 1102, 3, 18, 118, 14),
    ];
    for (src, msg, c, line, col, start, len) in cases {
        let e = err(src, |o| o.enable_private_methods = true);
        assert_eq!(e.message, msg.replace("{F}", F), "{src}");
        assert_eq!((code(&e), e.line, e.column), (Some(c), Some(line), Some(col)), "{src}");
        assert_eq!(start_len(&e), Some((Some(start), Some(len))), "{src}");
    }
}

/// (code, start, length) of every diagnostic.
fn all_locs(e: &CompileError) -> Vec<(Option<u32>, Option<u32>, Option<u32>)> {
    e.diagnostics.iter().map(|d| { let l = d.js_location(); (d.code, l.and_then(|l| l.start), l.and_then(|l| l.length)) }).collect()
}

#[test]
fn decorator_import_usage_matches_shipped_babel_references() {
    // Shipped validateImportedLwcDecoratorUsage: scope-resolved references, specifier order then source order;
    // LWC1104 at the reference's grandparent, then LWC1103 at the decorator's parent (Program -> 0/0 after a
    // trailing newline, whole file without one). Values from proofs/recovery-errors/usage/shipped.json.
    let rec = |o: &mut CompileOptions| o.experimental_error_recovery_mode = true;
    let two_refs = "import { api, track, LightningElement } from 'lwc';\nconst b = track;\nconst a = api;\nexport default class Test extends LightningElement {\n  @api onBar;\n}\n";
    assert_eq!(all_locs(&err(two_refs, rec)), vec![
        (Some(1104), Some(69), Some(14)), (Some(1103), Some(0), Some(0)),
        (Some(1104), Some(52), Some(16)), (Some(1103), Some(0), Some(0)), (Some(1108), Some(139), Some(11))]);
    assert_eq!(all_locs(&err(two_refs, |_| {})), vec![(Some(1104), Some(69), Some(14))]);
    let no_nl = "import { api, track, wire, LightningElement } from \"lwc\";\nimport { getFoo } from \"x/foo\";\nexport default track";
    assert_eq!(all_locs(&err(no_nl, rec)), vec![(Some(1104), Some(0), Some(110)), (Some(1103), Some(0), Some(110))]);
    let private_field = "import { track, LightningElement } from 'lwc';\nexport default class Test extends LightningElement {\n  @track #x = 1;\n}\n";
    assert_eq!(all_locs(&err(private_field, rec)), vec![(Some(1103), Some(102), Some(14)), (Some(1101), Some(102), Some(14)), (Some(1113), Some(102), Some(6))]);
    let class_deco = "import { track, LightningElement } from 'lwc';\n@track\nexport default class Test extends LightningElement {\n}\n";
    assert_eq!(all_locs(&err(class_deco, |_| {})), vec![(Some(1103), Some(47), Some(61))]);
    // Not errors in shipped: a shadowing parameter, and a decorator import used inside a decorator's arguments.
    let mut o = CompileOptions::default();
    o.name = "foo".into();
    o.namespace = "x".into();
    for ok in ["import { api, LightningElement } from 'lwc';\nfunction f(api) { return api; }\nexport default class Test extends LightningElement { @api x; }\n",
               "import { api, wire, LightningElement } from 'lwc';\nexport default class Test extends LightningElement {\n  @wire(api) w;\n}\n"] {
        assert!(lwc_js::transform(ok, F, &o).is_ok(), "{ok}");
    }
}

#[test]
fn recovery_errors_interleave_in_traversal_order() {
    // Shipped visits Class (decorator errors) and Import (dynamic-import errors) in source order; the
    // multi-error case from proofs/recovery-errors (shipped's duplicate LWC1096 for the `g` get/set pair dropped).
    let src = "import { api, track, wire, LightningElement } from 'lwc';\nimport { getRecord } from 'lightning/uiRecordApi';\nexport default class Test extends LightningElement {\n  @api dup;\n  @api dup;\n  @api data;\n  @track\n  m() {}\n  @wire('notAnId') w1;\n  @wire(getRecord, { a: 1 }) @api both;\n  @api get g() { return 1; }\n  @api set g(v) {}\n  load() { return import(this.x); }\n}\n";
    assert_eq!(all_locs(&err(src, |o| o.experimental_error_recovery_mode = true)), vec![
        (Some(1112), Some(311), Some(16)), (Some(1096), Some(176), Some(4)), (Some(1096), Some(164), Some(4)),
        (Some(1097), Some(225), Some(9)), (Some(1095), Some(242), Some(26)), (Some(1113), Some(201), Some(6)),
        (Some(1121), Some(353), Some(6))]);
    assert_eq!(code(&err(src, |_| {})), Some(1112));
}
