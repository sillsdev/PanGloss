//! Turning today's proposer output into the witness the filter contract reads.
//!
//! The existing proposer returns a bare [`Candidate`]: a morpheme sequence and a root position,
//! and nothing else. Everything else the contract's trace model can carry — role, allomorph set,
//! slot, stratum, surface span, local events — it simply does not establish, so every one of those
//! is [`TraceFact::Deferred`] here.
//!
//! Nothing is inferred and no sentinel stands in for an unknown. The root position is not a role
//! claim about a unit: it is the identity's own field, carried across unchanged, and a unit's role
//! stays deferred even at that index. Guessing here would be worse than useless — a pass may
//! reject on an established fact, so a fabricated one is a licence to delete an analysis HC would
//! have confirmed, which is the single error a recall-preserving filter may not make.
//!
//! The consequence is deliberate and worth stating plainly: a pass that needs any of the deferred
//! facts cannot reach a rejection through this adapter at all. It will defer, every time, until a
//! producer exists that establishes those facts.

use crate::candidate_filter::model::{
    CandidateWitness, DeferredFactReason, FeatureSet, LexicalOrigin, ProposalProducer,
    ProposalProvenance, ProposedCandidate, TraceFact, TraceUnit, WitnessId,
};
use crate::tags::Candidate;

/// The witness id every legacy proposal carries, since the proposer offers exactly one route.
const ONLY_WITNESS: WitnessId = WitnessId(1);

/// The reason every deferred fact carries: the producer emits nothing to establish it.
const REASON: DeferredFactReason = DeferredFactReason::ProducerDoesNotEmit;

/// One proposal per candidate, in input order, each with the single witness the proposer supports.
pub fn witnesses_for(
    candidates: &[Candidate],
    grammar_revision: u64,
    lexicon_revision: u64,
) -> Vec<ProposedCandidate> {
    candidates
        .iter()
        .map(|candidate| witness_for(candidate, grammar_revision, lexicon_revision))
        .collect()
}

/// Wraps one candidate identity in the one witness today's proposer can honestly claim.
pub fn witness_for(
    candidate: &Candidate,
    grammar_revision: u64,
    lexicon_revision: u64,
) -> ProposedCandidate {
    let units = candidate
        .morphemes
        .iter()
        .map(|&morpheme| TraceUnit {
            morpheme,
            role: TraceFact::Deferred(REASON),
            allomorphs: TraceFact::Deferred(REASON),
            slot: TraceFact::Deferred(REASON),
            stratum: TraceFact::Deferred(REASON),
            surface_span: TraceFact::Deferred(REASON),
            local_events: TraceFact::Deferred(REASON),
        })
        .collect();
    let witness = CandidateWitness {
        witness_id: ONLY_WITNESS,
        lexical_origin: LexicalOrigin::StaticGrammar,
        lexicon_revision,
        units,
        deferred: FeatureSet::empty(),
        provenance: ProposalProvenance {
            producer: ProposalProducer::LegacyProposer,
            grammar_revision,
        },
    };
    ProposedCandidate::new(candidate.clone(), vec![witness])
        .expect("one witness is neither empty nor a duplicate")
}
