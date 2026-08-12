//! Where filtered candidates and the audit trail of how they were decided are delivered.
//!
//! Two sinks, because the two streams have different lifetimes and different costs: retained
//! candidates flow onward to confirmation as they are produced, while the trace is diagnostic and
//! a caller may want nothing more than counters for it. Nothing here decides anything — a sink
//! that drops every record and a sink that stores all of them must produce identical filtering.
//!
//! Every canonical value is deterministic. Per-pass counters are keyed by stable pass ID in a
//! `BTreeMap` so a report's order is a property of the pass names rather than of a hash seed, and
//! no field records a duration: a wall-clock number in a canonical report invites certifying an
//! optimization by a measurement that is not reproducible.

use std::collections::BTreeMap;

use crate::candidate_filter::decision::{
    DeferReason, ProofCategory, ProofVerificationError, RejectionProof, StablePassId, StableRuleId,
};
use crate::candidate_filter::model::{ProposedCandidate, WitnessId};
use crate::tags::Candidate;

/// What one pass concluded about one witness, after verification.
///
/// `Rejected` is the only outcome that ends a witness; `ProofRejected` records a rejection that
/// was claimed and refused, which is diagnostically distinct from a pass that deferred on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassOutcome {
    Kept,
    Deferred(DeferReason),
    Rejected(RejectionProof),
    ProofRejected(ProofVerificationError),
}

/// One pass's visit to one witness.
///
/// The key is `(candidate_ordinal, witness_id)`: witness IDs are unique only within their own
/// candidate, so the ordinal is what keeps two candidates that both number a witness `1` apart.
/// The identity is carried as well, since an ordinal alone is meaningless outside its run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassEvent {
    pub event_ordinal: u64,
    pub pass_ordinal: u16,
    pub candidate_ordinal: u64,
    pub candidate_identity: Candidate,
    pub pass_id: StablePassId,
    pub witness_id: WitnessId,
    pub outcome: PassOutcome,
}

/// The verified rejection that ended one witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessDeath {
    pub witness_id: WitnessId,
    pub terminal_event_ordinal: u64,
    pub pass_id: StablePassId,
    pub rule_id: StableRuleId,
    pub category: ProofCategory,
}

/// A candidate every one of whose witnesses reached a verified rejection.
///
/// It links each witness to its own terminal event, which is what lets a reader answer both "where
/// did this trace die" and "why did the candidate die despite the alternative routes to it".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateDeath {
    pub candidate_ordinal: u64,
    pub identity: Candidate,
    pub witness_deaths: Vec<WitnessDeath>,
}

/// Receives candidates the filter retains, as they are decided.
pub trait RetainedCandidateSink {
    fn accept(&mut self, candidate: ProposedCandidate);
}

impl RetainedCandidateSink for Vec<ProposedCandidate> {
    fn accept(&mut self, candidate: ProposedCandidate) {
        self.push(candidate);
    }
}

/// Receives the audit trail.
///
/// Retention and death are reported separately rather than as one outcome, because in shadow mode
/// a candidate is legitimately both: the filter records that it would have died and emits it
/// anyway. The defaulted methods let a sink that only wants pass events ignore the rest.
pub trait FilterTraceSink {
    fn record_pass_event(&mut self, event: &PassEvent);

    fn record_candidate_death(&mut self, death: &CandidateDeath);

    fn record_candidate_retained(&mut self, _candidate_ordinal: u64, _identity: &Candidate) {}

    fn record_ordinal_overflow(&mut self) {}
}

/// Per-pass outcome counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassCounters {
    pub keeps: u64,
    pub defers: u64,
    pub rejections: u64,
    pub proof_failures: u64,
}

/// The compact, deterministic summary of one filter run.
///
/// `candidates_rejected` counts decisions, not removals: in shadow mode a candidate can be counted
/// here and still be emitted, and that difference is the whole content of a shadow run.
/// `witnesses_rejected` counts witnesses ended by a verified rejection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterCounters {
    pub pass_evaluations: u64,
    pub keeps: u64,
    pub defers: u64,
    pub witnesses_rejected: u64,
    pub proof_verification_failures: u64,
    pub candidates_rejected: u64,
    pub candidates_retained: u64,
    pub ordinal_overflow: bool,
    pub per_pass: BTreeMap<StablePassId, PassCounters>,
}

/// The ordinary trace sink: counters only, no per-witness records retained.
#[derive(Clone, Debug, Default)]
pub struct CountingTraceSink {
    counters: FilterCounters,
}

impl CountingTraceSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn counters(&self) -> &FilterCounters {
        &self.counters
    }

    pub fn into_counters(self) -> FilterCounters {
        self.counters
    }
}

