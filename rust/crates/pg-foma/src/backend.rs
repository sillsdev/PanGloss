//! The one seam every compiler-backend dispatch goes through: a `Backend` trait plus a closed table of three adapters, replacing three separate matches on `EmissionStrategy`/`LoweringAdapter` that used to live in `completed_build.rs`, `backend_runtime.rs` and `worker.rs`.

use pg_grammar::model::{Grammar, PhonRuleDef};
use serde::{Deserialize, Serialize};

use crate::analyzer::FomaProposer;
use crate::backend_selection::BackendSelection;
use crate::completed_build::{CompileAttempt, CompletedBackendBuild, CompletedBuildError};
use crate::enumerate::{EmissionStrategy, LoweredCandidate};
use crate::replace::SegAlphabet;

/// WHICH of this crate's compilers lowers a candidate into a network -- named as an adapter rather than left implicit in a match on `EmissionStrategy`. Measurement showed that axis to be decisive (two whole-grammar compilers win two languages).
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

    pub fn label(self) -> &'static str {
        match self {
            Self::ControllablePlanCompose => "controllable-plan-compose",
            Self::TunedSurfaceEmit => "tuned-surface-emit",
            Self::TemplatedUnderlyingEmit => "templated-underlying-emit",
        }
    }
}

/// The single interface every compiler backend implements. `strategy`/`interprets_plan` are plain facts a caller needs without running anything; `compile_for_measurement`/`realize_accuracy_proposer` are the two REAL entry points the old per-module matches dispatched to, kept as two methods rather than one because their callers need different artifacts for different reasons -- see this module's own doc and the report that shipped this trait for why a shared signature was rejected.
pub trait Backend {
    /// The `EmissionStrategy` this backend realizes. Exhaustive in both directions with [`backend_for`], so the correspondence is compiler-checked, not documented.
    fn strategy(&self) -> EmissionStrategy;
    /// Whether this backend interprets the candidate's own `Plan` (true only for the plan-composing backend; the two whole-grammar backends derive their own topology and ignore it).
    fn interprets_plan(&self) -> bool;
    /// The full, evidenced completed build `crate::completed_build::compile_completed_backend` hands to production selection: digests, completion proofs, closure validation. `ControllablePlanCompose` having no production compile arm is pinned by `plan_composed_backend_refuses_measurement_unconditionally` in this module's own tests.
    fn compile_for_measurement(
        &self,
        grammar: &Grammar,
        selection: &BackendSelection,
        request: &CompileAttempt,
    ) -> Result<CompletedBackendBuild, CompletedBuildError>;
    /// A bare, apply-ready proposer for the accuracy-scoring path -- lighter than [`Self::compile_for_measurement`] because a batch of candidates is scored without needing digest/proof machinery, and the plan-composing backend additionally needs `candidate`/`opts`/`alphabet`/`prules` the other two ignore.
    fn realize_accuracy_proposer(
        &self,
        candidate: &LoweredCandidate,
        grammar: &Grammar,
        opts: &foma::options::FomaOptions,
        alphabet: &SegAlphabet<'_>,
        prules: &[&PhonRuleDef],
    ) -> Result<FomaProposer, String>;
}

impl Backend for LoweringAdapter {
    fn strategy(&self) -> EmissionStrategy {
        match self {
            Self::ControllablePlanCompose => EmissionStrategy::PlanComposed,
            Self::TunedSurfaceEmit => EmissionStrategy::TunedSurfaceProbed,
            Self::TemplatedUnderlyingEmit => EmissionStrategy::TemplatedUnderlyingTokens,
        }
    }

    fn interprets_plan(&self) -> bool {
        matches!(self, Self::ControllablePlanCompose)
    }

