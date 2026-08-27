//! The
//! compile-time **profile** type — per-stage timings, per-group emitted-line counts, and the final
//! compiled network's own state/arc counts — collected from the PRODUCTION surface-prebaked
//! `crate::emit::emit_with_budget` -> `foma::lexcread::fsm_lexc_parse_string` path
//! (`crate::analyzer::FomaProposer::new_with_budget`). Measurements come from the production
//! emitter/analyzer once; this profile stores them without recomputation.
//!
//! # Scope: the production path only
//! This module profiles ONLY the production emitter/probe/lexc stages;
//! profiling the experimental `crate::replace`/`crate::gate` cascade (per-rule cascade own-net
//! metrics, alpha-tuple/group counts, the running composition
//! state/arc curve) needs that cascade wired into
//! the production constructor first — merely having experimental `crate::replace`/`crate::gate`
//! functions is insufficient. Every profile this module's production constructor
//! (`CompileProfileBuilder::production`) builds.
//!
//! # No observer-induced minimization
//! Every measurement here is a value the production path already computes for its own purposes:
//! `std::time::Instant` deltas around code that runs unconditionally anyway, [`EmitCounts::
//! lexc_lines`] snapshots (a plain integer field already incremented by `write_tag_entry`/
//! `write_bare`), and the compiled `foma::types::Fsm`'s own `statecount`/`arccount` fields (public,
//! free reads).
//! Nothing here calls `fsm_compose`/`fsm_union`/`fsm_minimize`/`fsm_lexc_parse_string` an extra
//! time, clones an `Fsm`, or otherwise performs automaton work solely to produce a metric.
//!
//! # Top-line compile time is mandatory
//! `CompileProfileBuilder::production` starts its own wall-clock timer at construction (called at
//! the very top of `FomaProposer::new_with_budget`, before `crate::emit::emit_with_budget_profiled`
//! even runs) and `CompileProfileBuilder::finish` (called after `fsm_lexc_parse_string` returns)
//! stamps `CompileProfile::total_elapsed_millis` from that SAME timer — the full
//! grammar-to-ready-network wall time, spanning both this crate's own emission work and the
//! vendored `foma` crate's lexc-parse call. Per-stage timings (`CompileProfile::stages`) are
//! attribution, not a guaranteed additive partition of the total: they cover the
//! stages this module names below, not every line of `emit_with_budget_profiled`'s own glue code
//! between them.
//!
//! # Stage boundaries
//! `CompileStage`'s six variants are the real, pre-existing sequential boundaries inside
//! `crate::emit::emit_with_budget_profiled` (the profiled core `crate::emit::emit_with_budget`
//! thinly wraps) plus the one boundary that lives in `crate::analyzer` instead (lexc parsing itself
//! is a vendored-crate call, not something `emit.rs` performs): `SurfaceSetup` (surface table +
//! precision catalog + phonology probe + structural-rule/rule-cache/morpher setup, all before any
//! root is collected), `RootCollection` (`crate::emit::collect_roots`), `PreexpandComposites`
//! (`crate::preexpand::build_composites_with_mode`), `StructuralComposites`
//! (`crate::emit::build_structural_composites`, zero-cost/zero-elapsed for a grammar with no
//! structural-composite candidate rules), `LexcConstruction` (every remaining lexc-source
//! string-building step: derivation layers, per-template-group slot chains, root-entry writing —
//! named as one stage here because, in this emitter's
//! own architecture, they are the SAME interleaved pass: `build_deriv_chain`/`build_slot_chain`
//! write directly into the growing `out: String` as they classify each rule, so there is no
//! separate "derive, then write" boundary to time independently without threading a timer through
//! every one of those shared helper functions -- which are also called by `crate::emit::
//! emit_underlying_templated`, a DIFFERENT emitter this change does not touch), and `LexcParse`
//! (`foma::lexcread::fsm_lexc_parse_string`, timed in `crate::analyzer`, the one stage this module
//! names but `emit.rs` never runs).
//!
//! # Per-template/continuation lexc line counts are per-GROUP, not per-literal-template
//! This emitter's own module doc ("Deliberate supersets" item 1) is explicit that templates sharing
//! an identical `required_syn_fs` are collapsed into ONE shared category-group root+derivation
//! section — lexc has no per-template graph-sharing equivalent, so line counts genuinely attach to
//! the GROUP a set of templates was collapsed into, not to any one template individually.
//! `GroupLineCount` therefore names the group index `emit::emit_with_budget_profiled`'s own
//! `group_keys`/`group_templates` vectors use (document order, first-seen `required_syn_fs`), the
//! coarsest-grained honest unit this emitter's architecture actually has — never a fabricated
//! per-template split this emitter cannot produce.
//!
//! # Judgment calls flagged for review
//! 1. **`CompileProfile` is JSON-serializable** (mirrors `crate::health`'s own canonical-JSON
//!    convention) with `Duration` fields stored as `u64` millis rather than `std::time::Duration`
//!    directly — `serde` has no built-in `Duration` support and this crate does not depend on
//!    `serde_with`; millis matches `crate::health::MetricValue::Millis`'s own unit exactly.
//! 2. **`final_state_count`/`final_arc_count` are `Option<i64>`, not `Option<u32>`**: mirrors
//!    `foma::types::Fsm`'s own `statecount`/`arccount: i32` fields exactly (widened to `i64` only to
//!    give callers a single non-negative-friendly integer type without a second `try_into`), and
//!    stays `None` (never a fabricated `0`) whenever the production path never reaches a compiled
//!    network at all (an `Unsupported`/budget-exceeded early return) -- see
//!    `crate::analyzer::FomaProposer::new_with_budget`'s own call site for exactly which outcomes
//!    leave this `None`.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// This profile's own pipeline fingerprint string: every compile profile names and fingerprints
/// the constructor/network it measures. Named after the exact
/// production call chain
/// (`crate::analyzer::FomaProposer::new_with_budget` -> `crate::emit::emit_with_budget_profiled` ->
/// `foma::lexcread::fsm_lexc_parse_string`) so a report reader can locate the real code path this
/// profile describes without guessing.
pub const PRODUCTION_PIPELINE: &str =
    "pg_foma::analyzer::FomaProposer::new_with_budget (crate::emit::emit_with_budget_profiled -> \
     foma::lexcread::fsm_lexc_parse_string)";

