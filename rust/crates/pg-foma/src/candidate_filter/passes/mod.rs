//! The seam every filter pass implements, however it is built.
//!
//! A pass sees one witness and the immutable context of the candidate that witness proposes, and
//! returns a decision. It gets no ability to remove anything and no view of the other witnesses:
//! deciding whether a candidate survives is the pipeline's job precisely because that decision
//! needs all of them, and a pass that could see its siblings could kill a candidate on the
//! strength of one route being impossible.
//!
//! `Send + Sync` because a pass is shared, immutable grammar-derived state that several words may
//! be filtered against at once; per-witness working state belongs in the method body.

pub mod structural;

use crate::candidate_filter::decision::{AdmissibleProof, PassDecision, StablePassId};
use crate::candidate_filter::model::CandidateWitness;
use crate::candidate_filter::pipeline::FilterContext;

pub trait CandidateFilterPass: Send + Sync {
    fn id(&self) -> StablePassId;

    /// Every `(rule, category)` pair this pass may claim a rejection under.
    ///
    /// This is the pass's only independent statement of what it decides: a rejection carries the
    /// rule and category as free fields, so re-deriving one afterwards has nothing else to hold it
    /// to. A pass that leaves this empty can therefore emit no proof that re-derives.
    fn admissible_proofs(&self) -> Vec<AdmissibleProof> {
        Vec::new()
    }

    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision;
}