impl FilterTraceSink for CountingTraceSink {
    fn record_pass_event(&mut self, event: &PassEvent) {
        let counters = &mut self.counters;
        counters.pass_evaluations = counters.pass_evaluations.saturating_add(1);
        let per_pass = counters.per_pass.entry(event.pass_id).or_default();
        match &event.outcome {
            PassOutcome::Kept => {
                counters.keeps = counters.keeps.saturating_add(1);
                per_pass.keeps = per_pass.keeps.saturating_add(1);
            }
            PassOutcome::Deferred(_) => {
                counters.defers = counters.defers.saturating_add(1);
                per_pass.defers = per_pass.defers.saturating_add(1);
            }
            PassOutcome::Rejected(_) => {
                counters.witnesses_rejected = counters.witnesses_rejected.saturating_add(1);
                per_pass.rejections = per_pass.rejections.saturating_add(1);
            }
            PassOutcome::ProofRejected(_) => {
                counters.proof_verification_failures =
                    counters.proof_verification_failures.saturating_add(1);
                per_pass.proof_failures = per_pass.proof_failures.saturating_add(1);
            }
        }
    }

    fn record_candidate_death(&mut self, _death: &CandidateDeath) {
        self.counters.candidates_rejected = self.counters.candidates_rejected.saturating_add(1);
    }

    fn record_candidate_retained(&mut self, _candidate_ordinal: u64, _identity: &Candidate) {
        self.counters.candidates_retained = self.counters.candidates_retained.saturating_add(1);
    }

    fn record_ordinal_overflow(&mut self) {
        self.counters.ordinal_overflow = true;
    }
}

/// How many detailed records of each kind a ledger will hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedgerCaps {
    pub max_events: usize,
    pub max_candidate_deaths: usize,
}

impl LedgerCaps {
    pub const fn unlimited() -> Self {
        Self {
            max_events: usize::MAX,
            max_candidate_deaths: usize::MAX,
        }
    }
}

/// The opt-in detailed trace sink: individual pass events and candidate deaths, up to a cap.
///
/// The cap bounds memory for a run that dies at scale, and what it drops it counts, so a reader
/// can tell a run that recorded everything from one that recorded a prefix. It is diagnostic only:
/// the counters are maintained exactly as the compact sink maintains them, and a ledger of any
/// capacity — including zero — retains exactly the same candidates as no ledger at all.
///
/// Detailed retention also stops when diagnostic ordinals saturate. Past that point events no
/// longer carry distinct keys, and storing records that collide would be worse than storing none.
#[derive(Clone, Debug)]
pub struct BoundedDeathLedger {
    caps: LedgerCaps,
    counting: CountingTraceSink,
    events: Vec<PassEvent>,
    candidate_deaths: Vec<CandidateDeath>,
    omitted_events: u64,
    omitted_candidate_deaths: u64,
    summary_only: bool,
}

impl BoundedDeathLedger {
    pub fn new(caps: LedgerCaps) -> Self {
        Self {
            caps,
            counting: CountingTraceSink::new(),
            events: Vec::new(),
            candidate_deaths: Vec::new(),
            omitted_events: 0,
            omitted_candidate_deaths: 0,
            summary_only: false,
        }
    }

    pub fn unlimited() -> Self {
        Self::new(LedgerCaps::unlimited())
    }

    pub fn events(&self) -> &[PassEvent] {
        &self.events
    }

    pub fn candidate_deaths(&self) -> &[CandidateDeath] {
        &self.candidate_deaths
    }

    pub fn omitted_events(&self) -> u64 {
        self.omitted_events
    }

    pub fn omitted_candidate_deaths(&self) -> u64 {
        self.omitted_candidate_deaths
    }

    pub fn counters(&self) -> &FilterCounters {
        self.counting.counters()
    }

    pub fn into_counters(self) -> FilterCounters {
        self.counting.into_counters()
    }

    pub fn is_summary_only(&self) -> bool {
        self.summary_only
    }
}

impl FilterTraceSink for BoundedDeathLedger {
    fn record_pass_event(&mut self, event: &PassEvent) {
        self.counting.record_pass_event(event);
        if self.summary_only || self.events.len() >= self.caps.max_events {
            self.omitted_events = self.omitted_events.saturating_add(1);
            return;
        }
        self.events.push(event.clone());
    }

    fn record_candidate_death(&mut self, death: &CandidateDeath) {
        self.counting.record_candidate_death(death);
        if self.summary_only || self.candidate_deaths.len() >= self.caps.max_candidate_deaths {
            self.omitted_candidate_deaths = self.omitted_candidate_deaths.saturating_add(1);
            return;
        }
        self.candidate_deaths.push(death.clone());
    }

    fn record_candidate_retained(&mut self, candidate_ordinal: u64, identity: &Candidate) {
        self.counting
            .record_candidate_retained(candidate_ordinal, identity);
    }

    fn record_ordinal_overflow(&mut self) {
        self.counting.record_ordinal_overflow();
        self.summary_only = true;
    }
}
