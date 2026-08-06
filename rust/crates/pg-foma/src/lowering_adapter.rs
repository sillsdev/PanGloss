//! The typed compiler axis: which of this crate's compilers lowers a candidate into a network.

use serde::{Deserialize, Serialize};

use crate::enumerate::EmissionStrategy;

/// WHICH of this crate's compilers lowers a candidate into a network -- named as an adapter rather
/// than left implicit in a `match` on `EmissionStrategy` scattered across `crate::recipe_runtime`.
/// Measurement showed that axis to be decisive (two whole-grammar compilers win two languages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoweringAdapter {
    /// `crate::build::build_controllable`: the only adapter that reads the candidate's own `Plan` at all.
    ControllablePlanCompose,
    /// `crate::emit`'s surface probe via `FomaProposer::new`: whole-grammar, derives its own topology and ignores the plan.
    TunedSurfaceEmit,
    /// `crate::emit::emit_underlying_templated` plus a compiled rewrite cascade: whole-grammar, likewise ignores the plan.
    TemplatedUnderlyingEmit,
}

impl LoweringAdapter {
    pub fn for_strategy(strategy: EmissionStrategy) -> Self {
        match strategy {
            EmissionStrategy::PlanComposed => Self::ControllablePlanCompose,
            EmissionStrategy::TunedSurfaceProbed => Self::TunedSurfaceEmit,
            EmissionStrategy::TemplatedUnderlyingTokens => Self::TemplatedUnderlyingEmit,
        }
    }

    /// The strategy this adapter realizes. Exhaustive in both directions so the correspondence is
    /// compiler-checked, not documented.
    pub fn strategy(self) -> EmissionStrategy {
        match self {
            Self::ControllablePlanCompose => EmissionStrategy::PlanComposed,
            Self::TunedSurfaceEmit => EmissionStrategy::TunedSurfaceProbed,
            Self::TemplatedUnderlyingEmit => EmissionStrategy::TemplatedUnderlyingTokens,
        }
    }

    /// Whether this adapter INTERPRETS the candidate's plan. `false` for both whole-grammar
    /// adapters: honouring one in `crate::recipe_runtime::build_candidate` would measure a
    /// different compiler than the candidate names.
    pub fn interprets_plan(self) -> bool {
        matches!(self, Self::ControllablePlanCompose)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ControllablePlanCompose => "controllable-plan-compose",
            Self::TunedSurfaceEmit => "tuned-surface-emit",
            Self::TemplatedUnderlyingEmit => "templated-underlying-emit",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The adapter axis is 1:1 with the strategy axis in both directions, required for the adapter identity to soundly stand in for the enum.
    #[test]
    fn every_strategy_has_exactly_one_adapter_and_back() {
        for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
            assert_eq!(
                LoweringAdapter::for_strategy(strategy).strategy(),
                strategy,
                "adapter/strategy correspondence must be total and injective"
            );
        }
        let adapters: BTreeSet<LoweringAdapter> = crate::strategy_coverage::ALL_STRATEGIES
            .iter()
            .map(|&s| LoweringAdapter::for_strategy(s))
            .collect();
        assert_eq!(
            adapters.len(),
            crate::strategy_coverage::ALL_STRATEGIES.len()
        );
        assert_eq!(
            adapters
                .iter()
                .filter(|adapter| adapter.interprets_plan())
                .count(),
            1,
            "exactly one adapter reads a plan"
        );
    }
}
