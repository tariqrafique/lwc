//! ORACLE_DIR has ONE source: `export const LWC` in harness/src/ssrRender.ts, the path the
//! exec runner reaches engine-dom and @lwc/compiler through (prepare.mjs imports it). AB-L0051 (2): derive, do not copy.
//! Shapes: `export const LWC = '<abs dir>/packages/@lwc';` (legacy literal) or
//! `export const LWC = `${oracleRoot()}/packages/@lwc`;` (D-0280; SEMDIFF_ORACLE_DIR from LWC_ORACLE,
//! or fail if LWC_ORACLE is unset). Any other shape fails the build; there is no other default.
use std::fs;
use std::path::Path;

const SRC: &str = "../../harness/src/ssrRender.ts";
const DECL: &str = "export const LWC = '";
const SUFFIX: &str = "/packages/@lwc';";
const PORTABLE: &str = "export const LWC = `${oracleRoot()}/packages/@lwc`;";

fn oracle_from_env() -> String {
    match std::env::var("LWC_ORACLE") {
        Ok(v) if !v.is_empty() => v,
        _ => panic!("semdiff build: LWC uses oracleRoot(); set LWC_ORACLE via harness/scripts/oracle-root.mjs (D-0280(b))"),
    }
}

fn main() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SRC);
    println!("cargo:rerun-if-changed={}", path.display());
    println!("cargo:rerun-if-env-changed=LWC_ORACLE");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("semdiff build: read {}: {e}", path.display()));
    let decls: Vec<&str> = text.lines().filter(|l| l.contains("const LWC ") || l.contains("const LWC=")).collect();
    let [line] = decls.as_slice() else {
        panic!("semdiff build: want exactly one `const LWC` in {}, found {decls:?}", path.display());
    };
    let root = if line.trim() == PORTABLE {
        oracle_from_env()
    } else {
        line
            .strip_prefix(DECL)
            .and_then(|r| r.strip_suffix(SUFFIX))
            .filter(|r| r.starts_with('/') && !r.is_empty() && !r.contains(['\'', '"', '`', '$', '\\']))
            .map(str::to_string)
            .unwrap_or_else(|| panic!("semdiff build: {} has `{line}`, want `{DECL}<abs dir>{SUFFIX}` or `{PORTABLE}`", path.display()))
    };
    println!("cargo:rustc-env=SEMDIFF_ORACLE_DIR={root}");
}
