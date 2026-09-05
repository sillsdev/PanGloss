//! The single post-compile decision that keeps a partial-bearing grammar's FST out of production.
//!
//! # What this is and is not
//! Partial morphemes are valid HermitCrab semantics: loading them and analysing with the full
//! engine must keep working, and this module never touches that path. What it decides is
//! *production readiness* of a COMPLETED FST — [`crate::health::Severity::NotProductionReady`] with
//! [`crate::health::FindingClass::Readiness`]. It is deliberately none of the other three answers:
//! not [`crate::health::Severity::CannotRepresent`] (the grammar is representable), not a
//! capability refusal (no backend is being denied the right to run), and not a containment event
//! (no monitor fired). Read `CONTEXT.md`'s three axes before reclassifying anything here.
//!
//! # Post-compile, and one owner
//! `assess_completed_fst` starts nothing, suppresses nothing, and consults no selector. A
//! contained measurement attempt may compile a complete FST from a partial-bearing grammar so
//! PanGloss can measure it; what this module denies is publishing, serializing, selecting or
//! reconstructing that result as a trusted artifact. Every strategy in
//! [`crate::strategy_coverage::ALL_STRATEGIES`] asks THIS function rather than re-deriving
//! partiality from its own view of the grammar, and the partiality fact itself is owned upstream by
//! `pg_grammar::model::Grammar::partial_morpheme_facts`.

use pg_grammar::model::{Grammar, PartialMorphemeFacts};
use pg_grammar::GrammarError;

use crate::enumerate::EmissionStrategy;
use crate::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Remedy, Severity,
    ValueProvenance,
};

/// How many authored identities a diagnostic names before it elides the rest.
const SAMPLED_AUTHORED_IDS: usize = 8;

/// One strategy's production verdict for one completed FST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FstProductionAdmission {
    strategy: EmissionStrategy,
    health: HealthReport,
}

impl FstProductionAdmission {
    /// Which compiler's completed result this verdict is about.
    pub fn strategy(&self) -> EmissionStrategy {
        self.strategy
    }

    pub fn health(&self) -> &HealthReport {
        &self.health
    }

    /// The publication gate: true from [`Severity::NotProductionReady`] upward. The severity names
    /// the blocking tier and the finding's `FindingClass` names which question it answers, so
    /// callers gate on this rather than comparing severities themselves.
    pub fn blocks_publication(&self) -> bool {
        health_blocks_publication(&self.health)
    }
}

/// The publication gate over ANY health report, so a caller holding only a merged
/// [`HealthReport`] shares this comparison instead of re-deriving it against
/// [`Severity::NotProductionReady`] by hand.
///
/// [`crate::backend_runtime::RuntimeEvaluation::production_blocks_publication`] is the other caller;
/// keeping both on this function is what stops the artifact boundary and the selection boundary
/// from drifting to different thresholds.
pub fn health_blocks_publication(health: &HealthReport) -> bool {
    health.admission() >= Severity::NotProductionReady
}

/// The production-admission decision for one completed FST built from `grammar` by `strategy`.
///
/// Reads only `Grammar::partial_morpheme_facts`, so an invalid grammar surfaces as a
/// [`GrammarError`] rather than as a quiet admission.
pub fn assess_completed_fst(
    grammar: &Grammar,
    strategy: EmissionStrategy,
) -> Result<FstProductionAdmission, GrammarError> {
    let facts = grammar.partial_morpheme_facts()?;
    let findings = if facts.has_partials() {
        vec![partial_morpheme_finding(&facts)]
    } else {
        Vec::new()
    };
    Ok(FstProductionAdmission {
        strategy,
        health: HealthReport::new(findings),
    })
}

