//! Port of shipped 9.5.3 `@lwc/shared` `api-version.ts`: `APIVersion`, `APIFeature`, `getAPIVersionFromNumber`,
//! `minApiVersion`, `isAPIFeatureEnabled`, `LOWEST_API_VERSION` / `HIGHEST_API_VERSION`. Discriminants match the
//! const-enum values in the shipped dist.

/// `APIVersion` (const enum; the value is the API version number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
#[allow(non_camel_case_types)] // names as in shipped `APIVersion`
pub enum ApiVersion {
    V58_244_SUMMER_23 = 58,
    V59_246_WINTER_24 = 59,
    V60_248_SPRING_24 = 60,
    V61_250_SUMMER_24 = 61,
    V62_252_WINTER_25 = 62,
    V63_254_SPRING_25 = 63,
    V64_256_SUMMER_25 = 64,
    V65_258_WINTER_26 = 65,
    V66_260_SPRING_26 = 66,
}

/// `allVersions`, ascending.
pub const ALL_VERSIONS: [u32; 9] = [58, 59, 60, 61, 62, 63, 64, 65, 66];
pub const LOWEST_API_VERSION: u32 = ALL_VERSIONS[0];
pub const HIGHEST_API_VERSION: u32 = ALL_VERSIONS[ALL_VERSIONS.len() - 1];

/// `APIFeature` (const enum; discriminants as shipped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ApiFeature {
    LowercaseScopeTokens = 0,
    TreatAllParse5ErrorsAsErrors = 1,
    UseFragmentsForLightDomSlots = 2,
    DisableObjectRestSpreadTransformation = 3,
    SkipUnnecessaryRegisterDecorators = 4,
    UseCommentsForFragmentBookends = 5,
    UseLightDomSlotForwarding = 6,
    EnableElementInternalsAndFace = 7,
    EnableThisDotHostElement = 8,
    EnableThisDotStyle = 9,
    TemplateClassNameObjectBinding = 10,
    EnableComplexTemplateExpressions = 11,
}

/// `getAPIVersionFromNumber`: unspecified -> highest; known -> itself; below lowest -> lowest;
/// otherwise the highest known version lower than the request; above highest -> highest.
pub fn get_api_version_from_number(version: Option<u32>) -> u32 {
    let Some(version) = version else { return HIGHEST_API_VERSION };
    if ALL_VERSIONS.contains(&version) { return version; }
    if version < LOWEST_API_VERSION { return LOWEST_API_VERSION; }
    for i in 1..ALL_VERSIONS.len() {
        if ALL_VERSIONS[i] > version { return ALL_VERSIONS[i - 1]; }
    }
    HIGHEST_API_VERSION
}

/// `minApiVersion`.
pub fn min_api_version(feature: ApiFeature) -> u32 {
    use ApiFeature::*;
    (match feature {
        LowercaseScopeTokens | TreatAllParse5ErrorsAsErrors => ApiVersion::V59_246_WINTER_24,
        DisableObjectRestSpreadTransformation | SkipUnnecessaryRegisterDecorators | UseCommentsForFragmentBookends
        | UseFragmentsForLightDomSlots => ApiVersion::V60_248_SPRING_24,
        EnableElementInternalsAndFace | UseLightDomSlotForwarding => ApiVersion::V61_250_SUMMER_24,
        EnableThisDotHostElement | EnableThisDotStyle | TemplateClassNameObjectBinding => ApiVersion::V62_252_WINTER_25,
        EnableComplexTemplateExpressions => ApiVersion::V66_260_SPRING_26,
    }) as u32
}

/// `isAPIFeatureEnabled`: `apiVersion >= minApiVersion(feature)`.
pub fn is_api_feature_enabled(feature: ApiFeature, api_version: u32) -> bool {
    api_version >= min_api_version(feature)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_shipped() {
        assert_eq!(get_api_version_from_number(None), 66);
        assert_eq!(get_api_version_from_number(Some(60)), 60);
        assert_eq!(get_api_version_from_number(Some(1)), 58);
        assert_eq!(get_api_version_from_number(Some(9999999)), 66);
        assert!(!is_api_feature_enabled(ApiFeature::EnableComplexTemplateExpressions, get_api_version_from_number(Some(65))));
        assert!(is_api_feature_enabled(ApiFeature::EnableComplexTemplateExpressions, get_api_version_from_number(None)));
        assert!(is_api_feature_enabled(ApiFeature::TreatAllParse5ErrorsAsErrors, 59));
        assert!(!is_api_feature_enabled(ApiFeature::TreatAllParse5ErrorsAsErrors, 58));
        for f in [ApiFeature::DisableObjectRestSpreadTransformation, ApiFeature::SkipUnnecessaryRegisterDecorators] {
            assert!(!is_api_feature_enabled(f, 59) && is_api_feature_enabled(f, 60));
        }
        assert_eq!(min_api_version(ApiFeature::UseLightDomSlotForwarding), 61);
        assert_eq!(min_api_version(ApiFeature::TemplateClassNameObjectBinding), 62);
    }
}
