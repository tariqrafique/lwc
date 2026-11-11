mod support;
use lwc_shared::CompileOptions;

fn opts() -> CompileOptions {
    CompileOptions {
        name: "test".into(),
        namespace: "lwc".into(),
        ..Default::default()
    }
}
fn t(src: &str, o: &CompileOptions) -> String {
    lwc_js::transform(src, "test.js", o).unwrap().code
}
const BASIC: &str = "import { LightningElement } from 'lwc';\nexport default class Test extends LightningElement {}\n";

#[test]
fn round_trip_is_stable() {
    let once = t("const a = 1;\nexport { a };\n", &opts());
    assert_eq!(t(&once, &opts()), once);
    assert!(!once.contains("registerComponent"));
}

#[test]
fn registers_basic_component() {
    let out = t(BASIC, &opts());
    assert!(out.contains("import _tmpl from \"./test.html\""), "{out}");
    assert!(
        out.contains("registerComponent as _registerComponent } from \"lwc\""),
        "{out}"
    );
    assert!(
        out.contains("const __lwc_component_class_internal = _registerComponent(Test, {"),
        "{out}"
    );
    assert!(out.contains("sel: \"lwc-test\""), "{out}");
    assert!(out.contains("apiVersion: 66"), "{out}");
    assert!(
        out.contains("class Test extends LightningElement {/*LWC compiler v9.5.3*/}"),
        "{out}"
    );
    assert!(
        out.contains("export default __lwc_component_class_internal"),
        "{out}"
    );
}

#[test]
fn template_basename_and_kebab_sel_and_api_version() {
    let o = CompileOptions {
        name: "fooBar".into(),
        namespace: "x".into(),
        api_version: Some(61),
        ..Default::default()
    };
    let out = lwc_js::transform(BASIC, "src/fooBar/fooBar.ts", &o)
        .unwrap()
        .code;
    assert!(out.contains("\"./fooBar.html\""), "{out}");
    assert!(out.contains("sel: \"x-foo-bar\""), "{out}");
    assert!(out.contains("apiVersion: 61"), "{out}");
}

#[test]
fn explicit_import_skips_registration() {
    let out = t(
        BASIC,
        &CompileOptions {
            is_explicit_import: true,
            ..opts()
        },
    );
    assert!(
        !out.contains("registerComponent") && !out.contains(".html"),
        "{out}"
    );
    assert!(out.contains("/*LWC compiler v"), "{out}");
}

#[test]
fn optional_meta_keys() {
    let o = CompileOptions {
        enable_private_methods: true,
        enable_synthetic_element_internals: true,
        component_feature_flag_module_path: "@salesforce/featureFlag/x".into(),
        ..opts()
    };
    let out = t(
        "import { LightningElement } from 'lwc';\nexport default class extends LightningElement { #m() {} }\n",
        &o,
    );
    for k in [
        "enablePrivateMethods: true",
        "enableSyntheticElementInternals: true",
        "componentFeatureFlag: {",
        "Boolean(_componentFeatureFlag)",
        "#m()",
    ] {
        assert!(out.contains(k), "missing {k}: {out}");
    }
}

#[test]
fn private_methods_rejected_when_flag_off() {
    let e = support::thrown("class A { #m() {} }", "a.js", &opts());
    assert!(e.errors.iter().any(|d| d.code == Some(1007)), "{e:?}");
    assert!(e.message.contains("Class private methods are not enabled. Please add `@babel/plugin-transform-private-methods` to your configuration."), "{}", e.message);
}

#[test]
fn non_registrable_default_exports_untouched() {
    for src in [
        "export default null;",
        "export default undefined;",
        "export default { a: 1 };",
    ] {
        assert!(!t(src, &opts()).contains("registerComponent"), "{src}");
    }
    assert!(t("export default foo();", &opts()).contains("_registerComponent(foo(), {"));
}

#[test]
fn scoped_css_and_parse_errors() {
    assert!(t("import s from './a.scoped.css';", &opts()).contains("./a.scoped.css?scoped=true"));
    assert!(lwc_js::transform("let = ;", "a.js", &opts()).is_err());
}

fn err(src: &str) -> String {
    support::thrown(src, "test.js", &opts()).message
}

#[test]
fn decorator_not_imported_from_lwc_errors() {
    let e = err(
        "import { LightningElement } from 'lwc';\nimport { api } from 'other';\nexport default class T extends LightningElement { @api foo; }",
    );
    assert!(
        e.contains("Invalid 'api' decorator usage")
            && e.contains("should be imported from \"lwc\""),
        "{e}"
    );
    assert!(err("import { LightningElement } from 'lwc';\nexport default class T extends LightningElement { @wire(x) foo; }").contains("'wire'"));
}

