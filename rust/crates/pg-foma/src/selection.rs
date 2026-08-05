//! Capability-safe plan selection over [`crate::enumerate::enumerate_candidates`]'s candidate list,
//! plus a deterministic default selection objective.
//!
//! # The invariant this relies on
//! > The enumerator emits only plans all of whose nodes pass the characteristics-check envelope.
//! > Every capability-passing plan is recall-preserving,
//! > so all produce the identical confirmed set — selection can never pick a fast-but-wrong plan; it
//! > only trades cost. The default selection objective is deterministic and cheap: minimize a
//! > measure-or-estimate of `(states + arcs)` (controllable path) / payload size (black-box foma
//! > path), tie-broken by content-address for reproducibility.
//!
//! [`select_plan`] is exactly this, done twice, in this order:
//! 1. **Filter**: run [`crate::capability::compose_envelope_for_strategy`] over every candidate. A candidate
//!    whose [`crate::capability::CompileDecision`] is `Refuse` is excluded from being chosen —
//!    capability-safe BY CONSTRUCTION, never a runtime check bolted on after the
//!    fact. This is the ONLY filter: an `Admit` candidate and a `ConfirmOnly` candidate are equally
//!    admissible here (both are recall-preserving; the line is drawn at `Refuse`, not
//!    at `ConfirmOnly` — see [`crate::capability::CompileDecision`]'s own doc, "`ConfirmOnly`... is
//!    first-class, not a failure").
//!
//!    **The filter is STRATEGY-AWARE**, and this is the only place in the crate that can be.
//!    "Every capability-passing plan is recall-preserving" rests on
//!    [`crate::capability::Disposition::ConfirmOnly`]'s precondition — "recall-preserving only if
//!    the proposer proposes the superset" — which is a claim about a PROPOSER, not about a grammar
//!    or a plan. A bare `compose_envelope` has no proposer in hand and so was checking that
//!    precondition against the union of every compiler's abilities; a
//!    [`crate::enumerate::LoweredCandidate`] carries the [`crate::enumerate::EmissionStrategy`] that
//!    will actually realize it, so here (and only here) the account can be taken against the right
//!    one. See [`crate::strategy_coverage`] for the table and the whole-construct recall hole that
//!    survived undetected without it.
//! 2. **Rank**: among admissible candidates, build each one via [`crate::build::build_controllable`]
//!    (the only builder that exists today, so measured `(states + arcs)`
//!    from its net is what ranking uses where available) and pick the minimum `states + arcs`,
//!    tie-broken by the candidate's root [`crate::plan::NodeId`] (a content address, already a
//!    total, deterministic order via `NodeId`'s derived `Ord`).
//!
//! # What this module deliberately does NOT do
//! No projected-cost model with error bounds, no committed-plan cache, no profile-guided
//! autotuning, no payload-size measurement for the black-box foma (composite/structural-composite)
//! path: "enumerate, filter by capability, pick by
//! measured/estimated size, build" is the whole of what this module ships, nothing more.
//!
//! # A library capability, not a production compile path
//! [`select_plan`] is a library capability callers can invoke, not something `emit.rs`/`analyzer.rs`/
//! `composite.rs` calls today. Replacing the hardcoded `should_run`/
//! `probe_would_refuse`/`partition_entries` branching with a selected `Plan` stays a deliberately
//! separate, still-open question — this module's own existence does not imply that flip happened.
//!
//! # A candidate that fails to build is unmeasurable, not un-admissible
//! A [`crate::compose_budget::ComposeBudget`] cap can trip inside [`crate::build::
//! build_controllable`] independently of capability admissibility (a `ComposeError` is a resource
//! observation, not a recall-soundness one). Such a candidate stays in [`SelectionOutcome::
//! considered`] with `measure: None` and is never the MINIMUM-objective choice (there is no
//! objective value to compare), but it is not treated as inadmissible either — see [`select_plan`]'s
//! own fallback for the degenerate case where NO admissible candidate measures successfully.

