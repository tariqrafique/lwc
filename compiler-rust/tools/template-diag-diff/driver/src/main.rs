//! Rust side of harness/scripts/template-diag-diff.mjs: reads cases.jsonl
//! (`{id, src, filename, opts}` per line, opts in JS `Config` shape), runs
//! `lwc_template::compile` for ids in [from, to), prints one JSON line per case:
//! `{id, threw?, panic?, diags:[{code,level,message,location}], codeHash, codeLen}`.
//! Emits `{"start":id}` before each case so the caller can attribute an abort/hang.
use lwc_shared::{CustomRendererConfig, CustomRendererElementConfig};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// FNV-1a 32 of the code with the `/*LWC compiler vX*/` banner and all ASCII whitespace removed
/// (same normalization as the JS side; codegen-only is a count, not an exact codegen gate).
fn fnv1a(s: &str) -> u32 {
    let mut s = s.to_string();
    while let Some(a) = s.find("/*LWC compiler v") {
        let b = s[a..].find("*/").map(|e| a + e + 2).unwrap_or(s.len());
        s.replace_range(a..b, "");
    }
    let mut h: u32 = 0x811c9dc5;
    for b in s.as_bytes().iter().filter(|b| !b.is_ascii_whitespace()) {
        h ^= *b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

fn config(o: &Value) -> lwc_template::Config {
    let b = |k: &str| o.get(k).and_then(|v| v.as_bool());
    let s = |k: &str| o.get(k).and_then(|v| v.as_str()).map(String::from);
    let crc = o.get("customRendererConfig").filter(|v| v.is_object()).map(|v| CustomRendererConfig {
        directives: v["directives"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
        elements: v["elements"].as_array().map(|a| a.iter().map(|e| CustomRendererElementConfig {
            tag_name: e["tagName"].as_str().unwrap_or_default().into(),
            namespace: e["namespace"].as_str().map(String::from),
            attributes: e["attributes"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        }).collect()).unwrap_or_default(),
    });
    lwc_template::Config {
        name: s("name"),
        namespace: s("namespace"),
        // JS getAPIVersionFromNumber snaps fractional versions down; floor is equivalent here.
        api_version: o.get("apiVersion").and_then(|v| v.as_f64()).map(|v| if v < 0.0 { 0 } else { v.floor().min(u32::MAX as f64) as u32 }),
        experimental_computed_member_expression: b("experimentalComputedMemberExpression"),
        experimental_complex_expressions: b("experimentalComplexExpressions"),
        experimental_dynamic_directive: b("experimentalDynamicDirective"),
        enable_dynamic_components: b("enableDynamicComponents"),
        preserve_html_comments: b("preserveHtmlComments"),
        enable_static_content_optimization: b("enableStaticContentOptimization"),
        enable_lwc_on: b("enableLwcOn"),
        disable_synthetic_shadow_support: b("disableSyntheticShadowSupport"),
        custom_renderer_config: crc,
        ..Default::default()
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (file, from, to) = (&a[1], a[2].parse::<u64>().unwrap(), a[3].parse::<u64>().unwrap());
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::io::stdout();
    let mut out = out.lock();
    let rd = std::io::BufReader::new(std::fs::File::open(file).expect("cases file"));
    for line in rd.lines() {
        let line = line.unwrap();
        let c: Value = serde_json::from_str(&line).unwrap();
        let id = c["id"].as_u64().unwrap();
        if id < from { continue; }
        if id >= to { break; }
        writeln!(out, "{}", json!({"start": id})).unwrap();
        out.flush().unwrap();
        let src = c["src"].as_str().unwrap_or_default().to_string();
        let filename = c["filename"].as_str().unwrap_or("foo.html").to_string();
        let cfg = config(&c["opts"]);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || lwc_template::compile(&src, &filename, cfg)));
        let v = match r {
            Err(p) => {
                let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                json!({"id": id, "panic": msg})
            }
            Ok(Err(e)) => json!({"id": id, "threw": e.message}),
            Ok(Ok(r)) => json!({
                "id": id,
                "diags": r.warnings.iter().map(|d| json!({
                    "code": d.code, "level": d.level.js_value(), "message": d.message,
                    "location": d.location.map(|l| json!({"line": l.start_line, "column": l.start_column, "start": l.start, "length": l.end.zip(l.start).map(|(e, s)| e - s)})),
                })).collect::<Vec<_>>(),
                "codeHash": fnv1a(&r.code), "codeLen": r.code.len(), "code": if std::env::var("TDD_DRIVER_CODE").is_ok() { Some(r.code.clone()) } else { None },
            }),
        };
        writeln!(out, "{v}").unwrap();
    }
}
