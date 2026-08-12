//! What re-deriving a recorded rejection accepts, and what it catches.

#[path = "common/filter_fixture.rs"]
mod fixture;

#[path = "common/filter_sites.rs"]
mod sites;

use pg_foma::candidate_filter::decision::{
    AdmissibleProof, IdentityDefect, PassDecision, ProofCategory, ProofClaim,
    ProofVerificationError, ProofWitness, RejectionProof, SpanDefect, StablePassId, StableRuleId,
    TraceFactKind,
};
use pg_foma::candidate_filter::model::{
    CandidateWitness, DeferredFactReason, DeferredFeatureId, FeatureSet, LexicalOrigin, LocalEvent,
    NonEmpty, PartnerClassId, ProposalProducer, ProposalProvenance, ProposedCandidate, SurfaceSpan,
    TraceFact, TraceRole, TraceSlotId, TraceStratumId, TraceUnit, WitnessId,
};
use pg_foma::candidate_filter::pipeline::{FilterBudget, FilterContext, FilterMode};
use pg_foma::candidate_filter::report::{
    BoundedDeathLedger, CandidateDeath, FilterCounters, PassEvent, PassOutcome,
};
use pg_foma::candidate_filter::test_support::{
    filter_of, recorded_rejections, RejectionProofVerifier,
};
use pg_foma::candidate_filter::CandidateFilterPass;
use pg_foma::tags::Candidate;
use pg_grammar::model::{AllomorphId, MorphemeId};

const PASS: StablePassId = StablePassId("test.proof.v1");
const OTHER_PASS: StablePassId = StablePassId("test.other.v1");
const RULE: StableRuleId = StableRuleId {
    family: "test.proof",
    ordinal: 1,
};
const UNKNOWN_RULE: StableRuleId = StableRuleId {
    family: "test.proof",
    ordinal: 77,
};
const GRAMMAR_REVISION: u64 = 3;
const LEXICON_REVISION: u64 = 7;
const PARTNER: PartnerClassId = PartnerClassId(7);

const ALL_CATEGORIES: [ProofCategory; 9] = [
    ProofCategory::MalformedIdentity,
    ProofCategory::ImpossibleOwnership,
    ProofCategory::ForbiddenTransition,
    ProofCategory::MissingRequiredPartner,
    ProofCategory::StaticCoOccurrenceViolation,
    ProofCategory::NoCompatibleAllomorph,
    ProofCategory::StaticSignatureConflict,
    ProofCategory::ImpossibleSurfaceSpan,
    ProofCategory::ImpossibleLocalEnvironment,
];

struct ProofPass(RejectionProof);

impl CandidateFilterPass for ProofPass {
    fn id(&self) -> StablePassId {
        PASS
    }

    fn admissible_proofs(&self) -> Vec<AdmissibleProof> {
        ALL_CATEGORIES
            .iter()
            .map(|&category| AdmissibleProof {
                rule_id: RULE,
                category,
            })
            .collect()
    }

    fn evaluate(&self, _context: &FilterContext<'_>, _witness: &CandidateWitness) -> PassDecision {
        PassDecision::Reject(self.0.clone())
    }
}

/// A pass declaring one `(rule, category)` pair, for proofs that reach outside their own catalog.
struct NarrowPass(RejectionProof);

impl CandidateFilterPass for NarrowPass {
    fn id(&self) -> StablePassId {
        PASS
    }

    fn admissible_proofs(&self) -> Vec<AdmissibleProof> {
        vec![AdmissibleProof {
            rule_id: RULE,
            category: ProofCategory::ImpossibleOwnership,
        }]
    }

    fn evaluate(&self, _context: &FilterContext<'_>, _witness: &CandidateWitness) -> PassDecision {
        PassDecision::Reject(self.0.clone())
    }
}

/// A completed run: what it decided, and what re-deriving its recorded rejections then found.
struct Run {
    retained: usize,
    events: Vec<PassEvent>,
    deaths: Vec<CandidateDeath>,
    counters: FilterCounters,
    verification: Result<(), Vec<ProofVerificationError>>,
    envelopes_only: Result<(), Vec<ProofVerificationError>>,
}

fn run(proof: RejectionProof, witness: CandidateWitness) -> Run {
    run_pass(Box::new(ProofPass(proof)), witness)
}

fn run_pass(pass: Box<dyn CandidateFilterPass>, witness: CandidateWitness) -> Run {
    let passes: Vec<Box<dyn CandidateFilterPass>> = vec![pass];
    let verifier = RejectionProofVerifier::of_passes(&passes);
    let filter = filter_of(passes);
    let inputs = vec![proposal_of(witness)];

    let mut retained: Vec<ProposedCandidate> = Vec::new();
    let mut ledger = BoundedDeathLedger::unlimited();
    filter.filter_into(
        FilterMode::Enforce,
        inputs.clone(),
        &mut retained,
        &mut ledger,
        FilterBudget::unlimited(),
    );

    let records = recorded_rejections(&inputs, &ledger);
    Run {
        retained: retained.len(),
        events: ledger.events().to_vec(),
        deaths: ledger.candidate_deaths().to_vec(),
        counters: ledger.counters().clone(),
        verification: verifier.verify_recorded(&records),
        envelopes_only: verifier.verify_recorded_envelopes(&records),
    }
}

fn proposal_of(witness: CandidateWitness) -> ProposedCandidate {
    ProposedCandidate::new(identity(), vec![witness]).expect("one witness")
}

fn refusal(run: &Run) -> ProofVerificationError {
    match &run.verification {
        Err(errors) if errors.len() == 1 => errors[0],
        other => panic!("expected exactly one verification failure, got {other:?}"),
    }
}

fn identity() -> Candidate {
    Candidate {
        morphemes: vec![MorphemeId(10), MorphemeId(20), MorphemeId(30)],
        root_index: 5,
    }
}

fn other_identity() -> Candidate {
    Candidate {
        morphemes: vec![MorphemeId(10)],
        root_index: 0,
    }
}

fn allomorphs(ids: &[u32]) -> NonEmpty<AllomorphId> {
    NonEmpty::try_from_vec(ids.iter().copied().map(AllomorphId).collect())
        .expect("at least one allomorph")
}

fn unit(
    morpheme: u32,
    role: TraceRole,
    allomorph_ids: &[u32],
    slot: u32,
    span: (usize, usize),
    events: Vec<LocalEvent>,
) -> TraceUnit {
    TraceUnit {
        morpheme: MorphemeId(morpheme),
        role: TraceFact::Known(role),
        allomorphs: TraceFact::Known(allomorphs(allomorph_ids)),
        slot: TraceFact::Known(Some(TraceSlotId(slot))),
        stratum: TraceFact::Known(Some(TraceStratumId(0))),
        surface_span: TraceFact::Known(Some(SurfaceSpan {
            start: span.0,
            end: span.1,
        })),
        local_events: TraceFact::Known(events),
    }
}

