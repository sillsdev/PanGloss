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

use pg_grammar::model::{AllomorphId, MorphemeId};

use crate::candidate_filter::model::{
    LexicalOrigin, LocalEvent, PartnerClassId, SurfaceSpan, TraceRole, TraceSlotId, TraceStratumId,
    WitnessId,
};
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

/// A `(rule, category)` pair a pass is entitled to claim a rejection under.
///
/// A pass declares its own rule population, and a proof naming anything outside it is refused. The
/// pair is the unit rather than the rule alone because a rule that decides one kind of
/// impossibility says nothing about another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdmissibleProof {
    pub rule_id: StableRuleId,
    pub category: ProofCategory,
}

/// How a candidate identity fails to be a possible analysis at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdentityDefect {
    EmptyMorphemeSequence,
    /// A root position that is neither "no root" nor an index into the morpheme sequence.
    RootIndexOutOfRange {
        root_index: i32,
        morphemes: usize,
    },
}

/// How a unit's established surface span fails to be realizable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpanDefect {
    EndBeforeStart,
    OverlapsUnit { other_unit_index: usize },
}

/// The category-specific body of a rejection: everything a verifier needs to re-derive the claim
/// from the witness rather than take the pass's word for it.
///
/// Each variant names the trace units it rests on and restates the facts it read from them, so a
/// verifier can compare the claim against the witness in front of it. A claim that eliminates
/// alternatives carries all of them: exhaustion is the difference between "this reading is
/// impossible" and "every reading is impossible", and only the latter may end a witness.
///
/// How completely a claim can be re-derived differs by category. Identity, ownership, transition,
/// partner, span, and local-environment claims are settled by the trace alone. A co-occurrence or
/// signature conflict is a fact about grammar tables, so re-deriving one needs a grammar-fact
/// index as well; what the trace settles for those is that the cited units are the morphemes
/// named, that every allomorph alternative was examined, and — for a signature — that no deferred
/// feature is left to resolve the conflict later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProofClaim {
    MalformedIdentity(IdentityDefect),
    ImpossibleOwnership {
        unit_index: usize,
        morpheme: MorphemeId,
        role: TraceRole,
    },
    ForbiddenTransition {
        from_unit_index: usize,
        to_unit_index: usize,
        from_slot: TraceSlotId,
        to_slot: TraceSlotId,
        stratum: TraceStratumId,
    },
    MissingRequiredPartner {
        opened_at: usize,
        class: PartnerClassId,
    },
    StaticCoOccurrenceViolation {
        left_unit_index: usize,
        right_unit_index: usize,
        left_morpheme: MorphemeId,
        right_morpheme: MorphemeId,
        eliminated_pairs: Vec<(AllomorphId, AllomorphId)>,
    },
    NoCompatibleAllomorph {
        unit_index: usize,
        morpheme: MorphemeId,
        eliminated: Vec<AllomorphId>,
    },
    StaticSignatureConflict {
        unit_index: usize,
        morpheme: MorphemeId,
        eliminated: Vec<AllomorphId>,
        conflicting_unit_index: usize,
        conflicting_morpheme: MorphemeId,
        conflicting_eliminated: Vec<AllomorphId>,
    },
    ImpossibleSurfaceSpan {
        unit_index: usize,
        span: SurfaceSpan,
        defect: SpanDefect,
    },
    ImpossibleLocalEnvironment {
        unit_index: usize,
        events: Vec<LocalEvent>,
        neighbor_unit_index: usize,
        neighbor_events: Vec<LocalEvent>,
    },
}

impl ProofClaim {
    /// The category this claim is, as opposed to the one its proof declares it to be.
    pub fn category(&self) -> ProofCategory {
        match self {
            Self::MalformedIdentity(_) => ProofCategory::MalformedIdentity,
            Self::ImpossibleOwnership { .. } => ProofCategory::ImpossibleOwnership,
            Self::ForbiddenTransition { .. } => ProofCategory::ForbiddenTransition,
            Self::MissingRequiredPartner { .. } => ProofCategory::MissingRequiredPartner,
            Self::StaticCoOccurrenceViolation { .. } => ProofCategory::StaticCoOccurrenceViolation,
            Self::NoCompatibleAllomorph { .. } => ProofCategory::NoCompatibleAllomorph,
            Self::StaticSignatureConflict { .. } => ProofCategory::StaticSignatureConflict,
            Self::ImpossibleSurfaceSpan { .. } => ProofCategory::ImpossibleSurfaceSpan,
            Self::ImpossibleLocalEnvironment { .. } => ProofCategory::ImpossibleLocalEnvironment,
        }
    }

