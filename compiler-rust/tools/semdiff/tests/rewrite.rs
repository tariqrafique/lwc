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
        ("const a = 0x10;", "const a = 16;"), // numeric spelling is observed (D-0292; R-NUMERIC-SPELLING-1 retired)
        ("const n = 1000;", "const n = 1e3;"),
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

// Comment placement. D-0196 observes comments in order, and the printer (comments off) drops them, so the
// position of each comment relative to the compared tokens must come from the source, not the reprint.
#[test]
fn negative_comment_moved_within_declaration_is_not_equivalent() {
    // Same comment text, same printed text, same token spellings; only its position differs.
    let a = "const a = /*c*/ 1;\nexport { a };\n";
    let b = "const /*c*/ a = 1;\nexport { a };\n";
    assert_eq!(class(a, b), (Class::Unclassified, "comments-differ".into()));
}

#[test]
fn negative_comment_moved_into_or_out_of_function_body_is_not_equivalent() {
    // Moving a comment into a body changes Function#toString (the engine-visible channel for comments).
    let outside = "/*c*/\nfunction f() {\n  return 1;\n}\nexport { f };\n";
    let inside = "function f() {\n  /*c*/\n  return 1;\n}\nexport { f };\n";
    assert_eq!(
        class(outside, inside),
        (Class::Unclassified, "comments-differ".into())
    );
    assert_eq!(
        class(inside, outside),
        (Class::Unclassified, "comments-differ".into())
    );
}

#[test]
fn positive_whitespace_around_a_comment_is_equivalent() {
    let a = "const a = /*c*/ 1;\nexport { a };\n";
    let b = "const a=/*c*/1;\n\n\nexport {a};\n";
    assert_eq!(
        class(a, b),
        (Class::EquivalentByRewrite, "printer-equal".into())
    );
}

#[test]
fn positive_trailing_comma_next_to_a_comment_is_equivalent() {
    // D-0258: the trailing comma is skipped by the token walk. The skip must not move the comment's position.
    let a = "const a = [1, /*c*/];\nexport { a };\n";
    let b = "const a = [1 /*c*/];\nexport { a };\n";
    assert_eq!(
        class(a, b),
        (Class::EquivalentByRewrite, "printer-equal".into())
    );
}

#[test]
fn negative_comment_moved_across_a_trailing_comma_element_is_not_equivalent() {
    let a = "const a = [/*c*/ 1,];\nexport { a };\n";
    let b = "const a = [1, /*c*/];\nexport { a };\n";
    assert_eq!(class(a, b), (Class::Unclassified, "comments-differ".into()));
}

// Literal spelling. Numeric, BigInt, regex and template spelling are compared from the source token text; the
// reprint must not be the only check for them.
#[test]
fn negative_number_and_bigint_spelling_is_observed() {
    for (a, b) in [
        ("const n = 0x10n;", "const n = 16n;"),
        ("const n = 1.0;", "const n = 1;"),
        ("const n = 1_000;", "const n = 1000;"),
    ] {
        assert_eq!(class(a, b).0, Class::Unclassified, "{a:?} vs {b:?}");
    }
}

#[test]
fn negative_regex_spelling_is_observed() {
    assert_eq!(
        class("const r = /\\x41/;", "const r = /A/;").0,
        Class::Unclassified
    );
    assert_eq!(
        class("const r = /a/g;", "const r = /a/gi;").0,
        Class::Unclassified
    );
}

// R-IMPORT-MERGE collapses imports from one module, and the token walk then drops import tokens. A comment inside or
// between the collapsed imports must still be compared by its placement, or a move between them is a false pass.
#[test]
fn negative_comment_moved_between_same_module_imports_is_not_equivalent() {
    // Both sides merge to `import { a, b } from "m"`. The comment sits after `"m"` on one side and after `import` on
    // the other, so its placement relative to the import tokens differs even though the printed texts agree.
    let a = "import { a } from \"m\" /*c*/;\nimport { b } from \"m\";\nexport { a, b };\n";
    let b = "import { a } from \"m\";\nimport /*c*/ { b } from \"m\";\nexport { a, b };\n";
    assert_eq!(class(a, b), (Class::Unclassified, "comments-differ".into()));
}

#[test]
fn negative_comment_moved_from_before_to_between_same_module_imports_is_not_equivalent() {
    let a = "/*c*/\nimport { a } from \"m\";\nimport { b } from \"m\";\nexport { a, b };\n";
    let b = "import { a } from \"m\";\n/*c*/\nimport { b } from \"m\";\nexport { a, b };\n";
    assert_eq!(class(a, b), (Class::Unclassified, "comments-differ".into()));
}

#[test]
fn positive_whitespace_around_same_module_imports_is_equivalent() {
    let a = "import {a} from 'm';\nimport {b} from 'm';\nexport {a, b};\n";
    let b = "import {a}   from 'm';\n\n\nimport { b } from \"m\" ;\nexport { a,b };";
    assert_eq!(
        class(a, b),
        (Class::EquivalentByRewrite, "printer-equal".into())
    );
}

#[test]
fn positive_whitespace_around_comment_between_same_module_imports_is_equivalent() {
    // Same comment, same token count before it, same position among the imports: only whitespace differs.
    let a = "import { a } from \"m\";\n/*c*/\nimport { b } from \"m\";\nexport { a, b };\n";
    let b = "import {a} from 'm';/*c*/import {b} from 'm';export {a,b};";
    assert_eq!(
        class(a, b),
        (Class::EquivalentByRewrite, "printer-equal".into())
    );
}
