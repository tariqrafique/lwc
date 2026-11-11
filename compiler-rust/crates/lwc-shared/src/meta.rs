//! `@lwc/shared` `meta.ts` (AB-0010): the one LWC version the compiler stamps into its output.

macro_rules! lwc_version {
    () => {
        "9.5.3"
    };
}

/// `LWC_VERSION`: the shipped compiler version this port targets (the pinned oracle's). The workspace's
/// only definition (gate: `exactly_one_lwc_version_const`).
pub const LWC_VERSION: &str = lwc_version!();
/// `LWC_VERSION_COMMENT` (`LWC compiler v${LWC_VERSION}`); emitted as `/*LWC compiler v9.5.3*/`.
pub static LWC_VERSION_COMMENT: &str = concat!("LWC compiler v", lwc_version!());
/// `LWC_VERSION_COMMENT` as the Block comment both compilers emit into each component class body
/// (babel-plugin-component's version stamp; ssr-compiler compile-js/index.ts:137-140). The only
/// spelling of the comment in the workspace (gate: `exactly_one_lwc_version_comment`).
pub fn version_comment() -> String {
    format!("/*{LWC_VERSION_COMMENT}*/")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_the_oracles() {
        assert_eq!(
            super::LWC_VERSION_COMMENT,
            format!("LWC compiler v{}", super::LWC_VERSION)
        );
        assert_eq!(
            super::version_comment(),
            format!("/*LWC compiler v{}*/", super::LWC_VERSION)
        );
        assert_eq!(
            super::LWC_VERSION,
            crate::errors::SOURCE_VERSION,
            "meta and the generated error table disagree on the LWC version"
        );
        let pkg = crate::oracle_root().join("packages/@lwc/shared/package.json");
        let s = std::fs::read_to_string(&pkg).unwrap_or_else(|e| panic!("{}: {e}", pkg.display()));
        assert!(
            s.contains(&format!("\"version\": \"{}\"", super::LWC_VERSION)),
            "{} is not v{}",
            pkg.display(),
            super::LWC_VERSION
        );
    }
}
