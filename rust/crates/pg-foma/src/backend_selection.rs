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
//! # Correctness selects; cost does not
//! A backend is selected iff its own report is not `crate::capability::CompileDecision::Refuse` —
//! the binary correctness axis, and the only axis consulted here. `Admit` and `ConfirmOnly` are
//! both selected: `ConfirmOnly` is a recall-preserving mode, not a defect, and demoting it would
//! quietly turn a graded property into a rejection.
//!
//! No cost model is consulted at all. This layer computes no size, no build time and no growth
//! rate, so cost cannot exclude a backend here even by accident — which is exactly the property
//! docs/adr/0001-honest-capability-boundary.md asks for.
//!
//! # The one policy choice, stated plainly
//! When several backends are viable, `BackendSelection::preferred` returns the first in
//! `BACKEND_PREFERENCE` order. That order is a fixed, hand-written list, not a measurement: this
//! module has no cost data to rank with, and inventing one would be inventing selection policy.
//! A caller that wants a different order reads `BackendSelection::selected` and ranks it itself.

use pg_grammar::model::Grammar;

use crate::capability::{
    compose_envelope_across_strategies, default_registry, CapabilityDiagnostic, CompileDecision,
    StrategyEnvelope,
};
use crate::emit::surface_table;
use crate::enumerate::{enumerate_default, EmissionStrategy};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::replace::SegAlphabet;

/// The order `BackendSelection::preferred` breaks a tie between viable backends in.
///
/// `crate::enumerate::EmissionStrategy::TunedSurfaceProbed` leads because it is the backend this
/// crate's shipping analyzer realizes (`crate::analyzer::FomaProposer::EMISSION_STRATEGY`), so
/// "the preferred backend" and "the backend a `pangloss` invocation actually runs" name the same
/// thing unless a caller deliberately says otherwise. The remaining two are ordered whole-grammar
/// first, since `EmissionStrategy::is_whole_grammar` is the difference between compiling the
/// grammar and compiling its controllable subtree.
///
/// This is a policy constant, not a derived fact: see this module's own doc.
pub const BACKEND_PREFERENCE: &[EmissionStrategy] = &[
    EmissionStrategy::TunedSurfaceProbed,
    EmissionStrategy::TemplatedUnderlyingTokens,
    EmissionStrategy::PlanComposed,
];

/// One backend's place in the selection: its own compatibility report, plus whether that report
/// admits it as a path for this grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendReport {
    strategy: EmissionStrategy,
    decision: CompileDecision,
}

impl BackendReport {
    /// Which backend this report is about.
    pub fn strategy(&self) -> EmissionStrategy {
        self.strategy
    }

    /// The backend's own `crate::capability::CompileDecision` — kept whole, so a caller can tell an
    /// `Admit` path from a `ConfirmOnly` one rather than only "selected or not".
    pub fn decision(&self) -> &CompileDecision {
        &self.decision
    }

    /// Whether this backend is a path for the grammar: true for `Admit` and `ConfirmOnly`, false
    /// only for a refusal. Pinned by `a_refusing_backend_is_never_selected`.
    pub fn is_selected(&self) -> bool {
        !matches!(self.decision, CompileDecision::Refuse(_))
    }

    /// Why this backend was not selected: the diagnostics naming the construct it declined on, or
    /// an empty slice when it WAS selected. Non-empty exactly when `is_selected` is false, so the
    /// reason for an exclusion is never absent from a report that has one.
    pub fn declined_on(&self) -> &[CapabilityDiagnostic] {
        match &self.decision {
            CompileDecision::Refuse(diagnostics) => diagnostics,
            CompileDecision::Admit | CompileDecision::ConfirmOnly => &[],
        }
    }
}

/// The selector's answer for one grammar: every backend's report, in `BACKEND_PREFERENCE` order.
///
/// No path, one path and several paths are all ordinary states of this type — `selected` returns
/// an empty, one-element or many-element list respectively, and `reports` still carries every
/// declining backend's named construct in the empty case, which is the case a caller most needs to
/// explain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendSelection {
    reports: Vec<BackendReport>,
}

