//! compile-js/decorators/index.ts: `validateUniqueDecorator`, plus the `@api` / `@track` / `@wire`
//! predicates of decorators/{api,track,wire}.ts.
pub(super) mod api;
pub(super) mod track;
pub(super) mod wire;

use lwc_shared::CompileError;
use lwc_shared::errors::DecoratorErrors;
use oxc::ast::ast::Decorator;

use super::errors::generate_error;
use api::is_api_decorator;
use track::is_track_decorator;
use wire::is_wire_decorator;

/// `validateUniqueDecorator`: a member may not combine `@wire` with `@api` / `@track`, or `@api` with `@track`.
pub(super) fn validate_unique_decorator(
    decorators: &[Decorator],
    src: &str,
    filename: &str,
) -> Result<(), CompileError> {
    if decorators.len() < 2 {
        return Ok(());
    }
    let wire = decorators.iter().find(|d| is_wire_decorator(d));
    let api = decorators.iter().find(|d| is_api_decorator(d));
    let track = decorators.iter().find(|d| is_track_decorator(d));
    if let Some(wire) = wire {
        if api.is_some() {
            return Err(generate_error(
                src,
                filename,
                wire.span,
                &DecoratorErrors::CONFLICT_WITH_ANOTHER_DECORATOR,
                &["api"],
            ));
        }
        if track.is_some() {
            return Err(generate_error(
                src,
                filename,
                wire.span,
                &DecoratorErrors::CONFLICT_WITH_ANOTHER_DECORATOR,
                &["track"],
            ));
        }
    }
    if let (Some(api), Some(_)) = (api, track) {
        return Err(generate_error(
            src,
            filename,
            api.span,
            &DecoratorErrors::API_AND_TRACK_DECORATOR_CONFLICT,
            &[],
        ));
    }
    Ok(())
}
