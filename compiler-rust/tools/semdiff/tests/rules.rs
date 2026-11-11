//! D-0258 rewrite-leg rules (file rt-1): R-TRAILING-COMMA-1 and R-ASI-SEMI-1 (both arch_review = AB-L0064).
//! Each rule's positives and required negatives (AB-D0003 (6)) exercise the rule logic directly via
//! `compare_forcing_rules`. `compare()` applies both approved rules.
use semdiff::{compare, compare_forcing_rules, rule_table_version, Class};
use serde_json::json;

fn comma() -> String {
    semdiff::rules::table()
        .1
        .iter()
        .find(|r| r.id.starts_with("R-TRAILING-COMMA"))
        .unwrap()
        .tagged()
}
fn asi() -> String {
    semdiff::rules::table()
        .1
        .iter()
        .find(|r| r.id.starts_with("R-ASI-SEMI"))
        .unwrap()
        .tagged()
}

fn class_forced(a: &str, b: &str, force: &[&str]) -> (Class, String) {
    let v = compare_forcing_rules(a, b, force);
    (v.class, v.reason.as_str().to_string())
}
fn applied_forced(a: &str, b: &str, force: &[&str]) -> Vec<String> {
    compare_forcing_rules(a, b, force).json["rules_applied"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}
fn equiv(a: &str, b: &str, force: &[&str]) {
    assert_eq!(
        class_forced(a, b, force),
        (Class::EquivalentByRewrite, "printer-equal".into()),
        "{a:?} vs {b:?}"
    );
}
fn not_equiv(a: &str, b: &str, force: &[&str]) {
    let (c, r) = class_forced(a, b, force);
    assert_ne!(
        (c, r.as_str()),
        (Class::EquivalentByRewrite, "printer-equal"),
        "{a:?} vs {b:?} must not be rewrite-equivalent: {c:?} {r}"
    );
}

#[test]
fn table_lists_both_rules_applied() {
    assert!(semdiff::rules::pending().is_empty());
    // ASI-only on 681aba0 used the any-applied name "rt-1". Both rules applied must not reuse it.
    assert_ne!(rule_table_version(), "rt-1");
    assert_eq!(
        rule_table_version(),
        "rt-1:R-ARROW-PAREN-1@1+R-ASI-SEMI-1@1+R-COMMENT-STAR-SPACE-1@1+R-HOIST-DECL-1@1+R-IMPORT-MERGE-1@1+R-INERT-LINE-COMMENT-1@1+R-NUMERIC-SPELLING-1@1+R-OBJECT-LAST-KEY-1@1+R-OBJECT-SHORTHAND-1@1+R-REDUNDANT-PAREN-1@1+R-TRAILING-COMMA-1@1+R-VAR-LITERAL-IMPORT-1@1"
    );
    for prefix in ["R-ASI-SEMI", "R-TRAILING-COMMA"] {
        let meta = semdiff::rules::table()
            .1
            .iter()
            .find(|r| r.id.starts_with(prefix))
            .unwrap();
        assert_eq!(meta.arch_review, "AB-L0064");
        assert!(meta.is_applied());
    }
}

/// R-TRAILING-COMMA-1 is enabled, so compare() without forcing skips the extra comma.
#[test]
fn approved_trailing_comma_applies() {
    let a = "const a = [1, 2,];";
    let b = "const a = [1, 2];";
    let v = compare(a, b);
    assert_eq!(
        (v.class, v.reason.as_str()),
        (Class::EquivalentByRewrite, "printer-equal"),
        "{}",
        v.json
    );
    assert_eq!(v.json["rules_applied"], json!([comma()]));
    assert_eq!(v.json["rule_table_version"], rule_table_version());
}

/// R-ASI-SEMI-1 is enabled (arch_review = AB-L0064), so compare() without forcing skips the extra `;`.
#[test]
fn asi_rule_applies_when_approved() {
    let a = "const a = 1\n";
    let b = "const a = 1;\n";
    let v = compare(a, b);
    assert_eq!(
        (v.class, v.reason.as_str()),
        (Class::EquivalentByRewrite, "printer-equal"),
        "{}",
        v.json
    );
    assert_eq!(v.json["rules_applied"], json!([asi()]));
    assert_eq!(v.json["rule_table_version"], rule_table_version());
}

#[test]
fn trailing_comma_is_unobservable() {
    let c = comma();
    let force = [c.as_str()];
    for (a, b) in [
        ("const a = [1, 2,];", "const a = [1, 2];"),
        ("const a = [1,];", "const a = [1];"),
        ("const o = {a: 1,};", "const o = {a: 1};"),
        ("const o = {a,};", "const o = {a};"),
        ("function f(a,) {}", "function f(a) {}"),
        ("f(a,);", "f(a);"),
        ("new F(a,);", "new F(a);"),
        ("import {a,} from 'm';", "import {a} from 'm';"),
        ("import {a as b,} from 'm';", "import {a as b} from 'm';"),
        ("let a; export {a,};", "let a; export {a};"),
        // D-0266: destructuring patterns (declarator, assignment, param).
        ("let {a,} = o;", "let {a} = o;"),
        ("let [a,] = o;", "let [a] = o;"),
        ("({a,} = o);", "({a} = o);"),
        ("([a,] = o);", "([a] = o);"),
        ("function f({a,}) {}", "function f({a}) {}"),
        ("function f([a,]) {}", "function f([a]) {}"),
    ] {
        equiv(a, b, &force);
        assert!(
            applied_forced(a, b, &force).iter().any(|x| x == &c),
            "{a:?} vs {b:?}: {}",
            applied_forced(a, b, &force).join(",")
        );
    }
}

#[test]
fn a_hole_is_not_a_trailing_comma() {
    let c = comma();
    let force = [c.as_str()];
    for (a, b) in [
        ("const a = [1,,];", "const a = [1,];"),
        ("const a = [,];", "const a = [];"),
        ("const a = [1,,2];", "const a = [1,2];"),
        ("const a = [1, ,];", "const a = [1,];"),
    ] {
        not_equiv(a, b, &force);
        let applied = applied_forced(a, b, &force);
        assert!(
            !applied.iter().any(|x| x == &c)
                || class_forced(a, b, &force).0 != Class::EquivalentByRewrite
        );
    }
}

#[test]
fn pattern_elision_is_observed() {
    // D-0266: pattern holes stay OBSERVED. Same three positions as the required trailing-comma positives.
    let c = comma();
    let force = [c.as_str()];
    for (a, b) in [
        // [a,,]=o != [a,]=o (two commas / last slot is a hole)
        ("let [a,,] = o;", "let [a,] = o;"),
        ("([a,,] = o);", "([a,] = o);"),
        ("function f([a,,]) {}", "function f([a,]) {}"),
        // [,]=o vs []=o
        ("let [,] = o;", "let [] = o;"),
        ("([,] = o);", "([] = o);"),
        ("function f([,]) {}", "function f([]) {}"),
        // [,a] stays observed (leading hole)
        ("let [,a] = o;", "let [a] = o;"),
        ("([,a] = o);", "([a] = o);"),
        ("function f([,a]) {}", "function f([a]) {}"),
        // a hole nested in a param pattern
        ("function f([a,,]) {}", "function f([a,]) {}"),
        // two trailing commas is a hole, not two trailers (JS has no double-trailing-comma production)
        ("const a = [1,,];", "const a = [1,];"),
    ] {
        not_equiv(a, b, &force);
    }
    // No JS production outside D-0258/D-0266 yields a trailing comma that oxc parses as SourceType::mjs
    // (a sequence expression cannot end with a comma; `(a,)` is FormalParameters only as an arrow;
    // type-position commas require TypeScript, which this parse does not use).
}

#[test]
fn asi_semicolon_with_identical_ast_is_unobservable() {
    let a = asi();
    let force = [a.as_str()];
    for (x, y) in [
        ("f()\ng()\n", "f();\ng();\n"),
        ("const a = 1\n", "const a = 1;\n"),
        ("function f() { return\n}\n", "function f() { return; }\n"),
        (
            "function f() { return\nx }\n",
            "function f() { return;\nx }\n",
        ),
    ] {
        equiv(x, y, &force);
        assert!(
            applied_forced(x, y, &force).iter().any(|t| t == &a),
            "{x:?} vs {y:?}: {}",
            applied_forced(x, y, &force).join(",")
        );
    }
}

#[test]
fn asi_that_changes_the_ast_is_observed() {
    let a = asi();
    let force = [a.as_str()];
    not_equiv(
        "let a = f, b = 1;\na\n(b)\n",
        "let a = f, b = 1;\na;\n(b)\n",
        &force,
    );
    not_equiv(
        "function f() { return\nx }",
        "function f() { return x }",
        &force,
    );
    not_equiv(
        "let a = f, b = 1;\na\n[b]\n",
        "let a = f, b = 1;\na;\n[b]\n",
        &force,
    );
}

#[test]
fn class_has_no_equivalent_by_execution() {
    // Exhaustive match: re-adding Class::EquivalentByExecution fails to compile.
    fn name(c: Class) -> &'static str {
        match c {
            Class::EquivalentByRewrite => "EQUIVALENT-BY-REWRITE",
            Class::Different => "DIFFERENT",
            Class::Unclassified => "UNCLASSIFIED",
            Class::Crash => "CRASH",
        }
    }
    assert_eq!(name(Class::EquivalentByRewrite), "EQUIVALENT-BY-REWRITE");
    assert_eq!(name(Class::Different), "DIFFERENT");
}

