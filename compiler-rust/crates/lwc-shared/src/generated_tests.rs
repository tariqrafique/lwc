//! Checks on `generated.rs` and the hand-written lookups over it. Expected values were recorded from
//! @lwc/shared (PIN) with tsx.
use crate::aria::*;
use crate::custom_element::generate_custom_element_tag_name;
use crate::generated::sorted_key_lists;
use crate::html_attributes::*;
use crate::namespaces::*;
use crate::void_elements::is_void_element;

/// Every table except `ARIA_PROPERTY_NAMES` (upstream order) is looked up with `binary_search`.
#[test]
fn tables_sorted() {
    let lists = sorted_key_lists();
    assert!(lists.len() >= 11, "{}", lists.len());
    for (name, keys) in lists {
        // `BOOLEAN_ATTRIBUTES.<attr>` tag lists may be empty (= allowed on every tag).
        assert!(name.contains('.') || !keys.is_empty(), "{name} empty");
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "{name} not strictly sorted"
        );
    }
}

#[test]
fn void_elements() {
    assert!(is_void_element("br", HTML_NAMESPACE) && is_void_element("BR", HTML_NAMESPACE));
    assert!(
        is_void_element("\u{212A}eygen", HTML_NAMESPACE),
        "full-Unicode toLowerCase: KELVIN SIGN -> k"
    );
    assert!(!is_void_element("br", SVG_NAMESPACE) && !is_void_element("div", HTML_NAMESPACE));
}

#[test]
fn custom_element_tag_name() {
    assert_eq!(generate_custom_element_tag_name("x", "fooBar"), "x-foo-bar");
    assert_eq!(generate_custom_element_tag_name("x", "aBcD"), "x-a-bc-d");
    assert_eq!(generate_custom_element_tag_name("x", "fooBAR"), "x-foo-bar");
    assert_eq!(
        generate_custom_element_tag_name("", ""),
        "-",
        "upstream warns here (not ported) and returns '-'"
    );
}

#[test]
fn kebab_to_camel() {
    for (i, o) in [
        ("x-foo-bar", "xFooBar"),
        ("a--b", "a-B"),
        ("-a", "A"),
        ("x-\u{E9}", "x-\u{E9}"),
        ("a-B", "a-B"),
    ] {
        assert_eq!(kebab_case_to_camel_case(i), o, "{i}");
    }
}

#[test]
fn namespaces() {
    assert_eq!(HTML_NAMESPACE, "http://www.w3.org/1999/xhtml");
    assert_eq!(SVG_NAMESPACE, "http://www.w3.org/2000/svg");
    assert_eq!(MATHML_NAMESPACE, "http://www.w3.org/1998/Math/MathML");
    assert_eq!(XML_NAMESPACE, "http://www.w3.org/XML/1998/namespace");
    assert_eq!(XLINK_NAMESPACE, "http://www.w3.org/1999/xlink");
}

/// (name, isGlobalHtmlAttribute, isAriaAttribute, isBooleanAttribute(div), isBooleanAttribute(input),
/// htmlPropertyToAttribute, ID_REFERENCING has, DISALLOWED has, REFLECTIVE has, AMBIGUOUS get).
/// Prototype keys (`constructor`, `__proto__`, `toString`) are not members anywhere (AB-D0002 cond. 2).
#[allow(clippy::type_complexity)]
const PREDICATE_ROWS: &[(
    &str,
    bool,
    bool,
    bool,
    bool,
    &str,
    bool,
    bool,
    bool,
    Option<&str>,
)] = &[
    (
        "constructor",
        false,
        false,
        false,
        false,
        "constructor",
        false,
        false,
        false,
        None,
    ),
    (
        "__proto__",
        false,
        false,
        false,
        false,
        "__proto__",
        false,
        false,
        false,
        None,
    ),
    (
        "toString",
        false,
        false,
        false,
        false,
        "to-string",
        false,
        false,
        false,
        None,
    ),
    (
        "data-a!b", false, false, false, false, "data-a!b", false, false, false, None,
    ),
    (
        "aria-label",
        false,
        true,
        false,
        false,
        "aria-label",
        false,
        false,
        false,
        None,
    ),
    (
        "ARIA-LABEL",
        false,
        false,
        false,
        false,
        "-a-r-i-a--l-a-b-e-l",
        false,
        false,
        false,
        None,
    ),
    (
        "hidden", true, false, true, true, "hidden", false, false, true, None,
    ),
    (
        "checked", false, false, false, true, "checked", false, false, false, None,
    ),
    (
        "tabindex",
        true,
        false,
        false,
        false,
        "tabindex",
        false,
        false,
        false,
        Some("tabIndex"),
    ),
    (
        "tabIndex", false, false, false, false, "tabindex", false, false, true, None,
    ),
    (
        "htmlFor", false, false, false, false, "for", false, false, false, None,
    ),
    (
        "ariaLabel",
        false,
        false,
        false,
        false,
        "aria-label",
        false,
        false,
        false,
        None,
    ),
    (
        "fooBar", false, false, false, false, "foo-bar", false, false, false, None,
    ),
    (
        "aria-flowto",
        false,
        true,
        false,
        false,
        "aria-flowto",
        true,
        false,
        false,
        None,
    ),
    (
        "for", false, false, false, false, "for", true, false, false, None,
    ),
    (
        "is", true, false, false, false, "is", false, true, false, None,
    ),
];

#[test]
fn predicates_match_upstream() {
    for &(n, global, aria, bool_div, bool_input, p2a, idref, disallowed, reflective, ambiguous) in
        PREDICATE_ROWS
    {
        let got = (
            is_global_html_attribute(n),
            is_aria_attribute(n),
            is_boolean_attribute(n, "div"),
            is_boolean_attribute(n, "input"),
            html_property_to_attribute(n),
            is_id_referencing_attribute(n),
            is_disallowed_prop(n),
            is_reflective_global_property(n),
            ambiguous_prop(n),
        );
        assert_eq!(
            got,
            (
                global,
                aria,
                bool_div,
                bool_input,
                p2a.into(),
                idref,
                disallowed,
                reflective,
                ambiguous
            ),
            "{n}"
        );
    }
    assert_eq!(
        aria_prop_name_to_attr_name("ariaBrailleRoleDescription"),
        Some("aria-brailleroledescription")
    );
    assert_eq!(aria_attr_name_to_prop_name("aria-label"), Some("ariaLabel"));
    assert_eq!(special_property_to_attribute("htmlFor"), Some("for"));
    assert_eq!(special_attribute_to_property("for"), Some("htmlFor"));
    assert_eq!(
        ARIA_PROPERTY_NAMES.len(),
        ARIA_PROPERTY_NAMES
            .iter()
            .filter_map(|p| aria_prop_name_to_attr_name(p))
            .count()
    );
}
