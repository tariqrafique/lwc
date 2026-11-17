//! The static hoist walks the AST built during emit. Compiling a template that prints a static
//! object or array does not parse that printed text.
use lwc_template::Config;

fn count(src: &str) -> (u32, String) {
    let cfg = Config {
        enable_static_content_optimization: Some(false),
        ..Default::default()
    };
    lwc_template::reset_parse_count();
    let code = lwc_template::compile(src, "x.html", cfg).unwrap().code;
    (lwc_template::parse_count(), code)
}

#[test]
fn static_object_and_array_do_not_parse_printed_text() {
    // classMap / key are static objects produced by codegen, not by a source-expression parse.
    let (n, code) = count("<template><div class=\"a\"></div></template>");
    assert!(code.contains("const stc0"), "{code}");
    assert_eq!(n, 0, "printed tmpl text was parsed");

    // styleDecls is a static array of arrays.
    let (n, code) = count("<template><div style=\"color: red\"></div></template>");
    assert!(code.contains("const stc0"), "{code}");
    assert_eq!(n, 0, "printed tmpl text was parsed");
}

#[test]
fn source_expression_parse_is_not_the_hoist() {
    // classMap adds a static object to the printed tmpl. The source expression `{foo.bar}` is parsed
    // the same number of times with or without that object: the hoist does not parse the printed text.
    let (member, _) = count("<template><p>{ foo.bar }</p></template>");
    let (both, code) = count("<template><div class=\"a\">{ foo.bar }</div></template>");
    assert!(code.contains("const stc0"), "{code}");
    assert_eq!(both, member);
    assert!(member > 0);
}