    fn compile_for_measurement(
        &self,
        grammar: &Grammar,
        selection: &BackendSelection,
        request: &CompileAttempt,
    ) -> Result<CompletedBackendBuild, CompletedBuildError> {
        match self {
            Self::TunedSurfaceEmit => {
                crate::completed_build::compile_tuned_surface_for_measurement(grammar, request)
            }
            Self::TemplatedUnderlyingEmit => {
                crate::completed_build::compile_templated_underlying_for_measurement(
                    grammar, selection, request,
                )
            }
            Self::ControllablePlanCompose => {
                Err(CompletedBuildError::UnsupportedStrategy(EmissionStrategy::PlanComposed))
            }
        }
    }

    fn realize_accuracy_proposer(
        &self,
        candidate: &LoweredCandidate,
        grammar: &Grammar,
        opts: &foma::options::FomaOptions,
        alphabet: &SegAlphabet<'_>,
        prules: &[&PhonRuleDef],
    ) -> Result<FomaProposer, String> {
        match self {
            Self::TunedSurfaceEmit => crate::backend_runtime::realize_tuned_surface_proposer(grammar),
            Self::TemplatedUnderlyingEmit => {
                crate::backend_runtime::realize_templated_underlying_proposer(grammar)
            }
            Self::ControllablePlanCompose => crate::backend_runtime::realize_controllable_plan_proposer(
                candidate, grammar, opts, alphabet, prules,
            ),
        }
    }
}

static CONTROLLABLE_PLAN_COMPOSE: LoweringAdapter = LoweringAdapter::ControllablePlanCompose;
static TUNED_SURFACE_EMIT: LoweringAdapter = LoweringAdapter::TunedSurfaceEmit;
static TEMPLATED_UNDERLYING_EMIT: LoweringAdapter = LoweringAdapter::TemplatedUnderlyingEmit;

/// The closed adapter table, in `crate::strategy_coverage::ALL_STRATEGIES` declaration order.
pub const ALL_BACKENDS: &[&dyn Backend] =
    &[&CONTROLLABLE_PLAN_COMPOSE, &TUNED_SURFACE_EMIT, &TEMPLATED_UNDERLYING_EMIT];

/// The one backend realizing `strategy` -- the single call every former per-module match now goes through.
pub fn backend_for(strategy: EmissionStrategy) -> &'static dyn Backend {
    match strategy {
        EmissionStrategy::PlanComposed => &CONTROLLABLE_PLAN_COMPOSE,
        EmissionStrategy::TunedSurfaceProbed => &TUNED_SURFACE_EMIT,
        EmissionStrategy::TemplatedUnderlyingTokens => &TEMPLATED_UNDERLYING_EMIT,
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
        assert_eq!(adapters.len(), crate::strategy_coverage::ALL_STRATEGIES.len());
        assert_eq!(
            adapters.iter().filter(|adapter| adapter.interprets_plan()).count(),
            1,
            "exactly one adapter reads a plan"
        );
    }

    /// `backend_for` and `ALL_BACKENDS` must agree with each other and with `LoweringAdapter::for_strategy` on every strategy.
    #[test]
    fn backend_for_matches_all_backends_and_lowering_adapter() {
        for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
            assert_eq!(backend_for(strategy).strategy(), strategy);
            assert!(
                ALL_BACKENDS.iter().any(|backend| backend.strategy() == strategy),
                "ALL_BACKENDS is missing a backend for {strategy:?}"
            );
        }
        assert_eq!(ALL_BACKENDS.len(), crate::strategy_coverage::ALL_STRATEGIES.len());
    }

    /// `ControllablePlanCompose` has no production compile arm, pinned here at the trait seam too (mirrors `completed_build::tests::plan_composed_has_no_production_compile_arm`).
    #[test]
    fn plan_composed_backend_refuses_measurement_unconditionally() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>BackendTraitFixture</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let grammar = pg_grammar::load(XML).expect("fixture must load");
        let selection = crate::backend_selection::select_backends_for_grammar(&grammar);
        let request = CompileAttempt::try_new().expect("attempt id must construct");
        let result = backend_for(EmissionStrategy::PlanComposed)
            .compile_for_measurement(&grammar, &selection, &request);
        assert!(matches!(
            result,
            Err(CompletedBuildError::UnsupportedStrategy(EmissionStrategy::PlanComposed))
        ));
    }
}
