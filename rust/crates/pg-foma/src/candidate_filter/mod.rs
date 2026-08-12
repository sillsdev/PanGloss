//! Sound, recall-preserving rejection of proposed candidates before HC confirmation.
//!
//! The filter is deliberately incomplete: it may keep a candidate HC will later reject, but it may
//! never reject one HC would accept. Everything a caller can express here follows from that
//! asymmetry — unknown, unsupported, or unexhausted facts keep a candidate, and only an
//! established fact can ever contribute to a rejection.
//!
//! This module owns the input contract a candidate producer must satisfy: `model` states what a
//! proposal is and, just as importantly, how a producer says it does not know something.

pub mod model;

pub use model::{
    CandidateWitness, DeferredFactReason, DeferredFeatureId, FeatureSet, LexicalOrigin, LocalEvent,
    NonEmpty, NonEmptyError, PartnerClassId, ProposalModelError, ProposalProducer,
    ProposalProvenance, ProposedCandidate, SurfaceSpan, TraceFact, TraceRole, TraceSlotId,
    TraceStratumId, TraceUnit, WitnessId,
};
