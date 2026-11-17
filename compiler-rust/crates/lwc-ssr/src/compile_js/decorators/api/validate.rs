//! compile-js/decorators/api/validate.ts: checks on an `@api` member of the component, against the
//! `publicProperties` collected so far.
use lwc_shared::CompileError;
use lwc_shared::errors::DecoratorErrors;
use lwc_shared::html_attributes::{ambiguous_prop, is_disallowed_prop};
use oxc::ast::ast::{Expression, MethodDefinition, MethodDefinitionKind, PropertyDefinition};
use oxc::span::Span;

use crate::compile_js::errors::generate_error;

/// The node an entry of `state.publicProperties` maps to: upstream reads only its type and `kind`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::compile_js) enum ApiDefinition {
    Property,
    Method(MethodDefinitionKind),
}

/// `state.publicProperties`: `@api` names in insertion order (a JS `Map`), each with its node's kind and span.
pub(in crate::compile_js) type PublicProperties = Vec<(String, ApiDefinition, Span)>;

fn lookup<'p>(
    props: &'p PublicProperties,
    name: &str,
) -> Option<&'p (String, ApiDefinition, Span)> {
    props.iter().find(|(n, ..)| n == name)
}

/// The definition being checked: an `@api` member whose key is an identifier (`isKeyIdentifier`).
pub(in crate::compile_js) struct Definition<'d> {
    pub name: &'d str,
    pub span: Span,
    pub computed: bool,
}

fn validate_name(def: &Definition, src: &str, filename: &str) -> Result<(), CompileError> {
    let err = |info, args: &[&str]| Err(generate_error(src, filename, def.span, info, args));
    if def.computed {
        return err(&DecoratorErrors::PROPERTY_CANNOT_BE_COMPUTED, &[]);
    }
    let name = def.name;
    if name == "part" {
        return err(&DecoratorErrors::PROPERTY_NAME_PART_IS_RESERVED, &[name]);
    }
    if name.starts_with("on") {
        return err(
            &DecoratorErrors::PROPERTY_NAME_CANNOT_START_WITH_ON,
            &[name],
        );
    }
    // JS `length` counts UTF-16 units.
    if name.starts_with("data") && name.encode_utf16().count() > 4 {
        return err(
            &DecoratorErrors::PROPERTY_NAME_CANNOT_START_WITH_DATA,
            &[name],
        );
    }
    if is_disallowed_prop(name) {
        return err(&DecoratorErrors::PROPERTY_NAME_IS_RESERVED, &[name]);
    }
    if let Some(camel) = ambiguous_prop(name) {
        return err(&DecoratorErrors::PROPERTY_NAME_IS_AMBIGUOUS, &[name, camel]);
    }
    Ok(())
}

fn validate_property_value(
    node: &PropertyDefinition,
    src: &str,
    filename: &str,
) -> Result<(), CompileError> {
    if matches!(node.value.as_ref().map(|v| v.without_parentheses()), Some(Expression::BooleanLiteral(b)) if b.value)
    {
        return Err(generate_error(
            src,
            filename,
            node.span,
            &DecoratorErrors::INVALID_BOOLEAN_PUBLIC_PROPERTY,
            &[],
        ));
    }
    Ok(())
}

/// `validatePropertyUnique`.
pub(in crate::compile_js) fn validate_property_unique(
    def: &Definition,
    props: &PublicProperties,
    src: &str,
    filename: &str,
) -> Result<(), CompileError> {
    if lookup(props, def.name).is_some() {
        return Err(generate_error(
            src,
            filename,
            def.span,
            &DecoratorErrors::DUPLICATE_API_PROPERTY,
            &[def.name],
        ));
    }
    Ok(())
}

/// `validateApiProperty`.
pub(in crate::compile_js) fn validate_api_property(
    node: &PropertyDefinition,
    def: &Definition,
    props: &PublicProperties,
    src: &str,
    filename: &str,
) -> Result<(), CompileError> {
    validate_property_unique(def, props, src, filename)?;
    validate_name(def, src, filename)?;
    validate_property_value(node, src, filename)
}

/// `validateUniqueMethod`. Also what the re-traversal of an extracted `export default <expression>`
/// hits, where the member's own earlier entry is `field`.
pub(in crate::compile_js) fn validate_unique_method(
    def: &Definition,
    kind: MethodDefinitionKind,
    props: &PublicProperties,
    src: &str,
    filename: &str,
) -> Result<(), CompileError> {
    let Some((_, field, _)) = lookup(props, def.name) else {
        return Ok(());
    };
    let accessor = |k: MethodDefinitionKind| {
        matches!(k, MethodDefinitionKind::Get | MethodDefinitionKind::Set)
    };
    if matches!(field, ApiDefinition::Method(k) if accessor(*k)) && accessor(kind) {
        return Err(generate_error(
            src,
            filename,
            def.span,
            &DecoratorErrors::SINGLE_DECORATOR_ON_SETTER_GETTER_PAIR,
            &[def.name],
        ));
    }
    Err(generate_error(
        src,
        filename,
        def.span,
        &DecoratorErrors::DUPLICATE_API_PROPERTY,
        &[def.name],
    ))
}

/// `validateApiMethod`.
pub(in crate::compile_js) fn validate_api_method(
    node: &MethodDefinition,
    def: &Definition,
    props: &PublicProperties,
    src: &str,
    filename: &str,
) -> Result<(), CompileError> {
    validate_unique_method(def, node.kind, props, src, filename)?;
    validate_name(def, src, filename)
}
