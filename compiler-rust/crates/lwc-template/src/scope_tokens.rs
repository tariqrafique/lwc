//! Port of `@lwc/template-compiler/src/scopeTokens.ts`.

use std::path::Path;

/// Same 53-bit hash as TS `generateHashCode` (cyrb53, seed 0), over UTF-16 code units.
pub fn generate_hash_code(s: &str) -> u64 {
    let imul = |a: u32, b: u32| a.wrapping_mul(b);
    let mut h1: u32 = 0xdeadbeef;
    let mut h2: u32 = 0x41c6ce57;
    for ch in s.encode_utf16() {
        h1 = imul(h1 ^ ch as u32, 2654435761);
        h2 = imul(h2 ^ ch as u32, 1597334677);
    }
    h1 = imul(h1 ^ (h1 >> 16), 2246822507);
    h1 ^= imul(h2 ^ (h2 >> 13), 3266489909);
    h2 = imul(h2 ^ (h2 >> 16), 2246822507);
    h2 ^= imul(h1 ^ (h1 >> 13), 3266489909);
    4294967296 * (2097151 & h2 as u64) + h1 as u64
}

/// JS `Number.prototype.toString(32)` for non-negative integers.
fn to_radix_32(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuv";
    if n == 0 {
        return "0".into();
    }
    let mut v = Vec::new();
    while n > 0 {
        v.push(D[(n % 32) as usize]);
        n /= 32;
    }
    v.reverse();
    String::from_utf8(v).unwrap()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeTokens {
    pub scope_token: String,
    pub legacy_scope_token: String,
    pub css_scope_tokens: Vec<String>,
}

/// `None` formats as `"undefined"` to match the JS template string.
pub fn generate_scope_tokens(
    filename: &str,
    namespace: Option<&str>,
    component_name: Option<&str>,
) -> ScopeTokens {
    // path.basename(f, path.extname(f)): strip last extension (not for dotfiles).
    let base = Path::new(filename)
        .file_name()
        .and_then(|b| b.to_str())
        .unwrap_or("");
    let stem = match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    };
    let unique = format!(
        "{}-{}_{}",
        namespace.unwrap_or("undefined"),
        component_name.unwrap_or("undefined"),
        stem
    );
    let scope_token = format!("lwc-{}", to_radix_32(generate_hash_code(&unique)));
    let legacy_scope_token = unique.replace('@', "___at___").replace('#', "___hash___");
    let css_scope_tokens = vec![
        scope_token.clone(),
        format!("{scope_token}-host"),
        legacy_scope_token.clone(),
        format!("{legacy_scope_token}-host"),
    ];
    ScopeTokens {
        scope_token,
        legacy_scope_token,
        css_scope_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_token() {
        let t = generate_scope_tokens("path/to/valid.html", Some("x"), Some("valid"));
        assert!(t.scope_token.starts_with("lwc-"));
        assert_eq!(
            t.scope_token,
            generate_scope_tokens("valid.html", Some("x"), Some("valid")).scope_token
        );
        assert_eq!(t.scope_token, SNAPSHOT);
        assert_eq!(
            generate_scope_tokens("a.html", None, None).legacy_scope_token,
            "undefined-undefined_a"
        );
        assert_eq!(
            generate_scope_tokens("a.html", Some("@x"), Some("#y")).legacy_scope_token,
            "___at___x-___hash___y_a"
        );
    }

    // Cross-checked against the TS algorithm run in node.
    const SNAPSHOT: &str = "lwc-2jeqhu792ub";
}
