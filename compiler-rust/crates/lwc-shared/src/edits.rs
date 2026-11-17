//! Span-based source edits (magic-string style). Transforms read the oxc AST and
//! emit edits, which are applied to the source text.
//!
//! Overlap rules (Arch Board AB-D0001 condition 2):
//! - An edit is *contained* in a removal R (an edit with `start < end`) when it lies fully inside R
//!   and is not R's boundary: a non-empty edit with `R.start <= start && end <= R.end` and a span
//!   different from R's, or an insert strictly inside (`R.start < at < R.end`). Contained edits are
//!   dropped, because R's text replaces everything under it.
//! - Inserts at a removal's boundaries are kept (before the replacement at `start`, after it at `end`).
//! - Any other overlap is an [`OverlapError`]: two non-empty edits that partially overlap, or two
//!   non-empty edits with the same span (ambiguous: neither contains the other).
use std::fmt;

#[derive(Debug, Clone)]
pub struct Edit {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

impl Edit {
    pub fn insert(at: u32, text: impl Into<String>) -> Self {
        Self {
            start: at,
            end: at,
            text: text.into(),
        }
    }
    pub fn replace(start: u32, end: u32, text: impl Into<String>) -> Self {
        Self {
            start,
            end,
            text: text.into(),
        }
    }
}

/// One piece of an applied edit list: output `[out_start, out_end)` is either kept input text starting
/// at `in_start` or inserted/replacement text for an edit at input `in_start`.
#[derive(Debug, Clone, Copy)]
pub struct Piece {
    pub out_start: u32,
    pub out_end: u32,
    pub in_start: u32,
    pub kept: bool,
}

/// Two non-empty edits whose spans overlap without one containing the other (or with equal spans).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlapError {
    pub first: (u32, u32),
    pub second: (u32, u32),
}

impl fmt::Display for OverlapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "overlapping edits [{}, {}) and [{}, {})",
            self.first.0, self.first.1, self.second.0, self.second.1
        )
    }
}

impl std::error::Error for OverlapError {}

/// Applies edits (see the module docs for overlap rules). Inserts at the same offset keep emission order.
pub fn apply(src: &str, edits: Vec<Edit>) -> Result<String, OverlapError> {
    apply_mapped(src, edits).map(|(s, _)| s)
}

/// [`apply`] plus the output->input piece map (for source maps).
pub fn apply_mapped(src: &str, edits: Vec<Edit>) -> Result<(String, Vec<Piece>), OverlapError> {
    let mut edits = drop_contained(edits)?;
    edits.sort_by_key(|e| (e.start, e.end)); // stable sort keeps insertion order
    let mut out = String::with_capacity(src.len() + 256);
    let mut pieces = Vec::with_capacity(edits.len() * 2 + 1);
    let mut pos = 0usize;
    let mut push = |out: &mut String, text: &str, in_start: usize, kept: bool| {
        if text.is_empty() {
            return;
        }
        let s = out.len() as u32;
        out.push_str(text);
        pieces.push(Piece {
            out_start: s,
            out_end: out.len() as u32,
            in_start: in_start as u32,
            kept,
        });
    };
    for e in edits {
        let (s, t) = (e.start as usize, e.end as usize);
        push(&mut out, &src[pos..s], pos, true);
        push(&mut out, &e.text, s, false);
        pos = t.max(pos);
    }
    push(&mut out, &src[pos..], pos, true);
    Ok((out, pieces))
}

/// Drops edits contained in a removal; errors on partial overlap or equal non-empty spans.
/// Keeps the relative order of the surviving edits.
fn drop_contained(edits: Vec<Edit>) -> Result<Vec<Edit>, OverlapError> {
    // Outermost removals, by start ascending then end descending (an enclosing removal precedes the ones it holds).
    let mut removals: Vec<(u32, u32)> = edits
        .iter()
        .filter(|e| e.start < e.end)
        .map(|e| (e.start, e.end))
        .collect();
    removals.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut outer: Vec<(u32, u32)> = Vec::new();
    for r in removals {
        match outer.last() {
            Some(&o) if r == o => {
                return Err(OverlapError {
                    first: o,
                    second: r,
                });
            }
            Some(&o) if r.0 < o.1 && r.1 <= o.1 => {} // contained in `o`
            Some(&o) if r.0 < o.1 => {
                return Err(OverlapError {
                    first: o,
                    second: r,
                });
            }
            _ => outer.push(r),
        }
    }
    let contained = |e: &Edit| {
        // The last outermost removal starting at or before e.start is the only one that can contain e.
        let i = outer.partition_point(|o| o.0 <= e.start);
        i > 0 && {
            let o = outer[i - 1];
            if e.start == e.end {
                o.0 < e.start && e.start < o.1
            } else {
                e.end <= o.1 && (e.start, e.end) != o
            }
        }
    };
    Ok(edits.into_iter().filter(|e| !contained(e)).collect())
}

#[cfg(test)]
mod tests;
