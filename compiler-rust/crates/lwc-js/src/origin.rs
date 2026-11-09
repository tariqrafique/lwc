//! The user's original source plus the private-method forward-rename pieces (`#m` ->
//! `__lwc_component_class_internal_private_m`; empty when nothing was renamed). Babel frames every
//! error from the original text, so passes over the renamed source map their offsets back through this.
use lwc_shared::edits::Piece;

#[derive(Clone, Copy)]
pub struct Origin<'a> { pub src: &'a str, pub pieces: &'a [Piece] }

impl Origin<'_> {
    /// Renamed-source offset -> original offset. Inside a replaced run a start maps to the `#`, an end past the name.
    ///
    /// `edits::apply_mapped` tiles the input: piece `i` covers input `[in_start, pieces[i + 1].in_start)`, and the
    /// last piece ends at `src.len()`. So a replaced run's input end is the next piece's start, or the end of the
    /// input when the run is last. Every offset of a span in the renamed source is inside a piece, except its two
    /// boundaries: a start at the end of the output (maps to `src.len()`) and an end at 0 (maps to 0). Any other
    /// offset is a span outside the renamed source, an internal bug: it panics in every profile.
    pub fn map(&self, off: u32, is_end: bool) -> u32 {
        let src_end = self.src.len() as u32;
        for (i, p) in self.pieces.iter().enumerate() {
            let inside = if is_end { off > p.out_start && off <= p.out_end } else { off >= p.out_start && off < p.out_end };
            if !inside { continue; }
            if p.kept { return p.in_start + (off - p.out_start); }
            if !is_end { return p.in_start; }
            return self.pieces.get(i + 1).map_or(src_end, |n| n.in_start);
        }
        // The two span boundaries that lie in no piece; anything else panics in every profile (no fallback position).
        if is_end && off == 0 { return 0; }
        let out_end = self.pieces.last().map_or(0, |p| p.out_end);
        assert!(!is_end && off == out_end, "offset {off} (is_end={is_end}) is outside the renamed source (len {out_end})");
        src_end
    }
    pub fn is_identity(&self) -> bool { self.pieces.iter().all(|p| p.kept) }
}

#[cfg(test)]
mod tests {
    use super::Origin;
    use lwc_shared::edits::{apply_mapped, Edit};

    fn origin_for<'a>(src: &'a str, at: (u32, u32), text: &str, pieces: &'a mut Vec<lwc_shared::edits::Piece>) -> (String, Origin<'a>) {
        let (out, p) = apply_mapped(src, vec![Edit { start: at.0, end: at.1, text: text.into() }]).unwrap();
        *pieces = p;
        (out, Origin { src, pieces })
    }

    #[test]
    fn replaced_run_maps_to_its_input_span() {
        let mut ps = vec![];
        let (out, o) = origin_for("a #m b", (2, 4), "__lwc_m", &mut ps);
        let (s, e) = (out.find("__lwc_m").unwrap() as u32, (out.find("__lwc_m").unwrap() + 7) as u32);
        assert_eq!((o.map(s, false), o.map(e, true)), (2, 4));
        assert_eq!((o.map(s + 3, false), o.map(s + 3, true)), (2, 4));
        assert_eq!(o.map(out.len() as u32 - 1, false), 5);
    }

    #[test]
    fn replaced_run_at_end_of_input_ends_at_src_len() {
        // no kept piece follows the run: its input end is src.len(), not its own start
        let mut ps = vec![];
        let (out, o) = origin_for("a #m", (2, 4), "__lwc_m", &mut ps);
        assert_eq!(o.map(out.len() as u32, true), 4);
        assert_eq!(o.map(out.len() as u32, false), 4, "a start at the end of the output");
        assert_eq!(o.map(0, true), 0, "an end at 0");
    }

    #[test]
    #[should_panic(expected = "outside the renamed source")]
    fn offset_past_the_output_is_an_internal_bug() {
        let mut ps = vec![];
        let (out, o) = origin_for("a #m b", (2, 4), "__lwc_m", &mut ps);
        o.map(out.len() as u32 + 1, true);
    }
}
