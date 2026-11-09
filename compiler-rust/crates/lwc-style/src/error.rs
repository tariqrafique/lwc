/// Mirrors what the JS style compiler throws: PostCSS CssSyntaxError (name/reason/line/column, 1-based),
/// a plain JS `Error` (`name: "Error"`, `reason` = its message, no position; see [`StyleError::error`]),
/// and AggregateError.
/// `line`/`column` are `None` when PostCSS has no position (JS `undefined`), e.g. `Unknown word` raised
/// on a whitespace token, which carries no offset (`a{: ;}`).
#[derive(Debug, Clone, PartialEq)]
pub enum StyleError {
    Syntax { name: String, reason: String, line: Option<u32>, column: Option<u32> },
    Aggregate(Vec<StyleError>),
}
impl StyleError {
    pub fn reason(&self) -> &str {
        match self { StyleError::Syntax { reason, .. } => reason, StyleError::Aggregate(_) => "" }
    }
    pub fn syntax(reason: impl Into<String>, line: u32, column: u32) -> Self {
        Self::syntax_at(reason, Some((line, column)))
    }
    /// A plain JS `throw new Error(message)` from the LWC pipeline outside PostCSS (e.g. serialize.ts
    /// `Unexpected host rules ordering`): `name: "Error"`, no position, message not file-prefixed.
    pub fn error(message: impl Into<String>) -> Self {
        StyleError::Syntax { name: "Error".into(), reason: message.into(), line: None, column: None }
    }
    /// `pos = None`: PostCSS raised the error without a position.
    pub fn syntax_at(reason: impl Into<String>, pos: Option<(u32, u32)>) -> Self {
        StyleError::Syntax { name: "CssSyntaxError".into(), reason: reason.into(), line: pos.map(|p| p.0), column: pos.map(|p| p.1) }
    }
    /// PostCSS `CssSyntaxError#setMessage` (no plugin prefix in LWC's pipeline):
    /// `{file}:{line}:{column}: {reason}`, or `{file}: {reason}` without a position.
    /// A plain `Error` ([`StyleError::error`]) never went through `setMessage`: its message as-is.
    pub fn postcss_message(&self, file: &str) -> String {
        match self {
            StyleError::Syntax { name, reason, .. } if name != "CssSyntaxError" => reason.clone(),
            StyleError::Syntax { reason, line: Some(l), column, .. } => {
                let c = column.map_or("undefined".to_string(), |c| c.to_string());
                format!("{file}:{l}:{c}: {reason}")
            }
            StyleError::Syntax { reason, .. } => format!("{file}: {reason}"),
            StyleError::Aggregate(_) => String::new(),
        }
    }
}
impl std::fmt::Display for StyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for StyleError {}
