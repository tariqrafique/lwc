//! Dump a transform error as JSON (proof tooling). stdin = source; args: [epm] [rec] [filename=..] [name=..] [namespace=..]
use lwc_shared::CompileOptions;
use std::io::Read;

fn q(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
fn n(v: Option<u32>) -> String {
    v.map(|x| x.to_string()).unwrap_or("null".into())
}

fn main() {
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src).unwrap();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut o = CompileOptions::default();
    o.name = "foo".into();
    o.namespace = "x".into();
    let mut file = "/abs/dir/foo.js".to_string();
    for a in &args {
        match a.as_str() {
            "epm" => o.enable_private_methods = true,
            "rec" => o.experimental_error_recovery_mode = true,
            a if a.starts_with("filename=") => file = a["filename=".len()..].to_string(),
            a if a.starts_with("name=") => o.name = a["name=".len()..].to_string(),
            a if a.starts_with("namespace=") => o.namespace = a["namespace=".len()..].to_string(),
            _ => {}
        }
    }
    // The error `transformSync` throws (the lwc-compiler JS facade): one CompilerError, or the aggregate's.
    match lwc_compiler::dispatch(&src, &file, &o) {
        Ok(_) => println!("\"NO ERROR\""),
        Err(e) => {
            let (message, errors) = match e {
                lwc_shared::TransformError::Compiler(e) => (e.message.clone(), vec![e]),
                lwc_shared::TransformError::Aggregate(a) => (a.message, a.errors),
                lwc_shared::TransformError::Unnormalized(e) => (e.message, vec![]),
            };
            let loc = |d: &lwc_shared::CompilerError| match d.location {
                Some(l) => format!(
                    "{{\"line\":{},\"column\":{},\"start\":{},\"length\":{}}}",
                    l.line,
                    l.column,
                    n(l.start),
                    n(l.length)
                ),
                None => "null".into(),
            };
            let first = errors.first();
            let errs: Vec<String> = errors
                .iter()
                .map(|d| {
                    format!(
                        "{{\"code\":{},\"message\":{},\"location\":{}}}",
                        n(d.code),
                        q(&d.message),
                        loc(d)
                    )
                })
                .collect();
            let at = first.and_then(|d| d.location);
            println!(
                "{{\"code\":{},\"line\":{},\"column\":{},\"location\":{},\"message\":{},\"diagnostics\":[{}]}}",
                n(first.and_then(|d| d.code)),
                n(at.map(|l| l.line)),
                n(at.map(|l| l.column)),
                first.map(loc).unwrap_or("null".into()),
                q(&message),
                errs.join(",")
            );
        }
    }
}