use foma::options::FomaOptions;

use pg_grammar::model::{Grammar, PhonRuleDef};

use crate::build::build_controllable;
use crate::capability::{compose_envelope_for_strategy, CompileDecision, PredicateRegistry};
use crate::compose_budget::ComposeBudget;
use crate::enumerate::LoweredCandidate;
use crate::grammar_semantics::GrammarSemantics;
use crate::plan::NodeId;
use crate::replace::SegAlphabet;

/// The measured cost of one admissible candidate's built network (D3: "measured `(states + arcs)`
/// from `build_controllable`'s net where available"). `None` counterparts of this in
/// [`CandidateReport::measure`] cover the two cases where no measurement exists: the candidate was
/// `Refuse`d (never built at all — filtered before build), or `build_controllable` itself returned
/// `Err`/an empty net (module doc: unmeasurable, not un-admissible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanMeasure {
    pub states: i64,
    pub arcs: i64,
}

impl PlanMeasure {
    /// D3's objective: `states + arcs`.
    pub fn objective(&self) -> i64 {
        self.states + self.arcs
    }
}

/// One candidate's full provenance: which plan it was, what [`crate::capability::compose_envelope`]
/// decided, and (if admissible and buildable) its measured size — enough for a caller to explain
/// "why this plan, not that one" without re-running anything (deliverable 2's own requirement:
/// "return the choice plus enough provenance to explain it").
#[derive(Debug, Clone)]
pub struct CandidateReport {
    /// [`crate::enumerate::LoweredCandidate::label`], echoed back for readable reporting.
    pub label: &'static str,
    /// The candidate's root [`NodeId`] — D3's tie-break key, and a stable identity for this
    /// candidate independent of its position in the input list.
    pub root: NodeId,
    /// The full [`CompileDecision`] `compose_envelope` reached for this candidate (carries every
    /// [`crate::capability::CapabilityDiagnostic`] on a `Refuse`, not just a bool).
    pub decision: CompileDecision,
    /// `Some` iff this candidate was admissible (`decision` is not `Refuse`) AND
    /// `build_controllable` produced a real, non-empty net for it.
    pub measure: Option<PlanMeasure>,
}

impl CandidateReport {
    /// `true` iff [`Self::decision`] is not [`CompileDecision::Refuse`] — D3's admissibility
    /// predicate, named so callers don't have to match on `decision` themselves.
    pub fn is_admissible(&self) -> bool {
        !matches!(self.decision, CompileDecision::Refuse(_))
    }
}

/// The full result of one [`select_plan`] run: every candidate's provenance, plus which one (if any)
/// was chosen.
#[derive(Debug, Clone)]
pub struct SelectionOutcome {
    /// Every candidate considered, in the SAME order [`select_plan`] was given them.
    pub considered: Vec<CandidateReport>,
    /// The index into [`Self::considered`] (and, by construction, into the caller's own
    /// `candidates` slice) of the selected plan — `None` only if NO candidate was admissible at all
    /// (every one `Refuse`d).
    pub chosen: Option<usize>,
}

impl SelectionOutcome {
    /// The chosen candidate's report, if one was chosen.
    pub fn chosen_report(&self) -> Option<&CandidateReport> {
        self.chosen.map(|i| &self.considered[i])
    }
}

/// D3's selector: filter `candidates` to those [`crate::capability::compose_envelope`] does not `Refuse`, then pick the
/// minimum `states + arcs` among the ones [`build_controllable`] can actually measure, tie-broken by
/// root [`NodeId`] (module doc).
///
/// `g`/`registry`/`opts`/`alphabet`/`prules_in_order`/`budget` are the same grammar-derived and
/// build-configuration inputs [`crate::oracle::differential_oracle`] and [`build_controllable`]
/// themselves take — this function does not recompute or re-derive any of them, only threads them
/// through to `compose_envelope`/`build_controllable` for each candidate in turn (same trust
/// convention those functions already document for their own parameters).
///
/// # Panics
/// If `candidates` contains a [`crate::plan::Plan`] with no root set — a caller/plan-construction
/// contract violation ([`crate::enumerate::enumerate_candidates`] always sets a root), not a
/// judgment this function can make a decision about.
#[allow(clippy::too_many_arguments)] // mirrors build_controllable's/differential_oracle's own many
                                     // grammar-derived parameters, taken once per candidate here.