fn tag(prefix: &str) -> String {
    semdiff::rules::table()
        .1
        .iter()
        .find(|r| r.id.starts_with(prefix))
        .unwrap()
        .tagged()
}

#[test]
fn import_merge_same_module_is_unobservable() {
    let id = tag("R-IMPORT-MERGE");
    let force = [id.as_str()];
    for (x, y) in [
        (
            "import { b } from \"m\";\nimport { a } from \"m\";\nexport const x = a + b;\n",
            "import { a, b } from \"m\";\nexport const x = a + b;\n",
        ),
        (
            "import { b, a } from \"m\";\nexport const x = a + b;\n",
            "import { a, b } from \"m\";\nexport const x = a + b;\n",
        ),
        (
            "import \"m\";\nimport { a } from \"m\";\nexport const x = a;\n",
            "import { a } from \"m\";\nexport const x = a;\n",
        ),
    ] {
        equiv(x, y, &force);
        assert!(
            applied_forced(x, y, &force).iter().any(|t| t == &id),
            "{x} vs {y}"
        );
    }
}

#[test]
fn import_merge_does_not_reorder_modules() {
    let id = tag("R-IMPORT-MERGE");
    let force = [id.as_str()];
    not_equiv(
        "import \"a\";\nimport \"b\";\n",
        "import \"b\";\nimport \"a\";\n",
        &force,
    );
}

