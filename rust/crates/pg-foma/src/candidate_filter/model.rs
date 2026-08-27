//! The proposal/witness input model a candidate filter reads: one HC-facing candidate identity
//! plus the symbolic witnesses that claim it.
//!
//! Every possibly-unavailable fact is a `TraceFact`, so a producer that does not have a value says
//! so in the type rather than guessing one. A known-absent slot or span is `Known(None)`;
//! `Deferred` means the producer lacks the fact entirely. The distinction is load-bearing: a
//! sentinel or out-of-range identifier standing in for "unknown" would let a rejection be proved
//! against a fact nobody ever established, which is the one error class a recall-preserving filter
//! cannot make.
//!
//! Several identifiers here (`TraceSlotId`, `TraceStratumId`, `TraceRole`, `PartnerClassId`,
//! `DeferredFeatureId`) are filter-contract identities, not indices into today's grammar tables. A
//! producer must map into them explicitly; populating them by ordinal coincidence with some other
//! id space silently fabricates the evidence a pass would then reject on.

use std::collections::BTreeSet;
use std::fmt;
use std::ops::Index;

use pg_grammar::model::{AllomorphId, MorphemeId};

use crate::tags::Candidate;

/// Identifies one witness within a single `ProposedCandidate`.
///
/// Uniqueness is candidate-local by design: two different candidates may both carry witness `1`.
/// A report that spans candidates must key on the candidate's own ordinal or identity as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WitnessId(pub u64);

/// A collection that cannot be empty, so "at least one alternative remains" is a type-level fact.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NonEmpty<T> {
    head: T,
    tail: Vec<T>,
}

/// The one way to fail to build a `NonEmpty`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NonEmptyError {
    Empty,
}

impl fmt::Display for NonEmptyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "expected at least one element"),
        }
    }
}

impl std::error::Error for NonEmptyError {}

impl<T> NonEmpty<T> {
    pub fn new(head: T, tail: Vec<T>) -> Self {
        Self { head, tail }
    }

    pub fn try_from_vec(values: Vec<T>) -> Result<Self, NonEmptyError> {
        let mut values = values.into_iter();
        let head = values.next().ok_or(NonEmptyError::Empty)?;
        Ok(Self {
            head,
            tail: values.collect(),
        })
    }

    pub fn first(&self) -> &T {
        &self.head
    }

    pub fn len(&self) -> usize {
        1 + self.tail.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.head).chain(self.tail.iter())
    }

}

impl<T> Index<usize> for NonEmpty<T> {
    type Output = T;

    fn index(&self, index: usize) -> &T {
        match index.checked_sub(1) {
            None => &self.head,
            Some(rest) => &self.tail[rest],
        }
    }
}

impl<'a, T> IntoIterator for &'a NonEmpty<T> {
    type Item = &'a T;
    type IntoIter = std::iter::Chain<std::iter::Once<&'a T>, std::slice::Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(&self.head).chain(self.tail.iter())
    }
}

/// Where a witness's lexical material came from.
///
/// A runtime overlay carries the revision it was read at, so a proof built against one stem
/// population is never enforced against another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LexicalOrigin {
    StaticGrammar,
    RuntimeOverlay { revision: u64 },
}

impl LexicalOrigin {
    /// The overlay revision, or `None` for static grammar material, which has no overlay revision
    /// at all — deliberately not reported as revision zero, which would read as a real one.
    pub fn revision(&self) -> Option<u64> {
        match self {
            Self::StaticGrammar => None,
            Self::RuntimeOverlay { revision } => Some(*revision),
        }
    }
}

/// What produced a proposal, kept so a report can attribute a witness to its producer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProposalProducer {
    LegacyProposer,
    SyntheticFixture,
}

/// The producer and the grammar revision a witness's facts were established against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProposalProvenance {
    pub producer: ProposalProducer,
    pub grammar_revision: u64,
}

/// A surface-changing feature whose value a producer deferred rather than resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeferredFeatureId(pub u32);

/// A deterministically ordered set of deferred features.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeatureSet {
    ids: BTreeSet<DeferredFeatureId>,
}

impl FeatureSet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: DeferredFeatureId) -> bool {
        self.ids.insert(id)
    }

    pub fn contains(&self, id: DeferredFeatureId) -> bool {
        self.ids.contains(&id)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = DeferredFeatureId> + '_ {
        self.ids.iter().copied()
    }
}

