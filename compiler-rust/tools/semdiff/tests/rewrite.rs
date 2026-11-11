//! v0 rewrite-leg behaviour: formatting-only diffs are equivalent; any token-level semantic change is not.
use semdiff::{compare, to_stable_json, Class};

fn class(a: &str, b: &str) -> (Class, String) {
    let v = compare(a, b);
    (v.class, v.reason.as_str().to_string())
}

const BASE: &str = "import { registerTemplate } from \"lwc\";\nconst stc0 = { key: 0, props: { \"a\": 1, \"b\": 2 } };\nfunction tmpl($api, $cmp) {\n  const { h: api_element } = $api;\n  return [api_element(\"div\", stc0)];\n}\nexport default registerTemplate(tmpl);\ntmpl.stylesheets = [];\n";

#[test]
fn positive_whitespace_and_formatting_only_is_equivalent() {
    // D-0196: whitespace and string quote/escape spelling are the ONLY unobservable spelling (semicolons stay).
    let reformatted = "import {registerTemplate} from 'lwc';\n\n\nconst stc0={key:0,props:{'a':1,'\\x62':2}};\nfunction tmpl($api,$cmp){const {h:api_element}=$api;\nreturn [api_element('d\\u{69}v',stc0)];}\nexport default registerTemplate(tmpl);\ntmpl.stylesheets=[];";
    assert_eq!(
        class(BASE, reformatted),
        (Class::EquivalentByRewrite, "printer-equal".into())
    );
}

#[test]
fn positive_identical_text_is_equivalent() {
    assert_eq!(class(BASE, BASE).0, Class::EquivalentByRewrite);
}

#[test]
fn negative_swapped_object_keys_not_equivalent() {
    // Key order is observable (Object.keys / for-in / engine prop-patch order): never canonicalized in rt-0.
    let swapped = BASE.replace("\"a\": 1, \"b\": 2", "\"b\": 2, \"a\": 1");
    assert_eq!(
        class(BASE, &swapped),
        (Class::Unclassified, "printed-differs".into())
    );
}

#[test]
fn negative_swapped_statements_not_equivalent() {
    let a = "let x = 0;\nf();\ng();\nexport { x };\n";
    let b = "let x = 0;\ng();\nf();\nexport { x };\n";
    assert_eq!(class(a, b), (Class::Unclassified, "printed-differs".into()));
}

#[test]
fn negative_changed_literal_not_equivalent() {
    let changed = BASE.replace("\"div\"", "\"span\"");
    assert_eq!(
        class(BASE, &changed),
        (Class::Unclassified, "printed-differs".into())
    );
    let num = BASE.replace("\"b\": 2", "\"b\": 3");
    assert_eq!(class(BASE, &num).0, Class::Unclassified);
}

#[test]
fn negative_whitespace_inside_string_literal_not_equivalent() {
    // template-diag-diff's fnv1a strips ALL whitespace, string contents included; semdiff must not.
    let a = "export default \"a b\";\n";
    let b = "export default \"ab\";\n";
    assert_eq!(class(a, b), (Class::Unclassified, "printed-differs".into()));
}

#[test]
fn negative_comment_change_not_equivalent() {
    // The LWC version banner is read back via Function#toString by engine-core's version check.
    let a = "function f() { /*LWC compiler v9.0.0*/ return 1; }\nexport { f };\n";
    let b = "function f() { /*LWC compiler v9.0.1*/ return 1; }\nexport { f };\n";
    assert_eq!(class(a, b), (Class::Unclassified, "comments-differ".into()));
}

#[test]
fn parse_errors_are_unclassified_per_side() {
    assert_eq!(
        class("export default (;", BASE),
        (Class::Unclassified, "parse-error-oracle".into())
    );
    assert_eq!(
        class(BASE, "export default (;"),
        (Class::Unclassified, "parse-error-rust".into())
    );
}

#[test]
fn verdict_is_byte_reproducible_and_versioned() {
    let b = BASE.replace("\"div\"", "\"span\"");
    let (j1, j2) = (
        to_stable_json(&compare(BASE, &b).json),
        to_stable_json(&compare(BASE, &b).json),
    );
    assert_eq!(j1, j2);
    let v: serde_json::Value = serde_json::from_str(&j1).unwrap();
    assert_eq!(v["schema"], "semdiff-verdict/v1");
    assert_eq!(v["rule_table_version"], semdiff::rule_table_version());
    assert_eq!(v["rules_applied"], serde_json::json!([]));
    // first_diff is located in the oxc-PRINTED text (the compared artifact), not the source.
    assert_eq!(
        v["first_diff"]["oracle"],
        "\treturn [api_element(\"div\", stc0)];"
    );
    assert_eq!(
        v["first_diff"]["rust"],
        "\treturn [api_element(\"span\", stc0)];"
    );
}

/// D-0196 default = OBSERVED: spelling the model does not list must never be EQUIVALENT, even where oxc's printer and
/// `ContentEq` both erase it (all of these were EQUIVALENT before the token check).
#[test]
fn negative_unlisted_spelling_is_observed() {
    for (a, b) in [
        ("const a = 0x10;", "const a = 16;"), // numeric-literal spelling
        ("const a = 1.0;", "const a = 1;"),
        ("const \\u0061 = 1;", "const a = 1;"), // identifier escape (not string escape spelling)
    ] {
        assert_eq!(
            class(a, b),
            (Class::Unclassified, "token-spelling-differs".into()),
            "{a:?} vs {b:?}"
        );
    }
    // D-0258/D-0266: a trailing comma is applied (R-TRAILING-COMMA-1, AB-L0064).
    assert_eq!(
        class("const a = [1,];", "const a = [1];"),
        (Class::EquivalentByRewrite, "printer-equal".into())
    );
}

#[test]
fn negative_template_raw_text_is_observed() {
    // Template raw text is observed (String.raw / tagged templates), unlike string-literal escape spelling.
    assert_eq!(
        class("const t = `a\\x41`;", "const t = `aA`;").0,
        Class::Unclassified
    );
}

#[test]
fn positive_string_quote_and_escape_spelling_is_unobservable() {
    for (a, b) in [
        ("const a = 'x\\u0041';", "const a = \"xA\";"),
        ("'use strict';\nf();", "\"use strict\";\nf();"),
        ("import a from './a.js';", "import a from \"./a.js\";"),
        ("const o = {'k': 1};", "const o = {\"k\": 1};"),
    ] {
        assert_eq!(
            class(a, b),
            (Class::EquivalentByRewrite, "printer-equal".into()),
            "{a:?} vs {b:?}"
        );
    }
}
