//! An anonymous class expression bound by `const`/`let B = class { ... }` that refers to `B` in its
//! body. Babel names the lowered class `class B`, which would capture those references, so it renames
//! the outer binding (`_B`, `export { _B as B }`). Compared at RUNTIME, not bytes: the source module
//! (run natively by node, which supports class fields), the oracle `@lwc/compiler` output and the
//! Rust output must print the same observations. Checked: `B` inside a static / an instance arrow /
//! a method is the binding (live after reassignment, seen through an importer), the TDZ while the
//! class is defined, `B.name`, and per-instance arrow identity.
use lwc_shared::CompileOptions;
use std::path::Path;
use std::process::Command;

fn opts() -> CompileOptions {
    CompileOptions {
        name: "foo".into(),
        namespace: "x".into(),
        ..Default::default()
    }
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

/// Write `module` as m.mjs next to `driver` (which imports './m.mjs') and print the driver's stdout.
fn run(dir: &Path, tag: &str, module: &str, driver: &str) -> String {
    let d = dir.join(tag);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("m.mjs"), module).unwrap();
    std::fs::write(d.join("main.mjs"), driver).unwrap();
    let out = Command::new("node")
        .arg(d.join("main.mjs"))
        .output()
        .expect("node on PATH");
    assert!(
        out.status.success(),
        "{tag} failed:\n{}\n--- module ---\n{module}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn same_at_runtime(name: &str, src: &str, driver: &str, expected: &str) {
    let dir = std::env::temp_dir().join(format!(
        "lwc-js-class-self-ref-{}-{name}",
        std::process::id()
    ));
    let rust = lwc_js::transform(src, "foo.js", &opts()).unwrap().code;
    let oracle = oracle_code(src);
    let s = run(&dir, "source", src, driver);
    let o = run(&dir, "oracle", &oracle, driver);
    let r = run(&dir, "rust", &rust, driver);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(s, expected, "{name}: source semantics changed?");
    assert_eq!(o, s, "{name}: oracle differs from source\n{oracle}");
    assert_eq!(
        r, s,
        "{name}: rust differs from source/oracle\n--- rust ---\n{rust}\n--- oracle ---\n{oracle}"
    );
}

#[test]
fn const_static_self_reference() {
    same_at_runtime(
        "const-static",
        "export const B = class { static y = 2; static z = () => B; };\n",
        "import { B } from './m.mjs';\n\
         console.log(JSON.stringify([B.z() === B, B.name, B.y]));\n",
        r#"[true,"B",2]"#,
    );
}

#[test]
fn static_initializer_sees_the_binding_in_its_tdz() {
    same_at_runtime(
        "tdz",
        "export const B = class {\n\
           static w = (() => { try { return B === undefined ? 'undef' : 'class'; } catch (e) { return e.constructor.name; } })();\n\
         };\n",
        "import { B } from './m.mjs';\nconsole.log(JSON.stringify([B.w, B.name]));\n",
        r#"["ReferenceError","B"]"#,
    );
}

#[test]
fn let_binding_is_live_through_an_importer() {
    same_at_runtime(
        "let-live",
        "export let B = class { static y = 2; static z = () => B; m() { return B; } };\n\
         export const Orig = B;\n\
         export function reassign(v) { B = v; }\n",
        "import { B, Orig, reassign } from './m.mjs';\n\
         const before = [Orig.z() === Orig, new Orig().m() === Orig, B === Orig, Orig.name];\n\
         reassign(5);\n\
         console.log(JSON.stringify([...before, B, Orig.z(), new Orig().m()]));\n",
        r#"[true,true,true,"B",5,5,5]"#,
    );
}

#[test]
fn instance_field_arrow_self_reference() {
    same_at_runtime(
        "instance-arrow",
        "export const B = class { f = () => B; };\n",
        "import { B } from './m.mjs';\n\
         const a = new B(), b = new B();\n\
         console.log(JSON.stringify([a.f() === B, b.f() === B, a.f !== b.f, B.name]));\n",
        r#"[true,true,true,"B"]"#,
    );
}

#[test]
fn let_instance_arrow_and_static_follow_reassignment() {
    same_at_runtime(
        "let-instance-arrow",
        "export let B = class { f = () => B; static g = () => B; };\n\
         export const Orig = B;\n\
         export function reassign(v) { B = v; }\n",
        "import { B, Orig, reassign } from './m.mjs';\n\
         const a = new Orig();\n\
         const before = [a.f() === Orig, Orig.g() === Orig];\n\
         reassign(7);\n\
         console.log(JSON.stringify([...before, a.f(), Orig.g(), new Orig().f(), B]));\n",
        r#"[true,true,7,7,7,7]"#,
    );
}

#[test]
fn an_export_list_and_shorthand_see_the_renamed_binding() {
    same_at_runtime(
        "export-list",
        "let B = class { static y = 2; static z = () => B; };\n\
         export { B };\n\
         export const o = { B };\n\
         export function reassign(v) { B = v; }\n",
        "import { B, o, reassign } from './m.mjs';\n\
         const C = B;\n\
         const before = [o.B === B, C.z() === C, C.name];\n\
         reassign(3);\n\
         console.log(JSON.stringify([...before, B, C.z(), o.B === C]));\n",
        r#"[true,true,"B",3,3,true]"#,
    );
}

#[test]
fn no_self_reference_keeps_the_binding_name() {
    same_at_runtime(
        "no-self-ref",
        "export let B = class { static y = 2; x = 1; };\n\
         export const Orig = B;\n\
         export function reassign(v) { B = v; }\n",
        "import { B, Orig, reassign } from './m.mjs';\n\
         const before = [B.y, new B().x, B.name];\n\
         reassign(1);\n\
         console.log(JSON.stringify([...before, B, Orig.name]));\n",
        r#"[2,1,"B",1,"B"]"#,
    );
}