/// The real, sequential stage boundaries this module instruments (see module doc "Stage
/// boundaries"). Closed on purpose, same discipline `crate::health`'s own enums document: a new
/// stage is an additive variant, never a silent renumbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompileStage {
    /// Surface table lookup + FST precision catalog + phonology probe + structural-rule/rule-cache/
    /// morpher setup — everything before `Self::RootCollection`.
    SurfaceSetup,
    /// `crate::emit::collect_roots`.
    RootCollection,
    /// `crate::preexpand::build_composites_with_mode` (rule-application pre-expansion +
    /// boundary-fusion composite probing).
    PreexpandComposites,
    /// `crate::emit::build_structural_composites`, only when the grammar has at least one
    /// structural-composite candidate rule (zero-elapsed/absent from `CompileProfile::stages`
    /// otherwise — this crate's own "zero-cost when the construct is absent" convention).
    StructuralComposites,
    /// Every remaining lexc-source string-building step: derivation layers + per-template-group
    /// slot chains + root-entry writing (module doc: one interleaved pass in this emitter's own
    /// architecture, not two separable stages).
    LexcConstruction,
    /// `foma::lexcread::fsm_lexc_parse_string`, timed in `crate::analyzer` (the one stage this
    /// module names that `crate::emit` itself never runs).
    LexcParse,
}

impl CompileStage {
    /// A short, stable label for rendering (JSON already carries this via `serde`'s
    /// `rename_all = "snake_case"`; this is for a future plain-text/Markdown renderer, task A.4).
    pub const fn label(self) -> &'static str {
        match self {
            CompileStage::SurfaceSetup => "surface_setup",
            CompileStage::RootCollection => "root_collection",
            CompileStage::PreexpandComposites => "preexpand_composites",
            CompileStage::StructuralComposites => "structural_composites",
            CompileStage::LexcConstruction => "lexc_construction",
            CompileStage::LexcParse => "lexc_parse",
        }
    }
}

/// One stage's own elapsed wall time -- attribution, not a guaranteed additive
/// partition of `CompileProfile::total_elapsed_millis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageTiming {
    pub stage: CompileStage,
    pub elapsed_millis: u64,
}

