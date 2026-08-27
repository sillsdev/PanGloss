//! Shadow filtering in front of the real confirmer: that it changes no analysis, that it catches a pass which would have killed a confirmed one, and what confirmation actually spent on the candidates it would have removed.

#[path = "common/filter_fixture.rs"]
mod fixture;

use std::sync::Arc;

use pg_foma::candidate_filter::decision::{
    IdentityDefect, PassDecision, ProofClaim, ProofWitness, RejectionProof, StablePassId,
    StableRuleId,
};
use pg_foma::candidate_filter::index::FilterIndex;
use pg_foma::candidate_filter::legacy::witnesses_for;
use pg_foma::candidate_filter::model::{CandidateWitness, TraceFact};
use pg_foma::candidate_filter::pipeline::{FilterContext, FilterMode};
use pg_foma::candidate_filter::shadow::CandidateFilterSettings;
use pg_foma::candidate_filter::test_support::filter_of;
use pg_foma::candidate_filter::{CandidateFilterPass, OwnershipPass, StructuralTransitionPass};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::confirm::{build_morpheme_owners, confirm_batch_attributed};
use pg_foma::tags::Candidate;
use pg_grammar::model::{AllomorphId, Grammar, MorphemeId};
use pg_parse::Morpher;

const GRAMMAR_REVISION: u64 = 11;
const LEXICON_REVISION: u64 = 13;

const STUB_PASS: StablePassId = StablePassId("test.shadow.stub.v1");
const STUB_RULE: StableRuleId = StableRuleId {
    family: "test.shadow.stub",
    ordinal: 1,
};

/// Words the structural fixture proposes candidates for, and confirms none of.
const OVERGENERATED: [&str; 6] = ["k", "kl", "kq", "kab", "kabl", "klq"];

/// A word the structural fixture proposes exactly one candidate for, and confirms nothing for.
const OVERGENERATED_ONLY: &str = "k";

/// The word `CONFIRMING_XML` analyzes.
const CONFIRMED_WORD: &str = "ka";

/// A one-entry grammar whose only word confirms, so a false rejection has something to be false about.
const CONFIRMING_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>ShadowConfirming</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>ka</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