impl BackendSelection {
    /// Reads an already-composed `crate::capability::StrategyEnvelope` rather than composing one,
    /// so a caller that already holds the envelope pays for no second plan walk.
    ///
    /// A backend named by `BACKEND_PREFERENCE` but absent from `envelope` is simply omitted; the
    /// envelope, not this list, decides which backends were composed at all.
    pub fn from_envelope(envelope: &StrategyEnvelope) -> Self {
        let reports = BACKEND_PREFERENCE
            .iter()
            .filter_map(|&strategy| {
                envelope
                    .decision_for(strategy)
                    .map(|decision| BackendReport {
                        strategy,
                        decision: decision.clone(),
                    })
            })
            .collect();
        Self { reports }
    }

    /// Every backend's report, in `BACKEND_PREFERENCE` order — selected and excluded alike, since
    /// "why not that one" is answerable only from the excluded ones.
    pub fn reports(&self) -> &[BackendReport] {
        &self.reports
    }

    /// One named backend's report, or `None` if it was not composed.
    pub fn report_for(&self, strategy: EmissionStrategy) -> Option<&BackendReport> {
        self.reports.iter().find(|r| r.strategy == strategy)
    }

    /// The backends that can compile this grammar, in `BACKEND_PREFERENCE` order. Empty is the
    /// "no path" answer.
    pub fn selected(&self) -> Vec<EmissionStrategy> {
        self.reports
            .iter()
            .filter(|r| r.is_selected())
            .map(|r| r.strategy)
            .collect()
    }

    /// The single backend a caller should run when it can only run one: the first selected in
    /// `BACKEND_PREFERENCE` order, or `None` when no backend can compile the grammar.
    pub fn preferred(&self) -> Option<EmissionStrategy> {
        self.reports
            .iter()
            .find(|r| r.is_selected())
            .map(|r| r.strategy)
    }

    /// Every backend that declined, with the constructs it declined on — the per-backend
    /// attribution a single whole-grammar verdict cannot carry.
    pub fn excluded(&self) -> Vec<(EmissionStrategy, &[CapabilityDiagnostic])> {
        self.reports
            .iter()
            .filter(|r| !r.is_selected())
            .map(|r| (r.strategy, r.declined_on()))
            .collect()
    }

    /// Whether no backend at all can compile this grammar. Distinct from an empty `excluded`, which
    /// says the opposite. Pinned by `no_path_is_representable_and_carries_every_reason`.
    pub fn is_no_path(&self) -> bool {
        !self.reports.is_empty() && self.reports.iter().all(|r| !r.is_selected())
    }
}

/// Selects over an already-derived `crate::grammar_semantics::GrammarSemantics` — the primary form,
/// since deriving one runs the whole `crate::capability::characterize` walk and a caller that
/// already holds a semantics should never pay for a second.
pub fn select_backends(semantics: &GrammarSemantics<'_>) -> BackendSelection {
    let g = semantics.grammar();
    let alphabet = SegAlphabet::new(surface_table(g));
    let phon = PhonologyProbe::new_with_semantics(semantics);
    let plan = enumerate_default(g, &alphabet, semantics.prules_in_order(), phon.as_ref());
    let envelope = compose_envelope_across_strategies(semantics, &plan, &default_registry());
    BackendSelection::from_envelope(&envelope)
}

/// `select_backends` from a bare `&Grammar`, deriving the semantics itself. **Check-only**: nothing
/// here builds a `foma::types::Fsm`, runs foma, or alters any compile path.
pub fn select_backends_for_grammar(g: &Grammar) -> BackendSelection {
    select_backends(&GrammarSemantics::derive(g))
}

#[cfg(test)]
mod tests {
    //! Synthetic, delanguaged fixtures only, in this crate's established test-module style: XML
    //! through `pg_grammar::load` rather than a hand-built `Grammar`.

    use super::*;
    use crate::capability::CapabilityDiagnostic;