/// One template-GROUP's own emitted-lexc-line count (module doc: per-group, not per-literal-
/// template — see that section for why). `group_index` is `crate::emit::emit_with_budget_profiled`'s
/// own `group_keys`/`group_templates` document-order index (`G{group_index}...` lexicon names in
/// the emitted lexc source itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupLineCount {
    pub group_index: u32,
    pub lines: u64,
}

/// The production-path compile profile (this module's own doc). Every field is a value
/// the production path already computed for another reason — see module doc "No observer-induced
/// minimization" section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompileProfile {
    /// This profile's pipeline fingerprint (`PRODUCTION_PIPELINE`) — an owned `String` (not
    /// `&'static str`) so this type derives `Deserialize` without a lifetime parameter.
    pub pipeline: String,
    /// D3: total grammar-to-ready-network wall time, in milliseconds — spans this crate's own
    /// emission work AND the vendored `foma` crate's lexc-parse call.
    pub total_elapsed_millis: u64,
    /// Per-stage attribution (module doc "Stage boundaries") — not sorted/deduplicated by this
    /// type; `CompileProfileBuilder` pushes each stage exactly once, in the order it actually ran,
    /// including zero entries when the production path bails out early (an `Unsupported`/
    /// budget-exceeded outcome) before reaching a later stage at all.
    pub stages: Vec<StageTiming>,
    /// Per-template-group emitted-line counts (module doc "Per-template/continuation lexc line
    /// counts"). Empty for a template-less grammar (no `<AffixTemplate>` at all).
    pub group_lines: Vec<GroupLineCount>,
    /// The grammar's own total emitted lexc line count (`crate::emit::EmitCounts::lexc_lines`'s
    /// final value). `None` iff the production
    /// path bailed out before reaching `crate::emit::emit_with_budget_profiled`'s own final
    /// `EmitResult` (an early `Unsupported` verdict) — never a fabricated `0`.
    pub total_lexc_lines: Option<u64>,
    /// The compiled network's own final state count (`foma::types::Fsm::statecount`, a free read —
    /// module doc "D2"). `None` when the production path stopped before a compiled network
    /// existed, pinned by `fst_profile_finish_with_no_compiled_network_leaves_counts_none`.
    pub final_state_count: Option<i64>,
    /// The compiled network's own final arc count (`foma::types::Fsm::arccount`). Same `None`
    /// convention as `Self::final_state_count`.
    pub final_arc_count: Option<i64>,
}

/// The mutable accumulator `crate::emit::emit_with_budget_profiled`/
/// `crate::analyzer::FomaProposer::new_with_budget` push measurements into as the production path
/// runs, consumed once by `Self::finish`. `pub(crate)`: this is an internal plumbing detail, not
/// part of this module's public surface — callers outside this crate only ever see the finished
/// `CompileProfile`.
pub(crate) struct CompileProfileBuilder {
    pipeline: &'static str,
    start: Instant,
    stages: Vec<StageTiming>,
    group_lines: Vec<GroupLineCount>,
    total_lexc_lines: Option<u64>,
}

impl CompileProfileBuilder {
    /// Starts a production profile's wall-clock timer NOW — callers must construct this
    /// at the very top of the production entry point (`crate::analyzer::FomaProposer::
    /// new_with_budget`), before any emission work runs, so `Self::finish`'s
    /// `total_elapsed_millis` is genuinely D3's "grammar-to-ready-network" span.
    pub(crate) fn production() -> Self {
        CompileProfileBuilder {
            pipeline: PRODUCTION_PIPELINE,
            start: Instant::now(),
            stages: Vec::new(),
            group_lines: Vec::new(),
            total_lexc_lines: None,
        }
    }

    /// Records one stage's already-measured elapsed time. Callers time their own code with a plain
    /// `Instant::now()`/`.elapsed()` pair around the exact stage boundary (module doc "Stage
    /// boundaries") rather than this builder wrapping a closure — several of those boundaries sit
    /// across an early `return` in `emit_with_budget_profiled` (a budget trip, an empty-roots
    /// bail-out), and a closure cannot early-return its OUTER function, so a closure-based API would
    /// not fit every stage this module names.
    pub(crate) fn push_stage(&mut self, stage: CompileStage, elapsed: Duration) {
        self.stages.push(StageTiming {
            stage,
            elapsed_millis: elapsed.as_millis() as u64,
        });
    }

