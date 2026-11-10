//! TS `ParsedAttribute` (template-compiler/src/parser/attribute.ts): the element's attributes, keyed by
//! qualified name in source order. Directive parsers `pick` what they consume; whatever is left
//! (`get_attributes`) is what later validators (validateTemplate's 1145) see as unsupported.
//!
//! A child of `element_check` (declared there with `#[path]`), so [`normalize_attribute_value`] can be
//! `pub(super)`: the pre-pass is its only possible caller and any other call site is a compile error.
use super::{is_boolean_attribute, CheckedAttr};
use crate::parser::expr_check::is_expression;
use lwc_shared::errors::ParserDiagnostics;
use lwc_shared::template_ir::SourceLocation;
use lwc_shared::CompilerDiagnostic;
use parse5::Attribute;

/// TS normalizeAttributeValue's result.
pub(in crate::parser) struct NormalizedAttr {
    pub value: String,
    /// `\{...}`: the value is the literal text after the backslash, never an expression.
    pub escaped_expression: bool,
    /// The raw value is `"..."` (only complex expressions may be quoted).
    pub quoted_expression: bool,
}

/// TS `normalizeAttributeValue` (attribute.ts:83-169), once per attribute from getTemplateAttribute.
/// `pub(super)` = `element_check`: only the pre-pass can call it (AB-X0017); lowering reads its pairs.
/// `raw` is the attribute's source text (entities undecoded), `attr.value` parse5's decoded value.
/// `Err` is the TS throw: 1037/1036 boolean attribute set to "true"/"false", 1034 quoted expression without
/// experimentalComplexExpressions, 1035 ambiguous `{...}` string.
pub(super) fn normalize_attribute_value(
    complex_expressions: bool, raw: &str, tag: &str, attr: &Attribute, location: &SourceLocation,
) -> Result<NormalizedAttr, CompilerDiagnostic> {
    let (name, value) = (attr.name.as_str(), attr.value.as_str());
    if is_boolean_attribute(name, tag) {
        if value == "true" { return Err(ParserDiagnostics::BOOLEAN_ATTRIBUTE_TRUE.generate(&[tag, name, value], location.clone())); }
        if value == "false" { return Err(ParserDiagnostics::BOOLEAN_ATTRIBUTE_FALSE.generate(&[tag, name, value], location.clone())); }
    }
    // `raw.slice(raw.indexOf('=') + 1)`: the whole raw when there is no `=`.
    let raw_attr_val = &raw[raw.find('=').map_or(0, |i| i + 1)..];
    let is_quoted = raw_attr_val.starts_with('"') && raw_attr_val.ends_with('"');
    // TS isEscapedAttribute tests the raw (entity-undecoded) text; `&#123;a&#125;` decodes to `{a}` but is escaped.
    let is_escaped = raw_attr_val.is_empty() || !(raw_attr_val.contains('{') && raw_attr_val.contains('}'));
    let ok = |value: &str, escaped_expression| Ok(NormalizedAttr { value: value.to_string(), escaped_expression, quoted_expression: is_quoted });
    if !is_escaped && is_expression(value) {
        if is_quoted && !complex_expressions {
            let args = [raw, &raw.replace('"', ""), &raw.replacen("\"{", "\"\\{", 1)];
            return Err(ParserDiagnostics::AMBIGUOUS_ATTRIBUTE_VALUE.generate(&args, location.clone()));
        }
        return ok(value, false);
    } else if !is_escaped && is_potential_expression(value) {
        // `<input value={x}/>`: parse5 keeps the `/` in the unquoted value.
        if value.starts_with('{') && value.ends_with("}/") && !is_quoted { return ok(&value[..value.len() - 1], false); }
        if let Some(rest) = value.strip_prefix('\\').filter(|r| r.starts_with('{')) { return ok(rest, true); }
        // `raw.replace(/="?/, '="\\')`, closed with `"`.
        let mut escaped = match raw.find('=') {
            Some(i) => { let after = &raw[i + 1..]; format!("{}=\"\\{}", &raw[..i], after.strip_prefix('"').unwrap_or(after)) }
            None => raw.to_string(),
        };
        if !escaped.ends_with('"') { escaped.push('"'); }
        return Err(ParserDiagnostics::AMBIGUOUS_ATTRIBUTE_VALUE_STRING.generate(&[raw, &escaped], location.clone()));
    }
    ok(value, false)
}

/// TS `isPotentialExpression`: `/^.?{.+}.*$/` (`.` excludes line terminators).
fn is_potential_expression(s: &str) -> bool {
    if s.chars().any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')) { return false; }
    let cs: Vec<char> = s.chars().collect();
    let close_after = |p: usize| cs.iter().skip(p + 2).any(|&c| c == '}');
    (cs.first() == Some(&'{') && close_after(0)) || (cs.get(1) == Some(&'{') && close_after(1))
}