impl FromIterator<DeferredFeatureId> for FeatureSet {
    fn from_iter<I: IntoIterator<Item = DeferredFeatureId>>(iter: I) -> Self {
        Self {
            ids: iter.into_iter().collect(),
        }
    }
}

/// Why a producer could not supply a fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeferredFactReason {
    ProducerDoesNotEmit,
    UnsupportedConstruct,
    AmbiguityNotExhaustible,
}

/// A fact a producer either established or explicitly did not.
///
/// `Known(None)` on an optional payload is a positive claim that the value is absent;
/// `Deferred` is the absence of a claim. A pass may reject on the former and never on the latter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TraceFact<T> {
    Known(T),
    Deferred(DeferredFactReason),
}

impl<T> TraceFact<T> {
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Deferred(_) => None,
        }
    }

    pub fn deferred_reason(&self) -> Option<DeferredFactReason> {
        match self {
            Self::Known(_) => None,
            Self::Deferred(reason) => Some(*reason),
        }
    }

    pub fn is_deferred(&self) -> bool {
        matches!(self, Self::Deferred(_))
    }
}

/// A filter-contract morphotactic slot identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceSlotId(pub u32);

/// A filter-contract stratum identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceStratumId(pub u32);

/// A filter-contract class shared by the two halves of one finite partner pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartnerClassId(pub u32);

/// The structural part a trace unit plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TraceRole {
    Root,
    Prefix,
    Suffix,
    Infix,
    Stem,
    Other(u16),
}

/// A half-open byte range over the surface form the witness was proposed for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceSpan {
    pub start: usize,
    pub end: usize,
}

impl SurfaceSpan {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// A stable, explicitly emitted event at one trace unit.
///
/// Partner events are the only evidence a pairing pass may consult; a producer that collapses a
/// pair into a single unit emits none, and the pass therefore has nothing to reject on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LocalEvent {
    PartnerOpen(PartnerClassId),
    PartnerClose(PartnerClassId),
    Neutral,
}

/// One morpheme's position in a witness, with each fact either established or deferred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceUnit {
    pub morpheme: MorphemeId,
    pub role: TraceFact<TraceRole>,
    pub allomorphs: TraceFact<NonEmpty<AllomorphId>>,
    pub slot: TraceFact<Option<TraceSlotId>>,
    pub stratum: TraceFact<Option<TraceStratumId>>,
    pub surface_span: TraceFact<Option<SurfaceSpan>>,
    pub local_events: TraceFact<Vec<LocalEvent>>,
}

/// One producer-supplied route to a candidate identity.
///
/// Several witnesses may claim the same identity; they are alternatives, not duplicates, and the
/// candidate survives while any one of them does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateWitness {
    pub witness_id: WitnessId,
    pub lexical_origin: LexicalOrigin,
    pub lexicon_revision: u64,
    pub units: Vec<TraceUnit>,
    pub deferred: FeatureSet,
    pub provenance: ProposalProvenance,
}

/// An HC-facing candidate identity together with every witness proposing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposedCandidate {
    pub identity: Candidate,
    pub witnesses: NonEmpty<CandidateWitness>,
}

/// Why a witness list could not form a `ProposedCandidate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProposalModelError {
    NoWitnesses,
    DuplicateWitnessId(WitnessId),
}

impl fmt::Display for ProposalModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoWitnesses => write!(f, "a proposed candidate needs at least one witness"),
            Self::DuplicateWitnessId(id) => {
                write!(f, "witness id {} occurs twice in one candidate", id.0)
            }
        }
    }
}

impl std::error::Error for ProposalModelError {}

impl ProposedCandidate {
    /// Builds a proposal, refusing an empty witness list and any repeated `WitnessId`.
    ///
    /// The duplicate check is what makes `(candidate, witness_id)` a usable report key; witnesses
    /// are otherwise preserved exactly as supplied, including ones that differ only in a deferred
    /// fact.
    pub fn new(
        identity: Candidate,
        witnesses: Vec<CandidateWitness>,
    ) -> Result<Self, ProposalModelError> {
        let mut seen: BTreeSet<WitnessId> = BTreeSet::new();
        for witness in &witnesses {
            if !seen.insert(witness.witness_id) {
                return Err(ProposalModelError::DuplicateWitnessId(witness.witness_id));
            }
        }
        let witnesses =
            NonEmpty::try_from_vec(witnesses).map_err(|_| ProposalModelError::NoWitnesses)?;
        Ok(Self {
            identity,
            witnesses,
        })
    }
}
