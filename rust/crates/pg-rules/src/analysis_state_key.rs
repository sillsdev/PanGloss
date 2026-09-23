//! Order-independent identity of an analysis-cascade node (C# `AnalysisStateKey`,
//! AnalysisStateKey.cs:29-116).
//!
//! Its one consumer is `AnalyzerConfig::merge_equivalent` — C# `Morpher.MergeEquivalentAnalyses`,
//! which folds candidates reaching the same analysis state into one canonical word plus
//! `Word::alternatives`. It lived in the now-removed `pg-memo` crate while the analysis memo also
//! keyed on it (`docs/divergences/045-memoization-removed.md`); nothing else reads it.

use std::collections::BTreeMap;

use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{AllomorphId, MRuleId, MorphemeId, StratumId};
use pg_shape::Shape;

use crate::word::{FinalTemplateState, MorphStatus};

/// The source-bearing morphology history that distinguishes equal-shaped analysis arrivals.
///
/// Deliberately omits procedural fields such as `passed_over`, which do not identify source
/// morphology and would split otherwise identical arrivals.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MorphHistoryKey {
    pub allomorph: AllomorphId,
    pub morpheme: MorphemeId,
    pub order: u32,
    pub status: MorphStatus,
    pub runtime_identity: Option<String>,
}

/// Fields cover every analysis-side rule input plus the source-bearing morphology history.
///
/// Deliberately **excludes** procedural fields such as `passed_over` and the mrule trail's order:
/// the order-independent `rule_counts` multiset is what lets two arrivals differing only in
/// unapplication order fold together, while morph history keeps two source-distinct arrivals apart.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AnalysisStateKey {
    shape: Shape,
    stratum: StratumId,
    syntactic_fs: FeatureStruct,
    realizational_fs: FeatureStruct,
    non_head_count: u32,
    /// Per-rule unapplication multiset; sorted keys make order-independent arrivals compare alike.
    rule_counts: BTreeMap<MRuleId, u32>,
    /// Source-bearing history prevents equal shapes with different trails folding together.
    morph_history: Vec<MorphHistoryKey>,
    /// Final-template interleaving state.
    state: FinalTemplateState,
}

impl AnalysisStateKey {
    /// Build a key from a word's already-extracted components, with `rule_counts` saturated at each rule's `max_apps`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shape: Shape,
        stratum: StratumId,
        syntactic_fs: FeatureStruct,
        realizational_fs: FeatureStruct,
        non_head_count: u32,
        rule_counts: BTreeMap<MRuleId, u32>,
        state: FinalTemplateState,
        morph_history: Vec<MorphHistoryKey>,
    ) -> Self {
        AnalysisStateKey {
            shape,
            stratum,
            syntactic_fs,
            realizational_fs,
            non_head_count,
            rule_counts,
            morph_history,
            state,
        }
    }
}
