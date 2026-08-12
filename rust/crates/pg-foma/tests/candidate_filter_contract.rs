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

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use pg_foma::candidate_filter::decision::{
    DeferReason, PassDecision, ProofCategory, ProofWitness, RejectionProof, StablePassId,
    StableRuleId, TraceFactKind,
};
use pg_foma::candidate_filter::passes::CandidateFilterPass;
use pg_foma::candidate_filter::pipeline::{
    CandidateFilter, FilterBudget, FilterCompletion, FilterContext, FilterMode, FilterStopReason,
};
use pg_foma::candidate_filter::report::{
    CandidateDeath, FilterTraceSink, PassEvent, PassOutcome, RetainedCandidateSink,
};
use pg_foma::candidate_filter::test_support::{allow_list_filter, AllowedProof};

const KEEP_ALL: StablePassId = StablePassId("test.keep_all.v1");
const REJECT_ALL: StablePassId = StablePassId("test.reject_all.v1");
const REJECT_ONE: StablePassId = StablePassId("test.reject_one.v1");
const DEFER_ALL: StablePassId = StablePassId("test.defer_all.v1");
const FORGED: StablePassId = StablePassId("test.forged.v1");
const OFF_LIST: StablePassId = StablePassId("test.off_list.v1");
const RULE: StableRuleId = StableRuleId {
    family: "test",
    ordinal: 1,
};
const UNLISTED_RULE: StableRuleId = StableRuleId {
    family: "test",
    ordinal: 99,
};

fn proof(
    pass_id: StablePassId,
    rule_id: StableRuleId,
    context: &FilterContext<'_>,
    witness_id: WitnessId,
) -> RejectionProof {
    RejectionProof {
        pass_id,
        rule_id,
        category: ProofCategory::ImpossibleOwnership,
        witness: ProofWitness {
            candidate_identity: context.identity().clone(),
            witness_id,
            grammar_revision: 3,
            lexicon_revision: 7,
            unit_indices: vec![0],
        },
    }
}

struct KeepAll(StablePassId, Arc<AtomicUsize>);

impl CandidateFilterPass for KeepAll {
    fn id(&self) -> StablePassId {
        self.0
    }

    fn evaluate(&self, _context: &FilterContext<'_>, _witness: &CandidateWitness) -> PassDecision {
        self.1.fetch_add(1, Ordering::SeqCst);
        PassDecision::Keep
    }
}

struct RejectAll;

impl CandidateFilterPass for RejectAll {
    fn id(&self) -> StablePassId {
        REJECT_ALL
    }

    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision {
        PassDecision::Reject(proof(REJECT_ALL, RULE, context, witness.witness_id))
    }
}

struct RejectOne(WitnessId);

impl CandidateFilterPass for RejectOne {
    fn id(&self) -> StablePassId {
        REJECT_ONE
    }

    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision {
        if witness.witness_id == self.0 {
            PassDecision::Reject(proof(REJECT_ONE, RULE, context, witness.witness_id))
        } else {
            PassDecision::Keep
        }
    }
}

struct DeferAll;

impl CandidateFilterPass for DeferAll {
    fn id(&self) -> StablePassId {
        DEFER_ALL
    }

    fn evaluate(&self, _context: &FilterContext<'_>, _witness: &CandidateWitness) -> PassDecision {
        PassDecision::Defer(DeferReason::MissingTraceFact(TraceFactKind::Slot))
    }
}

struct ForgedWitnessId;

impl CandidateFilterPass for ForgedWitnessId {
    fn id(&self) -> StablePassId {
        FORGED
    }

    fn evaluate(&self, context: &FilterContext<'_>, _witness: &CandidateWitness) -> PassDecision {
        PassDecision::Reject(proof(FORGED, RULE, context, WitnessId(4242)))
    }
}

struct UnlistedRule;

impl CandidateFilterPass for UnlistedRule {
    fn id(&self) -> StablePassId {
        OFF_LIST
    }

    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision {
        PassDecision::Reject(proof(OFF_LIST, UNLISTED_RULE, context, witness.witness_id))
    }
}

struct Recorder(StablePassId, Arc<Mutex<Vec<String>>>);

impl CandidateFilterPass for Recorder {
    fn id(&self) -> StablePassId {
        self.0
    }

