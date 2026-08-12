//! Runs the declared passes over each proposal and decides what survives.
//!
//! The loop is candidate, then witness, then pass, and the nesting is the recall argument. A
//! witness stops at its first verified rejection, and a candidate is removed only once every one
//! of its witnesses has reached one: several witnesses for one identity are alternative routes to
//! the same analysis, so proving one route impossible proves nothing about the identity.
//!
//! Every uncertain path retains. A pass that defers, a proof that fails verification, and an
//! exhausted budget all leave the candidate in the stream; on budget exhaustion the pipeline stops
//! deciding entirely and forwards the remainder of the input unchanged, still streaming, so an
//! interrupted run costs recall nothing and does not first materialize what it gave up on.
//!
//! Reports are deterministic. Pass order is the declared order, ordinals are assigned by the
//! traversal itself, and the counters are keyed by stable pass ID, so two runs over the same input
//! produce byte-identical evidence. Every witness of a decided candidate is evaluated even once
//! one has survived: stopping at the first survivor would make the evidence depend on witness
//! order, and the three modes would then no longer be comparable to each other.

use crate::candidate_filter::decision::{
    PassDecision, ProofVerificationError, RejectionProof, StablePassId,
};
use crate::candidate_filter::model::{CandidateWitness, ProposedCandidate};
use crate::candidate_filter::passes::CandidateFilterPass;
use crate::candidate_filter::proof::{admissible_catalog, RejectionProofVerifier};
use crate::candidate_filter::report::{
    CandidateDeath, CountingTraceSink, FilterCounters, FilterTraceSink, PassEvent, PassOutcome,
    RetainedCandidateSink, WitnessDeath,
};
use crate::tags::Candidate;

/// How much of the filter's authority a caller wants exercised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    /// No pass runs and no step is consumed.
    Off,
    /// Passes run and decisions are recorded, but every input is still emitted.
    Shadow,
    /// The only mode that removes a candidate.
    Enforce,
}

/// How much of a claimed rejection's proof a run re-checks before admitting it.
///
/// Orthogonal to `FilterMode`, and deliberately so: the mode decides what a verified rejection is
/// allowed to do, while the depth decides what "verified" cost the run is willing to pay. Naming
/// them separately is what makes `Enforce` at `Full` available as a debugging lever rather than
/// something only a shadow run can reach.
///
/// `Off` is the production default because every pass is first-party compiled code in this crate:
/// nothing untrusted authors a proof, so re-checking one is first-party code checking itself. What
/// replaces that check is reproducibility — a recorded rejection names its pass, rule, category,
/// candidate identity, and witness, which with the grammar is enough to re-run the same input at
/// `Full` offline and re-derive the decision. The proof is always built, carried, and recorded; it
/// is the explanation, and only its checking is a testing and shadow instrument.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProofCheckDepth {
    /// The pass's rejection is taken at face value.
    #[default]
    Off,
    /// The proof's envelope and its claim are both re-established against the witness.
    Full,
}

impl ProofCheckDepth {
    /// The depth a mode runs at when a caller names no depth of its own.
    pub const fn for_mode(mode: FilterMode) -> Self {
        match mode {
            FilterMode::Shadow => Self::Full,
            FilterMode::Off | FilterMode::Enforce => Self::Off,
        }
    }
}

/// An upper bound on pass evaluations for one run.
///
/// The unit is one pass's visit to one witness, which is the only work unit the pipeline itself
/// controls; a pass's internal cost is its own concern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterBudget {
    max_steps: Option<u64>,
}

impl FilterBudget {
    pub const fn unlimited() -> Self {
        Self { max_steps: None }
    }

    pub const fn steps(max_steps: u64) -> Self {
        Self {
            max_steps: Some(max_steps),
        }
    }
}

/// Why a run stopped deciding before the input ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterStopReason {
    StepBudget,
}

/// Whether a run decided every candidate it was given.
///
/// `Incomplete` is not an error: the remaining input was forwarded unchanged, so the result is
/// still recall preserving and merely less filtered than it could have been.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterCompletion {
    Complete,
    Incomplete(FilterStopReason),
}

