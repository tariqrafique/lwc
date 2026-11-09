//! @lwc/template-compiler `src/shared/naming.ts`.

/// `kebabcaseToCamelcase(name)` (shared/naming.ts:7): the first `-` becomes `/`, each later `-` is
/// dropped and uppercases the next UTF-16 unit (JS semantics: full Unicode for BMP chars, astral
/// chars unchanged). `x-foo-bar` -> `x/fooBar`.
pub fn kebabcase_to_camelcase(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut ns_found = false;
    let mut upper = false;
    for c in name.chars() {
        if c == '-' {
            if !ns_found {
                ns_found = true;
                out.push('/');
            } else {
                upper = true;
            }
        } else {
            if upper {
                super::push_upper_utf16_unit(&mut out, c);
            } else {
                out.push(c);
            }
            upper = false;
        }
    }
    out
}
