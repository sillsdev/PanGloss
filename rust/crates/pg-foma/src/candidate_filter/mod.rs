//! Sound, recall-preserving rejection of proposed candidates before HC confirmation.
//!
//! The filter is deliberately incomplete: it may keep a candidate HC will later reject, but it may
//! never reject one HC would accept. Everything a caller can express here follows from that
//! asymmetry — unknown, unsupported, or unexhausted facts keep a candidate, and only an
//! established fact can ever contribute to a rejection.
//!
//! This module owns the input contract a candidate producer must satisfy: `model` states what a
//! proposal is and, just as importantly, how a producer says it does not know something.
//! `decision` states the closed vocabulary a rejection may speak, `passes` the seam that speaks
//! it, `pipeline` the traversal that turns those decisions into survival, and `report` where the
//! survivors and the audit trail go.

pub mod decision;
pub mod index;
pub mod legacy;
pub mod model;
pub mod passes;
pub mod pipeline;
pub mod report;
pub mod shadow;
#[cfg(feature = "test-support")]
pub mod test_support;

pub use decision::{
    AdmissibleProof, DeferReason, IdentityDefect, PassDecision, ProofCategory, ProofClaim,
    ProofVerificationError, ProofWitness, RejectionProof, SpanDefect, StablePassId, StableRuleId,
    TraceFactKind,
};
pub use index::{FilterIndex, RuleShape, SiteVerdict};
pub use legacy::{witness_for, witnesses_for};
pub use passes::structural::{OwnershipPass, StructuralTransitionPass};
pub use passes::surface_consistency::{
    SurfaceConsistencyIndex, SurfaceConsistencyPass, SurfaceVerdict,
};
pub use passes::CandidateFilterPass;
pub use pipeline::{
    CandidateFilter, FilterBudget, FilterCompletion, FilterContext, FilterMode, FilterOutcome,
    FilterStopReason,
};
pub use report::{
    BoundedDeathLedger, CandidateDeath, CountingTraceSink, FilterCounters, FilterTraceSink,
    LedgerCaps, PassCounters, PassEvent, PassOutcome, RetainedCandidateSink, WitnessDeath,
};
pub use shadow::{CandidateFilterSettings, FilterShadowReport, ShadowCostAttribution};

pub use model::{
    CandidateWitness, DeferredFactReason, DeferredFeatureId, FeatureSet, LexicalOrigin, LocalEvent,
    NonEmpty, NonEmptyError, PartnerClassId, ProposalModelError, ProposalProducer,
    ProposalProvenance, ProposedCandidate, SurfaceSpan, TraceFact, TraceRole, TraceSlotId,
    TraceStratumId, TraceUnit, WitnessId,
};
