//! D-0258 rewrite-leg rules (file rt-1): R-TRAILING-COMMA-1 and R-ASI-SEMI-1 (both arch_review = AB-L0064).
//! Each rule's positives and required negatives (AB-D0003 (6)) exercise the rule logic directly via
//! `compare_forcing_rules`. `compare()` applies both approved rules.
use semdiff::{compare, compare_forcing_rules, rule_table_version, Class};
use serde_json::json;

fn comma() -> String {
    semdiff::rules::table().1.iter().find(|r| r.id.starts_with("R-TRAILING-COMMA")).unwrap().tagged()
}
fn asi() -> String {
    semdiff::rules::table().1.iter().find(|r| r.id.starts_with("R-ASI-SEMI")).unwrap().tagged()
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
    assert_eq!(class_forced(a, b, force), (Class::EquivalentByRewrite, "printer-equal".into()), "{a:?} vs {b:?}");
}
fn not_equiv(a: &str, b: &str, force: &[&str]) {
    let (c, r) = class_forced(a, b, force);
    assert_ne!((c, r.as_str()), (Class::EquivalentByRewrite, "printer-equal"), "{a:?} vs {b:?} must not be rewrite-equivalent: {c:?} {r}");
}

#[test]
fn table_lists_both_rules_applied() {
    assert!(semdiff::rules::pending().is_empty());
    // ASI-only on 681aba0 used the any-applied name "rt-1". Both rules applied must not reuse it.
    assert_ne!(rule_table_version(), "rt-1");
    assert_eq!(rule_table_version(), "rt-1:R-ASI-SEMI-1@1+R-TRAILING-COMMA-1@1");
    for prefix in ["R-ASI-SEMI", "R-TRAILING-COMMA"] {
        let meta = semdiff::rules::table().1.iter().find(|r| r.id.starts_with(prefix)).unwrap();
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
    assert_eq!((v.class, v.reason.as_str()), (Class::EquivalentByRewrite, "printer-equal"), "{}", v.json);
    assert_eq!(v.json["rules_applied"], json!([comma()]));
    assert_eq!(v.json["rule_table_version"], rule_table_version());
}

/// R-ASI-SEMI-1 is enabled (arch_review = AB-L0064), so compare() without forcing skips the extra `;`.
#[test]
fn asi_rule_applies_when_approved() {
    let a = "const a = 1\n";
    let b = "const a = 1;\n";
    let v = compare(a, b);
    assert_eq!((v.class, v.reason.as_str()), (Class::EquivalentByRewrite, "printer-equal"), "{}", v.json);
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
        assert!(applied_forced(a, b, &force).iter().any(|x| x == &c), "{a:?} vs {b:?}: {}", applied_forced(a, b, &force).join(","));
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
        assert!(!applied.iter().any(|x| x == &c) || class_forced(a, b, &force).0 != Class::EquivalentByRewrite);
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
        ("function f() { return\nx }\n", "function f() { return;\nx }\n"),
    ] {
        equiv(x, y, &force);
        assert!(applied_forced(x, y, &force).iter().any(|t| t == &a), "{x:?} vs {y:?}: {}", applied_forced(x, y, &force).join(","));
    }
}

#[test]
fn asi_that_changes_the_ast_is_observed() {
    let a = asi();
    let force = [a.as_str()];
    not_equiv("let a = f, b = 1;\na\n(b)\n", "let a = f, b = 1;\na;\n(b)\n", &force);
    not_equiv("function f() { return\nx }", "function f() { return x }", &force);
    not_equiv("let a = f, b = 1;\na\n[b]\n", "let a = f, b = 1;\na;\n[b]\n", &force);
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
