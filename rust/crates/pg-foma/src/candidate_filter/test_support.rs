//! A filter whose enforcement authority is an explicit allow list, for contract tests.
//!
//! Rejection authority is otherwise crate-private, and a contract test lives in its own crate, so
//! without a seam here the pipeline's semantics could only be exercised from inside this module
//! tree. What is exposed is deliberately not the ability to supply a verifier: the verifier type
//! is fixed, and a caller may only enumerate which `(pass, rule, category)` triples it will admit.
//! Everything else it checks — identity, witness, both revisions, and cited trace units — is not
//! negotiable from outside, so no caller can widen a proof into one that admits an arbitrary
//! claim.

use std::collections::BTreeSet;

use crate::candidate_filter::decision::{
    ProofCategory, ProofVerificationError, RejectionProof, StablePassId, StableRuleId,
};
use crate::candidate_filter::model::CandidateWitness;
use crate::candidate_filter::passes::CandidateFilterPass;
use crate::candidate_filter::pipeline::{CandidateFilter, FilterContext, ProofVerifier};

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