#[test]
fn import_merge_does_not_drop_a_binding() {
    let id = tag("R-IMPORT-MERGE");
    let force = [id.as_str()];
    not_equiv(
        "import { a, b } from \"m\";\nexport const x = a;\n",
        "import { a } from \"m\";\nexport const x = a;\n",
        &force,
    );
    not_equiv(
        "import { a as x } from \"m\";\nexport const y = x;\n",
        "import { b as x } from \"m\";\nexport const y = x;\n",
        &force,
    );
}

#[test]
fn hoisted_decl_position_is_unobservable() {
    let id = tag("R-HOIST-DECL");
    let force = [id.as_str()];
    let a = "import { A } from \"m\";\nvar _Class;\nfunction helper() { return 1; }\n_Class = class extends A {}\n";
    let b = "function helper() { return 1; }\nvar _Class;\nimport { A } from \"m\";\n_Class = class extends A {}\n";
    equiv(a, b, &force);
    assert!(applied_forced(a, b, &force).iter().any(|t| t == &id));
    let c = "function b() { return 2; }\nfunction a() { return 1; }\n";
    let d = "function a() { return 1; }\nfunction b() { return 2; }\n";
    equiv(c, d, &force);
}

#[test]
fn let_and_initialized_var_are_not_hoisted() {
    let id = tag("R-HOIST-DECL");
    let force = [id.as_str()];
    not_equiv("let x;\nx = 1;\n", "x = 1;\nlet x;\n", &force);
    not_equiv("var x = 1;\nf();\n", "f();\nvar x = 1;\n", &force);
    not_equiv("const x = 1;\nf();\n", "f();\nconst x = 1;\n", &force);
}