#[test]
fn decorator_import_misuse_errors() {
    assert!(
        err("import { api } from 'lwc';\nconst x = api;")
            .contains("\"api\" can only be used as a class decorator")
    );
    assert!(err("import { track, LightningElement } from 'lwc';\nexport default class T extends LightningElement { @track m() {} }").contains("@track decorator can only be applied to class properties"));
    let e = err(
        "import { api, LightningElement } from 'lwc';\n@api class T extends LightningElement { x; }",
    );
    assert!(
        e.contains("LWC1103") && e.contains("can only be applied on class properties"),
        "{e}"
    );
}

#[test]
fn skip_register_decorators_without_superclass_by_api_version() {
    let src = "import { api } from 'lwc';\nexport class Plain { @api foo; }\n";
    let v59 = lwc_js::transform(
        src,
        "t.js",
        &CompileOptions {
            api_version: Some(59),
            ..opts()
        },
    )
    .unwrap()
    .code;
    assert!(
        v59.contains("_registerDecorators(Plain, { publicProps: { foo: { config: 0 } } })"),
        "{v59}"
    );
    // Default apiVersion: the class is skipped, so its `@api` stays and Babel class-features throws
    // "Decorators are not enabled." -> @lwc/compiler's LWC1198 sniff (shipped 9.5.3).
    let e = support::thrown(src, "test.js", &opts());
    assert_eq!(e.errors[0].code, Some(1198), "{}", e.message);
}

#[test]
fn class_expression_wrapped_and_aliased_import() {
    let out = t(
        "import { api as a, LightningElement } from 'lwc';\nconst C = class extends LightningElement { @a x; };\n",
        &opts(),
    );
    assert!(
        out.contains("const C = _registerDecorators(class extends LightningElement"),
        "{out}"
    );
    assert!(out.contains("publicProps: { x: { config: 0 } }"), "{out}");
    assert!(!out.contains("api as a"), "{out}");
}

fn pm() -> CompileOptions {
    CompileOptions {
        enable_private_methods: true,
        ..opts()
    }
}
const PM: &str = "import { LightningElement } from 'lwc';\nexport default class T extends LightningElement {\n  #m() { return 1; }\n  run() { return this.#m(); }\n}\n";

#[test]
fn private_method_survives_sandwich() {
    let out = t(PM, &pm());
    assert!(out.contains("#m()") && out.contains("this.#m()"), "{out}");
    assert!(
        !out.contains("__lwc_component_class_internal_private_"),
        "{out}"
    );
    assert!(out.contains("enablePrivateMethods: true"), "{out}");
}

#[test]
fn private_method_still_rejected_flag_off() {
    assert!(
        support::thrown(PM, "a.js", &opts())
            .message
            .contains("Class private methods are not enabled")
    );
}

#[test]
fn reserved_prefix_collision_errors_when_flag_on() {
    let src = "class A { __lwc_component_class_internal_private_x() {} }";
    let e = support::thrown(src, "a.js", &pm());
    assert!(
        e.message.contains("cannot start with reserved prefix"),
        "{}",
        e.message
    );
    assert!(lwc_js::transform(src, "a.js", &opts()).is_ok());
}

#[test]
fn decorated_private_method_and_private_accessor_error() {
    let e = support::thrown("class A { @api #m() {} }", "a.js", &pm());
    assert_eq!(e.errors[0].code, Some(1212), "{}", e.message);
    assert_eq!((e.line, e.column), (Some(1), Some(10)));
    let e = support::thrown("class A { get #v() { return 1; } }", "a.js", &pm());
    assert!(
        e.message.contains("Class private methods are not enabled"),
        "{}",
        e.message
    );
    assert_eq!(e.errors[0].code, Some(1007));
}

#[test]
fn field_init_moves_into_constructor() {
    let out = t("class A { x = 1; y; }", &opts());
    assert!(
        out.contains("constructor() {")
            && out.contains("this.x = 1;")
            && out.contains("this.y = void 0;"),
        "{out}"
    );
    assert!(!out.contains("super("), "{out}");
}

fn lwc_err(body: &str) -> support::Thrown {
    let src = format!(
        "import {{ api, track, wire, LightningElement }} from 'lwc';\nimport {{ getFoo }} from 'x';\nexport default class T extends LightningElement {{\n{body}\n}}"
    );
    support::thrown(&src, "test.js", &opts())
}

