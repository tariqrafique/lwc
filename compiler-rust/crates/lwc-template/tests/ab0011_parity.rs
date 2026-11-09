//! AB-0011 a1 oracle parity (design §1 rows + 40 fuzz rows from /workspace/scratch/ab0011/fz where
//! the pre-a1 Rust diverged). Expected values were recorded from upstream @lwc (PIN) with tsx:
//! `_` + toPropertyName, kebabcaseToCamelcase (template-compiler shared/), and @lwc/shared
//! kebabCaseToCamelCase, generateCustomElementTagName('x', s), isVoidElement(s, HTML_NAMESPACE).
use lwc_shared::custom_element::generate_custom_element_tag_name;
use lwc_shared::html_attributes::kebab_case_to_camel_case;
use lwc_shared::namespaces::HTML_NAMESPACE;
use lwc_shared::void_elements::is_void_element;
use lwc_template::shared::naming::kebabcase_to_camelcase;
use lwc_template::shared::utils::to_property_name;

/// (input, `_`+toPropertyName, kebabcaseToCamelcase, kebabCaseToCamelCase, tag name, isVoidElement)
const ROWS: &[(&str, &str, &str, &str, &str, bool)] = &[
    (
        "x-\u{E9}",
        "_x\u{C9}",
        "x/\u{E9}",
        "x-\u{E9}",
        "x-x-\u{E9}",
        false,
    ),
    ("x", "_x", "x", "x", "x-x", false),
    (
        "\u{17F}_-\u{E9}-\u{10428}\u{130}",
        "_\u{17F}_\u{C9}\u{10428}\u{130}",
        "\u{17F}_/\u{E9}\u{10428}\u{130}",
        "\u{17F}_-\u{E9}-\u{10428}\u{130}",
        "x-\u{17F}_-\u{E9}-\u{10428}i\u{307}",
        false,
    ),
    (
        "x-\u{10428}a",
        "_x\u{10428}a",
        "x/\u{10428}a",
        "x-\u{10428}a",
        "x-x-\u{10428}a",
        false,
    ),
    (
        "\u{212A}eygen",
        "_\u{212A}eygen",
        "\u{212A}eygen",
        "\u{212A}eygen",
        "x-keygen",
        true,
    ),
    (
        "x-\u{E9}bc-d\u{E9}f",
        "_x\u{C9}bcD\u{E9}f",
        "x/\u{E9}bcD\u{E9}f",
        "x-\u{E9}bcD\u{E9}f",
        "x-x-\u{E9}bc-d\u{E9}f",
        false,
    ),
    (
        "lwc:component",
        "_lwc:component",
        "lwc:component",
        "lwc:component",
        "x-lwc:component",
        false,
    ),
    (
        "K_--\u{10428}",
        "_K_\u{10428}",
        "K_/\u{10428}",
        "K_--\u{10428}",
        "x-k_--\u{10428}",
        false,
    ),
    (
        "\u{10428}ZK-\u{10428}:",
        "_\u{10428}ZK\u{10428}:",
        "\u{10428}ZK/\u{10428}:",
        "\u{10428}ZK-\u{10428}:",
        "x-\u{10428}zk-\u{10428}:",
        false,
    ),
    (
        "x-\u{DF}",
        "_xSS",
        "x/\u{DF}",
        "x-\u{DF}",
        "x-x-\u{DF}",
        false,
    ),
    ("role", "_role", "role", "role", "x-role", false),
    (
        "\u{E9}--\u{10428}",
        "_\u{E9}\u{10428}",
        "\u{E9}/\u{10428}",
        "\u{E9}--\u{10428}",
        "x-\u{E9}--\u{10428}",
        false,
    ),
    (
        "-\u{10428}\u{17F}-ZK",
        "_\u{10428}\u{17F}ZK",
        "/\u{10428}\u{17F}ZK",
        "-\u{10428}\u{17F}-ZK",
        "x--\u{10428}\u{17F}-zk",
        false,
    ),
    (
        "x-\u{1C6}a",
        "_x\u{1C4}a",
        "x/\u{1C6}a",
        "x-\u{1C6}a",
        "x-x-\u{1C6}a",
        false,
    ),
    (
        "tabindex",
        "_tabindex",
        "tabindex",
        "tabindex",
        "x-tabindex",
        false,
    ),
    (
        "\u{DF}--\u{10428}-\u{3A3}\u{DF}",
        "_\u{DF}\u{10428}\u{3A3}\u{DF}",
        "\u{DF}/\u{10428}\u{3A3}\u{DF}",
        "\u{DF}--\u{10428}-\u{3A3}\u{DF}",
        "x-\u{DF}--\u{10428}-\u{3C3}\u{DF}",
        false,
    ),
    (
        "\u{DF}1-\u{10428}K",
        "_\u{DF}1\u{10428}K",
        "\u{DF}1/\u{10428}K",
        "\u{DF}1-\u{10428}K",
        "x-\u{DF}1-\u{10428}k",
        false,
    ),
    (
        "x-\u{17F}",
        "_xS",
        "x/\u{17F}",
        "x-\u{17F}",
        "x-x-\u{17F}",
        false,
    ),
    ("for", "_for", "for", "for", "x-for", false),
    (
        "\u{130}\u{3A3}-:\u{E9}-\u{10428}",
        "_\u{130}\u{3A3}:\u{E9}\u{10428}",
        "\u{130}\u{3A3}/:\u{E9}\u{10428}",
        "\u{130}\u{3A3}-:\u{E9}-\u{10428}",
        "x-i\u{307}\u{3C2}-:\u{E9}-\u{10428}",
        false,
    ),
    (
        "K-\u{10428}1\u{3A3}:Z",
        "_K\u{10428}1\u{3A3}:Z",
        "K/\u{10428}1\u{3A3}:Z",
        "K-\u{10428}1\u{3A3}:Z",
        "x-k-\u{10428}1\u{3C3}:z",
        false,
    ),
    (
        "x-\u{131}",
        "_xI",
        "x/\u{131}",
        "x-\u{131}",
        "x-x-\u{131}",
        false,
    ),
    (
        "constructor",
        "_constructor",
        "constructor",
        "constructor",
        "x-constructor",
        false,
    ),
    (
        "\u{DF}_--\u{10428}",
        "_\u{DF}_\u{10428}",
        "\u{DF}_/\u{10428}",
        "\u{DF}_--\u{10428}",
        "x-\u{DF}_--\u{10428}",
        false,
    ),
    (
        "a-\u{10428}\u{130}",
        "_a\u{10428}\u{130}",
        "a/\u{10428}\u{130}",
        "a-\u{10428}\u{130}",
        "x-a-\u{10428}i\u{307}",
        false,
    ),
    (
        "\u{10428}-\u{DF}",
        "_\u{10428}SS",
        "\u{10428}/\u{DF}",
        "\u{10428}-\u{DF}",
        "x-\u{10428}-\u{DF}",
        false,
    ),
    (
        "__proto__",
        "___proto__",
        "__proto__",
        "__proto__",
        "x-__proto__",
        false,
    ),
    (
        "aZ-\u{E9}-\u{10428}",
        "_aZ\u{C9}\u{10428}",
        "aZ/\u{E9}\u{10428}",
        "aZ-\u{E9}-\u{10428}",
        "x-a-z-\u{E9}-\u{10428}",
        false,
    ),
    (
        "\u{E9}\u{DF}-\u{10428}:a\u{17F}",
        "_\u{E9}\u{DF}\u{10428}:a\u{17F}",
        "\u{E9}\u{DF}/\u{10428}:a\u{17F}",
        "\u{E9}\u{DF}-\u{10428}:a\u{17F}",
        "x-\u{E9}\u{DF}-\u{10428}:a\u{17F}",
        false,
    ),
    (
        "\u{3A3}-\u{17F}\u{10428}KZ",
        "_\u{3A3}S\u{10428}KZ",
        "\u{3A3}/\u{17F}\u{10428}KZ",
        "\u{3A3}-\u{17F}\u{10428}KZ",
        "x-\u{3C3}-\u{17F}\u{10428}kz",
        false,
    ),
    (
        "toString",
        "_toString",
        "toString",
        "toString",
        "x-to-string",
        false,
    ),
    (
        "Z\u{3A3}K--\u{10428}",
        "_Z\u{3A3}K\u{10428}",
        "Z\u{3A3}K/\u{10428}",
        "Z\u{3A3}K--\u{10428}",
        "x-z\u{3C3}k--\u{10428}",
        false,
    ),
    (
        "-\u{10428}\u{3A3}",
        "_\u{10428}\u{3A3}",
        "/\u{10428}\u{3A3}",
        "-\u{10428}\u{3A3}",
        "x--\u{10428}\u{3C2}",
        false,
    ),
    (
        "\u{17F}-\u{17F}",
        "_\u{17F}S",
        "\u{17F}/\u{17F}",
        "\u{17F}-\u{17F}",
        "x-\u{17F}-\u{17F}",
        false,
    ),
    (
        "hasOwnProperty",
        "_hasOwnProperty",
        "hasOwnProperty",
        "hasOwnProperty",
        "x-has-own-property",
        false,
    ),
    (
        "-\u{10428}K-_",
        "_\u{10428}K_",
        "/\u{10428}K_",
        "-\u{10428}K-_",
        "x--\u{10428}k-_",
        false,
    ),
    (
        "_:\u{17F}:-\u{17F}Z",
        "__:\u{17F}:SZ",
        "_:\u{17F}:/\u{17F}Z",
        "_:\u{17F}:-\u{17F}Z",
        "x-_:\u{17F}:-\u{17F}z",
        false,
    ),
    (
        "valueOf",
        "_valueOf",
        "valueOf",
        "valueOf",
        "x-value-of",
        false,
    ),
    (
        "\u{E9}-\u{10428}_:\u{130}",
        "_\u{E9}\u{10428}_:\u{130}",
        "\u{E9}/\u{10428}_:\u{130}",
        "\u{E9}-\u{10428}_:\u{130}",
        "x-\u{E9}-\u{10428}_:i\u{307}",
        false,
    ),
    (
        "\u{130}\u{DF}-\u{DF}",
        "_\u{130}\u{DF}SS",
        "\u{130}\u{DF}/\u{DF}",
        "\u{130}\u{DF}-\u{DF}",
        "x-i\u{307}\u{DF}-\u{DF}",
        false,
    ),
    (
        "x-\u{E9}a",
        "_x\u{C9}a",
        "x/\u{E9}a",
        "x-\u{E9}a",
        "x-x-\u{E9}a",
        false,
    ),
    ("BR", "_BR", "BR", "BR", "x-br", true),
    (
        "x-foo-bar",
        "_xFooBar",
        "x/fooBar",
        "xFooBar",
        "x-x-foo-bar",
        false,
    ),
    ("aBcD", "_aBcD", "aBcD", "aBcD", "x-a-bc-d", false),
    ("fooBAR", "_fooBAR", "fooBAR", "fooBAR", "x-foo-bar", false),
    ("a--b", "_aB", "a/B", "a-B", "x-a--b", false),
    ("-a", "_A", "/a", "A", "x--a", false),
];