    fn evaluate(&self, _context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision {
        self.1
            .lock()
            .expect("recorder log is not poisoned")
            .push(format!("{}@{}", self.0.as_str(), witness.witness_id.0));
        PassDecision::Keep
    }
}

struct NeverEvaluated;

impl CandidateFilterPass for NeverEvaluated {
    fn id(&self) -> StablePassId {
        StablePassId("test.never.v1")
    }

    fn evaluate(&self, _context: &FilterContext<'_>, _witness: &CandidateWitness) -> PassDecision {
        panic!("this pass must never be evaluated");
    }
}

fn allowed(pass_id: StablePassId) -> AllowedProof {
    AllowedProof {
        pass_id,
        rule_id: RULE,
        category: ProofCategory::ImpossibleOwnership,
    }
}

fn every_reject_allowed() -> Vec<AllowedProof> {
    vec![
        allowed(REJECT_ALL),
        allowed(REJECT_ONE),
        allowed(FORGED),
        allowed(OFF_LIST),
    ]
}

fn reject_all_filter() -> CandidateFilter {
    allow_list_filter(vec![Box::new(RejectAll)], every_reject_allowed())
}

fn one_candidate_with_witnesses(ids: &[u64]) -> Vec<ProposedCandidate> {
    let witnesses = ids.iter().map(|&id| witness(id, &[101])).collect();
    vec![ProposedCandidate::new(candidate(&[10], 0), witnesses).expect("distinct witness ids")]
}

fn numbered_candidates(count: u32) -> Vec<ProposedCandidate> {
    (0..count)
        .map(|index| {
            ProposedCandidate::new(candidate(&[index], 0), vec![witness(1, &[101])])
                .expect("one witness")
        })
        .collect()
}

#[derive(Default)]
struct RecordingTraceSink {
    events: Vec<PassEvent>,
    deaths: Vec<CandidateDeath>,
}

impl FilterTraceSink for RecordingTraceSink {
    fn record_pass_event(&mut self, event: &PassEvent) {
        self.events.push(event.clone());
    }

    fn record_candidate_death(&mut self, death: &CandidateDeath) {
        self.deaths.push(death.clone());
    }
}

struct LoggingIter {
    items: std::vec::IntoIter<ProposedCandidate>,
    log: Rc<RefCell<Vec<String>>>,
    pulled: usize,
}

impl Iterator for LoggingIter {
    type Item = ProposedCandidate;

    fn next(&mut self) -> Option<ProposedCandidate> {
        let item = self.items.next()?;
        self.log.borrow_mut().push(format!("pull {}", self.pulled));
        self.pulled += 1;
        Some(item)
    }
}

struct LoggingSink {
    log: Rc<RefCell<Vec<String>>>,
    retained: Vec<ProposedCandidate>,
}

impl RetainedCandidateSink for LoggingSink {
    fn accept(&mut self, candidate: ProposedCandidate) {
        self.log
            .borrow_mut()
            .push(format!("accept {}", candidate.identity.morphemes[0].0));
        self.retained.push(candidate);
    }
}

fn run_logged(
    filter: &CandidateFilter,
    inputs: Vec<ProposedCandidate>,
    budget: FilterBudget,
) -> (Vec<String>, usize, FilterCompletion) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let input = LoggingIter {
        items: inputs.into_iter(),
        log: Rc::clone(&log),
        pulled: 0,
    };
    let mut sink = LoggingSink {
        log: Rc::clone(&log),
        retained: Vec::new(),
    };
    let mut trace = RecordingTraceSink::default();
    let status = filter.filter_into(FilterMode::Enforce, input, &mut sink, &mut trace, budget);
    let entries = log.borrow().clone();
    (entries, sink.retained.len(), status)
}

#[test]
fn candidate_survives_when_any_witness_survives() {
    let filter = allow_list_filter(
        vec![
            Box::new(RejectOne(WitnessId(1))),
            Box::new(KeepAll(KEEP_ALL, Arc::new(AtomicUsize::new(0)))),
        ],
        every_reject_allowed(),
    );

    let outcome = filter.filter(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1, 2]),
        FilterBudget::unlimited(),
    );

    assert_eq!(outcome.retained.len(), 1);
    assert_eq!(outcome.report.witnesses_rejected, 1);
    assert_eq!(outcome.report.candidates_rejected, 0);
    assert_eq!(outcome.status, FilterCompletion::Complete);
}