pub fn select_plan(
    candidates: &[LoweredCandidate],
    g: &Grammar,
    registry: &PredicateRegistry,
    opts: &FomaOptions,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    budget: &ComposeBudget,
) -> SelectionOutcome {
    // Derived once, outside the loop.
    // This function used to call `compose_envelope(g, ..)` per candidate, and each of those calls
    // re-ran the whole `capability::characterize` grammar walk -- real `Simultaneous`-mode
    // `foma::types::Fsm` construction included -- for a profile that cannot differ between
    // candidates, because it is a function of the GRAMMAR and candidates differ only in their PLAN.
    let semantics = GrammarSemantics::derive(g);
    let considered: Vec<CandidateReport> = candidates
        .iter()
        .map(|candidate| {
            let root = candidate.plan.root().unwrap_or_else(|| {
                panic!(
                    "select_plan: candidate {:?} has no root set",
                    candidate.label
                )
            });

            // STRATEGY-AWARE (this is the point a candidate becomes selectable, and the one place
            // that holds both the plan and the compiler that will realize it). `compose_envelope`
            // alone checks a `ConfirmOnly` disposition against the UNION of every compiler's
            // abilities; `LoweredCandidate::strategy` names the compiler actually in use, so the
            // per-strategy account (`crate::strategy_coverage`) is met in here. See
            // `compose_envelope_for_strategy`'s own doc for why this can only lower a decision.
            let decision = compose_envelope_for_strategy(
                &semantics,
                &candidate.plan,
                candidate.strategy(),
                registry,
            );

            // D3: only an admissible (non-Refuse) candidate is even worth building/measuring --
            // a Refused candidate is excluded from selection by construction, so spending a real
            // foma build on it would be wasted work, never consulted by the ranking below.
            let measure = if matches!(decision, CompileDecision::Refuse(_)) {
                None
            } else {
                build_controllable(&candidate.plan, opts, g, alphabet, prules_in_order, budget)
                    .ok()
                    .and_then(|built| {
                        built.net.as_ref().map(|net| PlanMeasure {
                            states: i64::from(net.statecount),
                            arcs: i64::from(net.arccount),
                        })
                    })
            };

            CandidateReport {
                label: candidate.label,
                root,
                decision,
                measure,
            }
        })
        .collect();

    let chosen = choose(&considered);

    SelectionOutcome { considered, chosen }
}

/// The pure ranking core (module doc, D3's objective + tie-break), factored out of [`select_plan`]
/// so it can be unit-tested directly against synthetic [`CandidateReport`]s without building any
/// real `Fsm` — same discipline `crate::oracle::resolve_verdict` uses for its own pure selection
/// core.
///
/// Primary rule: among admissible (`is_admissible`) candidates with a `Some` measure, pick the
/// minimum `(objective, root)` pair -- `Ord` on that tuple IS the objective-then-content-address
/// tie-break D3 asks for. Fallback: if NO admissible candidate measured successfully (module doc:
/// unmeasurable is not un-admissible), pick the minimum-`NodeId` admissible candidate instead of
/// reporting "nothing selected" when a valid, if unmeasured, choice exists.
fn choose(considered: &[CandidateReport]) -> Option<usize> {
    let measured = considered
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_admissible() && c.measure.is_some())
        .min_by_key(|(_, c)| {
            let m = c.measure.expect("filtered to Some above");
            (m.objective(), c.root)
        })
        .map(|(i, _)| i);

    measured.or_else(|| {
        considered
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_admissible())
            .min_by_key(|(_, c)| c.root)
            .map(|(i, _)| i)
    })
}

