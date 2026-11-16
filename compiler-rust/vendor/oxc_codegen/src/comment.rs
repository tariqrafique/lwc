use std::{borrow::Cow, collections::hash_map::Entry};

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use oxc_ast::{
    Comment, CommentKind,
    ast::{Expression, Program},
};
use oxc_span::GetSpan;

use crate::{Codegen, LegalComment, options::CommentOptions};

type CommentList = SmallVec<[Comment; 1]>;

pub type CommentsMap = FxHashMap</* attached_to */ u32, CommentList>;

/// Whether a comment remains meaningful if its original AST anchor is removed.
/// Babel `adjustMultilineComment`: strip up to the comment's original column
/// of whitespace after each interior newline, then indent to `column`.
fn adjust_block_comment<'a>(
    raw: &'a str,
    comment: &Comment,
    source: &str,
    column: usize,
) -> std::borrow::Cow<'a, str> {
    if !raw.contains('\n') {
        return std::borrow::Cow::Borrowed(raw);
    }
    let orig = original_column(source, comment.span.start);
    if orig == 0 {
        return std::borrow::Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len() + column);
    for (i, line) in raw.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            let rest = strip_ws_chars(line, orig);
            if !rest.is_empty() {
                out.extend(std::iter::repeat_n(' ', column));
                out.push_str(rest);
            }
        } else {
            out.push_str(line);
        }
    }
    std::borrow::Cow::Owned(out)
}

fn strip_ws_chars(line: &str, n: usize) -> &str {
    let mut taken = 0usize;
    for (i, c) in line.char_indices() {
        if taken >= n || !matches!(c, ' ' | '\t') {
            return &line[i..];
        }
        taken += 1;
    }
    ""
}

fn original_column(source: &str, pos: u32) -> usize {
    let b = source.as_bytes();
    let pos = (pos as usize).min(b.len());
    let start = b[..pos]
        .iter()
        .rposition(|&c| c == b'\n')
        .map_or(0, |i| i + 1);
    source[start..pos].chars().count()
}

fn preserve_when_orphaned(comment: &Comment) -> bool {
    comment.is_legal() || comment.is_coverage_ignore_file()
}

/// A `pife`-marked arrow or function expression prints its leading comments
/// inside its own `(` wrap, so operand emission sites must not consume them.
fn is_pife_function(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::ArrowFunctionExpression(arrow) => arrow.pife,
        Expression::FunctionExpression(function) => function.pife,
        _ => false,
    }
}

/// Which annotation kind an emission site expects to recover from
/// [`Codegen::annotation_comments`].
///
/// `@__PURE__` / `#__PURE__` on a `CallExpression` or `NewExpression`, and
/// `@__NO_SIDE_EFFECTS__` / `#__NO_SIDE_EFFECTS__` on a function declaration or
/// expression, are not interchangeable: downstream tree-shakers only honor
/// each on its corresponding node kind. The filter prevents
/// [`Codegen::print_annotation_comment`] from emitting one kind where the
/// other was expected when both share an `attached_to`.
#[derive(Clone, Copy)]
pub enum AnnotationKind {
    Pure,
    NoSideEffects,
}

impl AnnotationKind {
    #[inline]
    fn matches(self, comment: &Comment) -> bool {
        match self {
            Self::Pure => comment.is_pure(),
            Self::NoSideEffects => comment.is_no_side_effects(),
        }
    }

    /// Canonical literal to emit when no verbatim source is available.
    /// `newline_after = true` is used at statement-level emission sites
    /// (function declarations, exports), `false` at inline emission sites
    /// (call / new / function expressions).
    #[inline]
    fn canonical(self, newline_after: bool) -> &'static str {
        match (self, newline_after) {
            (Self::Pure, false) => "/* @__PURE__ */ ",
            (Self::Pure, true) => "/* @__PURE__ */\n",
            (Self::NoSideEffects, false) => "/* @__NO_SIDE_EFFECTS__ */ ",
            (Self::NoSideEffects, true) => "/* @__NO_SIDE_EFFECTS__ */\n",
        }
    }
}

