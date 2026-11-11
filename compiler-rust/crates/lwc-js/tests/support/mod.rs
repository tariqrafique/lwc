//! The error `@lwc/compiler` `transformSync` throws for a `.js` file (lwc-compiler's JS facade over
//! `lwc_js::transform`, `transformers/javascript.ts`), flattened for assertions.
#![allow(dead_code)]
use lwc_shared::{CompileOptions, CompilerError, TransformError};

#[derive(Debug)]
pub struct Thrown {
    /// `e.message`.
    pub message: String,
    /// `AggregateError` for a `CompilerAggregateError`, else `None` (prototype `Error`).
    pub name: Option<&'static str>,
    /// `e.location.line` / `.column` (a `CompilerAggregateError` has none).
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// The `CompilerError` itself, or the aggregate's `errors`.
    pub errors: Vec<CompilerError>,
}

impl Thrown {
    pub fn code(&self) -> Option<u32> {
        self.errors.first().and_then(|e| e.code)
    }
}

impl From<TransformError> for Thrown {
    fn from(e: TransformError) -> Self {
        match e {
            TransformError::Compiler(e) => Thrown {
                message: e.message.clone(),
                name: None,
                line: e.location.map(|l| l.line),
                column: e.location.map(|l| l.column),
                errors: vec![e],
            },
            TransformError::Aggregate(a) => Thrown {
                message: a.message,
                name: Some("AggregateError"),
                line: None,
                column: None,
                errors: a.errors,
            },
            TransformError::Unnormalized(e) => panic!("JS errors are normalized: {e:?}"),
        }
    }
}

/// `transformSync(src, filename, o)`'s thrown error.
pub fn thrown(src: &str, filename: &str, o: &CompileOptions) -> Thrown {
    lwc_compiler::dispatch(src, filename, o)
        .expect_err("should fail")
        .into()
}
