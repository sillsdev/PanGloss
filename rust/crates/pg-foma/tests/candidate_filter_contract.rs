//! Contract tests for the candidate filter's proposal/witness input model.

use pg_foma::candidate_filter::{
    CandidateWitness, DeferredFactReason, DeferredFeatureId, FeatureSet, LexicalOrigin, LocalEvent,
    NonEmpty, NonEmptyError, ProposalModelError, ProposalProducer, ProposalProvenance,
    ProposedCandidate, SurfaceSpan, TraceFact, TraceRole, TraceSlotId, TraceStratumId, TraceUnit,
    WitnessId,
};
use pg_foma::tags::Candidate;
use pg_grammar::model::{AllomorphId, MorphemeId};

fn candidate(morphemes: &[u32], root_index: i32) -> Candidate {
    Candidate {
        morphemes: morphemes.iter().copied().map(MorphemeId).collect(),
        root_index,
    }
}

fn provenance() -> ProposalProvenance {
    ProposalProvenance {
        producer: ProposalProducer::SyntheticFixture,
        grammar_revision: 3,
    }
}

fn allomorph_set(ids: &[u32]) -> NonEmpty<AllomorphId> {
    NonEmpty::try_from_vec(ids.iter().copied().map(AllomorphId).collect())
        .expect("test fixture supplies at least one allomorph")
}

fn certain_unit(morpheme: u32, allomorphs: &[u32]) -> TraceUnit {
    TraceUnit {
        morpheme: MorphemeId(morpheme),
        role: TraceFact::Known(TraceRole::Root),
        allomorphs: TraceFact::Known(allomorph_set(allomorphs)),
        slot: TraceFact::Known(Some(TraceSlotId(0))),
        stratum: TraceFact::Known(Some(TraceStratumId(0))),
        surface_span: TraceFact::Known(Some(SurfaceSpan { start: 0, end: 3 })),
        local_events: TraceFact::Known(vec![LocalEvent::Neutral]),
    }
}

fn opaque_unit(morpheme: u32) -> TraceUnit {
    TraceUnit {
        morpheme: MorphemeId(morpheme),
        role: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        allomorphs: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        slot: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        stratum: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        surface_span: TraceFact::Deferred(DeferredFactReason::AmbiguityNotExhaustible),
        local_events: TraceFact::Deferred(DeferredFactReason::UnsupportedConstruct),
    }
}

fn witness(id: u64, allomorphs: &[u32]) -> CandidateWitness {
    CandidateWitness {
        witness_id: WitnessId(id),
        lexical_origin: LexicalOrigin::StaticGrammar,
        lexicon_revision: 7,
        units: vec![certain_unit(10, allomorphs)],
        deferred: FeatureSet::empty(),
        provenance: provenance(),
    }
}

fn known_allomorphs(unit: &TraceUnit) -> Vec<AllomorphId> {
    match &unit.allomorphs {
        TraceFact::Known(set) => set.iter().copied().collect(),
        TraceFact::Deferred(_) => Vec::new(),
    }
}

#[test]
fn proposal_preserves_distinct_witnesses_for_one_identity() {
    let identity = candidate(&[10, 20, 30], 1);
    let proposal = ProposedCandidate::new(
        identity.clone(),
        vec![witness(1, &[101]), witness(2, &[102])],
    )
    .unwrap();

    assert_eq!(proposal.identity, identity);
    assert_eq!(proposal.witnesses.len(), 2);
    assert_ne!(
        proposal.witnesses[0].witness_id,
        proposal.witnesses[1].witness_id
    );
}

#[test]
fn runtime_origin_is_revisioned() {
    let origin = LexicalOrigin::RuntimeOverlay { revision: 42 };
    assert_eq!(origin.revision(), Some(42));
    assert_eq!(LexicalOrigin::StaticGrammar.revision(), None);
}

#[test]
fn a_proposal_without_witnesses_is_rejected() {
    let error = ProposedCandidate::new(candidate(&[10], 0), Vec::new()).unwrap_err();
    assert_eq!(error, ProposalModelError::NoWitnesses);
}

#[test]
fn duplicate_witness_ids_within_one_candidate_are_rejected() {
    let error = ProposedCandidate::new(
        candidate(&[10], 0),
        vec![witness(1, &[101]), witness(1, &[102])],
    )
    .unwrap_err();
    assert_eq!(error, ProposalModelError::DuplicateWitnessId(WitnessId(1)));
}

#[test]
fn the_same_witness_id_is_legal_in_two_different_candidates() {
    let first = ProposedCandidate::new(candidate(&[10, 20], 0), vec![witness(1, &[101])]).unwrap();
    let second = ProposedCandidate::new(candidate(&[30, 40], 1), vec![witness(1, &[102])]).unwrap();

    assert_eq!(
        first.witnesses[0].witness_id,
        second.witnesses[0].witness_id
    );
    assert_ne!(first.identity, second.identity);
}

#[test]
fn a_known_allomorph_choice_is_non_empty() {
    assert_eq!(
        NonEmpty::<AllomorphId>::try_from_vec(Vec::new()).unwrap_err(),
        NonEmptyError::Empty
    );

    let unit = certain_unit(10, &[101, 102]);
    assert_eq!(known_allomorphs(&unit).len(), 2);
}

#[test]
fn unavailable_facts_are_deferred_rather_than_sentinel_ids() {
    let unit = opaque_unit(10);

    assert!(matches!(unit.role, TraceFact::Deferred(_)));
    assert!(matches!(unit.allomorphs, TraceFact::Deferred(_)));
    assert!(matches!(unit.surface_span, TraceFact::Deferred(_)));
    assert!(known_allomorphs(&unit).is_empty());
    assert!(!known_allomorphs(&unit).contains(&AllomorphId::GUESSED));
}

#[test]
fn a_known_absent_slot_differs_from_a_deferred_one() {
    let absent: TraceFact<Option<TraceSlotId>> = TraceFact::Known(None);
    let deferred: TraceFact<Option<TraceSlotId>> =
        TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit);

    assert_ne!(absent, deferred);
    assert_ne!(absent, TraceFact::Known(Some(TraceSlotId(0))));
}

#[test]
fn deferred_surface_changing_features_are_named_on_the_witness() {
    let mut witness = witness(1, &[101]);
    witness.deferred = FeatureSet::from_iter([DeferredFeatureId(4), DeferredFeatureId(2)]);
    witness.lexical_origin = LexicalOrigin::RuntimeOverlay { revision: 9 };

    assert!(witness.deferred.contains(DeferredFeatureId(2)));
    assert!(!witness.deferred.contains(DeferredFeatureId(3)));
    assert_eq!(
        witness.deferred.iter().collect::<Vec<_>>(),
        vec![DeferredFeatureId(2), DeferredFeatureId(4)]
    );
    assert_eq!(witness.lexical_origin.revision(), Some(9));
    assert_eq!(witness.lexicon_revision, 7);
}
