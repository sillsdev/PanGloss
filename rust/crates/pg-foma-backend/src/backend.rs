//! The one seam every compiler-backend dispatch goes through: a `Backend` trait plus a closed table of adapters, one per `EmissionStrategy`. Only `LexcMainline` (`TunedSurfaceProbed`) exists; add a backend by adding an `EmissionStrategy` variant, an adapter here, and a row in `ALL_BACKENDS`/`backend_for`.

use pg_grammar::model::Grammar;

use crate::analyzer::FomaProposer;
use crate::backend_runtime::{CorpusEvalContext, EvaluatedPlan};
use crate::backend_selection::BackendSelection;
use crate::completed_build::{CompileAttempt, CompletedBackendBuild, CompletedBuildError};
use crate::enumerate::{EmissionStrategy, LoweredCandidate};

/// The single interface every compiler backend implements. `strategy` is a plain fact a caller needs without running anything; `compile_for_measurement`, `realize_accuracy_proposer` and `evaluate_for_corpus` are the three jobs a backend does for the evaluators.
pub trait Backend: Send + Sync {
    /// The `EmissionStrategy` this backend realizes. Exhaustive in both directions with [`backend_for`], so the correspondence is compiler-checked, not documented.
    fn strategy(&self) -> EmissionStrategy;
    /// The full, evidenced completed build `crate::completed_build::compile_completed_backend` hands to production selection: digests, completion proofs, closure validation.
    fn compile_for_measurement(
        &self,
        grammar: &Grammar,
        selection: &BackendSelection,
        request: &CompileAttempt,
    ) -> Result<CompletedBackendBuild, CompletedBuildError>;
    /// A bare, apply-ready proposer for the accuracy-scoring path -- lighter than [`Self::compile_for_measurement`] because a batch of candidates is scored without needing digest/proof machinery.
    fn realize_accuracy_proposer(
        &self,
        candidate: &LoweredCandidate,
        grammar: &Grammar,
    ) -> Result<FomaProposer, String>;
    /// Realizes one candidate for CORPUS evaluation (propose+confirm scored over a word list) --
    /// `crate::backend_runtime::evaluate_plans_with_cache_mode`'s own per-candidate step, a third job
    /// distinct from [`Self::compile_for_measurement`] (an evidenced build, no propose/confirm) and
    /// [`Self::realize_accuracy_proposer`] (a bare proposer, no propose/confirm either).
    fn evaluate_for_corpus(
        &self,
        candidate: &LoweredCandidate,
        ctx: &mut CorpusEvalContext<'_>,
    ) -> EvaluatedPlan;
}

/// `crate::emit`'s surface probe via `FomaProposer::new`: whole-grammar, derives its own topology and ignores the plan.
pub struct LexcMainline;

impl Backend for LexcMainline {
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TunedSurfaceProbed
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
    ) -> Result<FomaProposer, String> {
        crate::backend_runtime::realize_tuned_surface_proposer(grammar)
    }

    fn evaluate_for_corpus(
        &self,
        _candidate: &LoweredCandidate,
        ctx: &mut CorpusEvalContext<'_>,
    ) -> EvaluatedPlan {
        if ctx.observe {
            crate::backend_runtime::evaluate_via_tuned_emit_mode::<true>(
                ctx.grammar,
                ctx.words,
                ctx.expected,
                ctx.budget,
            )
        } else {
            crate::backend_runtime::evaluate_via_tuned_emit_mode::<false>(
                ctx.grammar,
                ctx.words,
                ctx.expected,
                ctx.budget,
            )
        }
    }
}

static LEXC_MAINLINE: LexcMainline = LexcMainline;

/// The closed adapter table, in `crate::strategy_coverage::ALL_STRATEGIES` declaration order.
pub static ALL_BACKENDS: [&'static dyn Backend; 1] = [&LEXC_MAINLINE];

/// The one backend realizing `strategy`, and the one remaining match on `EmissionStrategy` this seam allows -- every other caller goes through this function or `ALL_BACKENDS` rather than matching the strategy itself.
pub fn backend_for(strategy: EmissionStrategy) -> &'static dyn Backend {
    match strategy {
        EmissionStrategy::TunedSurfaceProbed => &LEXC_MAINLINE,
    }
}

#[cfg(test)]
mod tests;
