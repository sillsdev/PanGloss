//! Filter construction and post-hoc proof re-derivation for this crate's own contract tests,
//! behind the `test-support` feature.
//!
//! Rejection authority is otherwise crate-private, and a contract test lives in its own crate, so
//! without a seam here the pipeline's semantics could only be exercised from inside this module
//! tree. The seam is genuinely unsafe to publish: a caller who can build a filter supplies the
//! passes, and a pass that rejects every witness deletes real analyses. The feature gate is what
//! keeps that out of a normal build.
//!
//! Proof re-derivation lives here rather than beside the pipeline for the same reason it is no
//! longer inline: a run acts on a rejection without checking it, so the checker belongs with the
//! tests that do the checking. It sits behind the gate as one copy rather than one per test
//! binary, which is what keeps two test binaries checking the same thing.

pub mod proof;

use crate::candidate_filter::model::{CandidateWitness, ProposedCandidate, WitnessId};
use crate::candidate_filter::passes::CandidateFilterPass;
use crate::candidate_filter::pipeline::{
    CandidateFilter, FilterBudget, FilterCompletion, FilterMode, OrdinalSeed,
};
use crate::candidate_filter::report::{
    BoundedDeathLedger, FilterTraceSink, PassOutcome, RetainedCandidateSink,
};

pub use proof::{RecordedRejection, RejectionProofVerifier};

/// Builds a filter over an ordered pass list, exactly as a profile would.
pub fn filter_of(passes: Vec<Box<dyn CandidateFilterPass>>) -> CandidateFilter {
    CandidateFilter::new(passes)
}

/// Every rejection a ledgered run recorded, joined to the witness it was emitted for.
///
/// The join key is the candidate ordinal, which a run assigns in input order, so `inputs` must be
/// the same sequence the run consumed and must have started at ordinal zero.
pub fn recorded_rejections<'a>(
    inputs: &'a [ProposedCandidate],
    ledger: &'a BoundedDeathLedger,
) -> Vec<RecordedRejection<'a>> {
    ledger
        .events()
        .iter()
        .filter_map(|event| {
            let PassOutcome::Rejected(proof) = &event.outcome else {
                return None;
            };
            let candidate = &inputs[usize::try_from(event.candidate_ordinal)
                .expect("a recorded candidate ordinal indexes the run's own input")];
            Some(RecordedRejection {
                identity: &candidate.identity,
                witness: witness_of(candidate, event.witness_id),
                emitting_pass: event.pass_id,
                proof,
            })
        })
        .collect()
}

fn witness_of(candidate: &ProposedCandidate, witness_id: WitnessId) -> &CandidateWitness {
    candidate
        .witnesses
        .iter()
        .find(|witness| witness.witness_id == witness_id)
        .expect("a recorded witness id belongs to the candidate it was recorded under")
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
