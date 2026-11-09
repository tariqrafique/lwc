//! AB-L0022 / AB-X0011 parity: JS whitespace (acorn `isWhiteSpace`, `/\\s/`, `String.prototype.trim`)
//! includes U+FEFF and excludes U+0085 (NEL), unlike `char::is_whitespace`. Rows recorded from upstream
//! @lwc/template-compiler compile() at /workspace/lwc b9e8007 (file `x.html`, default options):
//! `(source, "code:message" of each diagnostic joined by `|`, code with ASCII whitespace and the version
//! banner stripped from after the `lwc` import)`. NEL never reaches an expression or attribute check:
//! parse5 rejects it first (LWC1058 control-character-in-input-stream), so those rows pin that.

const ROWS: &[(&str, &str, &str)] = &[
    ("<template><p>\u{85}</p></template>", "1058:LWC1058: Invalid HTML syntax: control-character-in-input-stream. For more information, please visit https://html.spec.whatwg.org/multipage/parsing.html#parse-error-control-character-in-input-stream", ""),
    ("<template><p>\u{feff}</p></template>", "", "const$fragment1=parseFragment`<p${3}></p>`;functiontmpl($api,$cmp,$slotset,$ctx){const{st:api_static_fragment}=$api;return[api_static_fragment($fragment1,1)];}exportdefaultregisterTemplate(tmpl);tmpl.stylesheets=[];tmpl.stylesheetToken=\"lwc-anv79rp820\";if(_implicitStylesheets){tmpl.stylesheets.push.apply(tmpl.stylesheets,_implicitStylesheets);}if(_implicitScopedStylesheets){tmpl.stylesheets.push.apply(tmpl.stylesheets,_implicitScopedStylesheets);}freezeTemplate(tmpl);"),
    ("<template><p>\u{feff}\u{a0}\u{2028}</p></template>", "", "const$fragment1=parseFragment`<p${3}></p>`;functiontmpl($api,$cmp,$slotset,$ctx){const{st:api_static_fragment}=$api;return[api_static_fragment($fragment1,1)];}exportdefaultregisterTemplate(tmpl);tmpl.stylesheets=[];tmpl.stylesheetToken=\"lwc-anv79rp820\";if(_implicitStylesheets){tmpl.stylesheets.push.apply(tmpl.stylesheets,_implicitStylesheets);}if(_implicitScopedStylesheets){tmpl.stylesheets.push.apply(tmpl.stylesheets,_implicitScopedStylesheets);}freezeTemplate(tmpl);"),
    ("<template><p>{\u{feff}foo\u{feff}}</p></template>", "", "const$fragment1=parseFragment`<p${3}>${\"t1\"}</p>`;functiontmpl($api,$cmp,$slotset,$ctx){const{d:api_dynamic_text,sp:api_static_part,st:api_static_fragment}=$api;return[api_static_fragment($fragment1,1,[api_static_part(1,null,api_dynamic_text($cmp.foo))])];}exportdefaultregisterTemplate(tmpl);tmpl.stylesheets=[];tmpl.stylesheetToken=\"lwc-anv79rp820\";if(_implicitStylesheets){tmpl.stylesheets.push.apply(tmpl.stylesheets,_implicitStylesheets);}if(_implicitScopedStylesheets){tmpl.stylesheets.push.apply(tmpl.stylesheets,_implicitScopedStylesheets);}freezeTemplate(tmpl);"),
    ("<template><p>{foo\u{85}}</p></template>", "1058:LWC1058: Invalid HTML syntax: control-character-in-input-stream. For more information, please visit https://html.spec.whatwg.org/multipage/parsing.html#parse-error-control-character-in-input-stream", ""),
    ("<template><p>{\u{85}foo}</p></template>", "1058:LWC1058: Invalid HTML syntax: control-character-in-input-stream. For more information, please visit https://html.spec.whatwg.org/multipage/parsing.html#parse-error-control-character-in-input-stream", ""),
    ("<template><p id=\"a\u{85}b\"></p></template>", "1058:LWC1058: Invalid HTML syntax: control-character-in-input-stream. For more information, please visit https://html.spec.whatwg.org/multipage/parsing.html#parse-error-control-character-in-input-stream", ""),
    ("<template><p id=\"a\u{feff}b\"></p></template>", "1040:LWC1040: Invalid id value \"a\u{feff}b\". Id values must not contain any whitespace.", ""),
    ("<template><x-c a={\u{feff}true}></x-c></template>", "1060:Invalid expression {\u{feff}true} - LWC1060: Template expression doesn't allow Literal", ""),
    ("<template><x-c a={\u{feff}obj\u{feff}.\u{feff}b}></x-c></template>", "", "functiontmpl($api,$cmp,$slotset,$ctx){const{c:api_custom_element}=$api;return[api_custom_element(\"x-c\",_xC,{props:{\"a\":$cmp.obj.b},key:0})];}exportdefaultregisterTemplate(tmpl);tmpl.stylesheets=[];tmpl.stylesheetToken=\"lwc-anv79rp820\";if(_implicitStylesheets){tmpl.stylesheets.push.apply(tmpl.stylesheets,_implicitStylesheets);}if(_implicitScopedStylesheets){tmpl.stylesheets.push.apply(tmpl.stylesheets,_implicitScopedStylesheets);}freezeTemplate(tmpl);"),
    ("<template><x-c a={obj.\u{85}b}></x-c></template>", "1058:LWC1058: Invalid HTML syntax: control-character-in-input-stream. For more information, please visit https://html.spec.whatwg.org/multipage/parsing.html#parse-error-control-character-in-input-stream", ""),
];

fn strip(code: &str) -> String {
    let mut s: String = code.chars().filter(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}' | '\u{b}')).collect();
    if let Some(a) = s.find("/*LWCcompilerv") { let b = a + s[a..].find("*/").unwrap() + 2; s.replace_range(a..b, ""); }
    let from = s.find("from\"lwc\";").map_or(0, |i| i + 10);
    s[from..].to_string()
}

#[test]
fn nel_and_feff_like_upstream() {
    for (src, want_diags, want_code) in ROWS {
        let r = lwc_template::compile(src, "x.html", Default::default()).unwrap();
        let diags: Vec<String> = r.warnings.iter().map(|w| format!("{}:{}", w.code.unwrap_or(0), w.message)).collect();
        assert_eq!(diags.join("|"), *want_diags, "{src:?}");
        if want_diags.is_empty() { assert_eq!(strip(&r.code), *want_code, "{src:?}"); }
    }
}
