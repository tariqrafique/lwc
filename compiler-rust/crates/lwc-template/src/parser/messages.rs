//! Generated from @lwc/errors template-transform.ts: (code, level, message template).
use lwc_shared::DiagnosticLevel::{self, *};

pub(crate) const MESSAGES: &[(u32, DiagnosticLevel, &str)] = &[
    (1003, Error, "Invalid template"),
    (1028, Error, "Compiler options must be an object"),
    (1029, Error, "Unknown if modifier {0}"),
    (1030, Error, "Unknown option property {0}"),
    (1034, Error, "Ambiguous attribute value {0}. If you want to make it a valid identifier you should remove the surrounding quotes {1}. If you want to make it a string you should escape it {2}."),
    (1035, Error, "Ambiguous attribute value {0}. If you want to make it a string you should escape it {1}"),
    (1036, Error, "To not set a boolean attribute, try <{0}> instead of <{0} {1}=\"{2}\">. To represent a false value, the attribute has to be omitted altogether."),
    (1037, Error, "To set a boolean attributes, try <{0} {1}> instead of <{0} {1}=\"{2}\">. If the attribute is present, its value must either be the empty string or a value that is an ASCII case -insensitive match for the attribute's canonical name with no leading or trailing whitespace."),
    (1038, Error, "Template expression doesn't allow computed property access"),
    (1039, Error, "{0} directive is expected to be an expression"),
    (1040, Error, "Invalid id value \"{0}\". Id values must not contain any whitespace."),
    (1041, Warning, "Static id values are not allowed in iterators. Id values must be unique within a template and must therefore be computed with an expression."),
    (1042, Error, "Duplicate id value \"{0}\" detected. Id values must be unique within a template."),
    (1043, Error, "Event handler should be an expression"),
    (1044, Error, "for:each and for:item directives should be associated together."),
    (1045, Error, "for:each directive is expected to be a expression."),
    (1046, Error, "for:index directive is expected to be a string."),
    (1047, Error, "for:item directive is expected to be a string."),
    (1048, Error, "srcdoc attribute is disallowed on <iframe> for security reasons"),
    (1049, Error, "Forbidden svg namespace tag found in template: '<{0}>' tag is not allowed within <svg>"),
    (1050, Error, "Forbidden MathML namespace tag found in template: '<{0}>' tag is not allowed within <math>"),
    (1051, Error, "Forbidden tag found in template: '<{0}>' tag is not allowed."),
    (1052, Error, "Error parsing attribute: {0}"),
    (1053, Error, "Error parsing identifier: {0}"),
    (1054, Error, "If directive should be an expression"),
    (1055, Error, "{0} is not valid attribute for {1}. All attributes name should be all lowercase."),
    (1056, Error, "Invalid event type \"{0}\". Event type must begin with a lower-case alphabetic character and contain only lower-case alphabetic characters, underscores, and numeric characters"),
    (1057, Warning, "{0} is not valid attribute for {1}. For more information refer to https://developer.mozilla.org/en-US/docs/Web/HTML/Element/{1}"),
    (1058, Error, "Invalid HTML syntax: {0}. For more information, please visit https://html.spec.whatwg.org/multipage/parsing.html#parse-error-{0}"),
    (1059, Error, "{0} is not a valid identifier"),
    (1060, Error, "Template expression doesn't allow {0}"),
    (1061, Error, "The attribute \"tabindex\" can only be set to \"0\" or \"-1\"."),
    (1062, Error, "\"is\" attribute value can't be an expression"),
    (1063, Error, "\"is\" attribute is disallowed"),
    (1064, Error, "Key attribute value should be an expression"),
    (1065, Error, "Invalid key value for element <{0}>. Key cannot reference for:each index {1}"),
    (1066, Error, "Invalid key value for element <{0}>. Key cannot reference iterator index"),
    (1071, Error, "Missing key for element <{0}> inside of iterator. Elements within iterators must have a unique, computed key value."),
    (1072, Error, "Missing root template tag"),
    (1073, Error, "Template expression doesn't allow to modify iterators"),
    (1074, Error, "Multiple expressions found"),
    (1075, Error, "Multiple roots found"),
    (1076, Error, "Name attribute on slot tag can't be an expression."),
    (1077, Error, "Invalid template tag. A directive is expected to be associated with the template tag."),
    (1078, Error, "<{0}> has no matching closing tag."),
    (1079, Error, "Expected root tag to be template, found {0}"),
    (1080, Error, "Root template has unknown or disallowed attributes: {0}"),
    (1082, Error, "<slot> tag can't be associated with {0} template directives."),
    (1083, Error, "Error parsing template expression: {0}"),
    (1084, Error, "Unexpected if modifier {0}. Expected one of: true, false, strict-true"),
    (1085, Error, "Invalid value for \"lwc:dom\". 'lwc:dom' can only be set to {0}"),
    (1086, Error, "Invalid contents for element with \"lwc:dom\". Element must be empty"),
    (1087, Error, "Invalid directive \"lwc:dom\" on element {0}. \"lwc:dom\" cannot be added to a custom element"),
    (1088, Error, "Invalid directive \"lwc:dom\" on <slot>.. \"lwc:dom\" cannot be added to a <slot>"),
    (1122, Error, "The <style> element is disallowed inside the template. Please add css rules into '.css' file of your component bundle."),
    (1123, Warning, "Unknown html tag '<{0}>'. For more information refer to https://developer.mozilla.org/en-US/docs/Web/HTML/Element and https://developer.mozilla.org/en-US/docs/Web/SVG/Element"),
    (1124, Error, "{0} is not valid attribute for {1}. Attribute name must start with an underscore, dollar sign, or an optional hyphen character followed by an alphabetic character."),
    (1125, Error, "{0} is not valid attribute for {1}. Attribute name must end with alpha-numeric character."),
    (1127, Error, "Invalid directive \"{0}\" on element {1}."),
    (1128, Error, "Invalid lwc:dynamic usage. The LWC dynamic directive must be enabled in order to use this feature."),
    (1129, Error, "Invalid lwc:dynamic usage on element \"{0}\". This directive can only be used in a custom element."),
    (1130, Error, "Invalid lwc:dynamic usage on element \"{0}\". The directive binding must be an expression."),
    (1133, Error, "Invalid value for \"lwc:render-mode\". 'lwc:render-mode' can only be set to \"shadow\", or \"light\""),
    (1134, Error, "Invalid attribute(s) '{0}' on slot. Slots in Light DOM templates (which have 'lwc:render-mode' directive) can only have [{1}] attributes"),
    (1135, Error, "Invalid directive 'lwc:dom' on element {0}. 'lwc:dom' is not necessary in Light DOM components."),
    (1136, Error, "Invalid usage for 'for:each' and '{0}' directives on the same element."),
    (1137, Warning, "Invalid duplicate slot ({0})."),
    (1138, Warning, "Invalid slot ({0}). A slot cannot appear inside of an iterator."),
    (1139, Error, "Invalid event listener '{0}' on slot. Slots in Light DOM templates cannot have event listeners."),
    (1140, Error, "Invalid lwc:inner-html usage on element \"{0}\". The directive can't be used on a custom element or special LWC managed elements denoted with lwc:*."),
    (1141, Error, "Invalid lwc:inner-html usage on element \"{0}\". The directive can't be used on a slot or a template element."),
    (1142, Error, "Invalid lwc:inner-html usage on element \"{0}\". The directive can't be used on an element with content."),
    (1143, Error, "Invalid lwc:inner-html usage on element \"{0}\". The directive binding can only be an expression or a string."),
    (1144, Warning, "Invalid HTML detected, \"<{0}>\" was automatically inserted within \"<{1}>\"; the compiled template may not match the template source."),
    (1145, Warning, "Invalid attributes detected on template. The following attributes are not supported on template tags in LWC: {0}. For more information, please visit https://sfdc.co/template-directives"),
    (1146, Error, "lwc:preserve-comments must be a boolean attribute."),
    (1147, Warning, "Found multiple HTML attributes mapping to the same JavaScript property. \"{0}\" and \"{1}\" both map to \"{2}\"."),
    (1148, Warning, "Invalid HTML syntax: {0}. This will not be supported in future versions of LWC. For more information, please visit https://html.spec.whatwg.org/multipage/parsing.html#parse-error-{0}"),
    (1149, Warning, "Invalid key attribute on element <{0}>. The key attribute should be applied to an element with for:each or iterator:*, or to a direct child of a <template> element with for:each or iterator:*. This key will be ignored, and may throw an error in future versions of LWC."),
    (1150, Error, "customRendererConfig contains duplicate entry for {0} element tag"),
    (1151, Error, "customRendererConfig should not contain a custom element tag, but found {0}"),
    (1153, Warning, "Non-root template elements must contain valid LWC template directive attributes. Otherwise, the template and its children will be ignored. For more information please visit https://sfdc.co/template-directives"),
    (1155, Error, "Invalid lwc:spread usage on element \"{0}\". The directive binding must be an expression."),
    (1156, Error, "Invalid lwc:ref usage on element \"{0}\". The directive can't be used on a slot or a template element."),
    (1157, Error, "Invalid lwc:ref usage on element \"{0}\". The directive binding must be a non-empty string."),
    (1158, Error, "Invalid lwc:ref usage on element \"{0}\". lwc:ref cannot be used inside for:each or an iterator."),
    (1159, Error, "lwc:if directive value should be an expression"),
    (1160, Error, "lwc:elseif directive value should be an expression"),
    (1161, Error, "lwc:else directive cannot have a value"),
    (1162, Error, "Invalid usage of 'lwc:if' and '{0}' directives on the same element."),
    (1163, Error, "Invalid usage of 'lwc:elseif' and '{0}' directives on the same element."),
    (1164, Error, "Invalid usage of 'lwc:else' and '{0}' directives on the same element."),
    (1165, Error, "'{0}' directive must be used immediately after an element with 'lwc:if' or 'lwc:elseif'. No such element found."),
    (1166, Error, "'{0}' directive cannot be used with 'lwc:if', 'lwc:elseif', or 'lwc:else directives on the same element."),
    (1169, Error, "Invalid lwc:slot-bind usage on <slot> element. Scoped slots usage is allowed in Light DOM templates only."),
    (1170, Error, "Invalid lwc:slot-bind usage on element {0}. The directive binding must be an expression."),
    (1171, Error, "Invalid lwc:slot-bind usage on element {0}. The directive can be used on a <slot> element only."),
    (1172, Error, "Invalid duplicate scoped slots ({0})"),
    (1173, Error, "Mixing slot types disallowed for same ({0}) slot."),
    (1174, Error, "lwc:slot-data attribute value is expected to be a string."),
    (1176, Error, "lwc:slot-data directive can be used on <template> elements only."),
    (1177, Error, "<template> tag with lwc:slot-data directive cannot contain a comment or text node as a direct child."),
    (1178, Error, "<template> tag with lwc:slot-data directive must be the direct child of a custom element."),
    (1179, Error, "lwc:slot-data directive cannot be combined with other directives on the same <template> tag."),
    (1180, Error, "Invalid lwc:external directive usage: {0}. This directive can only be used on custom elements."),
    (1181, Error, "Invalid lwc:external directive usage: {0}. This directive is a boolean attribute and should not have a value."),
    (1182, Warning, "Multiple if: directives found on '{0}'. Only one if: directive is allowed; the rest are ignored."),
    (1183, Error, "<lwc:component> must have an 'lwc:is' attribute."),
    (1184, Warning, "{0} is not a special LWC tag name and will be treated as an HTML element."),
    (1185, Error, "Invalid lwc:is usage for value {0}. The value assigned to lwc:is must be an expression."),
    (1186, Error, "Invalid lwc:is usage for element {0}. The directive can only be used with <lwc:component>"),
    (1187, Warning, "The lwc:dynamic directive is deprecated and will be removed in a future release. Please use lwc:is instead."),
    (1188, Error, "Invalid dynamic components usage, lwc:component and lwc:is can only be used when dynamic components have been enabled."),
    (1189, Error, "Template expression doesn't allow {0} in arrow function params."),
    (1190, Error, "Statements are disallowed in template expressions."),
    (1191, Error, "Field mutations are only permitted within arrow functions."),
    (1192, Error, "Use of the delete operator is prohibited within template expressions."),
    (1193, Error, "The body of arrow functions in template expressions must be an expression."),
    (1194, Error, "Arrow functions in template expressions cannot be {0}."),
    (1195, Error, "Early-stage JS features are disallowed in template expressions."),
    (1196, Error, "Use of {0} is disallowed within template expressions."),
    (1197, Error, "Use of comments is disallowed within template expressions."),
    (1201, Warning, "The slot attribute in {0} will be ignored due to its ancestor {1}. It must be a direct child of the containing component."),
    (1203, Error, "Invalid lwc:on usage on element \"{0}\". The directive can't be used on a template element."),
    (1204, Error, "Invalid lwc:on usage on element \"{0}\". The directive binding must be an expression."),
    (1205, Error, "Invalid lwc:on usage on element \"{0}\". It is not permitted to use declarative event listeners alongside lwc:on"),
    (1206, Error, "Invalid lwc:on usage. The `lwc:on` directive must be enabled in order to use this feature."),
    (1207, Error, "Template expression doesn't allow computed property access unless the expression is surrounded by quotes: \"{0}\""),
    (1208, Error, "Template expression doesn't allow {0} unless the expression is surrounded by quotes: \"{1}\""),
    (1209, Error, "Template expression doesn't allow computed property access. The current component API version ({1}) is insufficient and must be increased to at least {2} for this type of expression."),
    (1210, Error, "Template expression doesn't allow {0}. The current component API version ({1}) is insufficient and must be increased to at least {2} for this type of expression."),
    (1211, Error, "{0} is a reserved keyword and cannot be used as an identifier."),
];

