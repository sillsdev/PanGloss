//! Deterministic grammar-derived recipe-space characterization, pruning accounting, feasible
//! bounds, and pilot sampling. Counts describe family instances, not arbitrary Plan trees.

use std::collections::{BTreeMap, BTreeSet};

use pg_grammar::model::{Grammar, MorphRuleDef, PhonRuleDef};
use serde::{Deserialize, Serialize};

use crate::capability::rhs_has_true_reduplication;
use crate::enumerate::prules_in_order;
use crate::gate::{find_gated_subrules, partition_entries};
use crate::plan::{NodeId, Plan};
use crate::recipe_registry::{MaterializeError, MaterializerContext, RecipeInstance, Registry};

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
        let prules = prules_in_order(grammar);
        let gated = find_gated_subrules(grammar, &prules);
        let partitions = partition_entries(grammar, &gated, &prules).len() as u64;
        let reduplicative_allomorphs = grammar.mrules.iter().map(reduplication_count).sum::<u64>();
        let operations = grammar
            .strata
            .iter()
            .map(|stratum| {
                (stratum.prules.len() + stratum.mrules.len() + stratum.templates.len()) as u64
            })
            .sum();
        let dependencies = grammar
            .strata
            .iter()
            .map(|stratum| {
                let n = stratum.prules.len() + stratum.mrules.len() + stratum.templates.len();
                n.saturating_sub(1) as u64
            })
            .sum();
        Self {
            ordered_operations: operations,
            ordering_dependencies: dependencies,
            gated_subrules: gated.len() as u64,
            partitions,
            templates: grammar.templates.len() as u64,
            branches: grammar.mrules.len() as u64,
            reduplicative_allomorphs,
            metathesis_rules: grammar
                .prules
                .iter()
                .filter(|rule| matches!(rule, PhonRuleDef::Metathesis(_)))
                .count() as u64,
            morphology_layers: grammar.strata.len() as u64,
        }
    }
}

/// How many of `rule`'s allomorphs genuinely reduplicate. A COUNT, not a bool — `GrammarFacts`
/// reports `reduplicative_allomorphs` as a magnitude — but the per-allomorph decision is
/// [`crate::capability::rhs_has_true_reduplication`], the single authority for the fact (see that
/// function's doc for why the `redup_hint != Implicit` shortcut this used to carry is a trap).
fn reduplication_count(rule: &MorphRuleDef) -> u64 {
    let allomorphs = match rule {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => return 0,
    };
    allomorphs
        .iter()
        .filter(|allomorph| rhs_has_true_reduplication(&allomorph.rhs))
        .count() as u64
}

#[derive(Debug)]
pub struct Characterization {
    pub facts: GrammarFacts,
    pub counts: SpaceCounts,
    pub admissible_instances: Vec<RecipeInstance>,
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
    let all_instances = registry.instances();
    let admissible_instances = registry.instances_for_grammar(grammar);
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
        match registry.materialize(&admissible_instances[index], &context) {
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
        facts: GrammarFacts::from_grammar(grammar),
        counts,
        admissible_instances,
        distinct_roots: roots.into_iter().collect(),
        seed,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StageMeasurement {
    pub materialize: u64,
    pub capability: u64,
    pub build: u64,
    pub evaluation: u64,
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
}

pub fn summarize_pilot(measurements: &[StageMeasurement], seed: u64) -> PilotSummary {
    let quantiles = |select: fn(&StageMeasurement) -> u64| Quantiles {
        p50: percentile(measurements.iter().map(select).collect(), 50),
        p95: percentile(measurements.iter().map(select).collect(), 95),
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
        materialize: quantiles(|m| m.materialize),
        capability: quantiles(|m| m.capability),
        build: quantiles(|m| m.build),
        evaluation: quantiles(|m| m.evaluation),
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
                build: 3,
                evaluation: 4,
                pruned: true,
            },
            StageMeasurement {
                materialize: 5,
                capability: 6,
                build: 7,
                evaluation: 8,
                pruned: false,
            },
            StageMeasurement {
                materialize: 9,
                capability: 10,
                build: 11,
                evaluation: 12,
                pruned: false,
            },
        ];
        let summary = summarize_pilot(&measurements, 42);
        assert_eq!(summary.materialize, Quantiles { p50: 5, p95: 9 });
        assert_eq!(summary.pruning_ratio_ppm, 333_333);
        assert_eq!(summary.seed, 42);
    }
}
