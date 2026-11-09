//! compile-js/decorators/wire.ts: `isWireDecorator`, `catalogWireAdapters` (the decorator's params,
//! the adapter id and config checks against the scope's bindings, the `'$prop'` config rewrite) and
//! `bWireAdaptersPlumbing`.
use lwc_shared::errors::DecoratorErrors;
use lwc_shared::js_literal::json_string;
use lwc_shared::{CompileError, LwcErrorInfo};
use oxc::ast::ast::{Argument, Decorator, Expression, IdentifierReference, ObjectExpression, ObjectPropertyKind};
use oxc::semantic::{Scoping, SymbolFlags};
use oxc::span::{GetSpan, Span};

use super::super::errors::generate_error;

/// types.ts `WireAdapter`, with the adapter id and the reactive config as source text.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::compile_js) struct WireAdapter {
    adapter_id: String,
    config: String,
    /// The decorated member's key name (`instance.${key}`, also for a computed `[key]`).
    field: String,
    /// `is.methodDefinition(field) && field.kind === 'method'`: the data callback calls it.
    is_method: bool,
}

/// What `catalogWireAdapters` reads of the decorated PropertyDefinition / MethodDefinition. A wired
/// getter or setter is cataloged as the property definition index.ts replaces it with, which has no
/// source range (`span: None`), so its errors carry no location.
pub(in crate::compile_js) struct WiredMember<'m, 'a> {
    pub decorators: &'m [Decorator<'a>],
    pub name: &'m str,
    pub span: Option<Span>,
    pub is_method: bool,
}

/// `isWireDecorator`: `@wire(...)`.
pub(in crate::compile_js) fn is_wire_decorator(decorator: &Decorator) -> bool {
    match decorator.expression.without_parentheses() {
        Expression::CallExpression(call) => matches!(call.callee.without_parentheses(), Expression::Identifier(i) if i.name == "wire"),
        _ => false,
    }
}

/// Errors at the decorated member, like `generateError(path.node, …)`, and binding lookups.
struct Checker<'s> {
    src: &'s str,
    filename: &'s str,
    scoping: &'s Scoping,
    member: Option<Span>,
}

