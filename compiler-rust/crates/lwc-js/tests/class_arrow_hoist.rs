//! Instance-field arrow whose body is a class with statics. The `_Class` binding belongs to the
//! arrow, not the constructor: each call returns a distinct class, a static that closes over the
//! class does not observe a later call, and two instances do not share it. Compared with the
//! oracle `@lwc/compiler` output.
use lwc_shared::CompileOptions;
use std::process::Command;

fn opts() -> CompileOptions {
    CompileOptions {
        name: "foo".into(),
        namespace: "x".into(),
        ..Default::default()
    }
}

const SRC: &str = "\
export class Outer {\n\
  make = () => class {\n\
    static n = 1;\n\
    static later = () => this;\n\
  };\n\
}\n";

const DRIVER: &str = "\
const a = new Outer();\n\
const A1 = a.make();\n\
const A2 = a.make();\n\
const b = new Outer();\n\
const B1 = b.make();\n\
A1.n = 9;\n\
const lines = [\n\
  A1 !== A2 && A1 !== B1 && A2 !== B1,\n\
  A2.n === 1 && B1.n === 1 && A1.n === 9,\n\
  A1.later() === A1,\n\
  A2.later() === A2,\n\
  B1.later() === B1,\n\
];\n\
console.log(lines.map((x) => (x ? '1' : '0')).join(''));\n";

fn run(module: &str) -> String {
    let src = format!("{module}\n;{DRIVER}\n");
    let out = Command::new("node")
        .args(["--input-type=module", "-e", &src])
        .output()
        .expect("node on PATH");
    assert!(
        out.status.success(),
        "node failed:\n{}\n--- module ---\n{src}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn oracle_code(src: &str) -> String {
    let root = std::env::var("LWC_ORACLE").expect("LWC_ORACLE");
    let script = format!(
        r#"
import {{ transformSync }} from '{root}/packages/@lwc/compiler/dist/index.js';
const r = transformSync({src:?}, 'foo.js', {{ name: 'foo', namespace: 'x' }});
process.stdout.write(r.code);
"#
    );
    let out = Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .output()
        .expect("node");
    assert!(
        out.status.success(),
        "oracle failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn arrow_class_statics_do_not_leak() {
    let rust = lwc_js::transform(SRC, "foo.js", &opts()).unwrap().code;
    let rust_out = run(&rust).trim().to_string();
    let oracle = oracle_code(SRC);
    let oracle_out = run(&oracle).trim().to_string();
    assert_eq!(rust_out, oracle_out, "rust:\n{rust}\n\noracle:\n{oracle}");
    assert_eq!(
        rust_out, "11111",
        "distinct classes, statics stay put, later() sees its own class"
    );
}