/// The immutable per-candidate view a pass and the proof verifier both decide against.
#[derive(Clone, Copy, Debug)]
pub struct FilterContext<'a> {
    identity: &'a Candidate,
    candidate_ordinal: u64,
    mode: FilterMode,
}

impl<'a> FilterContext<'a> {
    pub fn new(identity: &'a Candidate, candidate_ordinal: u64, mode: FilterMode) -> Self {
        Self {
            identity,
            candidate_ordinal,
            mode,
        }
    }

    pub fn identity(&self) -> &'a Candidate {
        self.identity
    }

    pub fn candidate_ordinal(&self) -> u64 {
        self.candidate_ordinal
    }

    pub fn mode(&self) -> FilterMode {
        self.mode
    }
}

/// Re-establishes a claimed rejection independently of the pass that claimed it.
///
/// Crate-private so that enforcement authority cannot be supplied from outside: a caller who could
/// install a verifier could install one that admits anything, which is the single way to turn a
/// recall-preserving filter into a lossy one.
pub(crate) trait ProofVerifier: Send + Sync {
    fn verify(
        &self,
        context: &FilterContext<'_>,
        witness: &CandidateWitness,
        proof: &RejectionProof,
    ) -> Result<(), ProofVerificationError>;
}

/// A collected run, for callers that already hold the whole proposal set.
///
/// `filter_into` is the streaming form and the one a production caller wants; this exists for
/// tests and for adapters whose upstream already produced a slice.
#[derive(Debug)]
pub struct FilterOutcome {
    pub retained: Vec<ProposedCandidate>,
    pub report: FilterCounters,
    pub status: FilterCompletion,
}

/// An ordered pass list plus the verifier that guards every rejection they claim.
pub struct CandidateFilter {
    passes: Vec<Box<dyn CandidateFilterPass>>,
    verifier: Box<dyn ProofVerifier>,
}

/// Remaining step allowance, or none at all.
struct StepAllowance {
    remaining: Option<u64>,
}

impl StepAllowance {
    fn new(budget: FilterBudget) -> Self {
        Self {
            remaining: budget.max_steps,
        }
    }

    fn take(&mut self) -> bool {
        match &mut self.remaining {
            None => true,
            Some(0) => false,
            Some(left) => {
                *left -= 1;
                true
            }
        }
    }
}

/// Where a run's diagnostic ordinals start.
///
/// Crate-private because the only reason to start anywhere but zero is to reach the saturation
/// behaviour without emitting `u64::MAX` events first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrdinalSeed {
    pub next_event: u64,
    pub next_candidate: u64,
}

/// Monotonic diagnostic ordinals; an overflow degrades reporting, never a filter decision.
#[derive(Default)]
struct Ordinals {
    next_event: u64,
    next_candidate: u64,
    overflowed: bool,
}

impl Ordinals {
    fn seeded(seed: OrdinalSeed) -> Self {
        Self {
            next_event: seed.next_event,
            next_candidate: seed.next_candidate,
            overflowed: false,
        }
    }

    fn take_event(&mut self, trace: &mut impl FilterTraceSink) -> u64 {
        let current = self.next_event;
        match self.next_event.checked_add(1) {
            Some(next) => self.next_event = next,
            None => self.flag_overflow(trace),
        }
        current
    }

    fn take_candidate(&mut self, trace: &mut impl FilterTraceSink) -> u64 {
        let current = self.next_candidate;
        match self.next_candidate.checked_add(1) {
            Some(next) => self.next_candidate = next,
            None => self.flag_overflow(trace),
        }
        current
    }

    fn flag_overflow(&mut self, trace: &mut impl FilterTraceSink) {
        if !self.overflowed {
            self.overflowed = true;
            trace.record_ordinal_overflow();
        }
    }
}

enum WitnessVerdict {
    Survives,
    Died(WitnessDeath),
    BudgetExhausted,
}

enum CandidateVerdict {
    Retain,
    Died(CandidateDeath),
    BudgetExhausted,
}

impl CandidateFilter {
    pub(crate) fn new(
        passes: Vec<Box<dyn CandidateFilterPass>>,
        verifier: Box<dyn ProofVerifier>,
    ) -> Self {
        Self { passes, verifier }
    }

