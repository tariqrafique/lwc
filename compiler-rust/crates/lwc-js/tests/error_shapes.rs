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
    // Shipped: "SyntaxError: LWC1007: /abs/dir/foo.js: Unexpected token (2:6)", location {line 2, column 6}.
    let e = err("export default class Foo {\n  x = ;\n}\n", |_| {});
    assert!(e.message.starts_with(&format!("SyntaxError: LWC1007: {F}: ")), "{}", e.message);
    assert!(e.message.ends_with(" (2:6)"), "{}", e.message);
    assert_eq!((code(&e), e.line, e.column), (Some(1007), Some(2), Some(6)));
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
