//! Mirrors @lwc/template-compiler `src/shared/`: the helpers ssr-compiler imports from
//! '@lwc/template-compiler' (ssr-compiler compile-template/transformers/component/component.ts:9).
//! AB-0011 a1 adds them with upstream semantics; callers move here in a2 (lwc-template) and c (lwc-ssr).
pub mod naming;
pub mod utils;

/// JS `s.charAt(i).toUpperCase()` on one UTF-16 code unit: full Unicode uppercase for a BMP char
/// (`ß` -> `SS`); a char above U+FFFF is two units, and uppercasing a lone surrogate is a no-op,
/// so it stays as it is.
fn push_upper_utf16_unit(out: &mut String, c: char) {
    if (c as u32) > 0xFFFF {
        out.push(c);
    } else {
        out.extend(c.to_uppercase());
    }
}
