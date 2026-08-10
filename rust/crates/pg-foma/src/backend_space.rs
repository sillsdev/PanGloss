//! Deterministic grammar-derived backend-space characterization, pruning accounting, feasible
//! bounds, and pilot sampling. Counts describe family instances, not arbitrary Plan trees.

use std::collections::{BTreeMap, BTreeSet};

use pg_grammar::model::Grammar;
use serde::{Deserialize, Serialize};

use crate::backend_registry::{BackendInstance, MaterializeError, MaterializerContext, Registry};
use crate::grammar_semantics::GrammarSemantics;
use crate::plan::{NodeId, Plan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Count {
    pub value: u64,
    pub overflowed: bool,
}

impl Count {
    pub const ZERO: Self = Self {
        value: 0,
        overflowed: false,
    };

    pub fn product(values: impl IntoIterator<Item = u64>) -> Self {
        values.into_iter().fold(
            Self {
                value: 1,
                overflowed: false,
            },
            |acc, value| match acc.value.checked_mul(value) {
                Some(product) if !acc.overflowed => Self {
                    value: product,
                    overflowed: false,
                },
                _ => Self {
                    value: u64::MAX,
                    overflowed: true,
                },
            },
        )
    }

    pub fn sum(values: impl IntoIterator<Item = Self>) -> Self {
        values.into_iter().fold(Self::ZERO, |acc, next| {
            match acc.value.checked_add(next.value) {
                Some(sum) if !acc.overflowed && !next.overflowed => Self {
                    value: sum,
                    overflowed: false,
                },
                _ => Self {
                    value: u64::MAX,
                    overflowed: true,
                },
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FeasibleCount {
    Exact {
        value: u64,
        overflowed: bool,
    },
    Estimate {
        lower: u64,
        upper: u64,
        sample_size: u64,
        uncertainty: u64,
        method: String,
        overflowed: bool,
    },
}

impl FeasibleCount {
    pub fn lower(&self) -> u64 {
        match self {
            Self::Exact { value, .. } => *value,
            Self::Estimate { lower, .. } => *lower,
        }
    }

    pub fn upper(&self) -> u64 {
        match self {
            Self::Exact { value, .. } => *value,
            Self::Estimate { upper, .. } => *upper,
        }
    }

    pub fn is_exact(&self) -> bool {
        matches!(self, Self::Exact { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceCounts {
    pub syntactic: Count,
    pub attested: Count,
    pub statically_admissible: Count,
    pub feasible: FeasibleCount,
    pub generated: u64,
    pub deduplicated: u64,
    pub rejected: u64,
    pub retained: u64,
    pub pruning: BTreeMap<String, u64>,
}

impl SpaceCounts {
    pub fn reconciles(&self) -> bool {
        let Some(classified) = self.rejected.checked_add(self.retained) else {
            return false;
        };
        self.deduplicated <= self.generated
            && classified == self.deduplicated
            && self
                .pruning
                .values()
                .try_fold(0u64, |sum, value| sum.checked_add(*value))
                .is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GrammarFacts {
    pub ordered_operations: u64,
    pub ordering_dependencies: u64,
    pub gated_subrules: u64,
    pub partitions: u64,
    pub templates: u64,
    pub branches: u64,
    pub reduplicative_allomorphs: u64,
    pub metathesis_rules: u64,
    pub morphology_layers: u64,
}

impl GrammarFacts {
    pub fn from_grammar(grammar: &Grammar) -> Self {
        Self::from_semantics(&GrammarSemantics::derive(grammar))
    }

    /// Every field is a PROJECTION of a
    /// fact `GrammarSemantics` already owns — the per-stratum operation/dependency sums, the gated
    /// subrules, the entry partition, the reduplicative-allomorph and metathesis counts. This struct
    /// used to re-walk the grammar for all of them, in parallel with
    /// `backend_registry::Applicability` doing the same walks for the boolean forms of the same
    /// questions.
    pub fn from_semantics(semantics: &GrammarSemantics<'_>) -> Self {
        Self {
            ordered_operations: semantics.ordered_operations(),
            ordering_dependencies: semantics.ordering_dependencies(),
            gated_subrules: semantics.gated_subrules().len() as u64,
            partitions: semantics.partition_count(),
            templates: semantics.template_count(),
            branches: semantics.mrule_count(),
            reduplicative_allomorphs: semantics.reduplicative_allomorph_count(),
            metathesis_rules: semantics.metathesis_rule_count(),
            morphology_layers: semantics.stratum_count() as u64,
        }
    }
}

#[derive(Debug)]
pub struct Characterization {
    pub facts: GrammarFacts,
    pub counts: SpaceCounts,
    pub admissible_instances: Vec<BackendInstance>,
    pub distinct_roots: Vec<NodeId>,
    pub seed: u64,
}

/// Characterizes and materializes up to `materialization_budget` statically admissible instances.
/// If the budget covers the whole static space, the feasible count is exact; otherwise the result
/// is an explicit lower/upper bound and never claims optimality.
pub fn characterize(
    grammar: &Grammar,
    registry: &Registry,
    baseline: &Plan,
    materialization_budget: u64,
    seed: u64,
) -> Result<Characterization, MaterializeError> {
    characterize_with_semantics(
        &GrammarSemantics::derive(grammar),
        registry,
        baseline,
        materialization_budget,
        seed,
    )
}

/// `characterize` over an already-derived `GrammarSemantics`. ONE derivation serves the admissible-instance
/// filter, every per-instance applicability re-check inside
/// `Registry::materialize_with_semantics`, and the `GrammarFacts` projection at the bottom.
/// Each of those three used to walk the grammar independently -- the per-instance one once per
/// sampled instance.
pub fn characterize_with_semantics(
    semantics: &GrammarSemantics<'_>,
    registry: &Registry,
    baseline: &Plan,
    materialization_budget: u64,
    seed: u64,
) -> Result<Characterization, MaterializeError> {
    let grammar = semantics.grammar();
    let all_instances = registry.instances();
    let admissible_instances = registry.instances_for_semantics(semantics);
    let syntactic = Count::sum(registry.families().map(|family| {
        Count::product(
            family
                .parameters
                .iter()
                .map(|parameter| parameter.domain.len() as u64),
        )
    }));
    let attested = Count::sum(registry.families().filter(|f| f.provenance.attested).map(
        |family| {
            Count::product(
                family
                    .parameters
                    .iter()
                    .map(|parameter| parameter.domain.len() as u64),
            )
        },
    ));
    let static_count = admissible_instances.len() as u64;
    let sample_indices = deterministic_sample_indices(
        admissible_instances.len(),
        materialization_budget.min(static_count) as usize,
        seed,
    );
    let context = MaterializerContext { grammar, baseline };
    let mut roots = BTreeSet::new();
    let mut materialization_rejected = 0u64;
    for index in sample_indices {
        match registry.materialize_with_semantics(&admissible_instances[index], &context, semantics)
        {
            Ok(candidate) => {
                let root = candidate.plan.root().ok_or_else(|| {
                    MaterializeError::RootlessPlan(admissible_instances[index].family_id.clone())
                })?;
                roots.insert(root);
            }
            Err(MaterializeError::Inapplicable(_)) | Err(MaterializeError::Invalid(_)) => {
                materialization_rejected += 1;
            }
            Err(error) => return Err(error),
        }
    }
    let sampled = materialization_budget.min(static_count);
    let duplicates = sampled
        .saturating_sub(materialization_rejected)
        .saturating_sub(roots.len() as u64);
    let feasible = if sampled == static_count {
        FeasibleCount::Exact {
            value: roots.len() as u64,
            overflowed: false,
        }
    } else {
        let lower = roots.len() as u64;
        FeasibleCount::Estimate {
            lower,
            upper: static_count,
            sample_size: sampled,
            uncertainty: static_count.saturating_sub(lower),
            method: "deterministic-seeded-bounded-materialization".to_owned(),
            overflowed: false,
        }
    };
    let inapplicable = (all_instances.len() as u64).saturating_sub(static_count);
    let unvisited = static_count.saturating_sub(sampled);
    let mut pruning = BTreeMap::new();
    pruning.insert("family-inapplicable".to_owned(), inapplicable);
    pruning.insert(
        "materialization-rejected".to_owned(),
        materialization_rejected,
    );
    pruning.insert("content-address-duplicate".to_owned(), duplicates);
    pruning.insert("not-visited-within-budget".to_owned(), unvisited);
    let generated = all_instances.len() as u64;
    let deduplicated = generated
        .saturating_sub(inapplicable)
        .saturating_sub(duplicates);
    let rejected = materialization_rejected;
    let retained = deduplicated.saturating_sub(rejected);
    let counts = SpaceCounts {
        syntactic,
        attested,
        statically_admissible: Count {
            value: static_count,
            overflowed: false,
        },
        feasible,
        generated,
        deduplicated,
        rejected,
        retained,
        pruning,
    };
    Ok(Characterization {
        facts: GrammarFacts::from_semantics(semantics),
        counts,
        admissible_instances,
        distinct_roots: roots.into_iter().collect(),
        seed,
    })
}

/// One pilot row: what each stage of ONE candidate's pre-search pass actually cost.
///
/// # Why `build`/`evaluation` are `Option` and `materialize`/`capability` are not
/// A pilot candidate that the capability envelope REFUSES is never built and never evaluated. This
/// type used to record `build: 0, evaluation: 0` for such a row, and `summarize_pilot` folded those
/// literal zeros into the build/evaluation percentiles — so a pilot sample containing refusals
/// reported a build cost pulled toward zero *for a stage that never executed*. That is not a
/// cosmetic error: those percentiles are summed into `PilotCosts` and decide which search strategy
/// the run uses.
///
/// `materialize` and `capability` stay unconditional because a refused row genuinely HAS both — the
/// candidate was materialized, and the capability check is the very thing that refused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StageMeasurement {
    pub materialize: u64,
    pub capability: u64,
    /// `None` when this candidate was never built. Not `Some(0)`: see the type doc.
    pub build: Option<u64>,
    /// `None` when this candidate was never evaluated. Not `Some(0)`: see the type doc.
    pub evaluation: Option<u64>,
    pub pruned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Quantiles {
    pub p50: u64,
    pub p95: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PilotSummary {
    pub sample_size: u64,
    pub seed: u64,
    pub pruning_ratio_ppm: u32,
    pub materialize: Quantiles,
    pub capability: Quantiles,
    pub build: Quantiles,
    pub evaluation: Quantiles,
    /// How many rows the `build`/`evaluation` quantiles were actually derived from.
    ///
    /// Those two quantiles are computed over the rows where those stages RAN, not over
    /// `sample_size`, so without this number a reader cannot tell a build p50 taken over 8 rows from
    /// one taken over 1. `#[serde(default)]` so reports written before this field existed still
    /// parse; a `0` there against a non-zero `build.p50` is itself the signal that the artifact
    /// predates the fix.
    #[serde(default)]
    pub executed_samples: u64,
}

pub fn summarize_pilot(measurements: &[StageMeasurement], seed: u64) -> PilotSummary {
    // Stages that ALWAYS run for a sampled candidate: percentiles over every row.
    let always = |select: fn(&StageMeasurement) -> u64| Quantiles {
        p50: percentile(measurements.iter().map(select).collect(), 50),
        p95: percentile(measurements.iter().map(select).collect(), 95),
    };
    // `filter_map` drops rows where the stage never ran rather than reading absence as a zero cost.
    let measured = |select: fn(&StageMeasurement) -> Option<u64>| {
        let values: Vec<u64> = measurements.iter().filter_map(select).collect();
        Quantiles {
            p50: percentile(values.clone(), 50),
            p95: percentile(values, 95),
        }
    };
    let pruned = measurements
        .iter()
        .filter(|measurement| measurement.pruned)
        .count() as u64;
    PilotSummary {
        sample_size: measurements.len() as u64,
        seed,
        pruning_ratio_ppm: if measurements.is_empty() {
            0
        } else {
            ((pruned.saturating_mul(1_000_000)) / measurements.len() as u64) as u32
        },
        materialize: always(|m| m.materialize),
        capability: always(|m| m.capability),
        build: measured(|m| m.build),
        evaluation: measured(|m| m.evaluation),
        executed_samples: measurements
            .iter()
            .filter(|measurement| measurement.build.is_some())
            .count() as u64,
    }
}

fn percentile(mut values: Vec<u64>, percentile: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let rank = ((values.len() * percentile).div_ceil(100)).max(1) - 1;
    values[rank.min(values.len() - 1)]
}

pub fn deterministic_sample_indices(population: usize, sample: usize, mut seed: u64) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..population).collect();
    for index in (1..indices.len()).rev() {
        seed = splitmix64(seed);
        indices.swap(index, seed as usize % (index + 1));
    }
    indices.truncate(sample.min(population));
    indices.sort_unstable();
    indices
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = value;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_arithmetic_reports_overflow() {
        assert_eq!(Count::product([2, 3]).value, 6);
        assert!(Count::product([u64::MAX, 2]).overflowed);
        assert!(
            Count::sum([
                Count {
                    value: u64::MAX,
                    overflowed: false
                },
                Count {
                    value: 1,
                    overflowed: false
                }
            ])
            .overflowed
        );
    }

    #[test]
    fn seeded_sampling_is_deterministic_and_seed_sensitive() {
        assert_eq!(
            deterministic_sample_indices(20, 5, 7),
            deterministic_sample_indices(20, 5, 7)
        );
        assert_ne!(
            deterministic_sample_indices(20, 5, 7),
            deterministic_sample_indices(20, 5, 8)
        );
    }

    #[test]
    fn nearest_rank_quantiles_and_pruning_ratio_are_truthful() {
        let measurements = [
            StageMeasurement {
                materialize: 1,
                capability: 2,
                build: Some(3),
                evaluation: Some(4),
                pruned: true,
            },
            StageMeasurement {
                materialize: 5,
                capability: 6,
                build: Some(7),
                evaluation: Some(8),
                pruned: false,
            },
            StageMeasurement {
                materialize: 9,
                capability: 10,
                build: Some(11),
                evaluation: Some(12),
                pruned: false,
            },
        ];
        let summary = summarize_pilot(&measurements, 42);
        assert_eq!(summary.materialize, Quantiles { p50: 5, p95: 9 });
        assert_eq!(summary.pruning_ratio_ppm, 333_333);
        assert_eq!(summary.seed, 42);
        assert_eq!(summary.executed_samples, 3);
    }

    /// No fake zero measurements: a refused candidate carries no build/evaluation reading, and quantiles must be taken only over rows that do.
    #[test]
    fn a_stage_that_never_ran_does_not_contribute_a_zero_to_its_quantiles() {
        let refused = StageMeasurement {
            materialize: 100,
            capability: 200,
            build: None,
            evaluation: None,
            pruned: true,
        };
        let ran = |build: u64| StageMeasurement {
            materialize: 100,
            capability: 200,
            build: Some(build),
            evaluation: Some(build * 2),
            pruned: false,
        };
        let summary = summarize_pilot(&[refused, refused, refused, ran(1_000), ran(3_000)], 1);
        assert_eq!(
            summary.build,
            Quantiles {
                p50: 1_000,
                p95: 3_000
            },
            "build quantiles must be taken over the two rows that were actually built"
        );
        assert_eq!(
            summary.evaluation,
            Quantiles {
                p50: 2_000,
                p95: 6_000
            }
        );
        assert_eq!(
            summary.executed_samples, 2,
            "the honest denominator for the build/evaluation quantiles, not sample_size"
        );
        assert_eq!(
            summary.sample_size, 5,
            "every sampled candidate still counts toward the pilot's size and pruning ratio"
        );
        assert_eq!(summary.pruning_ratio_ppm, 600_000);
        // The two stages a refused candidate genuinely DID pay are still summarized over every row.
        assert_eq!(summary.materialize, Quantiles { p50: 100, p95: 100 });
    }
}