fn confirming_grammar() -> Grammar {
    pg_grammar::load(CONFIRMING_XML).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

/// Rejects exactly the identities it was handed, so a test chooses whether the rejection is sound.
struct RejectExactly {
    doomed: Vec<Candidate>,
}

impl CandidateFilterPass for RejectExactly {
    fn id(&self) -> StablePassId {
        STUB_PASS
    }

    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision {
        let identity = context.identity();
        if !self.doomed.iter().any(|doomed| doomed == identity) {
            return PassDecision::Keep;
        }
        PassDecision::Reject(RejectionProof {
            pass_id: STUB_PASS,
            rule_id: STUB_RULE,
            category: ProofClaim::MalformedIdentity(IdentityDefect::EmptyMorphemeSequence)
                .category(),
            witness: ProofWitness {
                candidate_identity: identity.clone(),
                witness_id: witness.witness_id,
                grammar_revision: witness.provenance.grammar_revision,
                lexicon_revision: witness.lexicon_revision,
                lexical_origin: witness.lexical_origin,
                unit_indices: Vec::new(),
                claim: ProofClaim::MalformedIdentity(IdentityDefect::EmptyMorphemeSequence),
            },
        })
    }
}

fn shadow_settings(g: &Grammar, doomed: Vec<Candidate>) -> CandidateFilterSettings {
    CandidateFilterSettings::new(
        FilterMode::Shadow,
        Arc::new(filter_of(vec![Box::new(RejectExactly { doomed })])),
        Arc::new(FilterIndex::build(g)),
        GRAMMAR_REVISION,
        LEXICON_REVISION,
    )
}

fn structural_settings(g: &Grammar) -> CandidateFilterSettings {
    let index = Arc::new(FilterIndex::build(g));
    CandidateFilterSettings::new(
        FilterMode::Shadow,
        Arc::new(filter_of(vec![
            Box::new(OwnershipPass::new(Arc::clone(&index))),
            Box::new(StructuralTransitionPass::new(Arc::clone(&index))),
        ])),
        index,
        GRAMMAR_REVISION,
        LEXICON_REVISION,
    )
}

/// Every candidate the proposer offers for `word`, read through the detached proposal stage.
fn proposed(analyzer: &mut FomaAnalyzer<'_>, word: &str) -> Vec<Candidate> {
    let proposals = analyzer.propose_words(&[word.to_string()]);
    proposals[0].candidates().to_vec()
}

/// The identity of every analysis HC confirms for `word`.
fn confirmed_identities(analyzer: &mut FomaAnalyzer<'_>, word: &str) -> Vec<Candidate> {
    analyzer
        .analyze_word(word)
        .structured
        .iter()
        .map(|analysis| Candidate {
            morphemes: analysis
                .morpheme_ids
                .iter()
                .copied()
                .map(MorphemeId)
                .collect(),
            root_index: analysis.root_morpheme_index,
        })
        .collect()
}

/// The single analysis `CONFIRMING_XML` confirms for `CONFIRMED_WORD`.
fn the_confirmed_candidate(analyzer: &mut FomaAnalyzer<'_>) -> Candidate {
    let confirmed = confirmed_identities(analyzer, CONFIRMED_WORD);
    assert_eq!(
        confirmed.len(),
        1,
        "the confirming fixture must analyze its own word exactly once"
    );
    confirmed.into_iter().next().expect("checked just above")
}

/// The one candidate the structural fixture proposes for a word it confirms nothing for.
fn the_overgenerated_candidate(analyzer: &mut FomaAnalyzer<'_>) -> Candidate {
    let candidates = proposed(analyzer, OVERGENERATED_ONLY);
    assert_eq!(candidates.len(), 1);
    assert!(
        confirmed_identities(analyzer, OVERGENERATED_ONLY).is_empty(),
        "rejecting this candidate is only sound while the confirmer refuses it too"
    );
    candidates.into_iter().next().expect("checked just above")
}

#[test]
fn off_is_the_default_and_reports_no_filter_activity() {
    let g = confirming_grammar();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    assert_eq!(analyzer.candidate_filter().mode(), FilterMode::Off);

    let profiled = analyzer.analyze_word_with_diagnostics(CONFIRMED_WORD);
    let filter = &profiled.diagnostics.filter;
    assert_eq!(filter.mode, FilterMode::Off);
    assert_eq!(filter.filter_steps, 0);
    assert_eq!(filter.filter_rejections, 0);
    assert_eq!(filter.filter_candidates_removed, 0);
    assert_eq!(filter.shadow_false_rejections, 0);
    assert_eq!(
        filter.raw_candidate_identities,
        profiled.outcome.candidates_generated
    );
    assert_eq!(
        filter.hc_candidates_received,
        profiled.outcome.candidates_generated
    );
    assert_eq!(filter.candidate_witnesses, 0, "Off builds no witnesses");
}

#[test]
fn a_valid_shadow_rejection_leaves_the_analyses_untouched_and_counts_the_death() {
    let g = fixture::grammar();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    let doomed = the_overgenerated_candidate(&mut analyzer);
    let unfiltered =
        pg_parse::result_signature(&analyzer.analyze_word(OVERGENERATED_ONLY).analyses);

    analyzer.set_candidate_filter(shadow_settings(&g, vec![doomed]));
    let profiled = analyzer.analyze_word_with_diagnostics(OVERGENERATED_ONLY);
    let filter = &profiled.diagnostics.filter;

    assert_eq!(
        pg_parse::result_signature(&profiled.outcome.analyses),
        unfiltered,
        "shadow mode must not change what HC returns"
    );
    assert_eq!(filter.mode, FilterMode::Shadow);
    assert!(filter.filter_steps > 0, "the pass must actually have run");
    assert_eq!(filter.filter_rejections, 1);
    assert_eq!(filter.filter_candidates_removed, 1);
    assert_eq!(filter.shadow_false_rejections, 0);
    assert!(filter.false_rejection_deaths.is_empty());
    assert_eq!(
        filter.hc_candidates_received, filter.raw_candidate_identities,
        "shadow sends every candidate to HC"
    );
    assert_eq!(
        filter.candidate_witnesses, filter.raw_candidate_identities,
        "the legacy adapter emits exactly one witness per identity"
    );

    let attribution = &filter.attribution;
    assert_eq!(attribution.would_die_candidates, 1);
    assert_eq!(
        attribution.would_die_never_grouped
            + attribution.would_die_sole_member
            + attribution.would_die_shared_member,
        1,
        "every would-die candidate lands in exactly one attribution class"
    );
    assert!(attribution.removable_steps <= profiled.diagnostics.confirmation_steps);
    assert!(attribution.exact_steps_max >= attribution.exact_steps_median);
    eprintln!(
        "candidate_filter_shadow_gate: word={OVERGENERATED_ONLY:?} hc_steps={} {attribution:?}",
        profiled.diagnostics.confirmation_steps
    );
}

#[test]
fn a_false_shadow_rejection_is_caught_and_carries_its_death_record() {
    let g = confirming_grammar();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    let kept = the_confirmed_candidate(&mut analyzer);
    let unfiltered = pg_parse::result_signature(&analyzer.analyze_word(CONFIRMED_WORD).analyses);
    assert!(!unfiltered.is_empty());

    analyzer.set_candidate_filter(shadow_settings(&g, vec![kept.clone()]));
    let profiled = analyzer.analyze_word_with_diagnostics(CONFIRMED_WORD);
    let filter = &profiled.diagnostics.filter;

    assert_eq!(
        pg_parse::result_signature(&profiled.outcome.analyses),
        unfiltered,
        "a false shadow rejection must still not change what HC returns"
    );
    assert_eq!(filter.shadow_false_rejections, 1);
    assert_eq!(filter.false_rejection_deaths.len(), 1);
    assert_eq!(filter.false_rejection_deaths[0].identity, kept);
}

#[test]
fn the_legacy_adapter_defers_every_allomorph_and_never_names_a_guessed_one() {
    let g = fixture::grammar();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    let candidates = proposed(&mut analyzer, "kab");
    assert!(
        !candidates.is_empty(),
        "the adapter case is vacuous without candidates"
    );

    let proposals = witnesses_for(&candidates, GRAMMAR_REVISION, LEXICON_REVISION);
    assert_eq!(proposals.len(), candidates.len());

    let mut units = 0usize;
    for (proposal, candidate) in proposals.iter().zip(candidates.iter()) {
        assert_eq!(&proposal.identity, candidate);
        assert_eq!(proposal.witnesses.len(), 1);
        let witness = proposal.witnesses.first();
        assert_eq!(witness.units.len(), candidate.morphemes.len());
        for (unit, &morpheme) in witness.units.iter().zip(candidate.morphemes.iter()) {
            units += 1;
            assert_eq!(
                unit.morpheme, morpheme,
                "morpheme ids are carried, not guessed"
            );
            match &unit.allomorphs {
                TraceFact::Deferred(_) => {}
                TraceFact::Known(set) => panic!(
                    "the legacy proposer establishes no allomorph, but the adapter claimed {:?}",
                    set.iter().copied().collect::<Vec<AllomorphId>>()
                ),
            }
            assert!(unit.role.is_deferred());
            assert!(unit.slot.is_deferred());
            assert!(unit.stratum.is_deferred());
            assert!(unit.surface_span.is_deferred());
            assert!(unit.local_events.is_deferred());
        }
    }
    assert!(units > 0, "at least one unit must have been checked");
}

#[test]
fn the_structural_passes_cannot_reject_through_the_legacy_adapter() {
    let g = fixture::grammar();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    analyzer.set_candidate_filter(structural_settings(&g));

    let mut evaluations = 0u64;
    let mut defers = 0u64;
    let mut rejections = 0u64;
    let mut witnesses = 0usize;
    for word in OVERGENERATED {
        let filter = analyzer
            .analyze_word_with_diagnostics(word)
            .diagnostics
            .filter;
        evaluations += filter.filter_steps;
        defers += filter.filter_defers;
        rejections += filter.filter_rejections;
        witnesses += filter.candidate_witnesses;
    }

    assert!(witnesses > 0, "the run must have offered real witnesses");
    assert!(evaluations > 0, "both passes must have been reached");
    assert!(
        defers > 0,
        "a deferred fact is what the structural passes see through this adapter"
    );
    assert_eq!(
        rejections, 0,
        "neither structural pass has an established fact to reject on here"
    );
    eprintln!(
        "candidate_filter_shadow_gate: structural passes over {} witnesses --          {evaluations} evaluation(s), {defers} defer(s), {rejections} rejection(s)",
        witnesses
    );
}

#[test]
fn a_candidate_whose_pins_fail_enters_no_confirmation_chunk() {
    let g = fixture::grammar();
    let morpher = Morpher::new(&g, usize::MAX);
    let owners = build_morpheme_owners(&g);
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    let mut candidates = proposed(&mut analyzer, OVERGENERATED_ONLY);
    assert!(!candidates.is_empty());
    candidates.push(Candidate {
        morphemes: vec![fixture::unowned_morpheme(&g)],
        root_index: 0,
    });
    let pin_failing = candidates.len() - 1;

    let (buckets, chunks) =
        confirm_batch_attributed(&g, &owners, &morpher, &candidates, OVERGENERATED_ONLY);
    assert_eq!(buckets.len(), candidates.len());
    assert!(buckets[pin_failing].is_empty());
    assert!(
        chunks
            .iter()
            .all(|chunk| !chunk.members.contains(&pin_failing)),
        "a pin-failing candidate is skipped before any parse, so no chunk can own its cost"
    );
    assert!(
        chunks.iter().any(|chunk| !chunk.members.is_empty()),
        "the rest of the batch must still have been confirmed"
    );
}

#[test]
fn filter_state_survives_all_four_cached_round_trips() {
    let g = fixture::grammar();
    let settings = shadow_settings(&g, Vec::new());
    let expected = settings.pass_ids();
    assert!(!expected.is_empty());

    let analyzer = FomaAnalyzer::new(&g)
        .expect("fixture compiles")
        .with_candidate_filter(settings);

    let (proposer, peeler, owners, carried) = analyzer.into_parts();
    assert_eq!(carried.pass_ids(), expected);
    assert_eq!(carried.mode(), FilterMode::Shadow);
    assert_eq!(carried.grammar_revision(), GRAMMAR_REVISION);
    assert_eq!(carried.lexicon_revision(), LEXICON_REVISION);

    let analyzer = FomaAnalyzer::from_cached(&g, proposer, peeler, owners, carried);
    assert_eq!(analyzer.candidate_filter().pass_ids(), expected);

    let (proposer, peeler, owners, morpher, carried) = analyzer.into_parts_with_morpher();
    assert_eq!(carried.pass_ids(), expected);
    assert_eq!(carried.mode(), FilterMode::Shadow);

    let analyzer =
        FomaAnalyzer::from_cached_with_morpher(&g, proposer, peeler, owners, morpher, carried);
    assert_eq!(analyzer.candidate_filter().pass_ids(), expected);
    assert_eq!(
        analyzer.candidate_filter().grammar_revision(),
        GRAMMAR_REVISION
    );
    assert_eq!(
        analyzer.candidate_filter().lexicon_revision(),
        LEXICON_REVISION
    );
}

#[test]
fn every_confirmation_entry_point_agrees_under_shadow() {
    let g = confirming_grammar();
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    let word = CONFIRMED_WORD.to_string();
    let kept = the_confirmed_candidate(&mut analyzer);
    let words = vec![word.clone()];

    let unfiltered = pg_parse::result_signature(&analyzer.analyze_word(&word).analyses);
    assert!(!unfiltered.is_empty());
    let settings = shadow_settings(&g, vec![kept]);
    let expected_passes = settings.pass_ids();
    analyzer.set_candidate_filter(settings);

    let plain = pg_parse::result_signature(&analyzer.analyze_word(&word).analyses);
    assert_eq!(plain, unfiltered);

    let budgeted = match analyzer
        .analyze_word_budgeted(&word, &pg_foma::compose_budget::ApplyBudget::unbounded())
    {
        pg_foma::composite::FomaApplyOutcome::Complete(outcome) => {
            pg_parse::result_signature(&outcome.analyses)
        }
        pg_foma::composite::FomaApplyOutcome::Incomplete { dimension, .. } => {
            panic!("an unbounded budget tripped on {dimension:?}")
        }
    };
    assert_eq!(budgeted, unfiltered);

    let profiled = analyzer.analyze_word_with_diagnostics(&word);
    assert_eq!(
        pg_parse::result_signature(&profiled.outcome.analyses),
        unfiltered
    );

    let batched = analyzer.analyze_words(&words);
    assert_eq!(
        pg_parse::result_signature(&batched[0].0.analyses),
        unfiltered
    );

    let owners = build_morpheme_owners(&g);
    let proposals = analyzer.propose_words(&words);
    let detached = pg_foma::composite::confirm_proposed_words(&g, &owners, &words, proposals, 1);
    assert_eq!(
        pg_parse::result_signature(&detached[0].0.analyses),
        unfiltered
    );
}
