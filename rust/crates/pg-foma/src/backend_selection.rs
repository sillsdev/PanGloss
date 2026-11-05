//! Builds per-backend capability reports from one shared grammar-semantics analysis.

use pg_grammar::model::Grammar;

use crate::capability::{
    baseline_envelope_with_semantics, compose_envelope_across_strategies,
    default_grammar_wide_checks, default_registry, CapabilityContributions, CompileDecision,
    StrategyEnvelope,
};
use crate::enumerate::{enumerate_default, EmissionStrategy};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::strategy_coverage::ALL_STRATEGIES;
pub use pg_health::backend_selection::{
    AdviceReference, BackendReport, BackendSelection, BackendStatus,
};

/// Capability refusals use grammar-health advice; compiler-cost shapes have their own catalog.
#[cfg(feature = "test-support")]
pub fn capability_shape_key_for_test(
    diagnostic: &crate::capability::CapabilityDiagnostic,
) -> Option<&'static str> {
    use crate::capability::CapabilityRefusal;
    match diagnostic.refusal {
        CapabilityRefusal::AlphaEnvironment
        | CapabilityRefusal::CircumfixNoRoute { .. }
        | CapabilityRefusal::ReduplicationNoRoute { .. }
        | CapabilityRefusal::PatternRootShape { .. }
        | CapabilityRefusal::RootSpellingBudget { .. }
        | CapabilityRefusal::StrategyCoverage => None,
    }
}

pub fn refused(
    strategy: EmissionStrategy,
    decision: CompileDecision,
) -> Result<BackendReport, &'static str> {
    BackendReport::refused(strategy, decision)
}

/// One report per backend in [`crate::strategy_coverage::ALL_STRATEGIES`] declaration order.
fn build_backend_selection(envelope: &StrategyEnvelope) -> BackendSelection {
    let reports = ALL_STRATEGIES
        .iter()
        .map(|&strategy| {
            let Some(decision) = envelope.decision_for(strategy) else {
                return BackendReport::missing(strategy, "backend was not available");
            };
            if matches!(decision, CompileDecision::Refuse(_)) {
                refused(strategy, decision.clone())
                    .expect("refused backend selection requires a Refuse decision")
            } else {
                BackendReport::accepted(strategy, decision.clone(), Vec::new())
                    .expect("a non-refusing decision is always accepted")
            }
        })
        .collect();
    BackendSelection::from_reports(reports)
}

/// The ADVISORY-ONLY, best-of-every-backend `CompileDecision` -- the BEST verdict any compiler
/// offers, joined via `crate::capability::StrategyEnvelope::global`. A free function, not a method
/// on `pg_health::backend_selection::BackendSelection`: it composes its OWN envelope over
/// `crate::capability::baseline_grammar_wide_checks` rather than reading an existing selection's
/// reports (which are composed over the richer `default_grammar_wide_checks` a real per-backend
/// decision needs), and that composition needs this crate's capability registry -- machinery
/// `pg-health` does not and must not depend on. A non-`Refuse` here says nothing about the backend
/// a caller is about to run -- `BackendSelection::decision_for` is the entry point that decides.
pub fn best_case_across_backends(semantics: &GrammarSemantics<'_>) -> CompileDecision {
    best_case_envelope(semantics).global()
}

/// The per-compiler envelope `best_case_across_backends` joins, for a reporter that must name which compilers declined.
pub fn best_case_envelope(semantics: &GrammarSemantics<'_>) -> StrategyEnvelope {
    let g = semantics.grammar();
    let phon = PhonologyProbe::new_with_semantics(semantics);
    let plan = enumerate_default(g, semantics.prules_in_order(), phon.as_ref());
    let registry = default_registry();
    baseline_envelope_with_semantics(semantics, &plan, &registry)
}

/// [`best_case_across_backends`] over a bare `&Grammar`, deriving the semantics itself.
pub fn best_case_for_grammar(g: &Grammar) -> CompileDecision {
    best_case_across_backends(&GrammarSemantics::derive(g))
}

/// Selects over an already-derived `crate::grammar_semantics::GrammarSemantics` — the primary form,
/// since deriving one runs the whole `crate::capability::characterize` walk and a caller that
/// already holds a semantics should never pay for a second.
pub fn select_backends(semantics: &GrammarSemantics<'_>) -> BackendSelection {
    let g = semantics.grammar();
    let phon = PhonologyProbe::new_with_semantics(semantics);
    let plan = enumerate_default(g, semantics.prules_in_order(), phon.as_ref());
    let registry = default_registry();
    let grammar_wide = default_grammar_wide_checks();
    let contributions = CapabilityContributions::new(&registry, &grammar_wide);
    let envelope = compose_envelope_across_strategies(semantics, &plan, &contributions);
    build_backend_selection(&envelope)
}

/// `select_backends` from a bare `&Grammar`, deriving the semantics itself. **Check-only**: nothing
/// here builds a `foma::types::Fsm`, runs foma, or alters any compile path.
pub fn select_backends_for_grammar(g: &Grammar) -> BackendSelection {
    select_backends(&GrammarSemantics::derive(g))
}

#[cfg(test)]
mod tests;
