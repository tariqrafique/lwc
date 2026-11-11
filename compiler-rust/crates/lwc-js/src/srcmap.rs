//! Source maps for `outputConfig.sourcemap` (babel `sourceMaps: true | 'inline'`).
//!
//! The pipeline is a chain of text stages: span-edit passes (magic-string style) and oxc codegen
//! passes (parse -> [transform] -> print). Each stage records an output-offset -> input-offset map:
//! edit passes exactly (kept text shifts, inserted/replaced text maps to the edit start), codegen
//! passes from oxc's own token map. The final map takes the tokens of the LAST codegen pass, moves
//! their generated positions forward through the edit passes after it (version stamps, unforgeables
//! is before, helper prelude), and moves their original positions back through every earlier stage
//! to the user's source. Columns are UTF-16 (source map spec). Valid and sensible, not babel-identical.
use lwc_shared::edits::Piece;
use lwc_shared::location::LineIndex;
use oxc_sourcemap::{SourceMap, SourceMapBuilder};

enum Stage {
    Edit(Vec<Piece>),
    /// sorted (output offset, input offset) points of an oxc codegen map
    Codegen(Vec<(u32, u32)>),
}

pub struct Chain {
    enabled: bool,
    stages: Vec<Stage>,
}

impl Chain {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            stages: vec![],
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn push_edit(&mut self, pieces: Vec<Piece>) {
        if self.enabled {
            self.stages.push(Stage::Edit(pieces));
        }
    }

    /// Record an oxc codegen pass `input` -> `output` with its map.
    pub fn push_codegen(&mut self, map: &SourceMap<'_>, input: &str, output: &str) {
        if !self.enabled {
            return;
        }
        let (li, lo) = (LineIndex::new(input), LineIndex::new(output));
        let mut pts: Vec<(u32, u32)> = map
            .get_tokens()
            .filter_map(|t| {
                Some((
                    lo.to_offset(t.get_dst_line(), t.get_dst_col())?,
                    li.to_offset(t.get_src_line(), t.get_src_col())?,
                ))
            })
            .collect();
        pts.sort_by_key(|p| p.0);
        pts.dedup_by_key(|p| p.0);
        self.stages.push(Stage::Codegen(pts));
    }

    /// Build the final map for `output` against the original `src`.
    pub fn finish(&self, src: &str, filename: &str, output: &str) -> Option<SourceMap<'static>> {
        if !self.enabled {
            return None;
        }
        let k = self
            .stages
            .iter()
            .rposition(|s| matches!(s, Stage::Codegen(_)))?;
        let Stage::Codegen(pts) = &self.stages[k] else {
            unreachable!()
        };
        let (lsrc, lout) = (LineIndex::new(src), LineIndex::new(output));
        let mut out: Vec<(u32, u32)> = pts
            .iter()
            .filter_map(|&(o, i)| {
                let mut o = o;
                for s in &self.stages[k + 1..] {
                    let Stage::Edit(p) = s else { return None };
                    o = forward(p, o)?;
                }
                let mut i = i;
                for s in self.stages[..k].iter().rev() {
                    i = match s {
                        Stage::Edit(p) => back_edit(p, i),
                        Stage::Codegen(p) => back_codegen(p, i),
                    };
                }
                Some((o, i))
            })
            .collect();
        out.sort_by_key(|p| p.0);
        out.dedup_by_key(|p| p.0);
        let mut b = SourceMapBuilder::default();
        let id = b.add_source_and_content(filename, src);
        for (o, i) in out {
            // Identifier fidelity (Babel maps identifiers to their own `loc` and names them): a generated identifier
            // must land on the same identifier in the source, else it is synthesized code mapped to a neighbour.
            let name = match (ident_at(output, o), ident_at(src, i)) {
                (Some(g), Some(s)) if g == s => Some(b.add_name(g)),
                (Some(_), _) => continue,
                _ => None,
            };
            let (dl, dc) = lout.to_line_col(o);
            let (sl, sc) = lsrc.to_line_col(i);
            b.add_token(dl, dc, sl, sc, Some(id), name);
        }
        let mut map = b.into_sourcemap().into_owned();
        map.set_ignore_list(vec![]);
        Some(map)
    }
}

/// The JS identifier starting exactly at byte `at` (not in the middle of a longer word).
fn ident_at(text: &str, at: u32) -> Option<&str> {
    let at = at as usize;
    let rest = text.get(at..)?;
    let part = |c: char| c == '_' || c == '$' || c.is_alphanumeric();
    if text[..at].chars().next_back().is_some_and(part) {
        return None;
    }
    let first = rest.chars().next()?;
    if !(first == '_' || first == '$' || first.is_alphabetic()) {
        return None;
    }
    let end = rest.find(|c: char| !part(c)).unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Input offset -> output offset; None inside replaced text.
fn forward(p: &[Piece], i: u32) -> Option<u32> {
    // kept pieces have disjoint, increasing input ranges; the candidate is the last kept one starting <= i
    let idx = p.partition_point(|x| x.in_start <= i);
    let x = p[..idx].iter().rev().find(|x| x.kept)?;
    let len = x.out_end - x.out_start;
    (i <= x.in_start + len).then(|| x.out_start + (i - x.in_start))
}

fn back_edit(p: &[Piece], o: u32) -> u32 {
    let idx = p.partition_point(|x| x.out_end <= o);
    match p.get(idx) {
        Some(x) if x.kept => x.in_start + (o - x.out_start),
        Some(x) => x.in_start,
        None => p
            .last()
            .map(|x| x.in_start + if x.kept { x.out_end - x.out_start } else { 0 })
            .unwrap_or(o),
    }
}

fn back_codegen(p: &[(u32, u32)], o: u32) -> u32 {
    let idx = p.partition_point(|x| x.0 <= o);
    if idx == 0 {
        p.first().map(|x| x.1).unwrap_or(0)
    } else {
        p[idx - 1].1
    }
}