    /// Builds a filter whose rejections are guarded by the production proof verifier.
    ///
    /// The verifier is not a parameter: it is derived from the passes themselves, so what a
    /// rejection may claim is fixed by what those passes declared they decide.
    pub(crate) fn verifying(passes: Vec<Box<dyn CandidateFilterPass>>) -> Self {
        let catalog = admissible_catalog(&passes);
        Self::new(passes, Box::new(RejectionProofVerifier::new(catalog)))
    }

    /// The declared pass order, which is also the evaluation order for every witness.
    pub fn pass_ids(&self) -> Vec<StablePassId> {
        self.passes.iter().map(|pass| pass.id()).collect()
    }

    /// Filters a proposal stream at the depth the mode implies.
    pub fn filter_into<I, R, T>(
        &self,
        mode: FilterMode,
        input: I,
        retained: &mut R,
        trace: &mut T,
        budget: FilterBudget,
    ) -> FilterCompletion
    where
        I: IntoIterator<Item = ProposedCandidate>,
        R: RetainedCandidateSink,
        T: FilterTraceSink,
    {
        self.filter_into_at(
            mode,
            ProofCheckDepth::for_mode(mode),
            input,
            retained,
            trace,
            budget,
        )
    }

    /// Filters a proposal stream at a named depth, emitting retained candidates as they are decided.
    pub fn filter_into_at<I, R, T>(
        &self,
        mode: FilterMode,
        depth: ProofCheckDepth,
        input: I,
        retained: &mut R,
        trace: &mut T,
        budget: FilterBudget,
    ) -> FilterCompletion
    where
        I: IntoIterator<Item = ProposedCandidate>,
        R: RetainedCandidateSink,
        T: FilterTraceSink,
    {
        self.filter_into_seeded(
            mode,
            depth,
            input,
            retained,
            trace,
            budget,
            OrdinalSeed::default(),
        )
    }

    pub(crate) fn filter_into_seeded<I, R, T>(
        &self,
        mode: FilterMode,
        depth: ProofCheckDepth,
        input: I,
        retained: &mut R,
        trace: &mut T,
        budget: FilterBudget,
        seed: OrdinalSeed,
    ) -> FilterCompletion
    where
        I: IntoIterator<Item = ProposedCandidate>,
        R: RetainedCandidateSink,
        T: FilterTraceSink,
    {
        let mut ordinals = Ordinals::seeded(seed);
        let mut allowance = StepAllowance::new(budget);
        let mut stop: Option<FilterStopReason> = None;
        let deciding = !matches!(mode, FilterMode::Off);

        for candidate in input {
            let candidate_ordinal = ordinals.take_candidate(trace);
            if !deciding || stop.is_some() {
                Self::emit(retained, trace, candidate_ordinal, candidate);
                continue;
            }

            let verdict = self.evaluate_candidate(
                mode,
                depth,
                &candidate,
                candidate_ordinal,
                &mut ordinals,
                &mut allowance,
                trace,
            );
            match verdict {
                CandidateVerdict::Retain => {
                    Self::emit(retained, trace, candidate_ordinal, candidate);
                }
                CandidateVerdict::Died(death) => {
                    trace.record_candidate_death(&death);
                    if !matches!(mode, FilterMode::Enforce) {
                        Self::emit(retained, trace, candidate_ordinal, candidate);
                    }
                }
                CandidateVerdict::BudgetExhausted => {
                    stop = Some(FilterStopReason::StepBudget);
                    Self::emit(retained, trace, candidate_ordinal, candidate);
                }
            }
        }

        match stop {
            None => FilterCompletion::Complete,
            Some(reason) => FilterCompletion::Incomplete(reason),
        }
    }

    /// Filters an entire proposal set into memory at the depth the mode implies.
    pub fn filter<I>(&self, mode: FilterMode, input: I, budget: FilterBudget) -> FilterOutcome
    where
        I: IntoIterator<Item = ProposedCandidate>,
    {
        self.filter_at(mode, ProofCheckDepth::for_mode(mode), input, budget)
    }