    fn diagnostic(construct: &str) -> CapabilityDiagnostic {
        CapabilityDiagnostic {
            predicate: "synthetic.test-only",
            construct: construct.to_string(),
            witness: "synthetic".to_string(),
        }
    }

    fn envelope_of(rows: &[(EmissionStrategy, CompileDecision)]) -> BackendSelection {
        BackendSelection {
            reports: BACKEND_PREFERENCE
                .iter()
                .filter_map(|&strategy| {
                    rows.iter()
                        .find(|(s, _)| *s == strategy)
                        .map(|(_, decision)| BackendReport {
                            strategy,
                            decision: decision.clone(),
                        })
                })
                .collect(),
        }
    }

    /// A refusing backend is excluded and carries its diagnostics; a `ConfirmOnly` one is selected, since `ConfirmOnly` is recall-preserving rather than a defect.
    #[test]
    fn a_refusing_backend_is_never_selected() {
        let selection = envelope_of(&[
            (
                EmissionStrategy::TunedSurfaceProbed,
                CompileDecision::Refuse(vec![diagnostic("stratum 0 (Unordered)")]),
            ),
            (
                EmissionStrategy::TemplatedUnderlyingTokens,
                CompileDecision::ConfirmOnly,
            ),
            (EmissionStrategy::PlanComposed, CompileDecision::Admit),
        ]);

        assert_eq!(
            selection.selected(),
            vec![
                EmissionStrategy::TemplatedUnderlyingTokens,
                EmissionStrategy::PlanComposed
            ]
        );
        assert_eq!(
            selection.preferred(),
            Some(EmissionStrategy::TemplatedUnderlyingTokens),
            "preference order decides among viable backends, and the refused one is not viable"
        );
        let excluded = selection.excluded();
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].0, EmissionStrategy::TunedSurfaceProbed);
        assert_eq!(excluded[0].1[0].construct, "stratum 0 (Unordered)");
        assert!(!selection.is_no_path());
    }

    /// Every backend refusing is a first-class answer, and every backend's own reason survives into it.
    #[test]
    fn no_path_is_representable_and_carries_every_reason() {
        let selection = envelope_of(&[
            (
                EmissionStrategy::TunedSurfaceProbed,
                CompileDecision::Refuse(vec![diagnostic("tuned construct")]),
            ),
            (
                EmissionStrategy::TemplatedUnderlyingTokens,
                CompileDecision::Refuse(vec![diagnostic("templated construct")]),
            ),
            (
                EmissionStrategy::PlanComposed,
                CompileDecision::Refuse(vec![diagnostic("plan construct")]),
            ),
        ]);

        assert!(selection.is_no_path());
        assert!(selection.selected().is_empty());
        assert_eq!(selection.preferred(), None);
        let constructs: Vec<&str> = selection
            .excluded()
            .iter()
            .map(|(_, diags)| diags[0].construct.as_str())
            .collect();
        assert_eq!(
            constructs,
            vec!["tuned construct", "templated construct", "plan construct"],
            "no path must still name what each backend declined on"
        );
    }

    /// The selector reads the SAME per-backend verdicts the envelope holds, so an envelope whose backends disagree is not collapsed to its join.
    #[test]
    fn the_selection_is_per_backend_not_the_envelope_join() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SelectorFixture</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <RealizationalRule id="rr1">
                  <Name>Realiz</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </RealizationalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = pg_grammar::load(XML).expect("fixture must load");
        let selection = select_backends_for_grammar(&g);

        let plan_composed = selection
            .report_for(EmissionStrategy::PlanComposed)
            .expect("every backend must be reported");
        assert!(
            !plan_composed.is_selected(),
            "the plan-composed backend has no lexicon emitter for a realizational rule, so it must \
             be excluded here: {:?}",
            plan_composed.decision()
        );
        assert!(
            selection
                .report_for(EmissionStrategy::TunedSurfaceProbed)
                .expect("every backend must be reported")
                .is_selected(),
            "the same grammar must stay a path for the backend that can represent it"
        );
        assert_eq!(
            selection.preferred(),
            Some(EmissionStrategy::TunedSurfaceProbed)
        );
    }
}