    /// Records one template-group's emitted-line delta (module doc "Per-template/continuation lexc
    /// line counts").
    pub(crate) fn push_group_lines(&mut self, group_index: usize, lines: usize) {
        self.group_lines.push(GroupLineCount {
            group_index: group_index as u32,
            lines: lines as u64,
        });
    }

    /// Records the grammar's own total emitted lexc line count (`CompileProfile::total_lexc_lines`'s
    /// own doc). Called once, at the point `crate::emit::emit_with_budget_profiled` has its final
    /// `EmitCounts` in hand.
    pub(crate) fn set_total_lexc_lines(&mut self, lines: usize) {
        self.total_lexc_lines = Some(lines as u64);
    }

    /// Finishes the profile: stamps `total_elapsed_millis` from this builder's own start time (D3)
    /// and attaches the compiled network's final state/arc counts (`None` when the production path
    /// has no compiled network to report at all — see `CompileProfile::final_state_count`'s doc).
    pub(crate) fn finish(
        self,
        final_state_count: Option<i32>,
        final_arc_count: Option<i32>,
    ) -> CompileProfile {
        CompileProfile {
            pipeline: self.pipeline.to_string(),
            total_elapsed_millis: self.start.elapsed().as_millis() as u64,
            stages: self.stages,
            group_lines: self.group_lines,
            total_lexc_lines: self.total_lexc_lines,
            final_state_count: final_state_count.map(i64::from),
            final_arc_count: final_arc_count.map(i64::from),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fst_profile_stage_label_is_stable_and_exhaustive() {
        // Closed-enum discipline: every variant has a label and there is no catch-all arm, so adding a stage breaks this match until labeled.
        for stage in [
            CompileStage::SurfaceSetup,
            CompileStage::RootCollection,
            CompileStage::PreexpandComposites,
            CompileStage::StructuralComposites,
            CompileStage::LexcConstruction,
            CompileStage::LexcParse,
        ] {
            assert!(!stage.label().is_empty());
        }
    }

    #[test]
    fn fst_profile_builder_finish_stamps_total_elapsed_and_stages() {
        let mut builder = CompileProfileBuilder::production();
        builder.push_stage(CompileStage::SurfaceSetup, Duration::from_millis(5));
        builder.push_stage(CompileStage::RootCollection, Duration::from_millis(7));
        builder.push_group_lines(0, 42);
        builder.set_total_lexc_lines(1234);
        let profile = builder.finish(Some(100), Some(250));

        assert_eq!(profile.pipeline, PRODUCTION_PIPELINE);
        assert_eq!(profile.stages.len(), 2);
        assert_eq!(profile.stages[0].stage, CompileStage::SurfaceSetup);
        assert_eq!(profile.stages[0].elapsed_millis, 5);
        assert_eq!(profile.stages[1].elapsed_millis, 7);
        assert_eq!(
            profile.group_lines,
            vec![GroupLineCount {
                group_index: 0,
                lines: 42
            }]
        );
        assert_eq!(profile.final_state_count, Some(100));
        assert_eq!(profile.final_arc_count, Some(250));
        assert_eq!(profile.total_lexc_lines, Some(1234));
    }

    #[test]
    fn fst_profile_finish_with_no_compiled_network_leaves_counts_none() {
        // The production path can bail out before ever reaching a compiled network -- `None`, never a fabricated `0`.
        let profile = CompileProfileBuilder::production().finish(None, None);
        assert_eq!(profile.final_state_count, None);
        assert_eq!(profile.final_arc_count, None);
        assert_eq!(profile.total_lexc_lines, None);
    }

    #[test]
    fn fst_profile_json_round_trips() {
        let mut builder = CompileProfileBuilder::production();
        builder.push_stage(CompileStage::LexcConstruction, Duration::from_millis(12));
        let profile = builder.finish(Some(10), Some(20));
        let json = serde_json::to_string(&profile).expect("serialize");
        let parsed: CompileProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, profile);
    }

}