    /// Filters an entire proposal set into memory at a named depth.
    pub fn filter_at<I>(
        &self,
        mode: FilterMode,
        depth: ProofCheckDepth,
        input: I,
        budget: FilterBudget,
    ) -> FilterOutcome
    where
        I: IntoIterator<Item = ProposedCandidate>,
    {
        let mut retained: Vec<ProposedCandidate> = Vec::new();
        let mut trace = CountingTraceSink::new();
        let status = self.filter_into_at(mode, depth, input, &mut retained, &mut trace, budget);
        FilterOutcome {
            retained,
            report: trace.into_counters(),
            status,
        }
    }

    fn emit<R: RetainedCandidateSink, T: FilterTraceSink>(
        retained: &mut R,
        trace: &mut T,
        candidate_ordinal: u64,
        candidate: ProposedCandidate,
    ) {
        trace.record_candidate_retained(candidate_ordinal, &candidate.identity);
        retained.accept(candidate);
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_candidate<T: FilterTraceSink>(
        &self,
        mode: FilterMode,
        depth: ProofCheckDepth,
        candidate: &ProposedCandidate,
        candidate_ordinal: u64,
        ordinals: &mut Ordinals,
        allowance: &mut StepAllowance,
        trace: &mut T,
    ) -> CandidateVerdict {
        let context = FilterContext::new(&candidate.identity, candidate_ordinal, mode);
        let mut witness_deaths = Vec::new();
        let mut survivors = 0usize;

        for witness in candidate.witnesses.iter() {
            match self.evaluate_witness(&context, depth, witness, ordinals, allowance, trace) {
                WitnessVerdict::Survives => survivors += 1,
                WitnessVerdict::Died(death) => witness_deaths.push(death),
                WitnessVerdict::BudgetExhausted => return CandidateVerdict::BudgetExhausted,
            }
        }

        if survivors > 0 {
            return CandidateVerdict::Retain;
        }

        CandidateVerdict::Died(CandidateDeath {
            candidate_ordinal,
            identity: candidate.identity.clone(),
            witness_deaths,
        })
    }

    fn evaluate_witness<T: FilterTraceSink>(
        &self,
        context: &FilterContext<'_>,
        depth: ProofCheckDepth,
        witness: &CandidateWitness,
        ordinals: &mut Ordinals,
        allowance: &mut StepAllowance,
        trace: &mut T,
    ) -> WitnessVerdict {
        for (index, pass) in self.passes.iter().enumerate() {
            if !allowance.take() {
                return WitnessVerdict::BudgetExhausted;
            }

            let outcome = match pass.evaluate(context, witness) {
                PassDecision::Keep => PassOutcome::Kept,
                PassDecision::Defer(reason) => PassOutcome::Deferred(reason),
                PassDecision::Reject(proof) => {
                    self.admit(context, depth, witness, pass.id(), proof)
                }
            };

            let event = PassEvent {
                event_ordinal: ordinals.take_event(trace),
                pass_ordinal: u16::try_from(index).unwrap_or(u16::MAX),
                candidate_ordinal: context.candidate_ordinal(),
                candidate_identity: context.identity().clone(),
                pass_id: pass.id(),
                witness_id: witness.witness_id,
                outcome,
            };
            trace.record_pass_event(&event);

            if let PassOutcome::Rejected(proof) = event.outcome {
                return WitnessVerdict::Died(WitnessDeath {
                    witness_id: witness.witness_id,
                    terminal_event_ordinal: event.event_ordinal,
                    pass_id: event.pass_id,
                    rule_id: proof.rule_id,
                    category: proof.category,
                });
            }
        }

        WitnessVerdict::Survives
    }

    fn admit(
        &self,
        context: &FilterContext<'_>,
        depth: ProofCheckDepth,
        witness: &CandidateWitness,
        declared: StablePassId,
        proof: RejectionProof,
    ) -> PassOutcome {
        if matches!(depth, ProofCheckDepth::Off) {
            return PassOutcome::Rejected(proof);
        }
        if proof.pass_id != declared {
            return PassOutcome::ProofRejected(ProofVerificationError::PassIdMismatch {
                declared,
                claimed: proof.pass_id,
            });
        }
        match self.verifier.verify(context, witness, &proof) {
            Ok(()) => PassOutcome::Rejected(proof),
            Err(error) => PassOutcome::ProofRejected(error),
        }
    }
}
