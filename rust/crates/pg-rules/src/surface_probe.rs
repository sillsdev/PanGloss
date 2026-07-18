//! F2 prerequisite (HYBRID_FST_RUST_PLAN.md §7.1 item 2a): "run this stratum's/language's
//! synthesis cascade over this shape" -- the build-time probing API `hc_hybrid::surface`'s
//! `SurfacePhonology` needs (`Variants`/`DeletionJunctions`), exposed on top of the existing
//! engine machinery rather than cloning it (per the plan's own instruction). See
//! `crate::rewrite`'s "F2 prerequisite" module note for the position-preserving mechanism
//! (soft-delete instead of physical removal) this builds on and why it reproduces C#'s node-count
//! arithmetic exactly.
//!
//! C# `SurfacePhonology.SurfaceNodes` (`SurfacePhonology.cs:305-322`) runs a fresh `Word` (no
//! lexical entry, no morphological rules -- just a bare segmented shape) through EVERY stratum's
//! phonological-rule cascade, in `language.Strata` order, unconditionally: there is no "which
//! stratum owns this word" entry gate the way the real per-word `SynthesisStratumRule.Apply` has,
//! because this probe `Word` is not associated with any lexical entry at all. [`probe_synthesize`]
//! mirrors that exactly.

use pg_featstruct::flat_unifiable;
use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::Grammar;
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};

use crate::cache::RuleCache;
use crate::rewrite::{self, MutShape, ProbeOutcome};

/// One SEGMENT-kind node surviving a probe cascade (boundaries/anchors already excluded -- C#
/// `SurfaceNodes`'s `Where(n => n.Annotation.Type() == HCFeatureSystem.Segment)`,
/// `SurfacePhonology.cs:321` -- boundaries never appear in `Variants`/`DeletionJunctions`
/// rendering at all, only in the verbatim `underlying` string each seeds its own result with).
#[derive(Clone, Debug)]
pub struct ProbeSeg {
    pub char_def: u32,
    pub lanes: Vec<u64>,
    pub deleted: bool,
}

/// Run every stratum's phonological-rule cascade over `shape` (C# `SurfacePhonology.SurfaceNodes`),
/// then filter to SEGMENT-kind nodes only, preserving positions (deleted nodes are marked, never
/// dropped -- see `crate::rewrite`'s module note). Returns `None` iff the cascade reaches a
/// structurally-unrepresentable rule (`ProbeOutcome::Refused` -- an empty-LHS/Epenthesis rule or a
/// metathesis rule; unreachable on the three reference grammars, see `crate::rewrite`'s note).
///
/// `cache` (built ONCE by the caller, e.g. `SurfacePhonology::new` -> `RuleCache::build`, and reused
/// across every probe) is not optional here: `SurfacePhonology` probes an affix underlying form
/// against every alphabet representative (up to alphabet² for `DeletionJunctions`), so recompiling
/// each rule's FST per call -- rather than once, up front -- turns Amharic's already-known-slow
/// 417-segment-alphabet probing (the plan's own ~112s C# figure) into something orders of magnitude
/// worse. See `crate::rewrite::probe_apply_rule_cached`'s doc for the measured motivation.
pub fn probe_synthesize(g: &Grammar, shape: &Shape, cache: &RuleCache) -> Option<Vec<ProbeSeg>> {
    let mut ms = MutShape::from_shape(shape);
    for sd in &g.strata {
        if let ProbeOutcome::Refused =
            rewrite::probe_synthesize_stratum(g, &sd.prules, &mut ms, cache)
        {
            return None;
        }
    }
    Some(
        ms.nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Segment)
            .map(|n| ProbeSeg {
                char_def: n.char_def,
                lanes: n.lanes.clone(),
                deleted: n.deleted,
            })
            .collect(),
    )
}

/// C# `SurfacePhonology.RenderNodes` (`SurfacePhonology.cs:173-190`): render `segs` to their
/// surface string, OMITTING any deleted node, returning `None` if a SURVIVING node has no single
/// matching representation (an under-specified node -- ported for fidelity to the C# `null`-return
/// contract; never expected to fire for a concretely-segmented probe node in practice).
pub fn render_nodes(table: &CharDefTable, segs: &[ProbeSeg]) -> Option<String> {
    let mut out = String::new();
    for seg in segs {
        if seg.deleted {
            continue;
        }
        let reps = matching_reps(table, seg.char_def, &seg.lanes);
        let first = reps.into_iter().next()?;
        out.push_str(&first);
    }
    Some(out)
}

/// Every char-def whose representation this SEGMENT node currently matches, first-match order --
/// PARITY: a duplicate, not a reuse, of `pg_parse::surface::matching_reps_for_node`'s Segment
/// branch (`pg-parse/src/surface.rs`); pg-parse depends on pg-rules, so the reverse dependency this
/// module would need is unavailable, and this crate's own probing nodes never carry the abstract
/// `CdSet::Members` case that function's general signature also handles (`segment_with_features`
/// always segments to concrete char-defs; only a feature-change rule's rewrite can later clear a
/// node's `char_def` to [`NO_CHAR_DEF`] -- `crate::rewrite::syn_feature`'s doc -- which is exactly
/// the identity-vs-unrestricted split below). Table document order, matching every other rendering
/// site in this port.
fn matching_reps(table: &CharDefTable, char_def: u32, lanes: &[u64]) -> Vec<String> {
    let mut out = Vec::new();
    for (id, cd) in table.iter() {
        if cd.kind() != CharDefKind::Segment {
            continue;
        }
        let member = if char_def != NO_CHAR_DEF {
            id.0 == char_def
                || table
                    .unifiable_cds(CharDefId(char_def))
                    .is_some_and(|b| b.contains(id.0))
        } else {
            true // NO_CHAR_DEF (post-rewrite abstract node): pure lane unification, no identity gate.
        };
        if !member {
            continue;
        }
        if flat_unifiable(lanes, cd.feature_lanes()) {
            out.extend(cd.representations().iter().cloned());
        }
    }
    out
}