#[test]
fn witness_evaluation_stops_at_its_first_verified_rejection() {
    let filter = allow_list_filter(
        vec![Box::new(RejectAll), Box::new(NeverEvaluated)],
        every_reject_allowed(),
    );

    let outcome = filter.filter(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1]),
        FilterBudget::unlimited(),
    );

    assert_eq!(outcome.retained.len(), 0);
    assert_eq!(outcome.report.pass_evaluations, 1);
}

#[test]
fn defer_retains_the_candidate_and_does_not_end_the_pass_loop() {
    let reached = Arc::new(AtomicUsize::new(0));
    let filter = allow_list_filter(
        vec![
            Box::new(DeferAll),
            Box::new(KeepAll(KEEP_ALL, Arc::clone(&reached))),
        ],
        Vec::new(),
    );

    let outcome = filter.filter(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1]),
        FilterBudget::unlimited(),
    );

    assert_eq!(outcome.retained.len(), 1);
    assert_eq!(outcome.report.defers, 1);
    assert_eq!(outcome.report.keeps, 1);
    assert_eq!(reached.load(Ordering::SeqCst), 1);
}

#[test]
fn passes_run_in_the_declared_order_for_every_witness() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let filter = allow_list_filter(
        vec![
            Box::new(Recorder(StablePassId("test.zeta.v1"), Arc::clone(&log))),
            Box::new(Recorder(StablePassId("test.alpha.v1"), Arc::clone(&log))),
        ],
        Vec::new(),
    );

    filter.filter(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1, 2]),
        FilterBudget::unlimited(),
    );

    assert_eq!(
        log.lock().expect("log is not poisoned").clone(),
        vec![
            "test.zeta.v1@1".to_string(),
            "test.alpha.v1@1".to_string(),
            "test.zeta.v1@2".to_string(),
            "test.alpha.v1@2".to_string(),
        ]
    );
}

#[test]
fn pass_counters_are_keyed_by_stable_pass_id_in_sorted_order() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let filter = allow_list_filter(
        vec![
            Box::new(Recorder(StablePassId("test.zeta.v1"), Arc::clone(&log))),
            Box::new(Recorder(StablePassId("test.alpha.v1"), Arc::clone(&log))),
        ],
        Vec::new(),
    );

    let outcome = filter.filter(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1]),
        FilterBudget::unlimited(),
    );

    let keys: Vec<&str> = outcome
        .report
        .per_pass
        .keys()
        .map(StablePassId::as_str)
        .collect();
    assert_eq!(keys, vec!["test.alpha.v1", "test.zeta.v1"]);
    assert_eq!(
        outcome.report.per_pass[&StablePassId("test.zeta.v1")].keeps,
        1
    );
}

#[test]
fn off_mode_bypasses_every_pass() {
    let filter = allow_list_filter(
        vec![Box::new(NeverEvaluated), Box::new(RejectAll)],
        every_reject_allowed(),
    );

    let outcome = filter.filter(
        FilterMode::Off,
        numbered_candidates(3),
        FilterBudget::unlimited(),
    );

    assert_eq!(outcome.retained.len(), 3);
    assert_eq!(outcome.report.pass_evaluations, 0);
    assert_eq!(outcome.report.witnesses_rejected, 0);
    assert_eq!(outcome.status, FilterCompletion::Complete);
}

#[test]
fn shadow_mode_records_the_death_and_still_retains_the_candidate() {
    let outcome = reject_all_filter().filter(
        FilterMode::Shadow,
        numbered_candidates(3),
        FilterBudget::unlimited(),
    );

    assert_eq!(outcome.retained.len(), 3);
    assert_eq!(outcome.report.candidates_rejected, 3);
    assert_eq!(outcome.report.witnesses_rejected, 3);
    assert_eq!(outcome.report.candidates_retained, 3);
}

#[test]
fn enforce_mode_removes_a_candidate_whose_every_witness_is_rejected() {
    let outcome = reject_all_filter().filter(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1, 2]),
        FilterBudget::unlimited(),
    );

    assert_eq!(outcome.retained.len(), 0);
    assert_eq!(outcome.report.witnesses_rejected, 2);
    assert_eq!(outcome.report.candidates_rejected, 1);
    assert_eq!(outcome.report.candidates_retained, 0);
}

