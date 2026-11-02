//! Span-based source edits (magic-string style). Transforms read the oxc AST and
//! emit edits; the result is re-parsed and printed by oxc codegen.
#[derive(Debug, Clone)]
pub struct Edit { pub start: u32, pub end: u32, pub text: String }

impl Edit {
    pub fn insert(at: u32, text: impl Into<String>) -> Self { Self { start: at, end: at, text: text.into() } }
    pub fn replace(start: u32, end: u32, text: impl Into<String>) -> Self { Self { start, end, text: text.into() } }
}

/// One piece of an applied edit list: output `[out_start, out_end)` is either kept input text starting
/// at `in_start` or inserted/replacement text for an edit at input `in_start`.
#[derive(Debug, Clone, Copy)]
pub struct Piece { pub out_start: u32, pub out_end: u32, pub in_start: u32, pub kept: bool }

/// Applies non-overlapping edits. Inserts at the same offset keep emission order.
pub fn apply(src: &str, edits: Vec<Edit>) -> String { apply_mapped(src, edits).0 }

/// [`apply`] plus the output->input piece map (for source maps).
pub fn apply_mapped(src: &str, mut edits: Vec<Edit>) -> (String, Vec<Piece>) {
    edits.sort_by_key(|e| (e.start, e.end)); // stable sort keeps insertion order
    let mut out = String::with_capacity(src.len() + 256);
    let mut pieces = Vec::with_capacity(edits.len() * 2 + 1);
    let mut pos = 0usize;
    let mut push = |out: &mut String, text: &str, in_start: usize, kept: bool| {
        if text.is_empty() { return; }
        let s = out.len() as u32;
        out.push_str(text);
        pieces.push(Piece { out_start: s, out_end: out.len() as u32, in_start: in_start as u32, kept });
    };
    for e in edits {
        let (s, t) = (e.start as usize, e.end as usize);
        debug_assert!(s >= pos, "overlapping edits");
        let s = s.max(pos);
        push(&mut out, &src[pos..s], pos, true);
        push(&mut out, &e.text, s, false);
        pos = t.max(pos);
    }
    push(&mut out, &src[pos..], pos, true);
    (out, pieces)
}