impl Codegen<'_> {
    pub(crate) fn build_comments(&mut self, comments: &[Comment]) {
        if self.options.comments == CommentOptions::disabled() {
            return;
        }
        // Each retained comment can create at most one map entry. Reserving
        // this upper bound avoids incremental map growth while preprocessing.
        self.comments.reserve(comments.len());
        for comment in comments {
            // Stash pure / no-side-effects comments by `attached_to` so the
            // emission site can recover the verbatim source text instead of
            // falling back to the canonical literal (rolldown#9408).
            // Best-effort: when several annotation comments share an
            // `attached_to`, only the last survives; the emission site falls
            // back to the canonical literal for the dropped ones.
            if comment.is_pure() || comment.is_no_side_effects() {
                if comment.is_leading() && self.options.print_annotation_comment() {
                    self.annotation_comments
                        .insert(comment.attached_to, comment.clone());
                }
                continue;
            }

            // D-0292: trailing comments are printed too. Babel keeps a trailing
            // comment on its token (`a = 1; // trail`) and an inner comment in the
            // gap it came from. Both are normal comments; pure annotations stay on
            // the annotation path above.
            let add = if comment.is_leading() {
                (comment.is_legal() && self.options.print_legal_comment())
                    || (comment.is_jsdoc() && self.options.print_jsdoc_comment())
                    || (comment.is_annotation() && self.options.print_annotation_comment())
                    || (comment.is_normal() && self.options.print_normal_comment())
            } else {
                comment.is_normal() && self.options.print_normal_comment()
            };

            if add {
                self.has_property_key_annotations |= comment.is_property_key_annotation();
                if preserve_when_orphaned(comment)
                    && let Err(idx) = self.orphan_comment_keys.binary_search(&comment.attached_to)
                {
                    self.orphan_comment_keys.insert(idx, comment.attached_to);
                }
                self.comments
                    .entry(comment.attached_to)
                    .or_default()
                    .push(comment.clone());
            }
        }
    }

    pub(crate) fn has_comment(&self, start: u32) -> bool {
        self.comments.contains_key(&start)
    }

    /// Emit a pure / no-side-effects annotation comment for the AST node at
    /// `start`, falling back to the canonical literal when no verbatim source
    /// can be recovered.
    ///
    /// The fallback covers four cases:
    /// - no annotation comment is stashed at `start`,
    /// - the stashed comment's kind doesn't match the emission site (e.g. a
    ///   `@__NO_SIDE_EFFECTS__` slot being queried by a `CallExpression`
    ///   site that needs `@__PURE__`),
    /// - the comment is a line comment but the site can't break the line, or
    /// - source text is unavailable (e.g. the [`Codegen::print_expression`]
    ///   path that skips [`Codegen::build_comments`]).
    ///
    /// Export sites pass `self.span.start` and only recover verbatim when the
    /// annotation precedes the `export` keyword. The rarer
    /// `export /* @__NO_SIDE_EFFECTS__ */ function …` form (annotation between
    /// `export` and `function`) attaches to the inner function's span and
    /// falls back to canonical here.
    pub(crate) fn print_annotation_comment(
        &mut self,
        start: u32,
        kind: AnnotationKind,
        newline_after: bool,
    ) {
        // `entry` probes the map once, and the comment is moved out of it rather than cloned.
        // `entry` reserves capacity when the key is absent, which allocates if the map is empty,
        // so check for that first. `annotation_comments` being empty is a common case.
        if self.source_text.is_some()
            && !self.annotation_comments.is_empty()
            && let Entry::Occupied(entry) = self.annotation_comments.entry(start)
            && kind.matches(entry.get())
            // Inline line comments would swallow the rest of the line
            && (!entry.get().is_line() || newline_after)
        {
            let comment = entry.remove();
            self.print_comment(&comment);
            if newline_after {
                self.print_hard_newline();
            } else {
                self.print_str(" ");
            }
            return;
        }
        self.print_str(kind.canonical(newline_after));
    }

    pub(crate) fn print_leading_comments(&mut self, start: u32) {
        if let Some(comments) = self.take_positioned(start, true) {
            self.print_comments(&comments);
        }
    }

    /// Trailing comments anchored at `end` (the previous token's exclusive end).
    /// Returns whether the printed text already ends in a newline.
    pub(crate) fn print_trailing_comments_at(&mut self, end: u32) -> bool {
        let Some(comments) = self.take_positioned(end, false) else {
            return false;
        };
        if let Some(b) = self.last_byte()
            && !matches!(b, b' ' | b'\t' | b'\n')
        {
            self.print_hard_space();
        }
        self.print_comments(&comments);
        self.last_byte() == Some(b'\n')
    }

    /// Comments whose text starts in `[from, to)`.
    ///
    /// `keep_attached_to` leaves a comment keyed at that anchor so the node that
    /// owns it can print it (`/* mid */` sits before the comma; the next
    /// property's leading comment stays on the property).
    pub(crate) fn take_comments_between(
        &mut self,
        from: u32,
        to: u32,
        keep_attached_to: Option<u32>,
    ) -> CommentList {
        if self.comments.is_empty() || from >= to {
            return CommentList::new();
        }
        let keys: Vec<u32> = self
            .comments
            .iter()
            .filter_map(|(&k, list)| {
                if keep_attached_to == Some(k) {
                    return None;
                }
                list.iter()
                    .any(|c| c.span.start >= from && c.span.start < to)
                    .then_some(k)
            })
            .collect();
        let mut out = CommentList::new();
        for k in keys {
            let Some(mut list) = self.comments.remove(&k) else {
                continue;
            };
            let (hit, rest): (Vec<_>, Vec<_>) = list
                .drain(..)
                .partition(|c| c.span.start >= from && c.span.start < to);
            if !rest.is_empty() {
                self.comments.insert(k, CommentList::from(rest));
            }
            out.extend(hit);
        }
        out.sort_by_key(|c| c.span.start);
        out
    }

    /// First `byte` in the source span `[from, to)`.
    pub(crate) fn find_byte(&self, from: u32, to: u32, byte: u8) -> Option<u32> {
        let src = self.source_text?;
        let start = from as usize;
        let end = (to as usize).min(src.len());
        if start >= end {
            return None;
        }
        src[start..end].find(byte as char).map(|i| from + i as u32)
    }

    pub(crate) fn print_gap_comments(
        &mut self,
        from: u32,
        to: u32,
        keep_attached_to: Option<u32>,
    ) -> bool {
        let comments = self.take_comments_between(from, to, keep_attached_to);
        let any = !comments.is_empty();
        if any {
            self.print_comments(&comments);
        }
        any
    }

    /// Leading (`leading`) or trailing comments stored at `at`.
    fn take_positioned(&mut self, at: u32, leading: bool) -> Option<CommentList> {
        if self.comments.is_empty() {
            return None;
        }
        let list = self.comments.get_mut(&at)?;
        let hit: CommentList = list
            .iter()
            .filter(|c| c.is_leading() == leading)
            .cloned()
            .collect();
        list.retain(|c| c.is_leading() != leading);
        if list.is_empty() {
            self.comments.remove(&at);
        }
        if hit.is_empty() { None } else { Some(hit) }
    }

    pub(crate) fn get_comments(&mut self, start: u32) -> Option<CommentList> {
        self.take_positioned(start, true)
    }

    /// One blank line in the source immediately before `pos` (the comment start).
    /// Indent spaces on the comment's line do not count. A blank line whose only
    /// job is to separate `{`, `(`, or `[` from the comment is not kept: Babel
    /// prints the first comment in a block on the next line either way.
    fn source_has_blank_line_before(&self, pos: u32) -> bool {
        let Some(src) = self.source_text else {
            return false;
        };
        let b = src.as_bytes();
        let mut i = pos as usize;
        if i > b.len() {
            return false;
        }
        let mut newlines = 0u32;
        while i > 0 {
            match b[i - 1] {
                b' ' | b'\t' | b'\r' => i -= 1,
                b'\n' => {
                    newlines += 1;
                    i -= 1;
                    if newlines == 2 {
                        break;
                    }
                }
                _ => break,
            }
        }
        if newlines < 2 {
            return false;
        }
        while i > 0 && matches!(b[i - 1], b' ' | b'\t' | b'\r') {
            i -= 1;
        }
        !(i > 0 && matches!(b[i - 1], b'{' | b'(' | b'['))
    }

    /// One blank line in the source immediately after `pos` (the comment end).
    fn source_has_blank_line_after(&self, pos: u32) -> bool {
        let Some(src) = self.source_text else {
            return false;
        };
        let b = src.as_bytes();
        let mut i = pos as usize;
        let mut newlines = 0u32;
        while i < b.len() {
            match b[i] {
                b' ' | b'\t' | b'\r' => i += 1,
                b'\n' => {
                    newlines += 1;
                    i += 1;
                }
                _ => break,
            }
        }
        // Babel does not keep a blank line between a trailing comment and `}` / `]` / `)`.
        if i < b.len() && matches!(b[i], b'}' | b']' | b')') {
            return false;
        }
        newlines >= 2
    }

    /// Column Babel's generator would use here: two spaces per indent level,
    /// plus any non-indent characters already on this line.
    fn babel_column(&self) -> usize {
        let bytes = self.code().as_bytes();
        let line_start = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let tail = &bytes[line_start..];
        let mut col = 0usize;
        for &b in tail {
            if b == b'\t' {
                col += 2;
            } else {
                col += 1;
            }
        }
        col
    }

    /// The current output line is empty or only the indent we just printed.
    fn current_line_is_indent(&self) -> bool {
        let bytes = self.code().as_bytes();
        let line_start = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        bytes[line_start..].iter().all(|&b| matches!(b, b' ' | b'\t'))
    }

    fn output_ends_with_blank_line(&self) -> bool {
        let b = self.code().as_bytes();
        let mut i = b.len();
        while i > 0 && matches!(b[i - 1], b' ' | b'\t') {
            i -= 1;
        }
        if i == 0 || b[i - 1] != b'\n' {
            return false;
        }
        i -= 1;
        while i > 0 && matches!(b[i - 1], b' ' | b'\t' | b'\r') {
            i -= 1;
        }
        i > 0 && b[i - 1] == b'\n'
    }

    #[inline]
    pub(crate) fn print_comments_at(&mut self, start: u32) {
        if let Some(comments) = self.get_comments(start) {
            self.print_comments(&comments);
        }
    }

    /// Print leading comments at `start` and glue the next token to them: after a
    /// group ending in a newline (line comment / `followed_by_newline`), print the
    /// indent — mid-expression callers have no statement machinery to do it, and an
    /// unindented next token renders differently once the parser re-anchors the
    /// comments to it (codegen would no longer be idempotent). Otherwise consume the
    /// pending indent-as-space so the token glues with a single space.
    #[inline]
    pub(crate) fn print_leading_comments_anchored_to_self(&mut self, start: u32) {
        if let Some(comments) = self.get_comments(start) {
            self.print_comments(&comments);
            if self.last_byte() == Some(b'\n') {
                self.print_indent();
            } else {
                self.consume_pending_indent_space();
            }
        }
    }

    /// Print a property-key annotation attached directly to a string or template literal.
    #[inline]
    pub(crate) fn print_property_key_annotation(&mut self, start: u32) {
        if !self.has_property_key_annotations {
            return;
        }
        if self
            .comments
            .get(&start)
            .is_some_and(|comments| comments.iter().any(Comment::is_property_key_annotation))
        {
            self.print_leading_comments_anchored_to_self(start);
        }
    }

    /// Print comments attached to an expression that survives codegen.
    ///
    /// Probes the parenthesized layers too: `a || /* c */ (x)` anchors the
    /// comment at the `(`, `a || (/* c */ x)` at `x` — an operand printer only
    /// sees one node, so the walk happens here for every emission site.
    pub(crate) fn print_leading_comments_before_expression(&mut self, expression: &Expression<'_>) {
        if self.comments.is_empty() || is_pife_function(expression) {
            return;
        }
        self.print_leading_comments_anchored_to_self(expression.span().start);
        if let Expression::ParenthesizedExpression(paren) = expression {
            self.print_leading_comments_before_expression(&paren.expression);
        }
    }

    /// Print an expression's comment group only when it contains an annotation
    /// comment, probing parenthesized layers like
    /// [`Self::print_leading_comments_before_expression`].
    ///
    /// This is the variant for emission sites that mutating consumers move
    /// statements into (the minifier merges `if(a)x;if(b)x;` into
    /// `if(a||(b,..))x`; rolldown finalizes moved nodes with their original
    /// spans). Comments are anchored by source position, so a dissolved
    /// statement's leading normal-comment group can coincide with the moved
    /// operand's span start — printing it there misplaces statement-level
    /// trivia inside an expression and is not idempotent
    /// (`test_normal_comment_before_logical_rhs_not_printed` documents the
    /// falsifier). Annotations are the one comment kind with expression-level
    /// meaning, so they still pass through.
    pub(crate) fn print_annotation_comments_before_expression(
        &mut self,
        expression: &Expression<'_>,
    ) {
        if self.comments.is_empty() || is_pife_function(expression) {
            return;
        }
        let start = expression.span().start;
        if self
            .comments
            .get(&start)
            .is_some_and(|comments| comments.iter().any(Comment::is_annotation))
        {
            self.print_leading_comments_anchored_to_self(start);
        }
        if let Expression::ParenthesizedExpression(paren) = expression {
            self.print_annotation_comments_before_expression(&paren.expression);
        }
    }

    /// Whether an orphan comment with `attached_to < end` is still pending.
    /// Used by block emitters to keep an empty body multi-line.
    #[inline]
    pub(crate) fn has_orphan_comments_before(&self, end: u32) -> bool {
        self.orphan_comment_keys
            .iter()
            .take_while(|&&k| k < end)
            .any(|k| self.comments.contains_key(k))
    }

    /// Drain pending orphan comments with `attached_to < end` and emit them in
    /// source order. Called at every statement boundary so legal and file-level
    /// coverage comments survive when their original anchor was removed by an
    /// upstream pass.
    #[inline]
    pub(crate) fn print_orphan_comments_before(&mut self, end: u32) {
        if self.orphan_comment_keys.is_empty() {
            return;
        }
        let idx = self.orphan_comment_keys.partition_point(|&k| k < end);
        if idx == 0 {
            return;
        }
        // Concatenate across keys so `print_comments` sees one sequence;
        // per-key calls would leak `print_next_indent_as_space` and produce
        // stray leading spaces.
        let mut orphans: Vec<Comment> = Vec::new();
        let comments = &mut self.comments;
        for k in self.orphan_comment_keys.drain(..idx) {
            let Some(entry) = comments.get_mut(&k) else {
                continue;
            };
            debug_assert!(entry.iter().any(preserve_when_orphaned));
            entry.retain(|comment| {
                if preserve_when_orphaned(comment) {
                    orphans.push(comment.clone());
                    false
                } else {
                    true
                }
            });
            if entry.is_empty() {
                comments.remove(&k);
            }
        }
        if let Some(last) = orphans.last_mut() {
            // Orphans aren't in their original position, so the source's
            // `followed_by_newline` hint no longer applies. Force it on so
            // `print_comments` emits a trailing newline instead of setting
            // `print_next_indent_as_space` — otherwise the next indent (often
            // before `}`) collapses to a space and pass 2 stops matching.
            last.set_followed_by_newline(true);
            self.print_comments(&orphans);
        }
    }

    /// Print comments attached to any position in the given range `(start, end)` (exclusive).
    /// Returns `true` if any comments were printed.
    pub(crate) fn print_comments_in_range(&mut self, start: u32, end: u32) -> bool {
        if self.comments.is_empty() {
            return false;
        }
        // Find and remove the first key in the range.
        let key = self
            .comments
            .keys()
            .find(|&&k| k > start && k < end)
            .copied();
        if let Some(key) = key {
            let comments = self.comments.remove(&key).unwrap();
            self.print_comments(&comments);
            return true;
        }
        false
    }

    pub(crate) fn print_expr_comments(&mut self, start: u32) -> bool {
        if self.comments.is_empty() {
            return false;
        }
        let Some(comments) = self.comments.remove(&start) else {
            return false;
        };

        for comment in &comments {
            self.print_hard_newline();
            self.print_indent();
            self.print_comment(comment);
        }

        if comments.is_empty() {
            false
        } else {
            self.print_hard_newline();
            true
        }
    }

    /// Put the printer on the line a comment should occupy.
    ///
    /// D-0292: Babel keeps one blank line before a leading comment when the
    /// source had one (`stmt;\n\n// c`). `preceded_by_newline` is only "not on
    /// the same line"; the extra line is read from the source span.
    fn prepare_comment_line(&mut self, comment: &Comment) {
        if comment.preceded_by_newline() {
            let blank = self.source_has_blank_line_before(comment.span.start)
                && !self.output_ends_with_blank_line();
            if let Some(b) = self.last_byte() {
                match b {
                    b'\n' => {
                        if blank {
                            self.print_hard_newline();
                        }
                        self.print_indent();
                    }
                    // A tab we just printed as indent is the line so far. A tab
                    // that trails a comment (`// note\t`) is not: break first.
                    b'\t' | b' ' if self.current_line_is_indent() => {}
                    _ => {
                        self.print_hard_newline();
                        if blank {
                            self.print_hard_newline();
                        }
                        self.print_indent();
                    }
                }
            }
        } else if let Some(b) = self.last_byte()
            && !matches!(b, b' ' | b'\t' | b'\n' | b'(' | b'[' | b'{')
        {
            // `1 /* c */` and `1 // c` keep the space Babel prints. Openers stay
            // tight: `{/* c */}`, `[/* c */]`, `function f() {/* c */}`.
            self.print_hard_space();
        } else if !self.consume_pending_indent_space()
            && matches!(self.last_byte(), None | Some(b'\n'))
        {
            // Only indent at a line start. Mid-line emission sites (`a ?? /* c */ b`,
            // `key: /* c */ value`, `${/* c */ expr}`) would otherwise get a full
            // indent injected mid-line, growing indentation on every codegen pass.
            self.print_indent();
        }
    }

    fn finish_comment(&mut self, comment: &Comment) {
        if comment.is_line() || comment.followed_by_newline() {
            self.print_hard_newline();
            // A blank line between this comment and the next token.
            if self.source_has_blank_line_after(comment.span.end)
                && !self.output_ends_with_blank_line()
            {
                self.print_hard_newline();
            }
        } else {
            self.print_next_indent_as_space = true;
        }
    }

    pub(crate) fn print_comments(&mut self, comments: &[Comment]) {
        let Some((first, rest)) = comments.split_first() else {
            return;
        };

        self.prepare_comment_line(first);
        self.print_comment(first);

        if let Some((last, middle)) = rest.split_last() {
            for comment in middle {
                if comment.preceded_by_newline() {
                    self.prepare_comment_line(comment);
                } else if comment.is_legal() {
                    self.print_hard_newline();
                } else {
                    self.print_soft_space();
                }
                self.print_comment(comment);
            }

            if last.preceded_by_newline() {
                self.prepare_comment_line(last);
            } else if last.is_legal() {
                self.print_hard_newline();
            } else {
                self.print_soft_space();
            }
            self.print_comment(last);
            self.finish_comment(last);
        } else {
            self.finish_comment(first);
        }
    }

    fn print_comment(&mut self, comment: &Comment) {
        let Some(source_text) = self.source_text else {
            return;
        };
        let comment_source = comment.span.source_text(source_text);
        match comment.kind {
            CommentKind::Line | CommentKind::HtmlOpen | CommentKind::SingleLineBlock => {
                self.print_str_escaping_script_close_tag(comment_source);
            }
            CommentKind::HtmlClose => {
                // `-->` is only a comment at the start of a line, allowing indentation.
                // Annotation emission can place it after code even without minification.
                let is_mid_line = self
                    .code()
                    .as_bytes()
                    .iter()
                    .rfind(|&&byte| !matches!(byte, b' ' | b'\t'))
                    .is_some_and(|&byte| byte != b'\n');
                let text = if is_mid_line {
                    self.print_str("//");
                    comment.content_span().source_text(source_text)
                } else {
                    comment_source
                };
                self.print_str_escaping_script_close_tag(text);
            }
            // D-0292: Babel prints a block comment from its source text. A comment
            // that stays at its original column is unchanged, tabs included.
            // One that moves (a field comment carried into the constructor) has
            // each continuation line's leading whitespace shifted by the column
            // difference: strip up to the original start column, then indent to
            // the column Babel would be at (two spaces per indent level).
            CommentKind::MultiLineBlock => {
                let adjusted =
                    adjust_block_comment(comment_source, comment, source_text, self.babel_column());
                self.print_str_escaping_script_close_tag(&adjusted);
            }
        }
    }

    /// Handle Eof / Linked / External Comments.
    /// Return a list of comments of linked or external.
    pub(crate) fn handle_eof_linked_or_external_comments(
        &mut self,
        program: &Program<'_>,
    ) -> Vec<Comment> {
        let legal_comments = &self.options.comments.legal;
        if matches!(legal_comments, LegalComment::None | LegalComment::Inline) {
            return vec![];
        }

        // Dedupe legal comments for smaller output size.
        let mut set = FxHashSet::default();
        let mut comments = vec![];

        let source_text = program.source_text;
        for comment in program.comments.iter().filter(|c| c.is_legal()) {
            let text = Cow::Borrowed(comment.span.source_text(source_text));
            // D-0292: legal-comment dedup uses the verbatim source text, the same
            // bytes `print_comment` emits. Re-indenting here split one comment
            // into two once the printer stopped re-indenting.
            if set.insert(text) {
                comments.push(comment.clone());
            }
        }

        if comments.is_empty() {
            return vec![];
        }

        match legal_comments {
            LegalComment::Eof => {
                self.print_hard_newline();
                // Clear the flag to ensure consistent formatting for all EOF comments
                self.print_next_indent_as_space = false;
                for c in comments {
                    self.print_comment(&c);
                    self.print_hard_newline();
                }
                vec![]
            }
            LegalComment::Linked(path) => {
                let path = path.clone();
                self.print_hard_newline();
                self.print_str("/*! For license information please see ");
                self.print_str(&path);
                self.print_str(" */");
                comments
            }
            LegalComment::External => comments,
            LegalComment::None | LegalComment::Inline => unreachable!(),
        }
    }
}
