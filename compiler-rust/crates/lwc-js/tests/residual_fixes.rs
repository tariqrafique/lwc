//! Unit tests for two js-transform-residual fixes (ac19ae2ea). Expected text was checked against
//! the oracle `@lwc/compiler` 9.5.3 output for the same input (log: semdiff-negatives-plants.log).
//! - class_properties.rs: an anonymous class expression with instance fields reserves Babel's
//!   `_Class` uid even though no alias is emitted, so a later class expression with statics gets `_Class2`.
//! - a trailing `// eslint-...` or `// istanbul ...` stays on its statement, where Babel prints it.
use lwc_shared::CompileOptions;

fn code(src: &str) -> String {
    let o = CompileOptions {
        name: "foo".into(),
        namespace: "x".into(),
        ..Default::default()
    };
    lwc_js::transform(src, "foo.js", &o).unwrap().code
}

#[test]
fn instance_field_class_expression_reserves_the_class_uid() {
    let out = code(
        "export function f() { return class { x = 1; }; }\n\
         export function g() { return class { static y = 2; }; }\n",
    );
    assert!(out.contains("var _Class2;"), "{out}");
    assert!(
        out.contains("_Class2 = class {}, _Class2.y = 2, _Class2"),
        "{out}"
    );
    assert!(!out.contains("var _Class;"), "{out}");
}

#[test]
fn a_class_without_instance_fields_reserves_nothing() {
    let out = code(
        "export function f() { return class { m() {} }; }\n\
         export function g() { return class { static y = 2; }; }\n",
    );
    assert!(out.contains("var _Class;"), "{out}");
    assert!(!out.contains("_Class2"), "{out}");
}

#[test]
fn trailing_eslint_and_istanbul_directives_are_emitted() {
    let out = code(
        "const a = 1; // eslint-disable-line no-console\n\
         export const b = a; // istanbul ignore next\n\
         export const c = 2; // eslint-disable-line\n",
    );
    for d in [
        "// eslint-disable-line no-console",
        "// istanbul ignore next",
        "// eslint-disable-line\n",
    ] {
        assert!(out.contains(d), "missing {d:?} in:\n{out}");
    }
    // Babel leaves an eslint directive on the statement's line. Oxc drops a
    // trailing istanbul comment, so that one is hoisted onto the previous line.
    assert!(
        out.contains("const a = 1; // eslint-disable-line no-console"),
        "{out}"
    );
    assert!(
        out.contains("export const c = 2; // eslint-disable-line"),
        "{out}"
    );
    let lines: Vec<&str> = out.lines().collect();
    let at = |s: &str| lines.iter().position(|l| l.contains(s)).unwrap();
    assert_eq!(
        at("// istanbul ignore next") + 1,
        at("export const b = a;"),
        "{out}"
    );
}

#[test]
fn a_plain_trailing_comment_is_not_moved() {
    // Only tool directives are hoisted; an ordinary `// note` keeps oxc's handling.
    let out = code("const a = 1; // our props\nexport const b = a;\n");
    assert!(!out.contains("// our props\nconst a"), "{out}");
}
