//! Runs this stratum's synthesis cascade over a shape -- the build-time probing API
//! `SurfacePhonology` needs (`Variants`/`DeletionJunctions`), built on the existing engine
//! machinery rather than cloning it. See
//! `crate::rewrite`'s "F2 prerequisite" module note for the position-preserving mechanism
//! (soft-delete instead of physical removal) this builds on and why it reproduces C#'s node-count
//! arithmetic exactly.
//!
//! C# `SurfacePhonology.SurfaceNodes` (`SurfacePhonology.cs:305-322`) runs a fresh `Word` (no
//! lexical entry, no morphological rules -- just a bare segmented shape) through EVERY stratum's
//! phonological-rule cascade, in `language.Strata` order, unconditionally: there is no "which
//! stratum owns this word" entry gate the way the real per-word `SynthesisStratumRule.Apply` has,
//! because this probe `Word` is not associated with any lexical entry at all. `probe_synthesize`
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
/// metathesis rule; verified absent from the three reference grammars, see `crate::rewrite`'s note).
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

/// `render_nodes` over a shape as it stands, no cascade: what `table` spells each SEGMENT node's bundle as, which is how a final table spells a root entered on an inner stratum (hc.dll `CharacterDefinitionTable.GetMatchingStrReps`).
pub fn render_shape(table: &CharDefTable, shape: &Shape) -> Option<String> {
    let segs: Vec<ProbeSeg> = shape
        .interior()
        .filter(|&(_, kind, _, _)| kind == NodeKind::Segment)
        .map(|(i, _, char_def, _)| ProbeSeg {
            char_def,
            lanes: shape.node_lanes(i).to_vec(),
            deleted: false,
        })
        .collect();
    render_nodes(table, &segs)
}

/// Every SEGMENT char-def in `table` whose bundle unifies with `lanes`: identity-gated only when `table` carries no features at all, so a `char_def` declared by ANOTHER table is matched by bundle, never by its foreign index.
/// Mirrors `pg_parse::surface::matching_reps_for_node`'s Segment branch (pg-parse depends on pg-rules, not the reverse).
fn matching_cd_ids(table: &CharDefTable, char_def: u32, lanes: &[u64]) -> Vec<CharDefId> {
    let feature_bearing_table = char_def != NO_CHAR_DEF
        && (char_def as usize) < table.len()
        && table.unifiable_cds(CharDefId(char_def)).is_some();
    let mut out = Vec::new();
    for (id, cd) in table.iter() {
        if cd.kind() != CharDefKind::Segment {
            continue;
        }
        let member = if char_def == NO_CHAR_DEF {
            true // NO_CHAR_DEF (post-rewrite abstract node): pure lane unification, no identity gate.
        } else {
            feature_bearing_table || id.0 == char_def
        };
        if member && flat_unifiable(lanes, cd.feature_lanes()) {
            out.push(id);
        }
    }
    out
}

/// Every representation of every char-def `matching_cd_ids` selects, in table document order.
fn matching_reps(table: &CharDefTable, char_def: u32, lanes: &[u64]) -> Vec<String> {
    matching_cd_ids(table, char_def, lanes)
        .into_iter()
        .flat_map(|id| table.get(id).representations().iter().cloned())
        .collect()
}