#[test]
fn duplicate_function_order_is_observed() {
    let id = tag("R-HOIST-DECL");
    let force = [id.as_str()];
    not_equiv(
        "function f() { return 1; }\nfunction f() { return 2; }\n",
        "function f() { return 2; }\nfunction f() { return 1; }\n",
        &force,
    );
}

#[test]
fn arrow_parens_around_one_binding_are_unobservable() {
    let id = tag("R-ARROW-PAREN");
    let force = [id.as_str()];
    let a = "const f = el => el;\n";
    let b = "const f = (el) => el;\n";
    equiv(a, b, &force);
    assert!(applied_forced(a, b, &force).iter().any(|t| t == &id));
}

#[test]
fn arrow_parens_do_not_hide_a_different_parameter_list() {
    let id = tag("R-ARROW-PAREN");
    let force = [id.as_str()];
    not_equiv("const f = (a, b) => a;\n", "const f = a => a;\n", &force);
    not_equiv(
        "const f = ({ el }) => el;\n",
        "const f = el => el;\n",
        &force,
    );
    not_equiv(
        "const f = (el = 1) => el;\n",
        "const f = el => el;\n",
        &force,
    );
    let a = "const f = type => type;\n";
    let b = "const f = (type) => type;\n";
    equiv(a, b, &force);
}

#[test]
fn numeric_spelling_with_the_same_value_is_unobservable() {
    let id = tag("R-NUMERIC-SPELLING");
    let force = [id.as_str()];
    for (a, b) in [
        ("const n = 1000;\n", "const n = 1e3;\n"),
        ("const n = 0.5;\n", "const n = .5;\n"),
    ] {
        let v = compare_forcing_rules(a, b, &force);
        assert_eq!(
            (v.class, v.reason.as_str()),
            (Class::EquivalentByRewrite, "printer-equal"),
            "{}",
            v.json
        );
        assert!(
            applied_forced(a, b, &force).iter().any(|t| t == &id),
            "{a} vs {b}: {}",
            v.json
        );
    }
}

#[test]
fn a_different_number_is_observed() {
    let id = tag("R-NUMERIC-SPELLING");
    let force = [id.as_str()];
    not_equiv("const n = 1000;\n", "const n = 1001;\n", &force);
    not_equiv("const n = 1e309;\n", "const n = 1e310;\n", &force);
    not_equiv("const n = 1000;\n", "const n = \"1000\";\n", &force);
}

#[test]
fn redundant_parens_with_identical_print_are_unobservable() {
    let id = tag("R-REDUNDANT-PAREN");
    let force = [id.as_str()];
    let a = "const n = ((a + b)) * c;\n";
    let b = "const n = (a + b) * c;\n";
    equiv(a, b, &force);
    assert!(
        applied_forced(a, b, &force).iter().any(|t| t == &id),
        "{a} vs {b}"
    );
}

#[test]
fn block_comment_star_indent_is_unobservable() {
    let id = tag("R-COMMENT-STAR-SPACE");
    let force = [id.as_str()];
    let a = "/*\n * Copyright\n */\nconst a = 1;\n";
    let b = "/*\n* Copyright\n */\nconst a = 1;\n";
    equiv(a, b, &force);
    assert!(
        applied_forced(a, b, &force).iter().any(|t| t == &id),
        "{a} vs {b}"
    );
}

#[test]
fn a_different_or_missing_comment_is_observed() {
    let id = tag("R-COMMENT-STAR-SPACE");
    let force = [id.as_str()];
    not_equiv(
        "/*\n * Copyright\n */\nconst a = 1;\n",
        "/*\n * Copyleft\n */\nconst a = 1;\n",
        &force,
    );
    not_equiv("// keep me\nconst a = 1;\n", "const a = 1;\n", &force);
}

