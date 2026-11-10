//! D-0265 / AB-L0074: each component is parsed twice (user source + the pre-print parse).
use lwc_shared::CompileOptions;

fn opts() -> CompileOptions {
    CompileOptions { name: "foo".into(), namespace: "x".into(), ..Default::default() }
}

fn lws() -> CompileOptions {
    CompileOptions { enable_lightning_web_security_transforms: true, ..opts() }
}

fn count(src: &str, o: &CompileOptions) -> u32 {
    lwc_js::reset_parse_count();
    let _ = lwc_js::transform(src, "foo.js", o);
    lwc_js::parse_count()
}

#[test]
fn each_component_is_parsed_twice() {
    let cases: &[(&str, CompileOptions)] = &[
        ("export default class A { x = 1; }", opts()),
        ("export default class A { #p = 1; get x(){ return this.#p; } }", opts()),
        ("export default class A { inner = class { z = 9; }; }", opts()),
        ("export default class A { inner = class { [k] = 1; z = 2; }; }", opts()),
        ("export default class A { m(){ class B { y = 1; } } x = 1; }", opts()),
        ("const C = class { #p = 1; static s = 2; }; export default C;", opts()),
        ("export default class A { #m(){ return 1; } x = this.#m(); }", CompileOptions { enable_private_methods: true, ..opts() }),
        ("export default class A { m(){ return foo.location; } }", lws()),
        ("export function f(x){ return { ...x }.location; }", CompileOptions { api_version: Some(59), enable_lightning_web_security_transforms: true, ..opts() }),
        ("export default class A extends LightningElement { #p = 1 /* async */ + 2; get x(){ return this.#p /* async */; } }", lws()),
        ("export default class A extends LightningElement { #p = 1; async m() { return this.#p; } }", lws()),
        ("export default class A { async m(){ return foo.location; } }", lws()),
    ];
    for (src, o) in cases {
        assert_eq!(count(src, o), 2, "parses for {src}");
    }
}