#[test]
fn api_validate_lwc_codes() {
    let cases: &[(&str, u32, &str)] = &[
        (
            "  @api class;",
            1110,
            "LWC1110: Invalid property name \"class\". \"class\" is a reserved attribute.",
        ),
        ("  @api slot;", 1110, "\"slot\" is a reserved attribute"),
        (
            "  @api foo; @api foo;",
            1096,
            "LWC1096: Duplicate @api property \"foo\".",
        ),
        (
            "  @api foo = true;",
            1099,
            "LWC1099: Boolean public property must default to false.",
        ),
        (
            "  @api dataFoo;",
            1107,
            "LWC1107: Invalid property name \"dataFoo\"",
        ),
        ("  @api onClick;", 1108, "LWC1108:"),
        (
            "  @api maxlength;",
            1109,
            "Consider renaming to \"maxLength\"",
        ),
        ("  @api part;", 1111, "LWC1111:"),
        (
            "  @api get s() { return 1; }\n  @api set s(v) {}",
            1112,
            "LWC1112: @api get s and @api set s detected",
        ),
        ("  @track @api x = 1;", 1093, "LWC1093:"),
        ("  @api ['x'] = 1;", 1106, "LWC1106:"),
        ("  @wire(getFoo) @wire(getFoo) x;", 1105, "LWC1105:"),
        (
            "  @wire(getFoo) @api x;",
            1095,
            "LWC1095: @wire method or property cannot be used with @api",
        ),
        ("  @wire() x;", 1092, "LWC1092:"),
        ("  @wire(getFoo, 1) x;", 1094, "LWC1094:"),
        ("  @track m() {}", 1113, "LWC1113:"),
    ];
    for (body, code, msg) in cases {
        let e = lwc_err(body);
        assert!(e.message.contains(msg), "{body}: {}", e.message);
        assert_eq!(e.errors.first().and_then(|d| d.code), Some(*code), "{body}");
    }
    let e = lwc_err("  @api class;");
    assert_eq!((e.line, e.column), (Some(4), Some(2)));
}

#[test]
fn wire_bigint_key_preserved() {
    let out = lwc_js::transform("import { wire, LightningElement } from 'lwc';\nimport { getFoo } from 'x';\nexport default class T extends LightningElement { @wire(getFoo, { [123n]: '$prop', b: 4n }) x; }", "test.js", &opts()).unwrap().code;
    assert!(
        out.contains("[123n]: $cmp.prop") && out.contains("b: 4n"),
        "{out}"
    );
}

fn rec() -> CompileOptions {
    CompileOptions {
        experimental_error_recovery_mode: true,
        ..opts()
    }
}

#[test]
fn error_recovery_single_error_is_aggregate() {
    let src = "import { LightningElement, api } from 'lwc';\nexport default class Foo extends LightningElement { @api dataInvalidProperty; }";
    let e = support::thrown(src, "foo.js", &rec());
    assert_eq!(e.name, Some("AggregateError"));
    assert_eq!(
        e.errors.iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![Some(1107)]
    );
    assert_eq!(e.message, "Multiple errors occurred during compilation.");
    assert!(
        e.errors[0]
            .message
            .starts_with("LWC1107: Invalid property name \"dataInvalidProperty\""),
        "{}",
        e.errors[0].message
    );
    // fail-fast without recovery
    let e = support::thrown(src, "foo.js", &opts());
    assert_eq!(e.name, None);
}

#[test]
fn error_recovery_collects_multiple() {
    let src = "import { LightningElement, api, track } from 'lwc';\nexport default class Foo extends LightningElement {\n@api onClickHandler;\n@api class;\n@track someMethod() { return 1; }\n@api dup; @api dup;\n}";
    let e = support::thrown(src, "foo.js", &rec());
    let codes: Vec<_> = e.errors.iter().filter_map(|d| d.code).collect();
    assert_eq!(codes.len(), e.errors.len());
    assert_eq!(codes, vec![1108, 1110, 1096, 1096, 1113], "{}", e.message);
    assert!(
        e.message
            .starts_with("Multiple errors occurred during compilation.")
    );
    let e = support::thrown(src, "foo.js", &opts());
    assert_eq!(
        e.errors.iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![Some(1108)]
    );
}

#[test]
fn error_recovery_ok_when_clean() {
    assert!(
        lwc_js::transform(BASIC, "test.js", &rec())
            .unwrap()
            .code
            .contains("registerComponent")
    );
}

#[test]
fn object_rest_spread_gated_by_api_version() {
    let src =
        "export const test = { ...a, b: 1 };\nexport function f({ x, ...rest }) { return rest; }\n";
    for (v, keep) in [
        (Some(59), false),
        (Some(58), false),
        (Some(60), true),
        (Some(66), true),
        (None, true),
    ] {
        let code = t(
            src,
            &CompileOptions {
                api_version: v,
                ..opts()
            },
        );
        assert!(code.contains("b: 1"), "{code}");
        assert_eq!(code.contains("...a"), keep, "{v:?}: {code}");
        assert_eq!(code.contains("...rest"), keep, "{v:?}: {code}");
    }
}

/// Babel resolves `filename` against cwd (`path.resolve(process.cwd(), filename)`) before the plugin sees it.
#[test]
fn resolve_matches_node_path_resolve() {
    let c = std::path::Path::new("/a/b");
    assert_eq!(lwc_js::resolve_like_node(c, "foo.js"), "/a/b/foo.js");
    assert_eq!(lwc_js::resolve_like_node(c, "./x/../foo.js"), "/a/b/foo.js");
    assert_eq!(lwc_js::resolve_like_node(c, "../foo.js"), "/a/foo.js");
    assert_eq!(
        lwc_js::resolve_like_node(c, "/abs//x/foo.js"),
        "/abs/x/foo.js"
    );
}
