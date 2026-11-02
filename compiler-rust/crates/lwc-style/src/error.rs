/// Mirrors PostCSS CssSyntaxError (name/reason/line/column, 1-based) and AggregateError.
#[derive(Debug, Clone, PartialEq)]
pub enum StyleError {
    Syntax { name: String, reason: String, line: u32, column: u32 },
    Aggregate(Vec<StyleError>),
}
impl StyleError {
    pub fn reason(&self) -> &str {
        match self { StyleError::Syntax { reason, .. } => reason, StyleError::Aggregate(_) => "" }
    }
    pub fn syntax(reason: impl Into<String>, line: u32, column: u32) -> Self {
        StyleError::Syntax { name: "CssSyntaxError".into(), reason: reason.into(), line, column }
    }
}
impl std::fmt::Display for StyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for StyleError {}
