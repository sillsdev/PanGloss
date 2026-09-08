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
pub use pg_health::backend_selection::{AdviceReference, BackendReport, BackendSelection, BackendStatus};
use crate::capability::{
    compose_envelope_across_strategies, default_grammar_wide_checks, default_registry,
    CapabilityContributions, CapabilityDiagnostic, CompileDecision, StrategyEnvelope,
};
use crate::enumerate::{enumerate_default, EmissionStrategy};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::strategy_coverage::ALL_STRATEGIES;

/// `PredicateId` -> `GrammarWideCheck::shape_key`, so a grammar-wide check's advice shape is a field it carries, not a second guess re-matched here.
fn grammar_wide_shape_keys() -> HashMap<crate::capability::PredicateId, &'static str> {
    crate::capability::default_grammar_wide_checks()
        .iter()
        .map(|check| (check.id(), check.shape_key()))
        .collect()
}

fn capability_shape_key(diagnostic: &CapabilityDiagnostic) -> &'static str {
    if let Some(&key) = grammar_wide_shape_keys().get(diagnostic.predicate) {
        return key;
    }
    match diagnostic.predicate {
        "circumfix-output-action.faithful-structural-composite" => "late-structural-reachability",
        "reduplication.peel-eligible-rule-kind" => "nonregular-process-morphology",
        "compounding.non-recursive" | "quantifier.bounded-expansion" => "repeated-application",
        "unordered-application.chain-depth-bounded" => "unordered-interactions",
        "multi-table.faithful-table-threading"
        | "right-to-left-rewrite.faithful-reversal-construction"
        | "metathesis.faithful-swap-construction"
        | "simultaneous.subrule-overlap"
        | "epenthesis.structural-composite-route" => "wide-phonology",
        _ if diagnostic
            .construct
            .to_ascii_lowercase()
            .contains("truncat")
            || diagnostic.construct.to_ascii_lowercase().contains("delet") =>
        {
            "structural-deletion-or-truncation"
        }
        _ if diagnostic.construct.to_ascii_lowercase().contains("slot") => {
            "optional-slot-branching"
        }
        _ if diagnostic.construct.to_ascii_lowercase().contains("null")
            || diagnostic
                .construct
                .to_ascii_lowercase()
                .contains("zero-surface") =>
        {
            "null-cycle"
        }
        _ => "nonregular-process-morphology",
    }
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
mod tests {
    use super::*;

    const ADMIT_XML: &str = r#"<HermitCrabInput><Language><Name>BackendSelectionFixture</Name>
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

    /// Mirrors `tests/admission_single_owner_gate.rs`'s whole-fixture-set measurement as a unit test.
    #[test]
    fn every_all_strategies_member_is_reported() {
        let g = pg_grammar::load(ADMIT_XML).expect("fixture must load");
        let selection = select_backends_for_grammar(&g);
        for &strategy in ALL_STRATEGIES {
            assert!(
                selection.report_for(strategy).is_some(),
                "{strategy:?} has no report; decision_for's fail-closed arm just became live policy"
            );
        }
    }

    /// A composed report's own decision passes through unchanged.
    #[test]
    fn decision_for_returns_the_composed_reports_own_decision() {
        let g = pg_grammar::load(ADMIT_XML).expect("fixture must load");
        let selection = select_backends_for_grammar(&g);
        for &strategy in ALL_STRATEGIES {
            let expected = selection
                .report_for(strategy)
                .expect("pinned above: every strategy has a report")
                .decision()
                .clone();
            assert_eq!(selection.decision_for(strategy), expected);
        }
    }
}