    /// Every trace unit this claim reads, each of which the proof must also cite.
    pub fn cited_units(&self) -> Vec<usize> {
        match self {
            Self::MalformedIdentity(_) => Vec::new(),
            Self::ImpossibleOwnership { unit_index, .. }
            | Self::NoCompatibleAllomorph { unit_index, .. } => vec![*unit_index],
            Self::MissingRequiredPartner { opened_at, .. } => vec![*opened_at],
            Self::ForbiddenTransition {
                from_unit_index,
                to_unit_index,
                ..
            } => vec![*from_unit_index, *to_unit_index],
            Self::StaticCoOccurrenceViolation {
                left_unit_index,
                right_unit_index,
                ..
            } => vec![*left_unit_index, *right_unit_index],
            Self::StaticSignatureConflict {
                unit_index,
                conflicting_unit_index,
                ..
            } => vec![*unit_index, *conflicting_unit_index],
            Self::ImpossibleLocalEnvironment {
                unit_index,
                neighbor_unit_index,
                ..
            } => vec![*unit_index, *neighbor_unit_index],
            Self::ImpossibleSurfaceSpan {
                unit_index, defect, ..
            } => match defect {
                SpanDefect::EndBeforeStart => vec![*unit_index],
                SpanDefect::OverlapsUnit { other_unit_index } => {
                    vec![*unit_index, *other_unit_index]
                }
            },
        }
    }
}

/// The machine-checkable body of a rejection: what it was proved against, and where.
///
/// The identity, witness, both revisions, and the lexical origin are carried so a verifier can
/// re-establish that the proof was built against the very witness now being enforced rather than a
/// sibling of it, an earlier state of the grammar, or another runtime stem population. The claim
/// carries the rest; `unit_indices` is the proof's own statement of which units it read, and a
/// claim may not rest on a unit it did not cite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofWitness {
    pub candidate_identity: Candidate,
    pub witness_id: WitnessId,
    pub grammar_revision: u64,
    pub lexicon_revision: u64,
    pub lexical_origin: LexicalOrigin,
    pub unit_indices: Vec<usize>,
    pub claim: ProofClaim,
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
///
/// Every variant is a retention. They are distinguished so a report can say whether the proof was
/// built against something other than this witness, cited a rule the pass never declared, rested
/// on a fact the producer never established, or stated an impossibility the witness contradicts.
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
    LexicalOriginMismatch,
    UnitIndexOutOfRange {
        index: usize,
        units: usize,
    },
    UnitNotCited {
        index: usize,
    },
    UnrecognizedRule(StableRuleId),
    CategoryNotSupported(ProofCategory),
    CategoryClaimMismatch {
        declared: ProofCategory,
        claimed: ProofCategory,
    },
    MorphemeMismatch {
        unit_index: usize,
    },
    FactNotEstablished {
        unit_index: usize,
        fact: TraceFactKind,
    },
    FactMismatch {
        unit_index: usize,
        fact: TraceFactKind,
    },
    AlternativesNotExhausted {
        unit_index: usize,
    },
    DeferredFeaturesUnresolved,
    IdentityDefectNotEstablished,
    UnitsNotAdjacent {
        from: usize,
        to: usize,
    },
    PartnerAlreadyClosed {
        unit_index: usize,
    },
    SpanDefectNotEstablished {
        unit_index: usize,
    },
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
            Self::LexicalOriginMismatch => {
                write!(f, "proof was built against other lexical material")
            }
            Self::UnitIndexOutOfRange { index, units } => {
                write!(f, "proof cites trace unit {index} of {units}")
            }
            Self::UnitNotCited { index } => {
                write!(f, "claim reads trace unit {index} without citing it")
            }
            Self::UnrecognizedRule(rule) => write!(f, "rule {rule} is not admissible here"),
            Self::CategoryNotSupported(category) => {
                write!(f, "category {category} is not verified")
            }
            Self::CategoryClaimMismatch { declared, claimed } => {
                write!(f, "proof declares {declared} and claims {claimed}")
            }
            Self::MorphemeMismatch { unit_index } => {
                write!(f, "trace unit {unit_index} is another morpheme")
            }
            Self::FactNotEstablished { unit_index, fact } => {
                write!(f, "trace unit {unit_index} has no established {fact:?}")
            }
            Self::FactMismatch { unit_index, fact } => {
                write!(f, "trace unit {unit_index} establishes another {fact:?}")
            }
            Self::AlternativesNotExhausted { unit_index } => {
                write!(f, "trace unit {unit_index} keeps an unexamined alternative")
            }
            Self::DeferredFeaturesUnresolved => {
                write!(f, "witness defers a feature the claim depends on")
            }
            Self::IdentityDefectNotEstablished => {
                write!(f, "the identity does not have the claimed defect")
            }
            Self::UnitsNotAdjacent { from, to } => {
                write!(f, "trace units {from} and {to} are not adjacent")
            }
            Self::PartnerAlreadyClosed { unit_index } => {
                write!(f, "trace unit {unit_index} closes the partner class")
            }
            Self::SpanDefectNotEstablished { unit_index } => {
                write!(
                    f,
                    "trace unit {unit_index} does not have the claimed span defect"
                )
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