fn base_units() -> Vec<TraceUnit> {
    vec![
        unit(
            10,
            TraceRole::Root,
            &[101, 102],
            0,
            (0, 3),
            vec![LocalEvent::PartnerOpen(PARTNER)],
        ),
        unit(
            20,
            TraceRole::Suffix,
            &[201],
            1,
            (2, 5),
            vec![LocalEvent::Neutral],
        ),
        unit(
            30,
            TraceRole::Suffix,
            &[301, 302],
            2,
            (5, 7),
            vec![LocalEvent::Neutral],
        ),
    ]
}

fn witness_with(units: Vec<TraceUnit>) -> CandidateWitness {
    CandidateWitness {
        witness_id: WitnessId(1),
        lexical_origin: LexicalOrigin::StaticGrammar,
        lexicon_revision: LEXICON_REVISION,
        units,
        deferred: FeatureSet::empty(),
        provenance: ProposalProvenance {
            producer: ProposalProducer::SyntheticFixture,
            grammar_revision: GRAMMAR_REVISION,
        },
    }
}

fn base_witness() -> CandidateWitness {
    witness_with(base_units())
}

fn opaque_witness() -> CandidateWitness {
    let opaque = |morpheme: u32| TraceUnit {
        morpheme: MorphemeId(morpheme),
        role: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        allomorphs: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        slot: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        stratum: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        surface_span: TraceFact::Deferred(DeferredFactReason::AmbiguityNotExhaustible),
        local_events: TraceFact::Deferred(DeferredFactReason::UnsupportedConstruct),
    };
    witness_with(vec![opaque(10), opaque(20), opaque(30)])
}

fn proof_of(category: ProofCategory) -> RejectionProof {
    let (unit_indices, claim) = match category {
        ProofCategory::MalformedIdentity => (
            Vec::new(),
            ProofClaim::MalformedIdentity(IdentityDefect::RootIndexOutOfRange {
                root_index: 5,
                morphemes: 3,
            }),
        ),
        ProofCategory::ImpossibleOwnership => (
            vec![0],
            ProofClaim::ImpossibleOwnership {
                unit_index: 0,
                morpheme: MorphemeId(10),
                role: TraceRole::Root,
            },
        ),
        ProofCategory::ForbiddenTransition => (
            vec![0, 1],
            ProofClaim::ForbiddenTransition {
                from_unit_index: 0,
                to_unit_index: 1,
                from_slot: TraceSlotId(0),
                to_slot: TraceSlotId(1),
                stratum: TraceStratumId(0),
            },
        ),
        ProofCategory::MissingRequiredPartner => (
            vec![0],
            ProofClaim::MissingRequiredPartner {
                opened_at: 0,
                class: PARTNER,
            },
        ),
        ProofCategory::StaticCoOccurrenceViolation => (
            vec![0, 1],
            ProofClaim::StaticCoOccurrenceViolation {
                left_unit_index: 0,
                right_unit_index: 1,
                left_morpheme: MorphemeId(10),
                right_morpheme: MorphemeId(20),
                eliminated_pairs: vec![
                    (AllomorphId(101), AllomorphId(201)),
                    (AllomorphId(102), AllomorphId(201)),
                ],
            },
        ),
        ProofCategory::NoCompatibleAllomorph => (
            vec![0],
            ProofClaim::NoCompatibleAllomorph {
                unit_index: 0,
                morpheme: MorphemeId(10),
                eliminated: vec![AllomorphId(101), AllomorphId(102)],
            },
        ),
        ProofCategory::StaticSignatureConflict => (
            vec![0, 2],
            ProofClaim::StaticSignatureConflict {
                unit_index: 0,
                morpheme: MorphemeId(10),
                eliminated: vec![AllomorphId(101), AllomorphId(102)],
                conflicting_unit_index: 2,
                conflicting_morpheme: MorphemeId(30),
                conflicting_eliminated: vec![AllomorphId(301), AllomorphId(302)],
            },
        ),
        ProofCategory::ImpossibleSurfaceSpan => (
            vec![0, 1],
            ProofClaim::ImpossibleSurfaceSpan {
                unit_index: 0,
                span: SurfaceSpan { start: 0, end: 3 },
                defect: SpanDefect::OverlapsUnit {
                    other_unit_index: 1,
                },
            },
        ),
        ProofCategory::ImpossibleLocalEnvironment => (
            vec![0, 1],
            ProofClaim::ImpossibleLocalEnvironment {
                unit_index: 0,
                events: vec![LocalEvent::PartnerOpen(PARTNER)],
                neighbor_unit_index: 1,
                neighbor_events: vec![LocalEvent::Neutral],
            },
        ),
    };

    RejectionProof {
        pass_id: PASS,
        rule_id: RULE,
        category,
        witness: ProofWitness {
            candidate_identity: identity(),
            witness_id: WitnessId(1),
            grammar_revision: GRAMMAR_REVISION,
            lexicon_revision: LEXICON_REVISION,
            lexical_origin: LexicalOrigin::StaticGrammar,
            unit_indices,
            claim,
        },
    }
}

/// A proof carrying `claim`, citing `unit_indices`, and otherwise identical to the category's own.
fn reclaimed(
    category: ProofCategory,
    unit_indices: Vec<usize>,
    claim: ProofClaim,
) -> RejectionProof {
    let mut proof = proof_of(category);
    proof.witness.unit_indices = unit_indices;
    proof.witness.claim = claim;
    proof
}

fn a_different_category(category: ProofCategory) -> ProofCategory {
    if category == ProofCategory::MalformedIdentity {
        ProofCategory::ImpossibleOwnership
    } else {
        ProofCategory::MalformedIdentity
    }
}