#[test]
fn oracle_rows() {
    assert_eq!(ROWS.len(), 47);
    let mut bad = Vec::new();
    for &(s, ident, path, camel, tag, void) in ROWS {
        let got = (
            format!("_{}", to_property_name(s)),
            kebabcase_to_camelcase(s),
            kebab_case_to_camel_case(s),
            generate_custom_element_tag_name("x", s),
            is_void_element(s, HTML_NAMESPACE),
        );
        if got
            != (
                ident.to_string(),
                path.to_string(),
                camel.to_string(),
                tag.to_string(),
                void,
            )
        {
            bad.push(format!("{s:?}: got {got:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// Design §1: `<x-ß>` binds `_xSS` imported from `x/ß`; `<x-éa>` binds `_xÉa` from `x/éa`.
#[test]
fn section1_component_bindings() {
    assert_eq!(to_property_name("x-\u{DF}"), "xSS");
    assert_eq!(kebabcase_to_camelcase("x-\u{DF}"), "x/\u{DF}");
    assert_eq!(to_property_name("x-\u{E9}a"), "x\u{C9}a");
    assert_eq!(kebabcase_to_camelcase("x-\u{E9}a"), "x/\u{E9}a");
    assert_eq!(
        to_property_name("x-\u{10428}a"),
        "x\u{10428}a",
        "astral char after '-' is not uppercased"
    );
}
