//! The one seam every compiler-backend dispatch goes through: a `Backend` trait plus a closed table of three adapters, replacing three separate matches on `EmissionStrategy` that used to live in `completed_build.rs`, `backend_runtime.rs` and `worker.rs`.

use pg_grammar::model::{Grammar, PhonRuleDef};

use crate::analyzer::FomaProposer;
use crate::backend_selection::BackendSelection;
use crate::completed_build::{CompileAttempt, CompletedBackendBuild, CompletedBuildError};
use crate::enumerate::{EmissionStrategy, LoweredCandidate};
use crate::replace::SegAlphabet;

/// The single interface every compiler backend implements. `strategy`/`interprets_plan` are plain facts a caller needs without running anything; `compile_for_measurement`/`realize_accuracy_proposer` are the two REAL entry points the old per-module matches dispatched to, kept as two methods rather than one because their callers need different artifacts for different reasons -- see this module's own doc and the report that shipped this trait for why a shared signature was rejected.
pub trait Backend: Send + Sync {
    /// The `EmissionStrategy` this backend realizes. Exhaustive in both directions with [`backend_for`], so the correspondence is compiler-checked, not documented.
    fn strategy(&self) -> EmissionStrategy;
    /// Whether this backend interprets the candidate's own `Plan` (true only for the plan-composing backend; the two whole-grammar backends derive their own topology and ignore it).
    fn interprets_plan(&self) -> bool;
    /// The full, evidenced completed build `crate::completed_build::compile_completed_backend` hands to production selection: digests, completion proofs, closure validation. `PlanComposed` having no production compile arm is pinned by `plan_composed_backend_refuses_measurement_unconditionally` in this module's own tests.
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

/// `crate::build::build_controllable`: the only backend that reads a candidate's own `Plan` at all, and the one with no production measurement compile (`crate::completed_build`'s own pinned `plan_composed_has_no_production_compile_arm`).
pub struct PlanComposed;

impl Backend for PlanComposed {
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::PlanComposed
    }

    fn interprets_plan(&self) -> bool {
        true
    }

    fn compile_for_measurement(
        &self,
        _grammar: &Grammar,
        _selection: &BackendSelection,
        _request: &CompileAttempt,
    ) -> Result<CompletedBackendBuild, CompletedBuildError> {
        Err(CompletedBuildError::UnsupportedStrategy(EmissionStrategy::PlanComposed))
    }

    fn realize_accuracy_proposer(
        &self,
        candidate: &LoweredCandidate,
        grammar: &Grammar,
        opts: &foma::options::FomaOptions,
        alphabet: &SegAlphabet<'_>,
        prules: &[&PhonRuleDef],
    ) -> Result<FomaProposer, String> {
        crate::backend_runtime::realize_controllable_plan_proposer(candidate, grammar, opts, alphabet, prules)
    }
}

/// `crate::emit`'s surface probe via `FomaProposer::new`: whole-grammar, derives its own topology and ignores the plan.
pub struct LexcMainline;

impl Backend for LexcMainline {
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TunedSurfaceProbed
    }

    fn interprets_plan(&self) -> bool {
        false
    }

    fn compile_for_measurement(
        &self,
        grammar: &Grammar,
        _selection: &BackendSelection,
        request: &CompileAttempt,
    ) -> Result<CompletedBackendBuild, CompletedBuildError> {
        crate::completed_build::compile_tuned_surface_for_measurement(grammar, request)
    }

    fn realize_accuracy_proposer(
        &self,
        _candidate: &LoweredCandidate,
        grammar: &Grammar,
        _opts: &foma::options::FomaOptions,
        _alphabet: &SegAlphabet<'_>,
        _prules: &[&PhonRuleDef],
    ) -> Result<FomaProposer, String> {
        crate::backend_runtime::realize_tuned_surface_proposer(grammar)
    }
}

/// `crate::emit::emit_underlying_templated` plus a compiled rewrite cascade: whole-grammar, likewise ignores the plan.
pub struct TemplatedUnderlyingTokens;

impl Backend for TemplatedUnderlyingTokens {
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TemplatedUnderlyingTokens
    }

    fn interprets_plan(&self) -> bool {
        false
    }

    fn compile_for_measurement(
        &self,
        grammar: &Grammar,
        selection: &BackendSelection,
        request: &CompileAttempt,
    ) -> Result<CompletedBackendBuild, CompletedBuildError> {
        crate::completed_build::compile_templated_underlying_for_measurement(grammar, selection, request)
    }

    fn realize_accuracy_proposer(
        &self,
        _candidate: &LoweredCandidate,
        grammar: &Grammar,
        _opts: &foma::options::FomaOptions,
        _alphabet: &SegAlphabet<'_>,
        _prules: &[&PhonRuleDef],
    ) -> Result<FomaProposer, String> {
        crate::backend_runtime::realize_templated_underlying_proposer(grammar)
    }
}

static PLAN_COMPOSED: PlanComposed = PlanComposed;
static LEXC_MAINLINE: LexcMainline = LexcMainline;
static TEMPLATED_UNDERLYING_TOKENS: TemplatedUnderlyingTokens = TemplatedUnderlyingTokens;

/// The closed adapter table, in `crate::strategy_coverage::ALL_STRATEGIES` declaration order.
pub static ALL_BACKENDS: [&'static dyn Backend; 3] = [&PLAN_COMPOSED, &LEXC_MAINLINE, &TEMPLATED_UNDERLYING_TOKENS];

/// The one backend realizing `strategy`, and the one remaining match on `EmissionStrategy` this seam allows -- every other caller goes through this function or `ALL_BACKENDS` rather than matching the enum itself.
pub fn backend_for(strategy: EmissionStrategy) -> &'static dyn Backend {
    match strategy {
        EmissionStrategy::PlanComposed => &PLAN_COMPOSED,
        EmissionStrategy::TunedSurfaceProbed => &LEXC_MAINLINE,
        EmissionStrategy::TemplatedUnderlyingTokens => &TEMPLATED_UNDERLYING_TOKENS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `backend_for` must realize every `ALL_STRATEGIES` member as itself, and `ALL_BACKENDS` must cover each exactly once -- the closed-table half of the adapter/strategy correspondence.
    #[test]
    fn backend_for_and_all_backends_cover_every_strategy_exactly_once() {
        for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
            assert_eq!(backend_for(strategy).strategy(), strategy);
        }
        let mut covered: Vec<EmissionStrategy> =
            ALL_BACKENDS.iter().map(|backend| backend.strategy()).collect();
        covered.sort_by_key(|strategy| strategy.label());
        let mut expected: Vec<EmissionStrategy> = crate::strategy_coverage::ALL_STRATEGIES.to_vec();
        expected.sort_by_key(|strategy| strategy.label());
        assert_eq!(covered, expected, "ALL_BACKENDS must cover every strategy exactly once");
    }

    /// Exactly one backend interprets a plan.
    #[test]
    fn exactly_one_backend_interprets_a_plan() {
        assert_eq!(
            ALL_BACKENDS.iter().filter(|backend| backend.interprets_plan()).count(),
            1
        );
    }

    /// `PlanComposed` has no production compile arm, pinned here at the trait seam too (mirrors `completed_build::tests::plan_composed_has_no_production_compile_arm`).
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