fn partial_morpheme_finding(facts: &PartialMorphemeFacts) -> HealthFinding {
    HealthFinding::new(
        FindingCode::PartialMorphemeProductionPolicy,
        Severity::NotProductionReady,
        Phase::Compile,
        Metric::PartialMorphemeCount,
        MetricValue::Count(facts.total_count() as u64),
        ValueProvenance::Observed,
        partial_morpheme_explanation(facts),
    )
    .affecting(
        facts
            .authored_ids()
            .take(SAMPLED_AUTHORED_IDS)
            .map(str::to_string)
            .collect(),
    )
    .with_remedies(vec![Remedy {
        rank: 1,
        description: "Analyse this grammar with the full engine; a completed FST built from it may \
                      be measured but never published."
            .to_string(),
        requires_linguistic_equivalence: false,
        caveat: None,
    }])
}

/// Counts by kind plus a bounded sample, so the diagnostic names morphemes without growing with the lexicon.
fn partial_morpheme_explanation(facts: &PartialMorphemeFacts) -> String {
    let sample: Vec<&str> = facts.authored_ids().take(SAMPLED_AUTHORED_IDS).collect();
    let elided = facts.total_count().saturating_sub(sample.len());
    let mut explanation = format!(
        "This grammar declares {} partial morpheme(s) ({} lexical entry/entries, {} affix-process \
         rule(s)), so a completed FST built from it is not eligible for production publication. \
         Affected: {}",
        facts.total_count(),
        facts.partial_entry_count(),
        facts.partial_rule_count(),
        sample.join(", "),
    );
    if elided != 0 {
        explanation.push_str(&format!(" (+{elided} more)"));
    }
    explanation.push('.');
    explanation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::FindingClass;

    /// Minimal, delanguaged grammar: one char table, one entry, no rules.
    const NO_PARTIAL_XML: &str = r#"<HermitCrabInput><Language><Name>NoPartialFixture</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t1">
          <Name>S</Name>
          <LexicalEntries>
            <LexicalEntry id="entry-plain">
              <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

    #[test]
    fn a_grammar_with_no_partials_is_production_admissible_for_every_strategy() {
        let grammar = pg_grammar::load(NO_PARTIAL_XML).expect("fixture must load");
        for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
            let admission =
                assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
            assert_eq!(admission.strategy(), strategy);
            assert_eq!(admission.health().admission(), Severity::WithinLimits);
            assert!(!admission.blocks_publication());
            assert!(admission.health().findings.is_empty());
        }
    }

    #[test]
    fn a_partial_entry_blocks_publication_as_readiness_only() {
        let mut grammar = pg_grammar::load(NO_PARTIAL_XML).expect("fixture must load");
        grammar.entries[0].partial = true;
        for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
            let admission =
                assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
            assert_eq!(admission.health().admission(), Severity::NotProductionReady);
            let by_class = admission.health().admission_by_class();
            assert_eq!(by_class.readiness, Severity::NotProductionReady);
            assert_eq!(by_class.representability, Severity::WithinLimits);
            assert_eq!(by_class.containment, Severity::WithinLimits);
            assert_eq!(by_class.process, Severity::WithinLimits);
            assert!(admission.blocks_publication());
            let finding = &admission.health().findings[0];
            assert_eq!(finding.class(), FindingClass::Readiness);
            assert_eq!(finding.metric, Metric::PartialMorphemeCount);
            assert_eq!(finding.value, MetricValue::Count(1));
            assert_eq!(finding.provenance, ValueProvenance::Observed);
            assert_eq!(finding.affected, vec!["entry-plain".to_string()]);
        }
    }

    /// The diagnostic must separate the two kinds; one count cannot stand in for the other.
    #[test]
    fn the_diagnostic_names_both_kinds_separately() {
        let mut grammar = pg_grammar::load(NO_PARTIAL_XML).expect("fixture must load");
        grammar.entries[0].partial = true;
        let admission = assess_completed_fst(&grammar, EmissionStrategy::TunedSurfaceProbed)
            .expect("valid grammar facts");
        let explanation = &admission.health().findings[0].explanation;
        assert!(
            explanation.contains("1 lexical entry/entries")
                && explanation.contains("0 affix-process rule(s)"),
            "explanation must count both kinds: {explanation}"
        );
    }
}
