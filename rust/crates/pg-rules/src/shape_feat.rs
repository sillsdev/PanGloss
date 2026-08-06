//! Feature-bearing shape construction (plan §5.2/§5.3).
//!
//! `pg_grammar::segment` returns width-0 shapes (segmentation gate only). The phonological-rule
//! engine matches on **phonological feature lanes**, so this module builds the feature-bearing
//! shape the matcher needs: segment the word, then set each node's `W = grammar.phon_features.len()`
//! lanes from its char-def's `feature_lanes()` (C#'s `CharacterDefinitionTable.Segment` attaching a
//! `FeatureStruct` per node when the phonological feature system is non-empty). Boundaries are
//! `OPTIONAL` and, as of plan §13.1 Tier-1 #1, carry their **real** char-def lanes (`Type=Boundary`
//! plus full-mask on every other lane) rather than a hardcoded fully-unconstrained row — see
//! `lanes_for`'s doc comment. Anchors bracket the shape with unconstrained lanes.

use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::Grammar;
use pg_grammar::segment::InvalidShape;
use pg_shape::{Shape, ShapeBuilder};

/// Segment `word` against `table` and attach per-node phonological feature lanes (`W` lanes each,
/// `W = grammar.phon_features.len()`, always >= 1 — see `pg_grammar::featsys` module docs on the
/// always-present synthetic `Type` feature). Boundaries become optional nodes carrying their real
/// char-def lanes (plan §13.1 Tier-1 #1 — previously hardcoded to fully unconstrained, which
/// silently discarded the boundary's own `Type` identity and let boundary-marker pattern nodes
/// match any segment; see `lanes_for`).
pub fn segment_with_features(
    grammar: &Grammar,
    table: &CharDefTable,
    word: &str,
) -> Result<Shape, InvalidShape> {
    // Reuses the vetted greedy longest-match segmentation for the node/char-def sequence, then re-emits it with feature lanes; segmenting twice is fine since that segmenter is the single source of truth for which char-defs a word decomposes into.
    let bare = pg_grammar::segment::segment(table, word)?;
    let w = grammar.phon_features.len() as u32;
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
    let raw = table.get(cd).feature_lanes();
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