/// `ParserDiagnostics[code]` as an `@lwc/errors` `LWCErrorInfo`. Panics on a code that is not in
/// [`MESSAGES`] (a typo at a call site, caught by any fixture that reaches it).
pub(crate) fn info(code: u32) -> lwc_shared::LwcErrorInfo {
    let &(code, level, message) = MESSAGES.iter().find(|m| m.0 == code).unwrap_or_else(|| panic!("unknown template diagnostic code {code}"));
    // INVALID_TEMPLATE is the only template-transform entry with a strictLevel.
    let strict_level = (code == 1003).then_some(Fatal);
    lwc_shared::LwcErrorInfo { code, message, level, url: Some(""), strict_level }
}

/// `generateErrorMessage(ParserDiagnostics[code], args)`: `LWC{code}: {message}`.
pub(crate) fn message(code: u32, args: &[&str]) -> String {
    lwc_shared::generate_error_message(&info(code), Some(args))
}

/// `ParserCtx.throwAtLocation` / `warnAtLocation`: `generateCompilerDiagnostic(info, { messageArgs, origin: { location } })`.
/// Level comes from the table, as in TS (the caller does not choose error vs warning).
pub(crate) fn lwc(code: u32, args: &[&str], location: lwc_shared::SourceLocation) -> lwc_shared::CompilerDiagnostic {
    let origin = lwc_shared::DiagnosticOrigin { filename: None, location: Some(location) };
    lwc_shared::CompilerDiagnostic::generate(&info(code), Some(args), Some(origin), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_sorted() {
        assert!(MESSAGES.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn lwc_prefix_args_and_level() {
        let d = lwc(1182, &["div"], lwc_shared::SourceLocation::default());
        assert_eq!(d.message, "LWC1182: Multiple if: directives found on 'div'. Only one if: directive is allowed; the rest are ignored.");
        assert_eq!(d.level, Warning);
        assert_eq!(message(1074, &[]), "LWC1074: Multiple expressions found");
    }
}
