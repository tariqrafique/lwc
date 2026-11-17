//! compile-js/decorators/track.ts.
use oxc::ast::ast::{Decorator, Expression};

/// `isTrackDecorator`: `@track`.
pub(in crate::compile_js) fn is_track_decorator(decorator: &Decorator) -> bool {
    matches!(decorator.expression.without_parentheses(), Expression::Identifier(i) if i.name == "track")
}
