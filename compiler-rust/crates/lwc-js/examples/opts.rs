//! Dev driver: `cargo run -p lwc-js --example opts -- <file> [lws] [sm|inline] [api=N] [explicit] [private]`
//! (namespace x, name foo, like the shipped oracle script). Prints code, then `MAP <json>` if any.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = std::fs::read_to_string(&a[1]).unwrap();
    let mut o = lwc_shared::CompileOptions {
        name: "foo".into(),
        namespace: "x".into(),
        ..Default::default()
    };
    for f in &a[2..] {
        match f.as_str() {
            "lws" => o.enable_lightning_web_security_transforms = true,
            "sm" => o.sourcemap = lwc_shared::Sourcemap::On,
            "inline" => o.sourcemap = lwc_shared::Sourcemap::Inline,
            "explicit" => o.is_explicit_import = true,
            "private" => o.enable_private_methods = true,
            s if s.starts_with("api=") => o.api_version = Some(s[4..].parse().unwrap()),
            s => panic!("unknown flag {s}"),
        }
    }
    let name = std::path::Path::new(&a[1])
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    match lwc_js::transform(&src, &name, &o) {
        Ok(r) => {
            print!("{}", r.code);
            if let Some(m) = r.map {
                print!("\nMAP {m}");
            }
        }
        Err(e) => println!("ERR {e}"),
    }
}
