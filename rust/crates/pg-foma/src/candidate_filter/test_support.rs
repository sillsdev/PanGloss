//! Filter construction for this crate's own contract tests, behind the `test-support` feature.
//!
//! Rejection authority is otherwise crate-private, and a contract test lives in its own crate, so
//! without a seam here the pipeline's semantics could only be exercised from inside this module
//! tree. The seam is genuinely unsafe to publish: a caller who can build a filter supplies both
//! the pass and the proof payload, and every field an allow-list verifier re-checks is copyable
//! off the witness, so a pass that rejects everything with a proof it wrote itself passes
//! verification and enforcement deletes real analyses. The feature gate, not the verifier, is what
//! keeps that out of a normal build.

use std::collections::BTreeSet;

use crate::candidate_filter::decision::{
    ProofCategory, ProofVerificationError, RejectionProof, StablePassId, StableRuleId,
};
use crate::candidate_filter::model::{CandidateWitness, ProposedCandidate};
use crate::candidate_filter::passes::CandidateFilterPass;
use crate::candidate_filter::pipeline::{
    CandidateFilter, FilterBudget, FilterCompletion, FilterContext, FilterMode, OrdinalSeed,
    ProofCheckDepth, ProofVerifier,
};
use crate::candidate_filter::report::{FilterTraceSink, RetainedCandidateSink};

/// One `(pass, rule, category)` triple an allow-list filter will admit rejections for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AllowedProof {
    pub pass_id: StablePassId,
    pub rule_id: StableRuleId,
    pub category: ProofCategory,
}

struct AllowListProofVerifier {
    admissible: BTreeSet<(StablePassId, StableRuleId, ProofCategory)>,
}

impl ProofVerifier for AllowListProofVerifier {
    fn verify(
        &self,
        context: &FilterContext<'_>,
        witness: &CandidateWitness,
        proof: &RejectionProof,
    ) -> Result<(), ProofVerificationError> {
        if &proof.witness.candidate_identity != context.identity() {
            return Err(ProofVerificationError::CandidateIdentityMismatch);
        }
        if proof.witness.witness_id != witness.witness_id {
            return Err(ProofVerificationError::WitnessIdMismatch);
        }
        if proof.witness.grammar_revision != witness.provenance.grammar_revision {
            return Err(ProofVerificationError::GrammarRevisionMismatch);
        }
        if proof.witness.lexicon_revision != witness.lexicon_revision {
            return Err(ProofVerificationError::LexiconRevisionMismatch);
        }
        for &index in &proof.witness.unit_indices {
            if index >= witness.units.len() {
                return Err(ProofVerificationError::UnitIndexOutOfRange {
                    index,
                    units: witness.units.len(),
                });
            }
        }
        if !self
            .admissible
            .contains(&(proof.pass_id, proof.rule_id, proof.category))
        {
            return Err(ProofVerificationError::UnrecognizedRule(proof.rule_id));
        }
        Ok(())
    }
}

/// Builds a filter that admits a rejection only for an enumerated triple, and only when the proof
/// re-establishes against the witness in front of it.
pub fn allow_list_filter(
    passes: Vec<Box<dyn CandidateFilterPass>>,
    allowed: Vec<AllowedProof>,
) -> CandidateFilter {
    let admissible = allowed
        .into_iter()
        .map(|entry| (entry.pass_id, entry.rule_id, entry.category))
        .collect();
    CandidateFilter::new(passes, Box::new(AllowListProofVerifier { admissible }))
}

/// Builds a filter guarded by the production proof verifier, exactly as a profile would.
pub fn verified_filter(passes: Vec<Box<dyn CandidateFilterPass>>) -> CandidateFilter {
    CandidateFilter::verifying(passes)
}

/// Runs a filter with its diagnostic ordinals already advanced, to reach saturation in one step.
#[allow(clippy::too_many_arguments)]
pub fn filter_into_from_ordinals<I, R, T>(
    filter: &CandidateFilter,
    mode: FilterMode,
    input: I,
    retained: &mut R,
    trace: &mut T,
    budget: FilterBudget,
    next_event: u64,
    next_candidate: u64,
) -> FilterCompletion
where
    I: IntoIterator<Item = ProposedCandidate>,
    R: RetainedCandidateSink,
    T: FilterTraceSink,
{
    filter.filter_into_seeded(
        mode,
        ProofCheckDepth::for_mode(mode),
        input,
        retained,
        trace,
        budget,
        OrdinalSeed {
            next_event,
            next_candidate,
        },
    )
}
