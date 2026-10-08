//! Constructs shapes with phonological lanes and the shared natural-class eligibility lane.

use pg_grammar_model::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar_model::model::Grammar;
use pg_grammar_model::segment::InvalidShape;
use pg_shape::{Shape, ShapeBuilder};

/// Segments text and attaches the model-owned matching lanes.
pub fn segment_with_features(
    grammar: &Grammar,
    table: &CharDefTable,
    word: &str,
) -> Result<Shape, InvalidShape> {
    // Reuses the vetted greedy longest-match segmentation for the node/char-def sequence, then re-emits it with feature lanes; segmenting twice is fine since that segmenter is the single source of truth for which char-defs a word decomposes into.
    let bare = pg_grammar_model::segment::segment(table, word)?;
    let w = pg_grammar_model::membership::width(&grammar.phon_features) as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, bare.len());
    for (_, kind, char_def, _flags) in bare.interior() {
        let lanes = lanes_for(table, CharDefId(char_def), w as usize);
        match kind {
            pg_shape::NodeKind::Segment => {
                b.push_segment_with_lanes(char_def, &lanes);
            }
            pg_shape::NodeKind::Boundary => {
                b.push_boundary_with_lanes(char_def, &lanes);
            }
            _ => unreachable!("interior() never yields anchors"),
        }
    }
    Ok(b.finish())
}

/// A char-def's feature lanes, padded/truncated to exactly `w`; the pad-with-full-mask fallback exists only in case `w` doesn't match the table's own grammar (never true in production, kept for robustness — mirrors `morph.rs`'s `fit`).
fn lanes_for(table: &CharDefTable, cd: CharDefId, w: usize) -> Vec<u64> {
    let raw = table.get(cd).matching_lanes();
    if raw.len() == w {
        raw.to_vec()
    } else {
        let mut v = vec![u64::MAX; w];
        v[..raw.len().min(w)].copy_from_slice(&raw[..raw.len().min(w)]);
        v
    }
}

/// Whether a char-def is a boundary (helper for the drivers' filter logic).
pub fn is_boundary(table: &CharDefTable, cd: CharDefId) -> bool {
    table.get(cd).kind() == CharDefKind::Boundary
}
