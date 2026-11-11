fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = std::fs::read_to_string(&a[1]).unwrap();
    let o = lwc_shared::CompileOptions {
        name: "test".into(),
        namespace: "lwc".into(),
        ..Default::default()
    };
    match lwc_js::transform(&src, "test.js", &o) {
        Ok(r) => print!("{}", r.code),
        Err(e) => println!("ERR {e}"),
    }
}