#[cfg(test)]
mod tests {
    //! Deliverable 3's four required properties, in order: (1) determinism across repeated/
    //! independently-built candidate lists; (2) a `Refuse`-ing candidate is excluded; (3) with ≥2
    //! admissible candidates the minimum-objective one wins, ties break by content-address; (4) the
    //! load-bearing invariant -- every admissible candidate AGREES via
    //! `crate::oracle::differential_oracle`, proving selection only ever trades cost.
    //!
    //! `choose`'s synthetic-report tests exercise the pure ranking core directly (no real `Fsm`);
    //! the `select_plan`-level tests run a real synthetic grammar end-to-end through
    //! `enumerate_candidates` -> `select_plan`, the shape a real caller would use.

    use pg_grammar::model::Grammar;

    use super::*;
    use crate::capability::{default_registry, CapabilityDiagnostic};
    use crate::enumerate::enumerate_candidates;
    use crate::junctions::PhonologyProbe;
    use crate::oracle::{differential_oracle, OracleResult};

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
        g.strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|&id| &g.prules[id.0 as usize])
            .collect()
    }

    /// One MPR-gated subrule and two entries realizing both truth values of that gate key --
    /// the same synthetic shape `enumerate.rs`/`build.rs`/`oracle.rs` each duplicate for their own
    /// test modules (see any of those for the same fixture-sharing rationale). Two real gate groups
    /// means [`enumerate_candidates`] yields both `"default"` and `"gate-group-permuted"`, exactly
    /// what a selection test over ≥2 candidates needs.
    fn gated_two_group_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SelectionGatedTwoGroupFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>gate1</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredMPRFeatures="mpr1">
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e1" partOfSpeech="posV" ruleFeatures="mpr1">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    }

    /// An ordinary, ungated affix+rewrite grammar (`capability_entry.rs`'s own `Admit` fixture,
    /// reused verbatim) -- `enumerate_candidates` yields exactly 1 candidate for it
    /// (`enumerate.rs`'s own test proves the single-group collapse), and `compose_envelope` must
    /// `Admit` it (no Compounding/Unordered/MPR-group/Simultaneous/etc. construct declared).
    fn ordinary_admit_fixture_xml() -> &'static str {
        r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>-a</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#
    }

    /// A single, non-recursive `Compounding` rule (`capability_entry.rs`'s own `ConfirmOnly`
    /// fixture, reused verbatim) -- `compose_envelope` must reach `ConfirmOnly`, not `Refuse`, so a
    /// grammar exercising this construct is still fully selectable (D3: `ConfirmOnly` is first-class
    /// admissible, not a failure).
    fn confirm_only_compounding_fixture_xml() -> &'static str {
        r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#
    }

    /// A grammar `compose_envelope` refuses, so a selector run over its candidates must find NONE
    /// admissible. Genuinely-overlapping simultaneous subrules are the construct: a construct
    /// refused only pending a proof (`compounding.recursive`) can be reclassified later.
    fn refuse_overwrite_mpr_group_fixture_xml() -> &'static str {
        include_str!("../../../../conformance-staging/edge-cases/simultaneous-subrule-genuine-overlap/grammar.xml")
    }

    fn report(
        label: &'static str,
        root: u64,
        decision: CompileDecision,
        measure: Option<(i64, i64)>,
    ) -> CandidateReport {
        CandidateReport {
            label,
            root: node_id_from_raw(root),
            decision,
            measure: measure.map(|(states, arcs)| PlanMeasure { states, arcs }),
        }
    }

    /// `NodeId` has no public constructor (by design -- D1: identity is always content-derived), so
    /// synthetic `choose`-only tests recover a real one the only sanctioned way: interning a `Leaf`
    /// whose `FragmentSpec::RewriteRule { rule }` carries a caller-chosen `PRuleId`, which is exactly
    /// as good a stand-in "distinct, ordered identity" as any other content for these tests' purposes
    /// (they only need several MUTUALLY DISTINCT, orderable `NodeId`s, never a specific hash value).
    fn node_id_from_raw(distinguishing_rule: u64) -> NodeId {
        use crate::plan::{FragmentSpec, Plan, PlanNodeKind, Provenance};
        use pg_grammar::model::PRuleId;

        let mut plan = Plan::new();
        plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::RewriteRule {
                rule: PRuleId(distinguishing_rule as u32),
            },
            provenance: Provenance::RewriteRule(PRuleId(distinguishing_rule as u32)),
        })
    }

    // -----------------------------------------------------------------------------------------
    // `choose` (pure ranking core): synthetic-report tests.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn choose_excludes_a_refused_candidate() {
        let refused = report(
            "refused",
            1,
            CompileDecision::Refuse(vec![CapabilityDiagnostic {
                predicate: "test.always-refuse",
                construct: "Test".to_string(),
                witness: "synthetic".to_string(),
            }]),
            Some((1, 1)), // even a tiny, cheap measure must not win -- Refuse always dominates
        );
        let admitted = report("admitted", 2, CompileDecision::Admit, Some((100, 100)));

        let considered = vec![refused, admitted];
        let chosen = choose(&considered);
        assert_eq!(
            chosen,
            Some(1),
            "the Refused candidate must never be chosen, regardless of its (irrelevant) measure"
        );
    }

    #[test]
    fn choose_picks_minimum_objective_among_admissible_candidates() {
        let big = report("big", 1, CompileDecision::Admit, Some((100, 50)));
        let small = report("small", 2, CompileDecision::ConfirmOnly, Some((10, 5)));
        let considered = vec![big, small];

        let chosen = choose(&considered).expect("an admissible candidate exists");
        assert_eq!(
            considered[chosen].label, "small",
            "the minimum states+arcs objective must win, regardless of Admit vs. ConfirmOnly \
             (D3: both are equally admissible)"
        );
    }

    /// `NodeId` is a content HASH (D1) -- unrelated monotonically to whatever raw seed
    /// [`node_id_from_raw`] was given -- so this test cannot simply declare "the candidate built
    /// from the smaller seed wins"; it must compute the two real `NodeId`s first, determine which
    /// one is actually smaller, and assert THAT candidate wins. What is being pinned is the
    /// tie-break's determinism/correctness (min-by-root), never a specific hash value.
    #[test]
    fn choose_breaks_equal_objective_ties_by_smallest_root_node_id() {
        let root_a = node_id_from_raw(5);
        let root_b = node_id_from_raw(3);
        assert_ne!(
            root_a, root_b,
            "test fixture must pick two distinct NodeIds"
        );

        let a = CandidateReport {
            label: "a",
            root: root_a,
            decision: CompileDecision::Admit,
            measure: Some(PlanMeasure {
                states: 10,
                arcs: 10,
            }),
        };
        let b = CandidateReport {
            label: "b",
            root: root_b,
            decision: CompileDecision::Admit,
            measure: Some(PlanMeasure {
                states: 10,
                arcs: 10,
            }), // same objective (20) as `a`
        };
        let expected_winner = if root_a < root_b { "a" } else { "b" };
        let considered = vec![a, b];

        let chosen = choose(&considered).expect("an admissible candidate exists");
        assert_eq!(
            considered[chosen].label, expected_winner,
            "equal-objective candidates must tie-break by the smaller root NodeId, deterministically"
        );
    }

    #[test]
    fn choose_falls_back_to_smallest_root_when_no_admissible_candidate_measured() {
        let root_a = node_id_from_raw(7);
        let root_b = node_id_from_raw(4);
        assert_ne!(
            root_a, root_b,
            "test fixture must pick two distinct NodeIds"
        );

        let unmeasured_a = CandidateReport {
            label: "a",
            root: root_a,
            decision: CompileDecision::Admit,
            measure: None,
        };
        let unmeasured_b = CandidateReport {
            label: "b",
            root: root_b,
            decision: CompileDecision::ConfirmOnly,
            measure: None,
        };
        let refused = report("refused", 1, CompileDecision::Refuse(vec![]), Some((1, 1)));
        let expected_winner = if root_a < root_b { "a" } else { "b" };
        let considered = vec![unmeasured_a, unmeasured_b, refused];

        let chosen =
            choose(&considered).expect("2 admissible candidates exist, even if unmeasured");
        assert_eq!(
            considered[chosen].label, expected_winner,
            "when no admissible candidate measured, fall back to the smallest-root admissible one, \
             never silently 'nothing selected' when a valid choice exists"
        );
    }

    #[test]
    fn choose_returns_none_when_every_candidate_is_refused() {
        let a = report("a", 1, CompileDecision::Refuse(vec![]), Some((1, 1)));
        let b = report("b", 2, CompileDecision::Refuse(vec![]), None);
        let considered = vec![a, b];

        assert_eq!(choose(&considered), None);
    }

    // -----------------------------------------------------------------------------------------
    // `select_plan` end-to-end: real synthetic grammars through enumerate_candidates.
    // -----------------------------------------------------------------------------------------

    /// Determinism (deliverable 3, bullet 1): running `select_plan` twice over two INDEPENDENTLY
    /// built candidate lists (two separate `enumerate_candidates` calls, not the same `Vec` reused)
    /// must choose the same candidate label and root NodeId both times.
    #[test]
    fn select_plan_is_deterministic_across_independently_built_candidate_lists() {
        let g = load(gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let registry = default_registry();

        let candidates_1 = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
        let candidates_2 = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
        assert_eq!(candidates_1.len(), 2, "fixture must yield 2 candidates");

        let outcome_1 = select_plan(&candidates_1, &g, &registry, &opts, &alphabet, &ro, &budget);
        let outcome_2 = select_plan(&candidates_2, &g, &registry, &opts, &alphabet, &ro, &budget);

        let chosen_1 = outcome_1
            .chosen_report()
            .expect("a candidate must be chosen");
        let chosen_2 = outcome_2
            .chosen_report()
            .expect("a candidate must be chosen");
        assert_eq!(chosen_1.label, chosen_2.label);
        assert_eq!(chosen_1.root, chosen_2.root);
    }

    /// Deliverable 3, bullet 2: a grammar whose `compose_envelope` verdict is `Refuse` (recursive
    /// Compounding) must select NOTHING -- every candidate excluded.
    #[test]
    fn select_plan_excludes_a_refusing_grammars_only_candidate() {
        let g = load(refuse_overwrite_mpr_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let registry = default_registry();

        let candidates = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
        let outcome = select_plan(&candidates, &g, &registry, &opts, &alphabet, &ro, &budget);

        assert_eq!(outcome.considered.len(), candidates.len());
        assert!(
            outcome.considered.iter().all(|c| !c.is_admissible()),
            "every candidate for a Refuse-verdict grammar must be inadmissible"
        );
        assert_eq!(
            outcome.chosen, None,
            "nothing may be selected when every candidate is Refused"
        );
        match &outcome.considered[0].decision {
            CompileDecision::Refuse(diags) => {
                assert!(diags
                    .iter()
                    .any(|d| d.predicate == "simultaneous.subrule-overlap"))
            }
            other => panic!("expected Refuse, got {other:?}"),
        }
    }

    /// Deliverable 3, bullet 3: with ≥2 admissible candidates (the gated-two-group fixture, whose
    /// grammar has no Refuse-triggering construct so BOTH `"default"` and `"gate-group-permuted"`
    /// are admissible), the minimum-`states+arcs` one is chosen; ties break by content-address --
    /// exercised here by the concrete fact that gate-group order cannot change compiled size at all
    /// (union is commutative, `crate::oracle`'s own module doc), so this fixture's two candidates
    /// are a genuine, real-world equal-objective tie, and the tie-break must be exactly the smaller
    /// root `NodeId`.
    #[test]
    fn select_plan_chooses_minimum_objective_tie_broken_by_content_address() {
        let g = load(gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let registry = default_registry();

        let candidates = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
        assert_eq!(candidates.len(), 2);

        let outcome = select_plan(&candidates, &g, &registry, &opts, &alphabet, &ro, &budget);
        assert!(
            outcome.considered.iter().all(|c| c.is_admissible()),
            "this fixture declares no Refuse-triggering construct -- both candidates must be \
             admissible: {:?}",
            outcome.considered
        );
        assert!(
            outcome.considered.iter().all(|c| c.measure.is_some()),
            "both candidates must build successfully on this small fixture: {:?}",
            outcome.considered
        );

        // Reordering gate groups cannot change compiled size (union commutative + final minimize,
        // crate::oracle's own module doc) -- so this is a REAL equal-objective tie, not a contrived
        // one, and the winner must be exactly the smaller root NodeId.
        let objectives: Vec<i64> = outcome
            .considered
            .iter()
            .map(|c| c.measure.unwrap().objective())
            .collect();
        assert_eq!(
            objectives[0], objectives[1],
            "gate-group reordering must not change measured size"
        );

        let expected_winner = if outcome.considered[0].root < outcome.considered[1].root {
            0
        } else {
            1
        };
        assert_eq!(
            outcome.chosen,
            Some(expected_winner),
            "an equal-objective tie must resolve to the smaller root NodeId, deterministically"
        );
    }

    /// The load-bearing invariant: EVERY capability-passing plan is recall-preserving, so all
    /// produce the identical confirmed set. For every grammar this module's own fixtures exercise,
    /// every pair of ADMISSIBLE candidates
    /// [`select_plan`] considered must AGREE under [`differential_oracle`] -- proving selection
    /// among them can only ever trade cost, never correctness. Run over the two grammars whose
    /// candidate sets actually contain ≥2 admissible plans (the ordinary-Admit fixture collapses to
    /// 1 candidate, so it is included too as a trivial/vacuous check of the same property).
    #[test]
    fn every_admissible_candidate_pair_agrees_under_the_differential_oracle() {
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();
        let registry = default_registry();

        for (xml, words) in [
            (gated_two_group_fixture_xml(), &["p", "q"][..]),
            (ordinary_admit_fixture_xml(), &["b", "ba"][..]),
            (confirm_only_compounding_fixture_xml(), &["a", "aa"][..]),
        ] {
            let g = load(xml);
            let alphabet = SegAlphabet::new(&g.char_tables[0]);
            let ro = prules_in_order(&g);
            let phon = PhonologyProbe::new(&g);

            let candidates = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
            let outcome = select_plan(&candidates, &g, &registry, &opts, &alphabet, &ro, &budget);

            let admissible_idxs: Vec<usize> = outcome
                .considered
                .iter()
                .enumerate()
                .filter(|(_, c)| c.is_admissible())
                .map(|(i, _)| i)
                .collect();
            assert!(
                !admissible_idxs.is_empty(),
                "every fixture in this table is expected to have >=1 admissible candidate: {xml}"
            );

            for i in 0..admissible_idxs.len() {
                for j in (i + 1)..admissible_idxs.len() {
                    let (idx_a, idx_b) = (admissible_idxs[i], admissible_idxs[j]);
                    let result = differential_oracle(
                        &candidates[idx_a].plan,
                        &candidates[idx_b].plan,
                        (candidates[idx_a].label, candidates[idx_b].label),
                        &opts,
                        &g,
                        &alphabet,
                        &ro,
                        &budget,
                        words,
                    )
                    .expect("both admissible candidates must build successfully");

                    match result {
                        OracleResult::Agree => {}
                        OracleResult::Disagree { word, only_in_a, only_in_b, .. } => panic!(
                            "two admissible (non-Refuse) candidates ({} vs {}) for grammar {xml} \
                             disagreed at {word:?} (only_in_a={only_in_a:?}, only_in_b={only_in_b:?}) \
                             -- this would falsify D3's recall-preserving-selection claim",
                            candidates[idx_a].label, candidates[idx_b].label
                        ),
                    }
                }
            }
        }
    }
}