#[test]
fn literal_var_beside_imports_is_unobservable() {
    let id = tag("R-VAR-LITERAL-IMPORT");
    let force = [id.as_str()];
    let a = "var id = 0;\nimport { a } from \"m\";\nexport const x = id + a;\n";
    let b = "import { a } from \"m\";\nvar id = 0;\nexport const x = id + a;\n";
    equiv(a, b, &force);
    assert!(applied_forced(a, b, &force).iter().any(|t| t == &id));
}

#[test]
fn literal_var_does_not_cross_a_call() {
    let id = tag("R-VAR-LITERAL-IMPORT");
    let force = [id.as_str()];
    not_equiv("f();\nvar x = 0;\n", "var x = 0;\nf();\n", &force);
}

#[test]
fn duplicate_data_key_keeps_the_last_value() {
    let id = tag("R-OBJECT-LAST-KEY");
    let force = [id.as_str()];
    let a = "const o = { selectedItem: 1, selectedItem: 1, currentContent: 1 };\n";
    let b = "const o = { selectedItem: 1, currentContent: 1 };\n";
    equiv(a, b, &force);
    assert!(applied_forced(a, b, &force).iter().any(|t| t == &id));
    equiv(
        "const o = { a: 1, a: 2 };\n",
        "const o = { a: 2 };\n",
        &force,
    );
}

#[test]
fn an_earlier_duplicate_value_is_not_the_result() {
    let id = tag("R-OBJECT-LAST-KEY");
    let force = [id.as_str()];
    not_equiv(
        "const o = { a: 1, a: 2 };\n",
        "const o = { a: 1 };\n",
        &force,
    );
    not_equiv(
        "const o = { a: 1, ...b, c: 2 };\n",
        "const o = { ...b, c: 2 };\n",
        &force,
    );
}

#[test]
fn an_inert_line_comment_is_unobservable() {
    let id = tag("R-INERT-LINE-COMMENT");
    let force = [id.as_str()];
    let a = "const a = 1; // our props\n";
    let b = "const a = 1;\n";
    equiv(a, b, &force);
    assert!(applied_forced(a, b, &force).iter().any(|t| t == &id));
}

#[test]
fn a_directive_line_comment_is_observed() {
    let id = tag("R-INERT-LINE-COMMENT");
    let force = [id.as_str()];
    not_equiv(
        "const a = 1; // eslint-disable-line no-console\n",
        "const a = 1;\n",
        &force,
    );
    not_equiv(
        "const a = 1; //# sourceMappingURL=a.js.map\n",
        "const a = 1;\n",
        &force,
    );
    not_equiv("/* keep */\nconst a = 1;\n", "const a = 1;\n", &force);
}

#[test]
fn explicit_property_shorthand_is_unobservable() {
    let id = tag("R-OBJECT-SHORTHAND");
    let force = [id.as_str()];
    let a = "const o = { itemIndex: itemIndex, url: url };\nconst { url: url } = o;\n";
    let b = "const o = { itemIndex, url };\nconst { url } = o;\n";
    equiv(a, b, &force);
    assert!(applied_forced(a, b, &force).iter().any(|t| t == &id));
}

#[test]
fn a_different_property_value_is_observed() {
    let id = tag("R-OBJECT-SHORTHAND");
    let force = [id.as_str()];
    not_equiv("const o = { a: b };\n", "const o = { a };\n", &force);
    not_equiv(
        "const o = { __proto__: __proto__ };\n",
        "const o = { __proto__ };\n",
        &force,
    );
    not_equiv("const { a: b } = o;\n", "const { a } = o;\n", &force);
}

#[test]
fn parens_that_change_the_parse_are_observed() {
    let id = tag("R-REDUNDANT-PAREN");
    let force = [id.as_str()];
    not_equiv(
        "const n = (a + b) * c;\n",
        "const n = a + (b * c);\n",
        &force,
    );
    not_equiv("f((a, b));\n", "f(a, b);\n", &force);
}