impl Checker<'_> {
    fn error(&self, info: &LwcErrorInfo) -> CompileError {
        match self.member {
            Some(span) => generate_error(self.src, self.filename, span, info, &[]),
            None => CompileError::generate(info, &[], self.filename, None),
        }
    }

    /// `path.scope.getBinding(name)?.kind`, as the binding's symbol flags (`None`: no binding).
    fn binding(&self, id: &IdentifierReference) -> Option<SymbolFlags> {
        let symbol = self.scoping.get_reference(id.reference_id.get()?).symbol_id()?;
        Some(self.scoping.symbol_flags(symbol))
    }

    /// `getWireParams`. index.ts calls the catalog only when the first decorator is `@wire(...)`.
    fn wire_params<'m, 'a>(&self, decorators: &'m [Decorator<'a>]) -> Result<&'m [Argument<'a>], CompileError> {
        if decorators.len() > 1 {
            return Err(self.error(&DecoratorErrors::ONE_WIRE_DECORATOR_ALLOWED));
        }
        let Expression::CallExpression(call) = decorators[0].expression.without_parentheses() else {
            return Err(self.error(&DecoratorErrors::FUNCTION_IDENTIFIER_SHOULD_BE_FIRST_PARAMETER));
        };
        if call.arguments.is_empty() {
            return Err(self.error(&DecoratorErrors::ADAPTER_SHOULD_BE_FIRST_PARAMETER));
        }
        Ok(&call.arguments)
    }

    /// A member expression's object: `foo` of `foo.bar`.
    fn member_root<'m, 'a>(&self, object: &'m Expression<'a>) -> Result<&'m IdentifierReference<'a>, CompileError> {
        match object.without_parentheses() {
            Expression::Identifier(root) => Ok(root),
            _ => Err(self.error(&DecoratorErrors::FUNCTION_IDENTIFIER_CANNOT_HAVE_NESTED_MEMBER_EXRESSIONS)),
        }
    }

    /// `validateWireId`: an identifier or a one-level non-computed member expression whose root is
    /// bound by an import.
    fn validate_wire_id(&self, id: &Argument) -> Result<(), CompileError> {
        let adapter_var = match id.as_expression().map(Expression::without_parentheses) {
            Some(Expression::ComputedMemberExpression(_)) => {
                return Err(self.error(&DecoratorErrors::FUNCTION_IDENTIFIER_CANNOT_HAVE_COMPUTED_PROPS))
            }
            Some(Expression::StaticMemberExpression(member)) => self.member_root(&member.object)?,
            Some(Expression::PrivateFieldExpression(member)) => self.member_root(&member.object)?,
            Some(Expression::Identifier(id)) => id,
            _ => return Err(self.error(&DecoratorErrors::FUNCTION_IDENTIFIER_SHOULD_BE_FIRST_PARAMETER)),
        };
        if !self.binding(adapter_var).is_some_and(SymbolFlags::is_import) {
            return Err(self.error(&DecoratorErrors::COMPUTED_PROPERTY_MUST_BE_CONSTANT_OR_LITERAL));
        }
        Ok(())
    }

    /// `validateWireConfig`: an object expression whose computed keys are `const` bindings, an unbound
    /// `undefined`, or non-regexp literals.
    fn validate_wire_config<'m, 'a>(&self, config: &'m Argument<'a>) -> Result<&'m ObjectExpression<'a>, CompileError> {
        let Some(Expression::ObjectExpression(object)) = config.as_expression().map(Expression::without_parentheses) else {
            return Err(self.error(&DecoratorErrors::CONFIG_OBJECT_SHOULD_BE_SECOND_PARAMETER));
        };
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else { continue };
            if !property.computed {
                continue;
            }
            match property.key.as_expression().map(Expression::without_parentheses) {
                Some(Expression::Identifier(key)) => {
                    let binding = self.binding(key);
                    if binding.is_some_and(SymbolFlags::is_const_variable) || (key.name == "undefined" && binding.is_none()) {
                        continue;
                    }
                }
                Some(
                    Expression::StringLiteral(_)
                    | Expression::NumericLiteral(_)
                    | Expression::BooleanLiteral(_)
                    | Expression::NullLiteral(_)
                    | Expression::BigIntLiteral(_),
                ) => continue,
                Some(Expression::TemplateLiteral(_)) => {
                    return Err(self.error(&DecoratorErrors::COMPUTED_PROPERTY_CANNOT_BE_TEMPLATE_LITERAL))
                }
                _ => {}
            }
            return Err(self.error(&DecoratorErrors::COMPUTED_PROPERTY_MUST_BE_CONSTANT_OR_LITERAL));
        }
        Ok(object)
    }

    /// The `produce` step: the config's source with each `'$a.b'` string value as `instance["a"]["b"]`.
    fn reactive_config(&self, object: &ObjectExpression) -> String {
        const DOLLAR: u16 = b'$' as u16;
        const DOT: u16 = b'.' as u16;
        let mut out = String::new();
        let mut at = object.span.start as usize;
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else { continue };
            let Expression::StringLiteral(value) = property.value.without_parentheses() else { continue };
            let units: Vec<u16> = value.value.encode_utf16().collect();
            let Some((&DOLLAR, path)) = units.split_first() else { continue };
            let chain: String = path.split(|&unit| unit == DOT).map(|prop| format!("[{}]", json_string(prop.iter().copied()))).collect();
            out.push_str(&self.src[at..value.span.start as usize]);
            out.push_str("instance");
            out.push_str(&chain);
            at = value.span.end as usize;
        }
        out.push_str(&self.src[at..object.span.end as usize]);
        out
    }
}

/// `catalogWireAdapters`: validates the member's `@wire(adapter, config?)` and returns its entry.
pub(in crate::compile_js) fn catalog_wire_adapter(
    member: &WiredMember,
    src: &str,
    filename: &str,
    scoping: &Scoping,
) -> Result<WireAdapter, CompileError> {
    let checker = Checker { src, filename, scoping, member: member.span };
    let params = checker.wire_params(member.decorators)?;
    let id = &params[0];
    checker.validate_wire_id(id)?;
    let config = match params.get(1) {
        Some(config) => checker.reactive_config(checker.validate_wire_config(config)?),
        None => "{}".into(),
    };
    let id = id.span();
    Ok(WireAdapter {
        adapter_id: src[id.start as usize..id.end as usize].into(),
        config,
        field: member.name.into(),
        is_method: member.is_method,
    })
}

/// generate-markup.ts: `bWireAdaptersPlumbing(state.wireAdapters)`, or `null` without adapters.
pub(in crate::compile_js) fn wire_adapters_plumbing(adapters: &[WireAdapter]) -> String {
    if adapters.is_empty() {
        return "null".into();
    }
    let info: Vec<String> = adapters
        .iter()
        .map(|WireAdapter { adapter_id, config, field, is_method }| {
            let action = if *is_method { format!("instance.{field}(newValue)") } else { format!("instance.{field} = newValue") };
            format!("{{ adapter: {adapter_id}, dataCallback: (instance) => (newValue) => {{ {action}; }}, config: (instance) => ({config}) }}")
        })
        .collect();
    format!("[{}]", info.join(", "))
}
