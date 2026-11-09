//! Source positions in the `@lwc/errors` / Babel shape, and the ONE byte-offset -> position conversion
//! (AB-D0001). Every crate turns a byte offset into `{line, column, start, length}` through [`LineIndex`].

use crate::SourceLocation;

/// `@lwc/errors` `Location` shape: `{ line, column, start?, length? }` (1-based line, 0-based UTF-16 column,
/// UTF-16 `start` / `length`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiagnosticLocation {
    pub line: u32,
    pub column: u32,
    pub start: Option<u32>,
    pub length: Option<u32>,
    /// Babel parser `Position.index` (UTF-16 offset). A Babel parse error's `err.loc` is a `Position`
    /// `{ line, column, index }`, and `normalizeToCompilerError` copies it verbatim as the location; set only
    /// there (then `start` / `length` are `None`).
    pub index: Option<u32>,
}

impl From<SourceLocation> for DiagnosticLocation {
    /// `@lwc/template-compiler` `normalizeLocation`: line/column = start line/column,
    /// start = start offset, length = end - start.
    fn from(l: SourceLocation) -> Self {
        Self { line: l.start_line, column: l.start_column, start: Some(l.start), length: Some(l.end.saturating_sub(l.start)), index: None }
    }
}

/// `@lwc/template-compiler` `normalizeLocation(location?)`: a node without a parse5 location (TS
/// `sourceCodeLocation ?? undefined`) reports `{ line: 0, column: 0, start: 0, length: 0 }`, as upstream does.
pub fn normalize_location(l: Option<SourceLocation>) -> DiagnosticLocation {
    match l {
        Some(l) => l.into(),
        None => DiagnosticLocation { line: 0, column: 0, start: Some(0), length: Some(0), index: None },
    }
}

/// Line starts of a source text. Line terminators are JS's (`\r\n`, `\n`, `\r`, U+2028, U+2029), as in
/// Babel's tokenizer and `@babel/code-frame`; columns and offsets it reports are UTF-16 code units, as
/// JS string indices are.
pub struct LineIndex<'s> {
    text: &'s str,
    starts: Vec<u32>,
}

impl<'s> LineIndex<'s> {
    pub fn new(text: &'s str) -> Self {
        let mut starts = vec![0];
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'\n' => starts.push(i as u32 + 1),
                b'\r' if b.get(i + 1) != Some(&b'\n') => starts.push(i as u32 + 1),
                0xE2 if b.get(i + 1) == Some(&0x80) && matches!(b.get(i + 2), Some(0xA8 | 0xA9)) => {
                    starts.push(i as u32 + 3);
                    i += 2;
                }
                _ => {}
            }
            i += 1;
        }
        Self { text, starts }
    }

    /// Byte offset of (0-based line, UTF-16 column); `None` past the last line.
    pub fn to_offset(&self, line: u32, col16: u32) -> Option<u32> {
        let start = *self.starts.get(line as usize)?;
        let end = self.starts.get(line as usize + 1).copied().unwrap_or(self.text.len() as u32);
        let mut c = 0u32;
        for (i, ch) in self.text[start as usize..end as usize].char_indices() {
            if c >= col16 {
                return Some(start + i as u32);
            }
            c += ch.len_utf16() as u32;
        }
        Some(end)
    }

    /// (0-based line, 0-based UTF-16 column) of byte offset `off` (clamped; a mid-char offset rounds down).
    pub fn to_line_col(&self, off: u32) -> (u32, u32) {
        let off = self.floor(off);
        let line = self.starts.partition_point(|&s| s <= off as u32) - 1;
        let start = self.starts[line] as usize;
        (line as u32, utf16_len(&self.text[start..off]))
    }

    /// Babel position of byte offset `off`: (1-based line, 0-based UTF-16 column), as on a node `loc.start`.
    pub fn babel_line_col(&self, off: u32) -> (u32, u32) {
        let (l, c) = self.to_line_col(off);
        (l + 1, c)
    }

    /// UTF-16 index of byte offset `off` (a JS string index; Babel `node.start` / `Position.index`).
    pub fn utf16_offset(&self, off: u32) -> u32 {
        utf16_len(&self.text[..self.floor(off)])
    }

    /// Byte offset of UTF-16 index `u16_idx` (clamped to the text).
    pub fn byte_offset(&self, u16_idx: u32) -> u32 {
        let mut n = 0u32;
        for (b, ch) in self.text.char_indices() {
            if n >= u16_idx {
                return b as u32;
            }
            n += ch.len_utf16() as u32;
        }
        self.text.len() as u32
    }

    /// The `@lwc/errors` `Location` of the byte range `start..end`: Babel `loc.start` line/column, UTF-16
    /// `start` and `length` (`node.start`, `node.end - node.start`).
    pub fn location(&self, start: u32, end: u32) -> DiagnosticLocation {
        let (line, column) = self.babel_line_col(start);
        let s = self.utf16_offset(start);
        DiagnosticLocation { line, column, start: Some(s), length: Some(self.utf16_offset(end).saturating_sub(s)), index: None }
    }

    /// The span-keeping [`SourceLocation`] of the byte range `start..end` (Babel line/columns, byte offsets).
    pub fn source_location(&self, start: u32, end: u32) -> SourceLocation {
        let ((start_line, start_column), (end_line, end_column)) = (self.babel_line_col(start), self.babel_line_col(end));
        SourceLocation { start_line, start_column, end_line, end_column, start, end }
    }

    fn floor(&self, off: u32) -> usize {
        let mut o = (off as usize).min(self.text.len());
        while !self.text.is_char_boundary(o) {
            o -= 1;
        }
        o
    }
}

fn utf16_len(s: &str) -> u32 {
    s.chars().map(char::len_utf16).sum::<usize>() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn astral_and_crlf_positions_are_utf16_like_js() {
        // "a😀b\r\n  é@x\rq\u{2028}z": 😀 is 4 bytes / 2 UTF-16 units, é 2 bytes / 1 unit.
        let src = "a😀b\r\n  é@x\rq\u{2028}z";
        let ix = LineIndex::new(src);
        let at = |needle: &str| src.find(needle).unwrap() as u32;
        assert_eq!(ix.babel_line_col(at("b")), (1, 3));
        assert_eq!(ix.utf16_offset(at("b")), 3);
        assert_eq!(ix.babel_line_col(at("@")), (2, 3));
        // JS: "a😀b\r\n  é".length == 9
        assert_eq!(ix.location(at("@"), at("@x") + 2), DiagnosticLocation { line: 2, column: 3, start: Some(9), length: Some(2), index: None });
        assert_eq!(ix.babel_line_col(at("q")), (3, 0)); // lone \r ends a line
        assert_eq!(ix.babel_line_col(at("z")), (4, 0)); // U+2028 ends a line
        assert_eq!(ix.byte_offset(9), at("@"));
        assert_eq!(ix.to_offset(1, 3), Some(at("@")));
        // a byte offset inside 😀 rounds down to its start
        assert_eq!(ix.babel_line_col(2), (1, 1));
    }
}
