//! What a pass may conclude about one witness, and the closed vocabulary a rejection speaks.
//!
//! A pass never removes anything itself. It returns a decision, and only a `Reject` carrying a
//! proof that an independent verifier re-establishes against the witness can end that witness.
//! `Keep` and `Defer` are both retention and differ only in what a report may say about why the
//! witness survived, so a pass that is unsure has no way to express anything but survival.
//!
//! The proof categories are closed and versioned on purpose: enforcement is only ever permitted
//! for a category a verifier knows how to re-derive, and an open-ended category would let a pass
//! assert a kind of impossibility nothing checks.

use std::fmt;

use crate::candidate_filter::model::WitnessId;
use crate::tags::Candidate;

/// A pass's stable identity, which reports, allow lists, and counters all key on.
///
/// It is a fixed string rather than an ordinal so that adding, reordering, or retiring a pass
/// cannot silently re-point historical evidence at a different pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StablePassId(pub &'static str);

impl StablePassId {
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for StablePassId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// The stable identity of the individual rule a rejection rests on.
///
/// `family` names the rule population (a static pass-owned family, or a grammar table) and
/// `ordinal` selects within it, so a proof stays attributable when a grammar is recompiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableRuleId {
    pub family: &'static str,
    pub ordinal: u32,
}

impl fmt::Display for StableRuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.family, self.ordinal)
    }
}

/// The closed set of impossibilities a rejection may claim.
///
/// Each variant names a fact that is decidable from established trace facts alone. Anything whose
/// abstraction loses information a decision would need — long-range phonology, uncertain harmony,
/// unbounded copying — has no variant here and therefore no way to reject.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProofCategory {
    MalformedIdentity,
    ImpossibleOwnership,
    ForbiddenTransition,
    MissingRequiredPartner,
    StaticCoOccurrenceViolation,
    NoCompatibleAllomorph,
    StaticSignatureConflict,
    ImpossibleSurfaceSpan,
    ImpossibleLocalEnvironment,
}

impl ProofCategory {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::MalformedIdentity => "malformed_identity",
            Self::ImpossibleOwnership => "impossible_ownership",
            Self::ForbiddenTransition => "forbidden_transition",
            Self::MissingRequiredPartner => "missing_required_partner",
            Self::StaticCoOccurrenceViolation => "static_co_occurrence_violation",
            Self::NoCompatibleAllomorph => "no_compatible_allomorph",
            Self::StaticSignatureConflict => "static_signature_conflict",
            Self::ImpossibleSurfaceSpan => "impossible_surface_span",
            Self::ImpossibleLocalEnvironment => "impossible_local_environment",
        }
    }
}

impl fmt::Display for ProofCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The machine-checkable body of a rejection: what it was proved against, and where.
///
/// The identity, witness, and both revisions are carried so a verifier can re-establish that the
/// proof was built against the very witness now being enforced rather than a sibling of it or an
/// earlier state of the grammar or runtime lexicon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofWitness {
    pub candidate_identity: Candidate,
    pub witness_id: WitnessId,
    pub grammar_revision: u64,
    pub lexicon_revision: u64,
    pub unit_indices: Vec<usize>,
}

/// A pass's claim that every concrete realization of one witness is impossible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectionProof {
    pub pass_id: StablePassId,
    pub rule_id: StableRuleId,
    pub category: ProofCategory,
    pub witness: ProofWitness,
}

/// Which trace fact a pass needed and did not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TraceFactKind {
    Role,
    Allomorphs,
    Slot,
    Stratum,
    SurfaceSpan,
    LocalEvents,
}

/// Why a pass declined to decide.
///
/// Every variant is a retention. They are distinguished only so a report can say whether the
/// producer withheld a fact, the construct is outside what the pass can decide, an ambiguity could
/// not be exhausted, or a rejection was claimed and then failed verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeferReason {
    MissingTraceFact(TraceFactKind),
    UnsupportedConstruct,
    AmbiguityNotExhausted,
    ProofVerificationFailed(ProofVerificationError),
}

/// Why a claimed rejection was not admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofVerificationError {
    PassIdMismatch {
        declared: StablePassId,
        claimed: StablePassId,
    },
    CandidateIdentityMismatch,
    WitnessIdMismatch,
    GrammarRevisionMismatch,
    LexiconRevisionMismatch,
    UnitIndexOutOfRange {
        index: usize,
        units: usize,
    },
    UnrecognizedRule(StableRuleId),
    CategoryNotSupported(ProofCategory),
}

impl fmt::Display for ProofVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PassIdMismatch { declared, claimed } => {
                write!(f, "pass {declared} claimed a proof stamped {claimed}")
            }
            Self::CandidateIdentityMismatch => write!(f, "proof names a different candidate"),
            Self::WitnessIdMismatch => write!(f, "proof names a different witness"),
            Self::GrammarRevisionMismatch => {
                write!(f, "proof was built at another grammar revision")
            }
            Self::LexiconRevisionMismatch => {
                write!(f, "proof was built at another lexicon revision")
            }
            Self::UnitIndexOutOfRange { index, units } => {
                write!(f, "proof cites trace unit {index} of {units}")
            }
            Self::UnrecognizedRule(rule) => write!(f, "rule {rule} is not admissible here"),
            Self::CategoryNotSupported(category) => {
                write!(f, "category {category} is not verified")
            }
        }
    }
}

impl std::error::Error for ProofVerificationError {}

/// The one thing a pass returns about one witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassDecision {
    Keep,
    Reject(RejectionProof),
    Defer(DeferReason),
}
