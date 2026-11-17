//! compile-js/decorators/api/index.ts.
pub(in crate::compile_js) mod validate;

use oxc::ast::ast::{Decorator, Expression};

/// `isApiDecorator`: `@api`.
pub(in crate::compile_js) fn is_api_decorator(decorator: &Decorator) -> bool {
    matches!(decorator.expression.without_parentheses(), Expression::Identifier(i) if i.name == "api")
}
