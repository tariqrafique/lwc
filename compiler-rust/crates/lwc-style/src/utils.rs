//! Shared helpers (upstream `@lwc/style-compiler/src/utils/`); the position logic stands in for
//! postcss `Node#error({ index, word })` as used by the selector validators.
use crate::ast::*;
use crate::StyleError;

/// postcss Node#error({index, word}) position: first occurrence of `word` in the rule source,
/// else the rule start.
pub(crate) fn rule_error(rule: &Rule, src: &str, reason: String, word: &str) -> StyleError {
    let text = src.get(rule.loc.offset..rule.end_offset).unwrap_or("");
    let (mut line, mut column) = (rule.loc.line, rule.loc.column);
    if let Some(idx) = text.find(word) {
        for ch in text[..idx].chars() { if ch == '\n' { line += 1; column = 1; } else { column += 1; } }
    }
    StyleError::syntax(reason, line, column)
}