/// Over the pre-pass pairs: each picked attribute carries the value the pre-pass normalized for it.
pub(in crate::parser) struct ParsedAttribute<'a> {
    attrs: Vec<&'a CheckedAttr<'a>>,
}

impl<'a> ParsedAttribute<'a> {
    /// TS parseAttributes: one entry per attribute (parse5 already drops duplicate names).
    pub(in crate::parser) fn new(attrs: &'a [CheckedAttr<'a>]) -> Self {
        Self { attrs: attrs.iter().collect() }
    }

    /// TS `pick('name')`, with the attribute's pre-pass value.
    pub(in crate::parser) fn pick_checked(&mut self, name: &str) -> Option<&'a CheckedAttr<'a>> {
        let i = self.attrs.iter().position(|c| c.attr().name == name)?;
        Some(self.attrs.remove(i))
    }

    /// TS `pick('name')`.
    pub(in crate::parser) fn pick(&mut self, name: &str) -> Option<&'a Attribute> {
        self.pick_checked(name).map(|c| c.attr())
    }

    /// TS `pick(/^prefix/)`: the first attribute (source order) whose name starts with `prefix`.
    pub(in crate::parser) fn pick_prefix(&mut self, prefix: &str) -> Option<&'a Attribute> {
        let i = self.attrs.iter().position(|c| c.attr().name.starts_with(prefix))?;
        Some(self.attrs.remove(i).attr())
    }

    /// TS `pickAll(/^prefix/)`.
    pub(in crate::parser) fn pick_all_prefix(&mut self, prefix: &str) -> Vec<&'a Attribute> {
        let (picked, rest): (Vec<_>, _) = self.attrs.iter().copied().partition(|c| c.attr().name.starts_with(prefix));
        self.attrs = rest;
        picked.into_iter().map(|c| c.attr()).collect()
    }

    /// A bare element (TS threw after parseBaseElement created it): no attribute applier ran on it.
    pub(in crate::parser) fn clear(&mut self) {
        self.attrs.clear();
    }

    /// TS `getAttributes()`: everything not picked yet, each with its pre-pass value.
    pub(in crate::parser) fn get_attributes(&self) -> &[&'a CheckedAttr<'a>] {
        &self.attrs
    }
}

/// TS `ATTRIBUTE_NAME_CHAR` (parser/constants.ts) as UTF-16 code-unit ranges, in source order.
/// AB-D0002 lane b moves this to the generated parser/constants.rs.
pub(crate) const ATTRIBUTE_NAME_CHAR: &[(u16, u16)] = &[
    (0x3A, 0x3A), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A), (0xC0, 0xD6), (0xD8, 0xF6), (0xF8, 0x2FF), (0x370, 0x37D),
    (0x37F, 0x1FFF), (0x200C, 0x200D), (0x2070, 0x218F), (0x2C00, 0x2FEF), (0x3001, 0xD7FF), (0xF900, 0xFDCF),
    (0xFDF0, 0xFFFD), (0x2D, 0x2D), (0x2E, 0x2E), (0x30, 0x39), (0xB7, 0xB7), (0x300, 0x36F), (0x203F, 0x2040),
];

/// TS `isDataAttribute`: `DATA_RE` = `^(data)-[ATTRIBUTE_NAME_CHAR]*$`. The regex has no `u` flag, so it tests
/// UTF-16 code units: an astral char is a surrogate pair, outside the class.
pub(crate) fn is_data_attribute(attr: &str) -> bool {
    attr.strip_prefix("data-").is_some_and(|rest| {
        rest.encode_utf16().all(|u| ATTRIBUTE_NAME_CHAR.iter().any(|&(lo, hi)| (lo..=hi).contains(&u)))
    })
}

/// The node kinds TS `isAttribute` reads (shared/ast.ts): `isComponent`, `isExternalComponent`,
/// `isLwcComponent` (the `Lwc` node named `lwc:component`, built by parseLwcComponent), anything else.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElementKind { Element, External, Component, LwcComponent }

/// TS `isAttribute(element, attrName)` (parser/attribute.ts): true keeps the name an attribute, false makes it a
/// property. Shared by applyAttributes (parser) and codegen.
pub(crate) fn is_attribute(kind: ElementKind, tag: &str, attr: &str) -> bool {
    if matches!(kind, ElementKind::Component | ElementKind::LwcComponent) {
        return matches!(attr, "style" | "class" | "key" | "slot" | "exportparts") || is_data_attribute(attr);
    }
    if kind == ElementKind::External {
        return true;
    }
    !(tag == "input" && (attr == "value" || attr == "checked"))
}