/// The corruptions every category shares: envelope fields nothing about the claim can excuse.
fn generic_forgeries(
    category: ProofCategory,
) -> Vec<(&'static str, RejectionProof, ProofVerificationError)> {
    let mut out = Vec::new();

    let mut forged = proof_of(category);
    forged.pass_id = OTHER_PASS;
    out.push((
        "pass id",
        forged,
        ProofVerificationError::PassIdMismatch {
            declared: PASS,
            claimed: OTHER_PASS,
        },
    ));

    let mut forged = proof_of(category);
    forged.rule_id = UNKNOWN_RULE;
    out.push((
        "rule id",
        forged,
        ProofVerificationError::UnrecognizedRule(UNKNOWN_RULE),
    ));

    let mut forged = proof_of(category);
    forged.witness.candidate_identity = other_identity();
    out.push((
        "candidate identity",
        forged,
        ProofVerificationError::CandidateIdentityMismatch,
    ));

    let mut forged = proof_of(category);
    forged.witness.witness_id = WitnessId(4242);
    out.push((
        "witness id",
        forged,
        ProofVerificationError::WitnessIdMismatch,
    ));

    let mut forged = proof_of(category);
    forged.witness.unit_indices.push(99);
    out.push((
        "unit index",
        forged,
        ProofVerificationError::UnitIndexOutOfRange {
            index: 99,
            units: 3,
        },
    ));

    let mut forged = proof_of(category);
    forged.witness.grammar_revision = GRAMMAR_REVISION + 1;
    out.push((
        "grammar revision",
        forged,
        ProofVerificationError::GrammarRevisionMismatch,
    ));

    let mut forged = proof_of(category);
    forged.witness.lexicon_revision = LEXICON_REVISION + 1;
    out.push((
        "lexicon revision",
        forged,
        ProofVerificationError::LexiconRevisionMismatch,
    ));

    let mut forged = proof_of(category);
    forged.witness.lexical_origin = LexicalOrigin::RuntimeOverlay { revision: 1 };
    out.push((
        "lexical origin",
        forged,
        ProofVerificationError::LexicalOriginMismatch,
    ));

    let mut forged = proof_of(category);
    let declared = a_different_category(category);
    forged.category = declared;
    out.push((
        "declared category",
        forged,
        ProofVerificationError::CategoryClaimMismatch {
            declared,
            claimed: category,
        },
    ));

    out
}