#[test]
fn an_unverifiable_proof_defers_instead_of_killing_the_witness() {
    let filter = allow_list_filter(vec![Box::new(ForgedWitnessId)], every_reject_allowed());

    let outcome = filter.filter(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1]),
        FilterBudget::unlimited(),
    );

    assert_eq!(outcome.retained.len(), 1);
    assert_eq!(outcome.report.proof_verification_failures, 1);
    assert_eq!(outcome.report.witnesses_rejected, 0);
}

#[test]
fn a_proof_outside_the_allow_list_defers_instead_of_killing_the_witness() {
    let filter = allow_list_filter(vec![Box::new(UnlistedRule)], every_reject_allowed());

    let outcome = filter.filter(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1]),
        FilterBudget::unlimited(),
    );

    assert_eq!(outcome.retained.len(), 1);
    assert_eq!(outcome.report.proof_verification_failures, 1);
}

#[test]
fn budget_exhaustion_passes_unvisited_candidates_through() {
    let outcome = reject_all_filter().filter(
        FilterMode::Enforce,
        numbered_candidates(3),
        FilterBudget::steps(1),
    );

    assert_eq!(
        outcome.status,
        FilterCompletion::Incomplete(FilterStopReason::StepBudget)
    );
    assert_eq!(outcome.retained.len(), 2);
}

#[test]
fn budget_exhaustion_forwards_the_remaining_stream_without_collecting_it() {
    let (log, retained, status) = run_logged(
        &reject_all_filter(),
        numbered_candidates(4),
        FilterBudget::steps(1),
    );

    assert_eq!(
        status,
        FilterCompletion::Incomplete(FilterStopReason::StepBudget)
    );
    assert_eq!(retained, 3);
    assert_eq!(
        log,
        vec![
            "pull 0".to_string(),
            "pull 1".to_string(),
            "accept 1".to_string(),
            "pull 2".to_string(),
            "accept 2".to_string(),
            "pull 3".to_string(),
            "accept 3".to_string(),
        ]
    );
}

#[test]
fn retained_candidates_are_emitted_before_the_input_ends() {
    let filter = allow_list_filter(
        vec![Box::new(KeepAll(KEEP_ALL, Arc::new(AtomicUsize::new(0))))],
        Vec::new(),
    );

    let (log, retained, status) =
        run_logged(&filter, numbered_candidates(3), FilterBudget::unlimited());

    assert_eq!(status, FilterCompletion::Complete);
    assert_eq!(retained, 3);
    assert_eq!(
        log,
        vec![
            "pull 0".to_string(),
            "accept 0".to_string(),
            "pull 1".to_string(),
            "accept 1".to_string(),
            "pull 2".to_string(),
            "accept 2".to_string(),
        ]
    );
}

#[test]
fn the_same_witness_id_in_two_candidates_gets_distinct_candidate_ordinals() {
    let mut sink: Vec<ProposedCandidate> = Vec::new();
    let mut trace = RecordingTraceSink::default();

    reject_all_filter().filter_into(
        FilterMode::Enforce,
        numbered_candidates(2),
        &mut sink,
        &mut trace,
        FilterBudget::unlimited(),
    );

    assert_eq!(trace.events.len(), 2);
    assert_eq!(trace.events[0].witness_id, trace.events[1].witness_id);
    assert_ne!(
        trace.events[0].candidate_ordinal,
        trace.events[1].candidate_ordinal
    );
    assert!(trace.events[0].event_ordinal < trace.events[1].event_ordinal);
}

#[test]
fn a_candidate_death_links_every_witness_to_its_terminal_pass() {
    let mut sink: Vec<ProposedCandidate> = Vec::new();
    let mut trace = RecordingTraceSink::default();

    reject_all_filter().filter_into(
        FilterMode::Enforce,
        one_candidate_with_witnesses(&[1, 2]),
        &mut sink,
        &mut trace,
        FilterBudget::unlimited(),
    );

    assert_eq!(trace.deaths.len(), 1);
    let death = &trace.deaths[0];
    assert_eq!(death.witness_deaths.len(), 2);
    assert_eq!(death.witness_deaths[0].witness_id, WitnessId(1));
    assert_eq!(death.witness_deaths[1].witness_id, WitnessId(2));
    for witness_death in &death.witness_deaths {
        assert_eq!(witness_death.pass_id.as_str(), "test.reject_all.v1");
        assert_eq!(witness_death.category, ProofCategory::ImpossibleOwnership);
    }
    assert!(matches!(trace.events[0].outcome, PassOutcome::Rejected(_)));
}
