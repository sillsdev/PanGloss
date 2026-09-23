//! The SELECTOR: which backend(s) compile a given grammar, and — for each one that does not — the
//! named construct it declined on.
//!
//! # The gap this fills
//! `crate::capability::StrategyEnvelope` already holds every backend's own compatibility report,
//! and `crate::capability::StrategyEnvelope::global` joins them into one whole-grammar answer. The
//! join is the right shape for "is this grammar compilable AT ALL" and the wrong shape for every
//! caller that is about to run ONE backend: a non-refusing join can mean "some other backend can do
//! this", which is no licence for the backend actually in hand. Nothing in this workspace turned
//! the envelope into a choice, so callers reached for the join and inherited that ambiguity.
//!
//! # `BackendStatus`/`BackendReport`/`BackendSelection` live in `pg-health`
//! Those three types are plain data the pack format (`pg-pack`) and the Runtime also read, so they
//! moved to `pg_health::backend_selection` and are re-exported here at their historical path.
//! `BackendReport::refused` there builds only the data-only half of a refusal (its `HealthFinding`
//! and failed-predicate list); `refused` below is this crate's own free function completing it
//! with the embedded advice catalog's shapes and remedies — a constructor that needs the
//! capability envelope's advice catalog cannot be an inherent method on a type `pg-health` owns.
use std::collections::HashMap;

use pg_grammar::model::Grammar;

use crate::advice_catalog::builtin_catalog;
use crate::capability::{
    compose_envelope_across_strategies, compose_envelope_with_semantics,
    default_grammar_wide_checks, default_registry, CapabilityContributions, CapabilityDiagnostic,
    CompileDecision, StrategyEnvelope,
};
use crate::enumerate::{enumerate_default, EmissionStrategy};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::strategy_coverage::ALL_STRATEGIES;
pub use pg_health::backend_selection::{
    AdviceReference, BackendReport, BackendSelection, BackendStatus,
};

/// `PredicateId` -> `GrammarWideCheck::shape_key`, so a grammar-wide check's advice shape is a field it carries, not a second guess re-matched here.
fn grammar_wide_shape_keys() -> HashMap<crate::capability::PredicateId, &'static str> {
    crate::capability::default_grammar_wide_checks()
        .iter()
        .map(|check| (check.id(), check.shape_key()))
        .collect()
}

/// `PredicateId` -> `CapabilityPredicate::shape_key`, mirroring `grammar_wide_shape_keys` above for the per-plan-node predicate axis.
fn registered_predicate_shape_keys() -> HashMap<crate::capability::PredicateId, &'static str> {
    default_registry()
        .predicates()
        .iter()
        .map(|predicate| (predicate.id(), predicate.shape_key()))
        .collect()
}

/// Every `Refuse` diagnostic's predicate id must resolve to a declared shape key: a `GrammarWideCheck`'s own field, a registered `CapabilityPredicate`'s own field, or `strategy_floor`'s named constant (the one refusal with no registry entry to own a `shape_key` method, `capability.rs`'s own doc on that constant explains why). A closed registry has no unknown members, so any other id panics naming it rather than guessing.
fn capability_shape_key(diagnostic: &CapabilityDiagnostic) -> &'static str {
    if let Some(&key) = grammar_wide_shape_keys().get(diagnostic.predicate) {
        return key;
    }
    if diagnostic.predicate == crate::capability::STRATEGY_FLOOR_NOT_REPRESENTABLE_PREDICATE {
        return crate::capability::STRATEGY_FLOOR_NOT_REPRESENTABLE_SHAPE_KEY;
    }
    if let Some(&key) = registered_predicate_shape_keys().get(diagnostic.predicate) {
        return key;
    }
    panic!(
        "no GrammarWideCheck, registered CapabilityPredicate, or strategy-floor constant declares \
         a shape key for predicate id {:?} -- the advice-shape registry is closed, so an unknown \
         id here is a bug in whichever caller minted it, not a grammar to route around",
        diagnostic.predicate
    );
}

/// Test-support access to the exact shape-key resolution `refused` uses internally, so a gate can
/// pin the (predicate, shape key) table this module actually produces rather than a
/// reimplementation of the same logic that could silently drift from it.
#[cfg(feature = "test-support")]
pub fn capability_shape_key_for_test(diagnostic: &CapabilityDiagnostic) -> &'static str {
    capability_shape_key(diagnostic)
}

/// This crate's own completion of `BackendReport::refused`: the data-only refusal `pg-health`
/// builds, plus the embedded advice catalog's shapes and remedies for every declined diagnostic —
/// the half of `refused` that needs the capability registry and catalog `pg-health` cannot depend
/// on. The only production caller of `BackendReport::refused` should go through this function.
pub fn refused(strategy: EmissionStrategy, decision: CompileDecision) -> BackendReport {
    let report = BackendReport::refused(strategy, decision);
    let CompileDecision::Refuse(diagnostics) = report.decision() else {
        return report;
    };
    let catalog = builtin_catalog().expect("the embedded backend advice catalog must validate");
    let mut shapes = Vec::new();
    let mut advice_references = Vec::new();
    for diagnostic in diagnostics {
        let shape_key = capability_shape_key(diagnostic);
        let entry = catalog
            .entry_for(shape_key)
            .expect("every capability-refusal shape must exist in the advice catalog");
        shapes.push(entry.shape_key.clone());
        advice_references.extend(entry.remedies.iter().map(|remedy| {
            AdviceReference::new(
                entry.shape_key.clone(),
                remedy.remedy_key.clone(),
                remedy.effort,
            )
        }));
    }
    report.with_capability_advice(shapes, advice_references)
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
    let g = semantics.grammar();
    let phon = PhonologyProbe::new_with_semantics(semantics);
    let plan = enumerate_default(g, semantics.prules_in_order(), phon.as_ref());
    let registry = default_registry();
    compose_envelope_with_semantics(semantics, &plan, &registry)
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