/// The corruptions only this category's payload can express.
fn payload_forgeries(
    category: ProofCategory,
) -> Vec<(&'static str, RejectionProof, ProofVerificationError)> {
    match category {
        ProofCategory::MalformedIdentity => vec![
            (
                "identity is not empty",
                reclaimed(
                    category,
                    Vec::new(),
                    ProofClaim::MalformedIdentity(IdentityDefect::EmptyMorphemeSequence),
                ),
                ProofVerificationError::IdentityDefectNotEstablished,
            ),
            (
                "root index is in range",
                reclaimed(
                    category,
                    Vec::new(),
                    ProofClaim::MalformedIdentity(IdentityDefect::RootIndexOutOfRange {
                        root_index: 1,
                        morphemes: 3,
                    }),
                ),
                ProofVerificationError::IdentityDefectNotEstablished,
            ),
        ],
        ProofCategory::ImpossibleOwnership => vec![
            (
                "wrong morpheme",
                reclaimed(
                    category,
                    vec![0],
                    ProofClaim::ImpossibleOwnership {
                        unit_index: 0,
                        morpheme: MorphemeId(99),
                        role: TraceRole::Root,
                    },
                ),
                ProofVerificationError::MorphemeMismatch { unit_index: 0 },
            ),
            (
                "wrong role",
                reclaimed(
                    category,
                    vec![0],
                    ProofClaim::ImpossibleOwnership {
                        unit_index: 0,
                        morpheme: MorphemeId(10),
                        role: TraceRole::Prefix,
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 0,
                    fact: TraceFactKind::Role,
                },
            ),
            (
                "stale ownership at another unit",
                reclaimed(
                    category,
                    vec![2],
                    ProofClaim::ImpossibleOwnership {
                        unit_index: 2,
                        morpheme: MorphemeId(30),
                        role: TraceRole::Root,
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 2,
                    fact: TraceFactKind::Role,
                },
            ),
        ],
        ProofCategory::ForbiddenTransition => vec![
            (
                "units are not adjacent",
                reclaimed(
                    category,
                    vec![0, 2],
                    ProofClaim::ForbiddenTransition {
                        from_unit_index: 0,
                        to_unit_index: 2,
                        from_slot: TraceSlotId(0),
                        to_slot: TraceSlotId(2),
                        stratum: TraceStratumId(0),
                    },
                ),
                ProofVerificationError::UnitsNotAdjacent { from: 0, to: 2 },
            ),
            (
                "wrong slot",
                reclaimed(
                    category,
                    vec![0, 1],
                    ProofClaim::ForbiddenTransition {
                        from_unit_index: 0,
                        to_unit_index: 1,
                        from_slot: TraceSlotId(9),
                        to_slot: TraceSlotId(1),
                        stratum: TraceStratumId(0),
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 0,
                    fact: TraceFactKind::Slot,
                },
            ),
            (
                "wrong stratum",
                reclaimed(
                    category,
                    vec![0, 1],
                    ProofClaim::ForbiddenTransition {
                        from_unit_index: 0,
                        to_unit_index: 1,
                        from_slot: TraceSlotId(0),
                        to_slot: TraceSlotId(1),
                        stratum: TraceStratumId(9),
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 0,
                    fact: TraceFactKind::Stratum,
                },
            ),
        ],
        ProofCategory::MissingRequiredPartner => vec![
            (
                "class was never opened",
                reclaimed(
                    category,
                    vec![0],
                    ProofClaim::MissingRequiredPartner {
                        opened_at: 0,
                        class: PartnerClassId(8),
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 0,
                    fact: TraceFactKind::LocalEvents,
                },
            ),
            (
                "wrong opening unit",
                reclaimed(
                    category,
                    vec![1],
                    ProofClaim::MissingRequiredPartner {
                        opened_at: 1,
                        class: PARTNER,
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 1,
                    fact: TraceFactKind::LocalEvents,
                },
            ),
        ],
        ProofCategory::StaticCoOccurrenceViolation => vec![
            (
                "one pair left unexamined",
                reclaimed(
                    category,
                    vec![0, 1],
                    ProofClaim::StaticCoOccurrenceViolation {
                        left_unit_index: 0,
                        right_unit_index: 1,
                        left_morpheme: MorphemeId(10),
                        right_morpheme: MorphemeId(20),
                        eliminated_pairs: vec![(AllomorphId(101), AllomorphId(201))],
                    },
                ),
                ProofVerificationError::AlternativesNotExhausted { unit_index: 0 },
            ),
            (
                "a pair nobody proposed",
                reclaimed(
                    category,
                    vec![0, 1],
                    ProofClaim::StaticCoOccurrenceViolation {
                        left_unit_index: 0,
                        right_unit_index: 1,
                        left_morpheme: MorphemeId(10),
                        right_morpheme: MorphemeId(20),
                        eliminated_pairs: vec![
                            (AllomorphId(101), AllomorphId(201)),
                            (AllomorphId(102), AllomorphId(201)),
                            (AllomorphId(999), AllomorphId(201)),
                        ],
                    },
                ),
                ProofVerificationError::AlternativesNotExhausted { unit_index: 0 },
            ),
            (
                "wrong co-occurrence key",
                reclaimed(
                    category,
                    vec![0, 1],
                    ProofClaim::StaticCoOccurrenceViolation {
                        left_unit_index: 0,
                        right_unit_index: 1,
                        left_morpheme: MorphemeId(10),
                        right_morpheme: MorphemeId(99),
                        eliminated_pairs: vec![
                            (AllomorphId(101), AllomorphId(201)),
                            (AllomorphId(102), AllomorphId(201)),
                        ],
                    },
                ),
                ProofVerificationError::MorphemeMismatch { unit_index: 1 },
            ),
        ],
        ProofCategory::NoCompatibleAllomorph => vec![
            (
                "one alternative left standing",
                reclaimed(
                    category,
                    vec![0],
                    ProofClaim::NoCompatibleAllomorph {
                        unit_index: 0,
                        morpheme: MorphemeId(10),
                        eliminated: vec![AllomorphId(101)],
                    },
                ),
                ProofVerificationError::AlternativesNotExhausted { unit_index: 0 },
            ),
            (
                "an alternative nobody proposed",
                reclaimed(
                    category,
                    vec![0],
                    ProofClaim::NoCompatibleAllomorph {
                        unit_index: 0,
                        morpheme: MorphemeId(10),
                        eliminated: vec![AllomorphId(101), AllomorphId(102), AllomorphId(103)],
                    },
                ),
                ProofVerificationError::AlternativesNotExhausted { unit_index: 0 },
            ),
            (
                "wrong morpheme",
                reclaimed(
                    category,
                    vec![0],
                    ProofClaim::NoCompatibleAllomorph {
                        unit_index: 0,
                        morpheme: MorphemeId(99),
                        eliminated: vec![AllomorphId(101), AllomorphId(102)],
                    },
                ),
                ProofVerificationError::MorphemeMismatch { unit_index: 0 },
            ),
        ],
        ProofCategory::StaticSignatureConflict => vec![
            (
                "signature holds for only one allomorph",
                reclaimed(
                    category,
                    vec![0, 2],
                    ProofClaim::StaticSignatureConflict {
                        unit_index: 0,
                        morpheme: MorphemeId(10),
                        eliminated: vec![AllomorphId(101)],
                        conflicting_unit_index: 2,
                        conflicting_morpheme: MorphemeId(30),
                        conflicting_eliminated: vec![AllomorphId(301), AllomorphId(302)],
                    },
                ),
                ProofVerificationError::AlternativesNotExhausted { unit_index: 0 },
            ),
            (
                "conflicting side not exhausted",
                reclaimed(
                    category,
                    vec![0, 2],
                    ProofClaim::StaticSignatureConflict {
                        unit_index: 0,
                        morpheme: MorphemeId(10),
                        eliminated: vec![AllomorphId(101), AllomorphId(102)],
                        conflicting_unit_index: 2,
                        conflicting_morpheme: MorphemeId(30),
                        conflicting_eliminated: vec![AllomorphId(301)],
                    },
                ),
                ProofVerificationError::AlternativesNotExhausted { unit_index: 2 },
            ),
            (
                "wrong conflicting morpheme",
                reclaimed(
                    category,
                    vec![0, 2],
                    ProofClaim::StaticSignatureConflict {
                        unit_index: 0,
                        morpheme: MorphemeId(10),
                        eliminated: vec![AllomorphId(101), AllomorphId(102)],
                        conflicting_unit_index: 2,
                        conflicting_morpheme: MorphemeId(99),
                        conflicting_eliminated: vec![AllomorphId(301), AllomorphId(302)],
                    },
                ),
                ProofVerificationError::MorphemeMismatch { unit_index: 2 },
            ),
        ],
        ProofCategory::ImpossibleSurfaceSpan => vec![
            (
                "span is not the one established",
                reclaimed(
                    category,
                    vec![0, 1],
                    ProofClaim::ImpossibleSurfaceSpan {
                        unit_index: 0,
                        span: SurfaceSpan { start: 0, end: 4 },
                        defect: SpanDefect::OverlapsUnit {
                            other_unit_index: 1,
                        },
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 0,
                    fact: TraceFactKind::SurfaceSpan,
                },
            ),
            (
                "cited units do not overlap",
                reclaimed(
                    category,
                    vec![0, 2],
                    ProofClaim::ImpossibleSurfaceSpan {
                        unit_index: 0,
                        span: SurfaceSpan { start: 0, end: 3 },
                        defect: SpanDefect::OverlapsUnit {
                            other_unit_index: 2,
                        },
                    },
                ),
                ProofVerificationError::SpanDefectNotEstablished { unit_index: 0 },
            ),
            (
                "span is not reversed",
                reclaimed(
                    category,
                    vec![0],
                    ProofClaim::ImpossibleSurfaceSpan {
                        unit_index: 0,
                        span: SurfaceSpan { start: 0, end: 3 },
                        defect: SpanDefect::EndBeforeStart,
                    },
                ),
                ProofVerificationError::SpanDefectNotEstablished { unit_index: 0 },
            ),
        ],
        ProofCategory::ImpossibleLocalEnvironment => vec![
            (
                "local environment mismatch",
                reclaimed(
                    category,
                    vec![0, 1],
                    ProofClaim::ImpossibleLocalEnvironment {
                        unit_index: 0,
                        events: vec![LocalEvent::Neutral],
                        neighbor_unit_index: 1,
                        neighbor_events: vec![LocalEvent::Neutral],
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 0,
                    fact: TraceFactKind::LocalEvents,
                },
            ),
            (
                "neighbour environment mismatch",
                reclaimed(
                    category,
                    vec![0, 1],
                    ProofClaim::ImpossibleLocalEnvironment {
                        unit_index: 0,
                        events: vec![LocalEvent::PartnerOpen(PARTNER)],
                        neighbor_unit_index: 1,
                        neighbor_events: vec![LocalEvent::PartnerClose(PARTNER)],
                    },
                ),
                ProofVerificationError::FactMismatch {
                    unit_index: 1,
                    fact: TraceFactKind::LocalEvents,
                },
            ),
            (
                "neighbour is not adjacent",
                reclaimed(
                    category,
                    vec![0, 2],
                    ProofClaim::ImpossibleLocalEnvironment {
                        unit_index: 0,
                        events: vec![LocalEvent::PartnerOpen(PARTNER)],
                        neighbor_unit_index: 2,
                        neighbor_events: vec![LocalEvent::Neutral],
                    },
                ),
                ProofVerificationError::UnitsNotAdjacent { from: 0, to: 2 },
            ),
        ],
    }
}

/// Which fact a category's proof rests on, and therefore may not be proved without.
fn decisive_fact(category: ProofCategory) -> Option<(usize, TraceFactKind)> {
    match category {
        ProofCategory::MalformedIdentity => None,
        ProofCategory::ImpossibleOwnership => Some((0, TraceFactKind::Role)),
        ProofCategory::ForbiddenTransition => Some((0, TraceFactKind::Slot)),
        ProofCategory::MissingRequiredPartner => Some((0, TraceFactKind::LocalEvents)),
        ProofCategory::StaticCoOccurrenceViolation => Some((0, TraceFactKind::Allomorphs)),
        ProofCategory::NoCompatibleAllomorph => Some((0, TraceFactKind::Allomorphs)),
        ProofCategory::StaticSignatureConflict => Some((0, TraceFactKind::Allomorphs)),
        ProofCategory::ImpossibleSurfaceSpan => Some((0, TraceFactKind::SurfaceSpan)),
        ProofCategory::ImpossibleLocalEnvironment => Some((0, TraceFactKind::LocalEvents)),
    }
}

/// Pairs every forgery test: a verifier that refused everything would pass those on its own.
#[test]
fn every_category_has_a_proof_that_kills_its_witness_and_re_derives() {
    let mut killed = 0;
    for category in ALL_CATEGORIES {
        let run = run(proof_of(category), base_witness());
        assert_eq!(run.retained, 0, "{category} must remove the candidate");
        assert_eq!(run.counters.witnesses_rejected, 1, "{category}");
        assert_eq!(run.verification, Ok(()), "{category}");
        assert!(
            matches!(run.events[0].outcome, PassOutcome::Rejected(_)),
            "{category} produced {:?}",
            run.events[0].outcome
        );
        assert_eq!(run.deaths.len(), 1, "{category}");
        assert_eq!(run.deaths[0].witness_deaths[0].category, category);
        killed += 1;
    }
    assert_eq!(killed, ALL_CATEGORIES.len());
}

#[test]
fn every_generic_forgery_is_caught_after_the_fact() {
    let mut checked = 0;
    for category in ALL_CATEGORIES {
        for (label, forged, expected) in generic_forgeries(category) {
            let run = run(forged, base_witness());
            assert_eq!(refusal(&run), expected, "{category}/{label}");
            checked += 1;
        }
    }
    assert_eq!(checked, 9 * ALL_CATEGORIES.len());
}

#[test]
fn every_payload_forgery_is_caught_after_the_fact() {
    let mut checked = 0;
    for category in ALL_CATEGORIES {
        let forgeries = payload_forgeries(category);
        assert!(!forgeries.is_empty(), "{category} needs payload forgeries");
        for (label, forged, expected) in forgeries {
            let run = run(forged, base_witness());
            assert_eq!(refusal(&run), expected, "{category}/{label}");
            checked += 1;
        }
    }
    assert_eq!(checked, 25);
}

#[test]
fn no_category_may_prove_a_rejection_on_a_fact_the_producer_never_established() {
    let mut checked = 0;
    for category in ALL_CATEGORIES {
        let Some((unit_index, fact)) = decisive_fact(category) else {
            continue;
        };
        let run = run(proof_of(category), opaque_witness());
        assert_eq!(
            refusal(&run),
            ProofVerificationError::FactNotEstablished { unit_index, fact },
            "{category}"
        );
        checked += 1;
    }
    assert_eq!(checked, ALL_CATEGORIES.len() - 1);
}

/// Measures what re-deriving the claim buys: an envelope check passes every one of these.
#[test]
fn every_payload_forgery_survives_a_check_that_skips_the_claim() {
    let mut checked = 0;
    for category in ALL_CATEGORIES {
        for (label, forged, _) in payload_forgeries(category) {
            let run = run(forged, base_witness());
            assert_eq!(run.envelopes_only, Ok(()), "{category}/{label}");
            assert!(run.verification.is_err(), "{category}/{label}");
            checked += 1;
        }
    }
    assert_eq!(checked, 25);
}

#[test]
fn a_deferred_fact_survives_a_check_that_skips_the_claim() {
    let mut checked = 0;
    for category in ALL_CATEGORIES {
        if decisive_fact(category).is_none() {
            continue;
        }
        let run = run(proof_of(category), opaque_witness());
        assert_eq!(run.envelopes_only, Ok(()), "{category}");
        assert!(run.verification.is_err(), "{category}");
        checked += 1;
    }
    assert_eq!(checked, ALL_CATEGORIES.len() - 1);
}

#[test]
fn a_known_absent_slot_is_not_a_slot_a_transition_may_cite() {
    let mut units = base_units();
    units[0].slot = TraceFact::Known(None);

    let run = run(
        proof_of(ProofCategory::ForbiddenTransition),
        witness_with(units),
    );

    assert_eq!(
        refusal(&run),
        ProofVerificationError::FactMismatch {
            unit_index: 0,
            fact: TraceFactKind::Slot,
        }
    );
}

#[test]
fn a_partner_that_is_closed_somewhere_is_not_missing() {
    let mut units = base_units();
    units[2].local_events = TraceFact::Known(vec![LocalEvent::PartnerClose(PARTNER)]);

    let run = run(
        proof_of(ProofCategory::MissingRequiredPartner),
        witness_with(units),
    );

    assert_eq!(
        refusal(&run),
        ProofVerificationError::PartnerAlreadyClosed { unit_index: 2 }
    );
}

#[test]
fn a_partner_absence_cannot_be_proved_past_an_unreadable_unit() {
    let mut units = base_units();
    units[2].local_events = TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit);

    let run = run(
        proof_of(ProofCategory::MissingRequiredPartner),
        witness_with(units),
    );

    assert_eq!(
        refusal(&run),
        ProofVerificationError::FactNotEstablished {
            unit_index: 2,
            fact: TraceFactKind::LocalEvents,
        }
    );
}

#[test]
fn a_signature_conflict_cannot_be_proved_while_a_feature_is_deferred() {
    let mut witness = base_witness();
    witness.deferred = FeatureSet::from_iter([DeferredFeatureId(1)]);

    let run = run(proof_of(ProofCategory::StaticSignatureConflict), witness);

    assert_eq!(
        refusal(&run),
        ProofVerificationError::DeferredFeaturesUnresolved
    );
}

#[test]
fn a_claim_may_only_rest_on_units_the_proof_cites() {
    let mut forged = proof_of(ProofCategory::ImpossibleOwnership);
    forged.witness.unit_indices = Vec::new();

    let run = run(forged, base_witness());

    assert_eq!(
        refusal(&run),
        ProofVerificationError::UnitNotCited { index: 0 }
    );
}

fn a_payload_forgery() -> RejectionProof {
    payload_forgeries(ProofCategory::ImpossibleOwnership)
        .into_iter()
        .next()
        .expect("ownership has payload forgeries")
        .1
}

/// Pins the production guarantee exactly: there is none. Every forgery below kills a real witness.
#[test]
fn production_acts_on_every_forgery_and_only_re_derivation_catches_it() {
    let mut checked = 0;
    for category in ALL_CATEGORIES {
        let forgeries = payload_forgeries(category)
            .into_iter()
            .chain(generic_forgeries(category));
        for (label, forged, _) in forgeries {
            let run = run(forged, base_witness());
            assert_eq!(
                run.retained, 0,
                "{category}/{label}: a rejection is taken at face value and kills its witness \
                 however corrupt its proof is"
            );
            assert_eq!(run.counters.witnesses_rejected, 1, "{category}/{label}");
            assert_eq!(run.deaths.len(), 1, "{category}/{label}");
            assert!(run.verification.is_err(), "{category}/{label}");
            checked += 1;
        }
    }
    assert_eq!(checked, 25 + 9 * ALL_CATEGORIES.len());
}

/// The diagnostic a rejection records, which an offline re-derivation starts from.
#[test]
fn a_rejection_records_what_a_rerun_needs() {
    let run = run(a_payload_forgery(), base_witness());

    assert_eq!(run.deaths.len(), 1);
    let death = &run.deaths[0];
    assert_eq!(death.identity, identity());
    let witness_death = &death.witness_deaths[0];
    assert_eq!(witness_death.witness_id, WitnessId(1));
    assert_eq!(witness_death.pass_id, PASS);
    assert_eq!(witness_death.rule_id, RULE);
    assert_eq!(witness_death.category, ProofCategory::ImpossibleOwnership);
    assert!(matches!(
        &run.events[0].outcome,
        PassOutcome::Rejected(proof) if proof.witness.claim == a_payload_forgery().witness.claim
    ));
}

#[test]
fn a_rule_the_pass_never_declared_is_not_admissible() {
    let mut forged = proof_of(ProofCategory::ImpossibleOwnership);
    forged.rule_id = UNKNOWN_RULE;

    let run = run_pass(Box::new(NarrowPass(forged)), base_witness());

    assert_eq!(run.retained, 0);
    assert_eq!(
        refusal(&run),
        ProofVerificationError::UnrecognizedRule(UNKNOWN_RULE)
    );
}

/// The category is a free field on a proof, so the pass's own catalog is all that binds it.
#[test]
fn a_category_the_pass_never_declared_is_not_admissible() {
    let run = run_pass(
        Box::new(NarrowPass(proof_of(ProofCategory::ForbiddenTransition))),
        base_witness(),
    );

    assert_eq!(run.retained, 0);
    assert_eq!(
        refusal(&run),
        ProofVerificationError::CategoryNotSupported(ProofCategory::ForbiddenTransition)
    );
}

/// What the two structural passes decide, and whether the proofs they emit re-derive.
mod structural {
    use std::sync::Arc;

    use pg_foma::candidate_filter::decision::{
        DeferReason, IdentityDefect, PassDecision, ProofCategory, ProofClaim,
        ProofVerificationError, RejectionProof, StablePassId, TraceFactKind,
    };
    use pg_foma::candidate_filter::index::FilterIndex;
    use pg_foma::candidate_filter::model::{
        CandidateWitness, DeferredFactReason, FeatureSet, LexicalOrigin, NonEmpty,
        ProposalProducer, ProposalProvenance, ProposedCandidate, TraceFact, TraceRole, TraceSlotId,
        TraceStratumId, TraceUnit, WitnessId,
    };
    use pg_foma::candidate_filter::passes::structural::{OwnershipPass, StructuralTransitionPass};
    use pg_foma::candidate_filter::pipeline::{FilterBudget, FilterContext, FilterMode};
    use pg_foma::candidate_filter::report::{BoundedDeathLedger, PassOutcome};
    use pg_foma::candidate_filter::test_support::{
        filter_of, RecordedRejection, RejectionProofVerifier,
    };
    use pg_foma::candidate_filter::CandidateFilterPass;
    use pg_foma::tags::Candidate;
    use pg_grammar::model::{AllomorphId, Grammar, MorphemeId};

    use crate::fixture;
    use crate::sites;

    const NO_ROOT: i32 = -1;

    struct World {
        grammar: Grammar,
        index: Arc<FilterIndex>,
    }

    fn world() -> World {
        let grammar = fixture::grammar();
        let index = Arc::new(FilterIndex::build(&grammar));
        World { grammar, index }
    }

    impl World {
        fn morpheme(&self, xml_key: &str) -> MorphemeId {
            sites::morpheme_of(&self.grammar, xml_key)
        }

        fn unowned(&self) -> MorphemeId {
            fixture::unowned_morpheme(&self.grammar)
        }

        /// The contract slot id of the site listing the rule that owns this element's morpheme.
        fn slot(&self, xml_key: &str) -> TraceSlotId {
            let rule = sites::rule_of(&self.grammar, self.morpheme(xml_key));
            let (template, slot) = sites::site_of(&self.grammar, rule);
            self.index
                .slot_id(template, slot)
                .expect("the fixture template slot is in the index")
        }

        fn stratum(&self, xml_key: &str) -> TraceStratumId {
            let rule = sites::rule_of(&self.grammar, self.morpheme(xml_key));
            let (template, _) = sites::site_of(&self.grammar, rule);
            let stratum = sites::stratum_of_template(&self.grammar, template);
            self.index
                .stratum_id(stratum)
                .expect("the fixture stratum is in the index")
        }
    }

    fn opaque_unit(morpheme: MorphemeId) -> TraceUnit {
        TraceUnit {
            morpheme,
            role: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            allomorphs: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            slot: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            stratum: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            surface_span: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            local_events: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        }
    }

    fn roled_unit(morpheme: MorphemeId, role: TraceRole) -> TraceUnit {
        TraceUnit {
            role: TraceFact::Known(role),
            ..opaque_unit(morpheme)
        }
    }

    fn sited_unit(morpheme: MorphemeId, slot: TraceSlotId, stratum: TraceStratumId) -> TraceUnit {
        TraceUnit {
            slot: TraceFact::Known(Some(slot)),
            stratum: TraceFact::Known(Some(stratum)),
            ..opaque_unit(morpheme)
        }
    }

    fn witness_of(units: Vec<TraceUnit>) -> CandidateWitness {
        CandidateWitness {
            witness_id: WitnessId(1),
            lexical_origin: LexicalOrigin::StaticGrammar,
            lexicon_revision: 0,
            units,
            deferred: FeatureSet::empty(),
            provenance: ProposalProvenance {
                producer: ProposalProducer::SyntheticFixture,
                grammar_revision: 0,
            },
        }
    }

    fn identity_of(witness: &CandidateWitness, root_index: i32) -> Candidate {
        Candidate {
            morphemes: witness.units.iter().map(|unit| unit.morpheme).collect(),
            root_index,
        }
    }

    /// One decision about one witness, with any proof it emitted already re-derived.
    fn decide(
        pass: Box<dyn CandidateFilterPass>,
        identity: &Candidate,
        witness: &CandidateWitness,
    ) -> (PassDecision, Result<(), Vec<ProofVerificationError>>) {
        let passes: Vec<Box<dyn CandidateFilterPass>> = vec![pass];
        let verifier = RejectionProofVerifier::of_passes(&passes);
        let context = FilterContext::new(identity, 0, FilterMode::Enforce);
        let decision = passes[0].evaluate(&context, witness);
        let records: Vec<RecordedRejection<'_>> = match &decision {
            PassDecision::Reject(proof) => vec![RecordedRejection {
                identity,
                witness,
                emitting_pass: passes[0].id(),
                proof,
            }],
            PassDecision::Keep | PassDecision::Defer(_) => Vec::new(),
        };
        let verification = verifier.verify_recorded(&records);
        (decision, verification)
    }

    fn ownership(world: &World) -> Box<dyn CandidateFilterPass> {
        Box::new(OwnershipPass::new(Arc::clone(&world.index)))
    }

    fn transition(world: &World) -> Box<dyn CandidateFilterPass> {
        Box::new(StructuralTransitionPass::new(Arc::clone(&world.index)))
    }

    fn rejected(decision: &PassDecision) -> &RejectionProof {
        match decision {
            PassDecision::Reject(proof) => proof,
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    #[test]
    fn a_root_owned_by_a_lexical_entry_is_kept() {
        let world = world();
        let witness = witness_of(vec![
            roled_unit(world.morpheme("eRoot"), TraceRole::Root),
            roled_unit(world.morpheme("mrP0"), TraceRole::Suffix),
        ]);
        let identity = identity_of(&witness, 0);

        let (decision, verification) = decide(ownership(&world), &identity, &witness);

        assert_eq!(decision, PassDecision::Keep);
        assert_eq!(verification, Ok(()));
    }

    #[test]
    fn extra_lexical_roots_are_kept() {
        let world = world();
        let witness = witness_of(vec![
            roled_unit(world.morpheme("eRoot"), TraceRole::Root),
            roled_unit(world.morpheme("eExtra"), TraceRole::Root),
            roled_unit(world.morpheme("mrP0"), TraceRole::Suffix),
        ]);
        let identity = identity_of(&witness, 0);

        let (decision, _) = decide(ownership(&world), &identity, &witness);

        assert_eq!(decision, PassDecision::Keep);
    }

    #[test]
    fn a_designated_root_owned_by_a_rule_has_a_verified_proof() {
        let world = world();
        let witness = witness_of(vec![
            roled_unit(world.morpheme("mrP0"), TraceRole::Root),
            roled_unit(world.morpheme("eRoot"), TraceRole::Suffix),
        ]);
        let identity = identity_of(&witness, 0);

        let (decision, verification) = decide(ownership(&world), &identity, &witness);

        let proof = rejected(&decision);
        assert_eq!(proof.category, ProofCategory::ImpossibleOwnership);
        assert!(matches!(
            proof.witness.claim,
            ProofClaim::ImpossibleOwnership { unit_index: 0, .. }
        ));
        assert_eq!(verification, Ok(()));
    }

    #[test]
    fn an_unowned_non_root_morpheme_has_a_verified_proof() {
        let world = world();
        let witness = witness_of(vec![
            roled_unit(world.morpheme("eRoot"), TraceRole::Root),
            roled_unit(world.unowned(), TraceRole::Suffix),
        ]);
        let identity = identity_of(&witness, 0);

        let (decision, verification) = decide(ownership(&world), &identity, &witness);

        let proof = rejected(&decision);
        assert_eq!(proof.category, ProofCategory::ImpossibleOwnership);
        assert!(matches!(
            proof.witness.claim,
            ProofClaim::ImpossibleOwnership { unit_index: 1, .. }
        ));
        assert_eq!(verification, Ok(()));
    }

    #[test]
    fn a_root_index_past_the_end_is_a_malformed_identity() {
        let world = world();
        let witness = witness_of(vec![roled_unit(world.morpheme("eRoot"), TraceRole::Root)]);
        let identity = identity_of(&witness, 4);

        let (decision, verification) = decide(ownership(&world), &identity, &witness);

        let proof = rejected(&decision);
        assert_eq!(proof.category, ProofCategory::MalformedIdentity);
        assert_eq!(
            proof.witness.claim,
            ProofClaim::MalformedIdentity(IdentityDefect::RootIndexOutOfRange {
                root_index: 4,
                morphemes: 1,
            })
        );
        assert_eq!(verification, Ok(()));
    }

    #[test]
    fn a_root_index_below_zero_is_a_malformed_identity() {
        let world = world();
        let witness = witness_of(vec![roled_unit(world.morpheme("eRoot"), TraceRole::Root)]);
        let identity = identity_of(&witness, -7);

        let (decision, verification) = decide(ownership(&world), &identity, &witness);

        assert_eq!(
            rejected(&decision).category,
            ProofCategory::MalformedIdentity
        );
        assert_eq!(verification, Ok(()));
    }

    /// `-1` is the producer established "no root at all", which no identity defect describes.
    #[test]
    fn an_absent_root_position_defers() {
        let world = world();
        let witness = witness_of(vec![roled_unit(world.morpheme("mrP0"), TraceRole::Suffix)]);
        let identity = identity_of(&witness, NO_ROOT);

        let (decision, _) = decide(ownership(&world), &identity, &witness);

        assert_eq!(
            decision,
            PassDecision::Defer(DeferReason::UnsupportedConstruct)
        );
    }

    #[test]
    fn an_unreadable_role_defers_instead_of_rejecting() {
        let world = world();
        let witness = witness_of(vec![
            roled_unit(world.morpheme("eRoot"), TraceRole::Root),
            opaque_unit(world.unowned()),
        ]);
        let identity = identity_of(&witness, 0);

        let (decision, _) = decide(ownership(&world), &identity, &witness);

        assert_eq!(
            decision,
            PassDecision::Defer(DeferReason::MissingTraceFact(TraceFactKind::Role))
        );
    }

    #[test]
    fn a_legal_transition_is_kept() {
        let world = world();
        let witness = witness_of(vec![
            sited_unit(
                world.morpheme("mrP0"),
                world.slot("mrP0"),
                world.stratum("mrP0"),
            ),
            sited_unit(
                world.morpheme("mrP1"),
                world.slot("mrP1"),
                world.stratum("mrP1"),
            ),
        ]);
        let identity = identity_of(&witness, NO_ROOT);

        let (decision, _) = decide(transition(&world), &identity, &witness);

        assert_eq!(decision, PassDecision::Keep);
    }

    #[test]
    fn a_slot_that_does_not_list_the_rule_has_a_verified_proof() {
        let world = world();
        let witness = witness_of(vec![
            sited_unit(
                world.morpheme("mrP0"),
                world.slot("mrP0"),
                world.stratum("mrP0"),
            ),
            sited_unit(
                world.morpheme("mrP1"),
                world.slot("mrP0"),
                world.stratum("mrP0"),
            ),
        ]);
        let identity = identity_of(&witness, NO_ROOT);

        let (decision, verification) = decide(transition(&world), &identity, &witness);

        let proof = rejected(&decision);
        assert_eq!(proof.category, ProofCategory::ForbiddenTransition);
        assert_eq!(proof.witness.unit_indices, vec![0, 1]);
        assert_eq!(verification, Ok(()));
    }

    #[test]
    fn a_step_across_two_strata_defers() {
        let world = world();
        let witness = witness_of(vec![
            sited_unit(
                world.morpheme("mrP0"),
                world.slot("mrP0"),
                world.stratum("mrP0"),
            ),
            sited_unit(
                world.morpheme("mrQ"),
                world.slot("mrQ"),
                world.stratum("mrQ"),
            ),
        ]);
        let identity = identity_of(&witness, NO_ROOT);

        let (decision, _) = decide(transition(&world), &identity, &witness);

        assert_eq!(
            decision,
            PassDecision::Defer(DeferReason::UnsupportedConstruct)
        );
    }

    #[test]
    fn a_loose_rule_outside_every_template_defers() {
        let world = world();
        let stratum = world.stratum("mrP0");
        let mut units = vec![
            sited_unit(world.morpheme("mrP0"), world.slot("mrP0"), stratum),
            sited_unit(world.morpheme("mrLoose"), world.slot("mrP1"), stratum),
        ];
        units[1].slot = TraceFact::Known(None);

        let witness = witness_of(units);
        let identity = identity_of(&witness, NO_ROOT);

        let (decision, _) = decide(transition(&world), &identity, &witness);

        assert_eq!(
            decision,
            PassDecision::Defer(DeferReason::UnsupportedConstruct)
        );
    }

    #[test]
    fn unknown_transition_metadata_defers() {
        let world = world();
        let witness = witness_of(vec![
            opaque_unit(world.morpheme("mrP0")),
            opaque_unit(world.morpheme("mrP1")),
        ]);
        let identity = identity_of(&witness, NO_ROOT);

        let (decision, _) = decide(transition(&world), &identity, &witness);

        assert_eq!(
            decision,
            PassDecision::Defer(DeferReason::MissingTraceFact(TraceFactKind::Slot))
        );
    }

    #[test]
    fn a_known_slot_without_a_stratum_defers() {
        let world = world();
        let mut units = vec![
            sited_unit(
                world.morpheme("mrP0"),
                world.slot("mrP0"),
                world.stratum("mrP0"),
            ),
            sited_unit(
                world.morpheme("mrP1"),
                world.slot("mrP1"),
                world.stratum("mrP1"),
            ),
        ];
        units[1].stratum = TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit);

        let witness = witness_of(units);
        let identity = identity_of(&witness, NO_ROOT);

        let (decision, _) = decide(transition(&world), &identity, &witness);

        assert_eq!(
            decision,
            PassDecision::Defer(DeferReason::MissingTraceFact(TraceFactKind::Stratum))
        );
    }

    /// A pass that cannot decide at all, for the guard around pass evaluation.
    struct PanickingPass;

    impl CandidateFilterPass for PanickingPass {
        fn id(&self) -> StablePassId {
            StablePassId("test.panicking.v1")
        }

        fn evaluate(
            &self,
            _context: &FilterContext<'_>,
            _witness: &CandidateWitness,
        ) -> PassDecision {
            panic!("a pass that cannot decide");
        }
    }

    #[test]
    fn a_panicking_pass_retains_the_witness_and_is_counted() {
        let world = world();
        let witness = witness_of(vec![roled_unit(world.morpheme("eRoot"), TraceRole::Root)]);
        let identity = identity_of(&witness, 0);
        let inputs = vec![ProposedCandidate::new(identity, vec![witness]).expect("one witness")];

        let filter = filter_of(vec![Box::new(PanickingPass)]);
        let mut retained: Vec<ProposedCandidate> = Vec::new();
        let mut ledger = BoundedDeathLedger::unlimited();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        filter.filter_into(
            FilterMode::Enforce,
            inputs,
            &mut retained,
            &mut ledger,
            FilterBudget::unlimited(),
        );
        std::panic::set_hook(previous);

        assert_eq!(retained.len(), 1);
        assert_eq!(ledger.counters().panics, 1);
        assert_eq!(ledger.counters().witnesses_rejected, 0);
        assert!(matches!(ledger.events()[0].outcome, PassOutcome::Panicked));
    }

    /// The allomorph fact neither structural pass reads leaves both decisions unchanged.
    #[test]
    fn neither_structural_pass_reads_an_allomorph_choice() {
        let world = world();
        let mut units = vec![
            roled_unit(world.morpheme("eRoot"), TraceRole::Root),
            roled_unit(world.morpheme("mrP0"), TraceRole::Suffix),
        ];
        units[1].allomorphs =
            TraceFact::Known(NonEmpty::try_from_vec(vec![AllomorphId(0)]).expect("one allomorph"));

        let witness = witness_of(units);
        let identity = identity_of(&witness, 0);

        assert_eq!(
            decide(ownership(&world), &identity, &witness).0,
            PassDecision::Keep
        );
        assert_eq!(
            decide(transition(&world), &identity, &witness).0,
            PassDecision::Defer(DeferReason::MissingTraceFact(TraceFactKind::Slot))
        );
    }
}
