//! Production evaluator for backend plans.

use crate::analyzer::FomaProposer;
use crate::backend::backend_for;
use crate::backend_accuracy::{AccuracyCounters, AccuracyVerdict, CandidateAccuracy};
use crate::backend_optimizer::{
    Certification, CorpusCompletenessEvidence, CorpusExclusion, OracleEligibilityConfig, Score,
};
use crate::compose_budget::{ApplyBudget, ComposeBudget};
use crate::composite::{FomaAnalyzer, ProfiledFomaApplyOutcomeWithCandidates};
use crate::enumerate::{EmissionStrategy, LoweredCandidate};
use crate::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use crate::parity::{
    certified_occurrence, IdentityDivergence, IdentityMismatchDirection, OccurrenceIdentities,
    ParitySide,
};
use crate::production_admission::assess_completed_fst;
use crate::tags::Candidate;
use pg_grammar::model::Grammar;
use pg_parse::WordAnalysis;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

/// What the ground-truth oracle concluded about ONE corpus occurrence: deliberately no `TimedOut`/`MemoryCapped` variant, since a wall-clock outcome is unreproducible under load and can mask a real work-cap exhaustion, so both become whole-run `OraclePreparationFault`s instead of per-word exclusions.
#[derive(Debug, Clone)]
enum OracleOutcome {
    /// The oracle finished this occurrence within its work cap. The analyses are the ground truth.
    Complete(Vec<WordAnalysis>),
    /// The oracle exhausted its work cap on this occurrence — the only eligibility classifier, deterministic per (grammar, word, cap).
    WorkCapped,
}

#[derive(Debug, Clone)]
struct PreparedWord {
    word: String,
    outcome: OracleOutcome,
}

/// A liveness/resource bound tripped while preparing the ground truth, so eligibility could NOT be
/// determined for the requested corpus.
///
/// Every variant is a whole-run abort. None of them is, or may become, a per-word exclusion — see
/// `OracleOutcome` for why that is a type-level guarantee rather than a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OraclePreparationFault {
    /// The wall-clock liveness net tripped before the work cap could classify this word, so the honest report is "could not determine", never "excluded".
    LivenessNetTripped {
        word: String,
        requested_ordinal: u64,
        net: Duration,
    },
    /// The declared resident-memory ceiling was exceeded during preparation — reported explicitly rather than left as a silent kill with no row emitted.
    MemoryCeilingExceeded {
        word: String,
        requested_ordinal: u64,
        ceiling_bytes: u64,
        observed_bytes: u64,
    },
    /// A memory ceiling was declared but this build cannot read the process's resident set; refusing rather than silently ignoring the ceiling.
    MemoryCeilingUnobservable { ceiling_bytes: u64 },
}

impl std::fmt::Display for OraclePreparationFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LivenessNetTripped {
                word,
                requested_ordinal,
                net,
            } => write!(
                f,
                "oracle liveness net tripped on word {word:?} (requested ordinal \
                 {requested_ordinal}) after {net:?} -- eligibility could not be determined"
            ),
            Self::MemoryCeilingExceeded {
                word,
                requested_ordinal,
                ceiling_bytes,
                observed_bytes,
            } => write!(
                f,
                "oracle memory ceiling exceeded on word {word:?} (requested ordinal \
                 {requested_ordinal}): {observed_bytes} bytes resident against a declared ceiling \
                 of {ceiling_bytes} -- eligibility could not be determined"
            ),
            Self::MemoryCeilingUnobservable { ceiling_bytes } => write!(
                f,
                "an oracle memory ceiling of {ceiling_bytes} bytes was declared but this build \
                 cannot sample resident memory -- eligibility could not be determined"
            ),
        }
    }
}

impl std::error::Error for OraclePreparationFault {}

/// Samples this process's RSS; `None` means "could not look", never "fine" — two cfg'd shapes share one signature so the preparation loop has no cfg in it.
#[cfg(not(target_arch = "wasm32"))]
struct RssSampler;

#[cfg(not(target_arch = "wasm32"))]
impl RssSampler {
    fn new() -> Self {
        Self
    }

    fn sample(&mut self) -> Option<u64> {
        pg_worker_containment::current_process_rss_bytes()
    }
}

#[cfg(target_arch = "wasm32")]
struct RssSampler;

#[cfg(target_arch = "wasm32")]
impl RssSampler {
    fn new() -> Self {
        Self
    }

    fn sample(&mut self) -> Option<u64> {
        None
    }
}

/// A run-scoped corpus prepared once from the oracle. The ground truth and the work-cap exclusion
/// latches are shared by the pilot and every candidate evaluation in that run — which is what makes
/// exclusions CANDIDATE-INDEPENDENT by construction: nothing a candidate does can reach this.
#[derive(Debug)]
pub struct PreparedCorpus {
    words: Vec<PreparedWord>,
    oracle_calls: usize,
    oracle: OracleEligibilityConfig,
}

impl PreparedCorpus {
    /// Runs the ground-truth oracle over `words` and classifies each occurrence by WORK CAP alone.
    ///
    /// Returns `Err` — aborting the whole run — if the wall-clock liveness net or the declared
    /// memory ceiling trips. Neither can produce a row.
    pub fn prepare(
        grammar: &Grammar,
        words: &[String],
        budget: RuntimeBudget,
    ) -> Result<Self, OraclePreparationFault> {
        let oracle = budget.oracle_eligibility_config();
        let cap = budget.resolved_oracle_work_cap();
        let net = budget.resolved_oracle_liveness_net();
        let ceiling = budget.resolved_oracle_memory_ceiling();
        let morpher = pg_parse::Morpher::new(grammar, cap).with_word_timeout(Some(net));
        let mut sampler = RssSampler::new();
        let mut records = Vec::with_capacity(words.len());
        for (requested_ordinal, word) in words.iter().enumerate() {
            let outcome = morpher.parse_word(word);
            // Checked first and unconditionally: `capped` can be true alongside `timed_out`, and treating that as an exclusion would mask a genuine timeout as a work-cap exclusion.
            if outcome.timed_out {
                return Err(OraclePreparationFault::LivenessNetTripped {
                    word: word.clone(),
                    requested_ordinal: requested_ordinal as u64,
                    net,
                });
            }
            if ceiling != u64::MAX {
                match sampler.sample() {
                    None => {
                        return Err(OraclePreparationFault::MemoryCeilingUnobservable {
                            ceiling_bytes: ceiling,
                        })
                    }
                    Some(observed_bytes) if observed_bytes > ceiling => {
                        return Err(OraclePreparationFault::MemoryCeilingExceeded {
                            word: word.clone(),
                            requested_ordinal: requested_ordinal as u64,
                            ceiling_bytes: ceiling,
                            observed_bytes,
                        })
                    }
                    Some(_) => {}
                }
            }
            records.push(PreparedWord {
                word: word.clone(),
                outcome: if outcome.capped {
                    OracleOutcome::WorkCapped
                } else {
                    OracleOutcome::Complete(outcome.structured)
                },
            });
        }
        Ok(Self {
            words: records,
            oracle_calls: words.len(),
            oracle,
        })
    }

    pub fn oracle_calls(&self) -> usize {
        self.oracle_calls
    }

    fn select(&self, requested: &[String]) -> PreparedSelection {
        let mut used = vec![false; self.words.len()];
        let mut comparable = Vec::new();
        let mut expected = Vec::new();
        let mut exclusions = Vec::new();
        let mut capped = false;
        for (requested_ordinal, word) in requested.iter().enumerate() {
            let Some((index, prepared)) = self
                .words
                .iter()
                .enumerate()
                .find(|(index, prepared)| !used[*index] && prepared.word == *word)
            else {
                exclusions.push(CorpusExclusion {
                    requested_ordinal: requested_ordinal as u64,
                    word: word.clone(),
                    reason: "corpus-row-not-prepared".into(),
                });
                continue;
            };
            used[index] = true;
            match &prepared.outcome {
                OracleOutcome::Complete(analyses) => {
                    comparable.push(word.clone());
                    expected.push((word.clone(), analyses.clone()));
                }
                OracleOutcome::WorkCapped => {
                    capped = true;
                    exclusions.push(CorpusExclusion {
                        requested_ordinal: requested_ordinal as u64,
                        word: word.clone(),
                        reason: "oracle-capped".into(),
                    });
                }
            }
        }
        PreparedSelection {
            comparable,
            expected,
            capped,
            exclusions,
        }
    }
}

#[derive(Debug)]
struct PreparedSelection {
    comparable: Vec<String>,
    expected: Vec<(String, Vec<WordAnalysis>)>,
    capped: bool,
    exclusions: Vec<CorpusExclusion>,
}

/// All prepared, reusable evaluation inputs for one optimizer run.
#[derive(Debug)]
pub struct RunEvaluationCache {
    corpus: PreparedCorpus,
    divergence: IdentityDivergence,
}

impl RunEvaluationCache {
    pub fn prepare(
        grammar: &Grammar,
        words: &[String],
        budget: RuntimeBudget,
    ) -> Result<Self, OraclePreparationFault> {
        Ok(Self {
            corpus: PreparedCorpus::prepare(grammar, words, budget)?,
            divergence: IdentityDivergence::default(),
        })
    }

    pub fn oracle_calls(&self) -> usize {
        self.corpus.oracle_calls()
    }

    /// This run's accumulated parity-set divergence across every candidate evaluated against this
    /// cache.
    ///
    /// `IdentityDivergence::candidate_only_identities` is the number the
    /// confirmation-free accuracy path's soundness rests on; see that type's doc for why it is
    /// counted rather than argued. It accumulates across calls, so a caller measuring one run must
    /// use one cache for it — which is already the run cache's whole purpose.
    pub fn identity_divergence(&self) -> IdentityDivergence {
        self.divergence
    }

    fn absorb_divergence(&mut self, divergence: IdentityDivergence) {
        self.divergence.absorb(divergence);
    }

    /// The prepared oracle analyses for the first COMPLETE occurrence of `word`, or `None` if this
    /// corpus has no such row.
    ///
    /// `None` deliberately covers two different facts — "not prepared" and "prepared but work-capped"
    /// — because neither is a set of analyses, and a caller must not be able to treat a work-capped
    /// row as an empty one. That distinction is what
    /// `CorpusCompletenessEvidence`/`Certification::Truncated` exist to carry; this accessor is
    /// for reading a ground truth that IS complete, not for classifying eligibility.
    ///
    /// Exposed so a gate can construct a negative control against real oracle analyses of a real
    /// grammar rather than against hand-built values — a fabricated ground truth proves only that the
    /// comparison compares.
    #[doc(hidden)]
    pub fn oracle_analyses(&self, word: &str) -> Option<&[WordAnalysis]> {
        self.corpus.words.iter().find_map(|prepared| {
            match (&prepared.outcome, prepared.word == word) {
                (OracleOutcome::Complete(analyses), true) => Some(analyses.as_slice()),
                _ => None,
            }
        })
    }

    /// The run-scoped eligibility ledger for `requested`, derived from THIS run's prepared oracle
    /// results.
    ///
    /// This is the in-band derivation requirement made available to the report writer: the ledger a
    /// report publishes has to come from the same preparation pass that decided eligibility, over
    /// the caller's RAW requested slice, so that "zero exclusions" is a positive claim about a named
    /// corpus rather than the absence of a claim. It is candidate-independent by construction —
    /// nothing here can observe a candidate.
    pub fn corpus_evidence(&self, requested: &[String]) -> CorpusCompletenessEvidence {
        let selection = self.corpus.select(requested);
        corpus_completeness_evidence(
            requested,
            &selection.comparable,
            &selection.exclusions,
            self.corpus.oracle,
        )
    }

    fn select(&self, words: &[String]) -> PreparedSelection {
        self.corpus.select(words)
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn corpus_hash(words: &[String]) -> String {
    let mut hash = Sha256::new();
    for word in words {
        hash.update((word.len() as u64).to_le_bytes());
        hash.update(word.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

// Identity digests: grammar and finished network

/// "These bytes are the whole state of one FINISHED proposer network."
///
/// The preimage is everything `foma::types::Fsm` carries that `apply_up` can observe — see
/// `finished_net_digest` for the field-by-field account and the two deliberate exclusions.
pub const FINISHED_NET_PROJECTION: &str = "pangloss.foma.finished-net/v1";

/// "This is the same loaded grammar." Preimage is the grammar's own derived `Debug` projection; see
/// `grammar_identity`.
pub const GRAMMAR_IDENTITY_PROJECTION: &str = "pangloss.foma.grammar-identity/v1";

/// Feeds `part` into `hash` length-prefixed, so no two different tuples of parts can share a preimage — unprefixed, `("ab", "c")` and `("a", "bc")` would hash alike.
fn framed(hash: &mut Sha256, part: &[u8]) {
    hash.update((part.len() as u64).to_le_bytes());
    hash.update(part);
}

/// A `std::fmt::Write` sink that hashes instead of allocating, so `grammar_identity` never materializes a multi-hundred-megabyte `Debug` string just to hash and drop it.
struct DigestWriter(Sha256);

impl std::fmt::Write for DigestWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.update(s.as_bytes());
        Ok(())
    }
}

/// The identity of a LOADED grammar: domain-framed SHA-256 over its derived `Debug` projection.
///
/// # Why `Debug` and not a hand-written field list
/// This digest's whole job is to stop one grammar's cached measurement being served to another, so
/// the failure mode that matters is a MISSED field: two grammars differing only in something the
/// projection forgot would collide, silently, and in the reassuring direction. A hand-written list
/// of "the fields that matter" is exactly the artifact that goes stale — `Grammar` gains fields, and
/// nothing fails. `Grammar` and every type it contains derive `Debug` (no hand-written `Debug` impl
/// exists anywhere in `pg-grammar` or `pg-featstruct`), so `{:?}` covers every field by
/// construction, including ones added after this line was written.
///
/// The residual hazard is stated rather than hidden: if some future member of the grammar tree
/// acquires a hand-written `Debug` that elides content, this digest silently narrows with it. That is
/// checked, not assumed — `grammar_identity_gate` in the dedup gate asserts that two grammars which
/// differ in a single allomorph shape get different identities.
///
/// # Why the cost is affordable
/// It is O(grammar), and it is only ever computed on a path that has just compiled the same grammar
/// into an FST — a strictly more
/// expensive O(grammar) operation. So it can never be more than a small fraction of work already
/// spent, and it is computed once per evaluator CALL, never once per plan.
pub fn grammar_identity(grammar: &Grammar) -> String {
    use std::fmt::Write as _;
    let mut writer = DigestWriter(Sha256::new());
    framed(&mut writer.0, GRAMMAR_IDENTITY_PROJECTION.as_bytes());
    write!(writer, "{grammar:?}").expect("a DigestWriter cannot fail to accept bytes");
    format!("{:x}", writer.0.finalize())
}

/// The identity of a finished proposer network: domain-framed SHA-256 over everything `apply_up` can observe about it, so any two nets sharing a digest must return identical raw paths for any query.
/// See `docs/research/pg-foma-recipe-runtime-design-notes.md` for the preimage completeness argument and the two deliberate exclusions.
pub(crate) fn finished_net_digest(net: &foma::types::Fsm) -> String {
    let mut hash = Sha256::new();
    framed(&mut hash, FINISHED_NET_PROJECTION.as_bytes());
    hash.update(net.arity.to_le_bytes());
    hash.update(net.arccount.to_le_bytes());
    hash.update(net.statecount.to_le_bytes());
    hash.update(net.linecount.to_le_bytes());
    hash.update(net.finalcount.to_le_bytes());
    hash.update(net.pathcount.to_le_bytes());
    for tern in [
        net.is_deterministic,
        net.is_pruned,
        net.is_minimized,
        net.is_epsilon_free,
        net.is_loop_free,
        net.is_completed,
    ] {
        hash.update((tern as i32).to_le_bytes());
    }
    hash.update([u8::from(net.arcs_sorted_in), u8::from(net.arcs_sorted_out)]);
    hash.update((net.sigma.len() as u64).to_le_bytes());
    for symbol in &net.sigma {
        hash.update(symbol.number.to_le_bytes());
        framed(&mut hash, symbol.symbol.as_bytes());
    }
    match &net.medlookup {
        None => hash.update([0u8]),
        Some(medlookup) => {
            hash.update([1u8]);
            hash.update((medlookup.confusion_matrix.len() as u64).to_le_bytes());
            for cell in &medlookup.confusion_matrix {
                hash.update(cell.to_le_bytes());
            }
        }
    }
    // Native CSR traversal, not `LineTable::rows()`: avoids materializing the whole flat row sequence into a fresh `Vec<FsmState>`.
    hash.update((net.states.blocks().len() as u64).to_le_bytes());
    for (block, arcs) in net.states.iter_blocks() {
        hash.update(block.state_no.to_le_bytes());
        hash.update(block.arc_len.to_le_bytes());
        hash.update([block.final_state as u8, block.start_state as u8]);
        hash.update((arcs.len() as u64).to_le_bytes());
        for arc in arcs {
            hash.update(arc.r#in.to_le_bytes());
            hash.update(arc.out.to_le_bytes());
            hash.update(arc.target.to_le_bytes());
        }
    }
    format!("{:x}", hash.finalize())
}

/// Default oracle (ground-truth `pg_parse::Morpher`) work cap, used whenever
/// `RuntimeBudget::oracle_work_cap` is left `None`.
///
/// Justified by measurement: on the
/// deep-truncation-chain stress grammar, the pathological corpus word that the fully-unbounded
/// `Morpher::new(g, usize::MAX)` call never returns for (>20s, previously observed >10 minutes)
/// completes in 91.6ms at an attempt cap of 20_000 (a work cap of 2_000_000 here), reporting `capped: true` and 2 analyses. That is also
/// Large enough that no reference/staged grammar's real analyses come close to
/// it (the work cap stays a no-op for every well-formed word); small enough that a pathological word
/// is stopped in well under a second instead of hanging the whole evaluator call.
pub const DEFAULT_ORACLE_WORK_CAP: usize = 2_000_000;

/// Default wall-clock LIVENESS NET, used whenever `RuntimeBudget::oracle_liveness_net` is `None`.
///
/// # This is not a classifier, and it used to be one
/// It exists for exactly one purpose: a word whose single step is pathologically expensive must not
/// hang the run forever. Tripping it is an `OraclePreparationFault` that aborts preparation; it
/// can never exclude a word. See `OracleOutcome` for the measured incidents behind that.
///
/// # Why 300 seconds and not 2
/// The old 2-second value was small enough to trip BEFORE a word reached its work cap, which both
/// made exclusions load-sensitive and masked the deterministic axis. A net's only job is to be
/// unreachable by anything except a genuine hang, so it is set far above any legitimate per-word
/// cost: the pathological deep-truncation-chain word that motivated these bounds completes in
/// 91.6ms at an attempt cap of 20_000 (a work cap of 2_000_000 here). Raising it costs
/// nothing on a healthy corpus and makes an abort mean what it says.
pub const DEFAULT_ORACLE_LIVENESS_NET: Duration = Duration::from_secs(300);

/// Default declared resident-memory ceiling for oracle preparation, used whenever
/// `RuntimeBudget::oracle_memory_ceiling` is `None`.
///
/// # Why memory is a declared axis at all
/// Measured: Aweti at a 200k attempt cap OOMed against a 16GB job-object ceiling, and a job-object
/// kill produces NO row — the run simply vanishes, and "I could not look" reads as an outcome.
/// Declaring a ceiling below the job object's turns that into a typed, word-naming abort that a
/// reader can act on.
///
/// # Why a fixed constant and not a fraction of installed RAM
/// This number is recorded in `CorpusCompletenessEvidence`, so a machine-derived value would make
/// two identical eligibility sets produce different evidence on different machines. A ceiling can
/// only ever ABORT, never classify, so the cost of it being wrong for a given machine is a loud
/// refusal rather than a wrong answer — which is the direction a reproducible artifact should fail
/// in. 12 GiB sits below the 16GB job ceiling that killed the Aweti run. Override with
/// `RuntimeBudget::oracle_memory_ceiling`; `Some(u64::MAX)` opts out entirely.
pub const DEFAULT_ORACLE_MEMORY_CEILING_BYTES: u64 = 12 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordEvidence {
    pub word: String,
    pub expected: Vec<WordAnalysis>,
    pub actual: Vec<WordAnalysis>,
    /// The final deduplicated candidate vector sent to confirmation for this word. This is
    /// populated only by the opt-in observed evaluator; ordinary optimizer runs do not retain it.
    pub proposals: Vec<Candidate>,
    /// The oracle's deduplicated identity set for THIS occurrence, carrying the duplicate-path
    /// count and the guessed/supplied annotations that deduplication erased.
    ///
    /// This is the evidence half of the parity relation: `certify_word` compares only the
    /// identities, so without this the report could not say that a candidate found one analysis by
    /// five paths where the oracle found it by one — a real and useful property of the compilation
    /// that the verdict is deliberately blind to.
    ///
    /// `None` means the projection FAILED for this occurrence, which is exactly the case in which
    /// the certification is a `crate::parity::ParityFault`-derived truncation. It never means
    /// "no analyses"; that is `Some` of an empty set.
    pub expected_identities: Option<OccurrenceIdentities>,
    /// The candidate's deduplicated identity set for this occurrence. Same contract as
    /// `Self::expected_identities`.
    pub actual_identities: Option<OccurrenceIdentities>,
}

/// One oracle-required analysis identity that `WordEvidence::proposals` did not offer at all for
/// `WordEvidence::word`.
///
/// The identity is the `(morpheme id sequence, root index)` pair a candidate and an analysis are
/// compared by; this type is what that comparison returns instead of asserting directly, so a
/// fixture-wide faithfulness sweep can classify the gap rather than panic on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainmentGap {
    pub word: String,
    pub morpheme_ids: Vec<u32>,
    pub root_morpheme_index: i32,
    pub required: usize,
    pub offered: usize,
}

impl std::fmt::Display for ContainmentGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "word {:?}: oracle identity (morphemes={:?}, root_index={}) required multiplicity {}, \
             proposal set offered {}",
            self.word, self.morpheme_ids, self.root_morpheme_index, self.required, self.offered
        )
    }
}

impl std::error::Error for ContainmentGap {}

/// Does `evidence.proposals` (the final, deduplicated candidate vector confirmation received)
/// CONTAIN every DISTINCT oracle identity `evidence.expected` names, at least once?
///
/// This is presence, not multiplicity: a pre-confirm identity offered once and an oracle identity
/// required twice (the same `(morphemes, root_index)` key reached by two derivation orders under an
/// unordered stratum, e.g. `mpr-gated-exception`'s "mentanukam") both read HELD here.
/// `crate::confirm::confirm_all`'s own doc names this "D4 multiplicity recovery": it re-parses each
/// proposed candidate against the UNRESTRICTED oracle engine and collects every matching analysis
/// the engine's own outcome contains, so a single deduplicated pre-confirm candidate can and does
/// expand back to the oracle's own multiplicity at confirm time -- measured for "mentanukam" (`TunedSurfaceProbed`: `evidence.proposals.len() == 1`,
/// `evidence.actual.len() == 2`, matching `evidence.expected.len() == 2` exactly, certified
/// `Certification::FullHcConfirmed`). Requiring pre-confirm multiplicity to already equal the
/// oracle's own would fail a pipeline that is, end to end, exact -- see
/// `tests/mentanukam_multiplicity_recovered_by_confirm.rs`.
///
/// This is still containment, not equality, along the other axis: a proposal set may offer MORE
/// than the oracle found (over-generation) and still pass here -- `check_proposal_ratio` is the
/// separate, existing guard against that. It is also the honest recall question for a
/// propose+confirm pipeline: confirmation can only ever select from what was proposed, so an
/// emitter that silently drops a construct's material entirely (offers ZERO candidates for an
/// identity the oracle requires) makes the proposal set under-generate, and this is where that
/// failure first becomes visible -- before confirmation, which would otherwise just report a
/// smaller `actual` with no way to tell "the oracle found less" apart from "the proposer never
/// offered it at all".
pub fn word_proposal_containment(evidence: &WordEvidence) -> Result<(), ContainmentGap> {
    let proposed: std::collections::BTreeSet<(Vec<u32>, i32)> = evidence
        .proposals
        .iter()
        .map(|candidate| {
            (
                candidate.morphemes.iter().map(|m| m.0).collect(),
                candidate.root_index,
            )
        })
        .collect();
    let oracle: std::collections::BTreeSet<(Vec<u32>, i32)> = evidence
        .expected
        .iter()
        .map(|analysis| (analysis.morpheme_ids.clone(), analysis.root_morpheme_index))
        .collect();
    for (morpheme_ids, root_morpheme_index) in oracle {
        if !proposed.contains(&(morpheme_ids.clone(), root_morpheme_index)) {
            return Err(ContainmentGap {
                word: evidence.word.clone(),
                morpheme_ids,
                root_morpheme_index,
                required: 1,
                offered: 0,
            });
        }
    }
    Ok(())
}

/// Read-only evidence returned by the opt-in observed evaluator. `None` means evaluation failed
/// before a complete evidence vector existed; `Some(empty)` is a real, observed empty corpus.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEvaluationObservation {
    pub requested_strategy: EmissionStrategy,
    pub evaluation: RuntimeEvaluation,
    pub words: Option<Vec<WordEvidence>>,
}

/// `Clone` so `RunEvaluationCache` can serve one candidate's whole measurement to the next candidate compiling to the identical network; the clone is bounded by the distinct-net count, not the plan count. No separate `divergence` field: `RuntimeEvaluation::divergence` is this candidate's divergence, and a second copy here would only invite drift.
#[derive(Debug, Clone)]
pub struct EvaluatedPlan {
    evaluation: RuntimeEvaluation,
    words: Option<Vec<WordEvidence>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalRatioViolation {
    pub strategy: EmissionStrategy,
    pub numerator: u64,
    pub denominator: u64,
    pub threshold: u64,
}

impl std::fmt::Display for ProposalRatioViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "proposal ratio violation: strategy={:?} numerator={} denominator={} threshold={}",
            self.strategy, self.numerator, self.denominator, self.threshold
        )
    }
}

impl std::error::Error for ProposalRatioViolation {}

pub fn check_proposal_ratio(
    strategy: EmissionStrategy,
    numerator: u64,
    denominator: u64,
    threshold: u64,
) -> Result<(), ProposalRatioViolation> {
    if numerator > denominator.saturating_mul(threshold) {
        Err(ProposalRatioViolation {
            strategy,
            numerator,
            denominator,
            threshold,
        })
    } else {
        Ok(())
    }
}

/// How many identities a mismatch detail names before it stops listing them — a pathological word can disagree about thousands, and the exact counts (not the list) are what a reader acts on.
const MISMATCH_DETAIL_SAMPLE: usize = 4;

/// Compare one word occurrence's analyses as **deduplicated
/// `pg_parse::identity::AnalysisIdentity` sets**.
///
/// This is `crate::parity`'s relation applied; read that module for why it is the relation. The
/// two things it is NOT are worth restating at the call site, because both were previously what
/// this function did:
///
/// - It is not full `WordAnalysis` structural equality. Two analyses differing only in fields
///   `AnalysisIdentity` does not capture (`syn_fs`, `mpr`, the per-morpheme supplied-root slots) are
///   the SAME analysis, and this function now says so.
/// - It is not multiset equality. Multiplicity is not part of the relation, so a candidate that
///   reached one identity by three derivational paths agrees with an oracle that reached it by one.
///   The collapsed-path count survives as evidence on `WordEvidence`, not as a verdict.
///
/// Deduplication is strictly WITHIN this one occurrence. Corpus rows are separate observations;
/// `certify_corpus` never compares one row against another.
///
/// A projection failure or a v1-scope refusal comes back as a non-selectable
/// `Certification::Truncated` naming the fault and the side it was found on — never as a
/// mismatch, which would report an internal fault as a grammar disagreement.
pub fn certify_word(
    grammar: &Grammar,
    word: impl Into<String>,
    expected: &[WordAnalysis],
    actual: &[WordAnalysis],
) -> Certification {
    certify_word_measured(grammar, word, expected, actual).0
}

/// `certify_word` plus the counted `IdentityDivergence` of the same comparison.
///
/// The two are one function rather than two because the divergence must be measured on the SAME
/// projection the verdict came from. A second, independent pass would both double the projection
/// cost of every ordinary run and — worse — leave open the possibility of the counter and the
/// verdict disagreeing about what they looked at, which is exactly the property a soundness counter
/// cannot afford to lose.
///
/// The divergence is `IdentityDivergence::not_compared` whenever the verdict is a
/// `crate::parity::ParityFault`-derived truncation: a fault means no comparison happened, and
/// silently reporting a clean divergence for it would let "I could not look" read as "nothing was
/// wrong".
pub fn certify_word_measured(
    grammar: &Grammar,
    word: impl Into<String>,
    expected: &[WordAnalysis],
    actual: &[WordAnalysis],
) -> (Certification, IdentityDivergence) {
    let word = word.into();
    let expected_identities = match certified_occurrence(expected, grammar, ParitySide::Oracle) {
        Ok(identities) => identities,
        Err(fault) => {
            return (
                Certification::Truncated {
                    stage: fault.stage(),
                    corpus: None,
                },
                IdentityDivergence::not_compared(1),
            )
        }
    };
    let actual_identities = match certified_occurrence(actual, grammar, ParitySide::Candidate) {
        Ok(identities) => identities,
        Err(fault) => {
            return (
                Certification::Truncated {
                    stage: fault.stage(),
                    corpus: None,
                },
                IdentityDivergence::not_compared(1),
            )
        }
    };
    let divergence = IdentityDivergence::compare(&expected_identities, &actual_identities);
    if expected_identities.same_identities(&actual_identities) {
        return (
            Certification::FullHcConfirmed {
                words: 1,
                corpus_hash: "runtime".into(),
            },
            divergence,
        );
    }
    (
        Certification::IdentityMismatch {
            word,
            detail: describe_set_difference(&expected_identities, &actual_identities),
            direction: IdentityMismatchDirection::from_mismatch_divergence(&divergence),
        },
        divergence,
    )
}

fn describe_set_difference(
    expected: &OccurrenceIdentities,
    actual: &OccurrenceIdentities,
) -> String {
    let oracle_only = expected.identities_absent_from(actual);
    let candidate_only = actual.identities_absent_from(expected);
    format!(
        "deduplicated identity sets differ: oracle has {} distinct identities, candidate has {}; \
         {} oracle-only, {} candidate-only. oracle-only sample: {:?}; candidate-only sample: {:?}",
        expected.len(),
        actual.len(),
        oracle_only.len(),
        candidate_only.len(),
        &oracle_only[..oracle_only.len().min(MISMATCH_DETAIL_SAMPLE)],
        &candidate_only[..candidate_only.len().min(MISMATCH_DETAIL_SAMPLE)],
    )
}

fn corpus_completeness_evidence(
    requested: &[String],
    included: &[String],
    exclusions: &[CorpusExclusion],
    oracle: OracleEligibilityConfig,
) -> CorpusCompletenessEvidence {
    CorpusCompletenessEvidence::from_selection(requested, included, exclusions.to_vec(), oracle)
}

/// Certify a whole corpus, one OCCURRENCE at a time.
///
/// The row-by-row zip is load-bearing and is NOT what changed here: repeated corpus rows are
/// separate observations of the same word, never deduplicated against each other, so the row count
/// and the row order both remain part of the relation. Only the WITHIN-row comparison moved to
/// deduplicated identity-set equality.
pub fn certify_corpus(
    grammar: &Grammar,
    expected: &[(String, Vec<WordAnalysis>)],
    actual: &[(String, Vec<WordAnalysis>)],
) -> Certification {
    certify_corpus_measured(grammar, expected, actual).0
}

/// `certify_corpus` plus the counted `IdentityDivergence` summed over every row.
///
/// The divergence is accumulated over ALL rows even though the verdict is decided by the first
/// failure: the verdict only needs one witness, whereas the soundness counter is a claim about the
/// whole corpus and would be worthless if it stopped at the first disagreement. See
/// `certify_word_measured` for why the count and the verdict share one projection pass.
///
/// Callers with no evidence that a real backend produced `actual` (every caller but
/// `measure_and_certify_inner`) get the conservative reading of the vacuous-pass guard below: see
/// `certify_corpus_with_execution_evidence`.
pub fn certify_corpus_measured(
    grammar: &Grammar,
    expected: &[(String, Vec<WordAnalysis>)],
    actual: &[(String, Vec<WordAnalysis>)],
) -> (Certification, IdentityDivergence) {
    certify_corpus_with_execution_evidence(grammar, expected, actual, false)
}

/// `certify_corpus_measured` plus `backend_ran_for_real`: whether `actual` came from a real
/// backend evaluating real words, as opposed to hand-constructed comparison data.
///
/// The vacuous-pass guard below cannot be answered from per-word proposal counts: a fixture may be
/// an honest all-negative regression pin (every word `expect_fail`) where the correct proposer
/// output is legitimately zero raw candidates for every word, exactly what a vacuous network would
/// also produce. `Score::proposals` cannot tell the two apart -- only "did a real evaluation
/// pipeline run at all" can, and only the caller running that pipeline knows it.
pub fn certify_corpus_with_execution_evidence(
    grammar: &Grammar,
    expected: &[(String, Vec<WordAnalysis>)],
    actual: &[(String, Vec<WordAnalysis>)],
    backend_ran_for_real: bool,
) -> (Certification, IdentityDivergence) {
    if expected.len() != actual.len() {
        return (
            Certification::Truncated {
                stage: "full-hc".into(),
                corpus: None,
            },
            IdentityDivergence::not_compared(expected.len().max(actual.len()) as u64),
        );
    }
    let mut divergence = IdentityDivergence::default();
    let mut failures = Vec::new();
    for ((expected_word, expected_analyses), (actual_word, actual_analyses)) in
        expected.iter().zip(actual)
    {
        if expected_word != actual_word {
            divergence.absorb(IdentityDivergence::not_compared(1));
            failures.push(Certification::Truncated {
                stage: "full-hc-word-order".into(),
                corpus: None,
            });
            continue;
        }
        let (verdict, row) =
            certify_word_measured(grammar, expected_word, expected_analyses, actual_analyses);
        divergence.absorb(row);
        if !verdict.selectable() {
            failures.push(verdict);
        }
    }
    failures.sort_by_key(|verdict| {
        verdict
            .shortest_disagreement()
            .map(|word| (word.chars().count(), word.to_owned()))
    });
    if let Some(failure) = failures.into_iter().next() {
        return (failure, divergence);
    }
    // Agreeing about nothing is not agreement: if the HC oracle produced no analysis for any word, every per-word comparison is empty-set against empty-set, which certify_word calls equal, so an all-empty corpus would "confirm" any candidate, including one whose network is empty. `backend_ran_for_real` is what tells a vacuous network apart from a real backend correctly finding nothing on an honest all-negative fixture (every word `expect_fail`), which must certify, not truncate.
    let analyses: usize = expected.iter().map(|(_, a)| a.len()).sum();
    if analyses == 0 && !backend_ran_for_real {
        return (
            Certification::Truncated {
                stage: "no-analyzable-words".into(),
                corpus: None,
            },
            divergence,
        );
    }
    (
        Certification::FullHcConfirmed {
            words: expected.len() as u64,
            corpus_hash: "runtime".into(),
        },
        divergence,
    )
}
#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeBudget {
    pub states: Option<u64>,
    pub arcs: Option<u64>,
    pub build: Option<u64>,
    pub apply: Option<u64>,
    pub proposals: Option<u64>,
    pub confirmation: Option<u64>,
    /// Ground-truth oracle work cap. UNLIKE every field above, `None` here does NOT mean
    /// "unbounded" — it means "caller did not override the default", because unbounded is exactly
    /// the defect this field exists to close (an unbounded oracle `Morpher` call is what hung the
    /// deep-truncation-chain grammar's pilot indefinitely). `evaluate_plans` resolves `None`
    /// to `DEFAULT_ORACLE_WORK_CAP`. A caller that genuinely wants the old unbounded behavior must
    /// say so explicitly with `Some(usize::MAX)`.
    pub oracle_work_cap: Option<usize>,
    /// Ground-truth oracle wall-clock LIVENESS NET — not a classifier. Same "`None` = use the
    /// default, not unbounded" convention as `oracle_work_cap` immediately above; resolves to
    /// `DEFAULT_ORACLE_LIVENESS_NET`. Tripping it aborts preparation with
    /// `OraclePreparationFault::LivenessNetTripped`.
    pub oracle_liveness_net: Option<Duration>,
    /// Declared resident-memory ceiling for oracle preparation, in bytes. Same `None` convention;
    /// resolves to `DEFAULT_ORACLE_MEMORY_CEILING_BYTES`. `Some(u64::MAX)` declares no ceiling
    /// (and is recorded as such in the evidence, so "unbounded" is a stated choice, not a silence).
    pub oracle_memory_ceiling: Option<u64>,
    /// CANDIDATE-side per-word raw `apply_up` path ceiling. Same "`None` = caller did not override
    /// the default, NOT unbounded" convention as `oracle_work_cap` above, and for the same reason in
    /// the mirror-image direction: the oracle field exists because an unbounded ORACLE hung the run,
    /// and this one exists because an unbounded CANDIDATE PROPOSE killed the process outright
    /// (see `crate::compose_budget::DEFAULT_EVALUATION_APPLY_PATH_BUDGET`
    /// for the full measurement and the calibration argument). Resolves to that constant. A caller
    /// that genuinely wants the old unbounded behavior must say so explicitly with `Some(usize::MAX)`.
    pub apply_path_budget: Option<usize>,
    /// CANDIDATE-side per-word distinct-candidate ceiling; same `None` convention, resolving to
    /// `crate::compose_budget::DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET`.
    pub apply_candidate_budget: Option<usize>,
}

impl RuntimeBudget {
    /// The per-word apply-path envelope this budget puts in force, resolved.
    ///
    /// `Some(usize::MAX)` on either field is honoured as `None` on the `ApplyBudget` — i.e. genuinely
    /// unbounded — because that is the explicit opt-out the two field docs name, and
    /// `ApplyBudget`'s own caps are `Option`s where `None` already means unbounded. Anything else,
    /// including the ordinary `None`, resolves to the calibrated default.
    pub fn resolved_apply_budget(&self) -> ApplyBudget {
        fn resolve(declared: Option<usize>, default: usize) -> Option<usize> {
            match declared {
                Some(usize::MAX) => None,
                Some(limit) => Some(limit),
                None => Some(default),
            }
        }
        ApplyBudget::with_caps(
            resolve(
                self.apply_path_budget,
                crate::compose_budget::DEFAULT_EVALUATION_APPLY_PATH_BUDGET,
            ),
            resolve(
                self.apply_candidate_budget,
                crate::compose_budget::DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET,
            ),
        )
    }

    pub fn resolved_oracle_work_cap(&self) -> usize {
        self.oracle_work_cap.unwrap_or(DEFAULT_ORACLE_WORK_CAP)
    }

    pub fn resolved_oracle_liveness_net(&self) -> Duration {
        self.oracle_liveness_net
            .unwrap_or(DEFAULT_ORACLE_LIVENESS_NET)
    }

    pub fn resolved_oracle_memory_ceiling(&self) -> u64 {
        self.oracle_memory_ceiling
            .unwrap_or(DEFAULT_ORACLE_MEMORY_CEILING_BYTES)
    }

    /// The three bounds, resolved, exactly as they are bound into the eligibility evidence.
    pub fn oracle_eligibility_config(&self) -> OracleEligibilityConfig {
        OracleEligibilityConfig {
            work_cap: self.resolved_oracle_work_cap() as u64,
            memory_ceiling_bytes: self.resolved_oracle_memory_ceiling(),
            liveness_net_ns: u64::try_from(self.resolved_oracle_liveness_net().as_nanos())
                .unwrap_or(u64::MAX),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEvaluation {
    pub certification: Certification,
    pub score: Score,
    /// Which compiler ACTUALLY produced the measured network, as opposed to which one the candidate
    /// declared.
    ///
    /// These can differ, and the difference is invisible without this field. Anything attributing a
    /// measurement -- a report field, diagram caption, or comparison -- must read THIS, not the
    /// declared strategy.
    pub realized_strategy: EmissionStrategy,
    /// This candidate's counted `crate::parity::IdentityDivergence` over its own corpus pass.
    ///
    /// Exposed here -- rather than discarded at the `evaluate_plans_with_cache` /
    /// `evaluate_plans_observed_with_cache` boundary, as it used to be -- because
    /// `IdentityDivergence` is the one place in this crate that separates a recall failure
    /// (`oracle_only_identities`, ADR-0001's "never miss") from a surviving over-generation
    /// (`candidate_only_identities`, a soundness defect) with typed fields, and every caller of
    /// those two entry points is exactly who needs to see the split. `IdentityDivergence::not_compared`
    /// whenever this evaluation short-circuited before any occurrence could be compared (a
    /// resource breach, an excluded corpus, a per-word apply-budget refusal) -- see that
    /// constructor's own doc for why "nothing compared" must never read as a clean zero.
    pub divergence: IdentityDivergence,
    /// `production_admission::assess_completed_fst`'s verdict on THIS candidate,
    /// orthogonal to `certification`: `certification` answers accuracy/parity against the oracle,
    /// this answers publishability of the compiled artifact, and neither stands in for the other
    /// -- a candidate can be accurate but unpublishable (a partial-bearing grammar), or compile
    /// cleanly for production while still failing to reproduce the oracle.
    pub production_health: HealthReport,
}

impl RuntimeEvaluation {
    /// Selection needs BOTH answers: an accurate candidate that may not be published is not selectable.
    pub fn selectable(&self) -> bool {
        self.certification.selectable() && !self.production_blocks_publication()
    }

    /// The one place this candidate's `production_health` is turned into a publication verdict, so
    /// every caller (this type's own `selectable`, and anything threading the fact onward into a
    /// `ConfirmationEvidence`/`CandidateReport`) reads the SAME comparison rather than each
    /// re-deriving it against `Severity::NotProductionReady` by hand.
    pub fn production_blocks_publication(&self) -> bool {
        crate::production_admission::health_blocks_publication(&self.production_health)
    }
}

/// Runs `words` through `analyzer`, scores, budget-checks, and certifies against `expected` — shared by every evaluation strategy so only the network-acquisition step can differ between them. The ordinary (unobserved) measurement; `words` is always `None` in `EvaluatedPlan`.
#[allow(clippy::too_many_arguments)]
fn measure_and_certify(
    realized_strategy: EmissionStrategy,
    grammar: &Grammar,
    analyzer: &mut FomaAnalyzer,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
    states: u64,
    arcs: u64,
    build: u64,
) -> EvaluatedPlan {
    measure_and_certify_inner::<false>(
        realized_strategy,
        grammar,
        analyzer,
        words,
        expected,
        budget,
        states,
        arcs,
        build,
    )
}

#[allow(clippy::too_many_arguments)]
fn measure_and_certify_observed(
    realized_strategy: EmissionStrategy,
    grammar: &Grammar,
    analyzer: &mut FomaAnalyzer,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
    states: u64,
    arcs: u64,
    build: u64,
) -> EvaluatedPlan {
    measure_and_certify_inner::<true>(
        realized_strategy,
        grammar,
        analyzer,
        words,
        expected,
        budget,
        states,
        arcs,
        build,
    )
}

/// The first declared resource limit `score` exceeds, in the fixed dimension order below, or `None` — kept as one function rather than two inline copies, since a net-level dedup hit re-decides this on its own `build` time, and two copies would risk drifting in the ORDER, which decides which dimension a breach names.
fn budget_breach(score: &Score, budget: RuntimeBudget) -> Option<(&'static str, u64, u64)> {
    [
        ("states", score.states, budget.states),
        ("arcs", score.arcs, budget.arcs),
        ("build", score.build, budget.build),
        ("apply", score.apply, budget.apply),
        ("proposals", score.proposals, budget.proposals),
        ("confirmation", score.confirmation, budget.confirmation),
    ]
    .into_iter()
    .find_map(|(dimension, value, limit)| {
        limit
            .filter(|limit| value > *limit)
            .map(|limit| (dimension, value, limit))
    })
}

/// Fail-closed health for a candidate that never reached a completed FST to assess -- "could not look" must never read as "everything is fine".
fn unassessed_production_health(detail: String) -> HealthReport {
    HealthReport::new(vec![HealthFinding::new(
        FindingCode::BuildProcessFailed,
        Severity::NotProductionReady,
        Phase::Compile,
        Metric::UnknownUnboundedWork,
        MetricValue::Unbounded,
        ValueProvenance::Observed,
        detail,
    )])
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn measure_and_certify_inner<const OBSERVE: bool>(
    realized_strategy: EmissionStrategy,
    grammar: &Grammar,
    analyzer: &mut FomaAnalyzer,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
    states: u64,
    arcs: u64,
    build: u64,
) -> EvaluatedPlan {
    // Computed once and shared by every return below: a pure function of (grammar, strategy), not of `words`.
    let production_health = match assess_completed_fst(grammar, realized_strategy) {
        Ok(admission) => admission.health().clone(),
        Err(e) => {
            unassessed_production_health(format!("production admission assessment failed: {e}"))
        }
    };
    let mut actual = Vec::new();
    let mut observed_proposals = OBSERVE.then(|| Vec::with_capacity(words.len()));
    let mut apply: u64 = 0;
    let mut proposals: u64 = 0;
    let mut confirmation: u64 = 0;
    let mut confirmation_work: u64 = 0;
    let mut raw_paths: u64 = 0;
    // The per-word apply-path envelope, declared and never unbounded by default — see DEFAULT_EVALUATION_APPLY_PATH_BUDGET for the measurement behind the default.
    let apply_budget = budget.resolved_apply_budget();
    for w in words {
        let t = Instant::now();
        // One call for both evidence modes: the non-observed arm just drops the candidate vector, so a refusal can't depend on which mode is active by construction, not by keeping two call sites in sync.
        let budgeted =
            analyzer.analyze_word_with_diagnostics_budgeted_with_candidates(w, &apply_budget);
        let (outcome, diagnostics, proposals_for_word) = match budgeted {
            ProfiledFomaApplyOutcomeWithCandidates::Complete(profiled) => (
                profiled.outcome,
                profiled.diagnostics,
                OBSERVE.then_some(profiled.candidates),
            ),
            // A refusal, not a truncation: the word proposed more than the declared envelope, so no partial proposal set is ever compared against the oracle, and the divergence is "nothing compared" rather than a clean zero.
            ProfiledFomaApplyOutcomeWithCandidates::Incomplete {
                dimension,
                value,
                limit,
                diagnostics,
            } => {
                let apply = apply.saturating_add(elapsed_ns(t).max(1));
                return EvaluatedPlan {
                    evaluation: RuntimeEvaluation {
                        certification: Certification::ResourceBreach {
                            dimension: format!("per-word apply {} ({w})", dimension.label()),
                            value: value as u64,
                            limit: limit as u64,
                        },
                        score: Score {
                            states,
                            arcs,
                            build,
                            apply,
                            proposals,
                            confirmation,
                            confirmation_work,
                            raw_paths: raw_paths.saturating_add(diagnostics.raw_paths as u64),
                        },
                        realized_strategy,
                        divergence: IdentityDivergence::not_compared(expected.len() as u64),
                        production_health: production_health.clone(),
                    },
                    words: None,
                };
            }
        };
        apply = apply.saturating_add(elapsed_ns(t).max(1));
        proposals = proposals.saturating_add(outcome.candidates_generated as u64);
        confirmation = confirmation.saturating_add(diagnostics.confirmation_calls as u64);
        confirmation_work = confirmation_work.saturating_add(diagnostics.confirmation_work as u64);
        raw_paths = raw_paths.saturating_add(diagnostics.raw_paths as u64);
        actual.push((w.clone(), outcome.structured));
        if let Some(proposals_for_word) = proposals_for_word {
            observed_proposals
                .as_mut()
                .expect("observed mode must initialize proposal evidence")
                .push(proposals_for_word);
        }
    }
    let score = Score {
        states,
        arcs,
        build,
        apply,
        proposals,
        confirmation,
        confirmation_work,
        raw_paths,
    };
    let breach = budget_breach(&score, budget);
    let (certification, divergence) = match breach {
        // A breach short-circuits before any comparison happens, so the honest divergence is "nothing compared", never a clean zero.
        Some((dimension, value, limit)) => (
            Certification::ResourceBreach {
                dimension: dimension.into(),
                value,
                limit,
            },
            IdentityDivergence::not_compared(expected.len() as u64),
        ),
        _ => {
            // Reaching here already proves a real backend built a network and evaluated every word in `words` -- that execution is the evidence, independent of how many raw candidates it happened to generate.
            let (verdict, divergence) = certify_corpus_with_execution_evidence(
                grammar,
                expected,
                &actual,
                !words.is_empty(),
            );
            let verdict = match verdict {
                Certification::FullHcConfirmed {
                    words: word_count, ..
                } => Certification::FullHcConfirmed {
                    words: word_count,
                    corpus_hash: corpus_hash(words),
                },
                failure => failure,
            };
            (verdict, divergence)
        }
    };
    let words = observed_proposals.map(|proposals| {
        expected
            .iter()
            .zip(actual)
            .zip(proposals)
            .map(|(((word, expected), (_, actual)), proposals)| {
                // Re-projected here (opt-in observed path only) rather than threaded out of certify_corpus, keeping the certification path free of an evidence out-parameter; `.ok()` is correct since a projection failing here failed there too, and the certification already carries the typed fault.
                let expected_identities = OccurrenceIdentities::project(expected, grammar).ok();
                let actual_identities = OccurrenceIdentities::project(&actual, grammar).ok();
                WordEvidence {
                    word: word.clone(),
                    expected: expected.clone(),
                    actual,
                    proposals,
                    expected_identities,
                    actual_identities,
                }
            })
            .collect()
    });
    EvaluatedPlan {
        evaluation: RuntimeEvaluation {
            certification,
            score,
            realized_strategy,
            divergence,
            production_health,
        },
        words,
    }
}

/// Shared constructor for every evaluation outcome whose `Score` is zeroed except `build`, so every future `Score` field addition has exactly one place to account for it instead of drifting across re-inlined literals.
fn failed_evaluation(
    realized_strategy: EmissionStrategy,
    certification: Certification,
    build: u64,
    divergence: IdentityDivergence,
    production_health: HealthReport,
) -> RuntimeEvaluation {
    RuntimeEvaluation {
        realized_strategy,
        certification,
        score: Score {
            states: 0,
            arcs: 0,
            build,
            apply: 0,
            proposals: 0,
            confirmation: 0,
            confirmation_work: 0,
            raw_paths: 0,
        },
        divergence,
        production_health,
    }
}

/// A failure that happened before any occurrence could be compared, recorded as `not_compared` so this run cannot report the clean zero of one that actually compared. No completed FST was ever produced, so `production_health` is always the fail-closed verdict, never the grammar's own admission.
fn failed_evaluated_over(
    realized_strategy: EmissionStrategy,
    certification: Certification,
    build: u64,
    occurrences: u64,
    production_health: HealthReport,
) -> EvaluatedPlan {
    let divergence = IdentityDivergence::not_compared(occurrences);
    EvaluatedPlan {
        evaluation: failed_evaluation(
            realized_strategy,
            certification,
            build,
            divergence,
            production_health,
        ),
        words: None,
    }
}

fn build_failed_evaluated(
    realized_strategy: EmissionStrategy,
    reason: String,
    build: u64,
    occurrences: u64,
) -> EvaluatedPlan {
    let production_health = unassessed_production_health(reason.clone());
    failed_evaluated_over(
        realized_strategy,
        Certification::BuildFailed { reason },
        build,
        occurrences,
        production_health,
    )
}

/// `EmissionStrategy::TunedSurfaceProbed`: the default compilation of this grammar, through `FomaProposer::new` (emit -> lexc -> foma compile).
pub(crate) fn evaluate_via_tuned_emit_mode<const OBSERVE: bool>(
    grammar: &Grammar,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
) -> EvaluatedPlan {
    let t = Instant::now();
    let proposer = match crate::analyzer::compile_proposer(grammar) {
        Ok(p) => p,
        Err(e) => {
            return build_failed_evaluated(
                EmissionStrategy::TunedSurfaceProbed,
                format!("tuned emit path failed to build: {e}"),
                elapsed_ns(t).max(1),
                expected.len() as u64,
            )
        }
    };
    let build = elapsed_ns(t).max(1);
    let (states, arcs) = proposer.network_counts();
    // No with_segment_query_encoder here: this net is in plain surface space, queried with plain NFD.
    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(grammar, proposer);
    if OBSERVE {
        measure_and_certify_observed(
            EmissionStrategy::TunedSurfaceProbed,
            grammar,
            &mut analyzer,
            words,
            expected,
            budget,
            states.max(0) as u64,
            arcs.max(0) as u64,
            build,
        )
    } else {
        measure_and_certify(
            EmissionStrategy::TunedSurfaceProbed,
            grammar,
            &mut analyzer,
            words,
            expected,
            budget,
            states.max(0) as u64,
            arcs.max(0) as u64,
            build,
        )
    }
}

/// Evaluates every candidate through its own `EmissionStrategy`/`crate::backend::Backend` and the production
/// propose→confirm pipeline. The caller-provided order is preserved.
///
/// Each candidate is measured by the adapter it names; no candidate is rerouted to a different adapter.
///
/// # Neither positional NOR parallel-slice baseline state is used here
/// This function used to derive `is_baseline` from POSITION (`i == 0`), and a second entry point took
/// it as a parallel `&[bool]` kept honest only by a length assertion. Both are gone: the fact is
/// `crate::enumerate::LoweredCandidate::role`, carried by the candidate it is a fact about. See
/// `crate::enumerate::CandidateRole` for the two measured failures those shapes produced.
pub fn evaluate_plans(
    grammar: &Grammar,
    plans: &[LoweredCandidate],
    words: &[String],
    budget: RuntimeBudget,
) -> Result<Vec<RuntimeEvaluation>, OraclePreparationFault> {
    eprintln!(
        "runtime-evaluate: preparing oracle for {} word(s)",
        words.len()
    );
    let mut cache = RunEvaluationCache::prepare(grammar, words, budget)?;
    eprintln!(
        "runtime-evaluate: oracle prepared; evaluating {} candidate(s)",
        plans.len()
    );
    Ok(evaluate_plans_with_cache(
        grammar, plans, words, budget, &mut cache,
    ))
}

/// `evaluate_plans` against a caller-owned run cache, so one optimizer run shares a single prepared
/// oracle.
pub fn evaluate_plans_with_cache(
    grammar: &Grammar,
    plans: &[LoweredCandidate],
    words: &[String],
    budget: RuntimeBudget,
    cache: &mut RunEvaluationCache,
) -> Vec<RuntimeEvaluation> {
    evaluate_plans_with_cache_mode::<false>(grammar, plans, words, budget, cache)
        .into_iter()
        .map(|result| result.evaluation)
        .collect()
}
/// Evaluate candidates against a caller-owned cache while retaining exact per-word oracle,
/// confirmed-analysis, and final-candidate evidence for equivalence gates.
#[doc(hidden)]
pub fn evaluate_plans_observed_with_cache(
    grammar: &Grammar,
    plans: &[LoweredCandidate],
    words: &[String],
    budget: RuntimeBudget,
    cache: &mut RunEvaluationCache,
) -> Vec<RuntimeEvaluationObservation> {
    evaluate_plans_with_cache_mode::<true>(grammar, plans, words, budget, cache)
        .into_iter()
        .zip(plans)
        .map(|(result, plan)| RuntimeEvaluationObservation {
            requested_strategy: plan.strategy(),
            evaluation: result.evaluation,
            words: result.words,
        })
        .collect()
}

/// Per-candidate inputs to `crate::backend::Backend::evaluate_for_corpus`: the run-invariant
/// grammar, corpus and budget pieces every backend reads.
pub struct CorpusEvalContext<'g> {
    pub grammar: &'g Grammar,
    pub words: &'g [String],
    pub expected: &'g [(String, Vec<WordAnalysis>)],
    pub budget: RuntimeBudget,
    pub observe: bool,
}

fn evaluate_plans_with_cache_mode<const OBSERVE: bool>(
    grammar: &Grammar,
    plans: &[LoweredCandidate],
    words: &[String],
    budget: RuntimeBudget,
    cache: &mut RunEvaluationCache,
) -> Vec<EvaluatedPlan> {
    let selection = cache.select(words);
    let comparable = selection.comparable;
    let expected = selection.expected;
    let oracle_capped = selection.capped;
    let exclusions = selection.exclusions;
    let corpus_evidence =
        corpus_completeness_evidence(words, &comparable, &exclusions, cache.corpus.oracle);
    let words = &comparable[..];
    // A work-capped oracle result must never reach certify_corpus: a genuinely incomplete candidate could look right against an equally truncated ground truth and wrongly certify, so this returns before build_candidate runs for any plan in the batch. Certification is all-or-nothing over the requested corpus — refusing the whole batch rather than silently certifying a subset under the full corpus's name and hash.
    if !exclusions.is_empty() {
        let stage = if oracle_capped {
            "oracle-capped"
        } else {
            "corpus-incomplete"
        };
        let refused = plans
            .iter()
            .map(|plan| {
                // Nothing compiled since the corpus itself was refused, so the honest answer is the strategy that was requested.
                failed_evaluated_over(
                    plan.strategy(),
                    Certification::Truncated {
                        stage: stage.into(),
                        corpus: Some(corpus_evidence.clone()),
                    },
                    0,
                    corpus_evidence.requested,
                    unassessed_production_health(format!(
                        "corpus {stage}: no completed FST was built to assess"
                    )),
                )
            })
            .collect::<Vec<_>>();
        for candidate in &refused {
            cache.absorb_divergence(candidate.evaluation.divergence);
        }
        return refused;
    }
    let evaluated: Vec<EvaluatedPlan> = plans
        .iter()
        .map(|candidate| {
            let mut ctx = CorpusEvalContext {
                grammar,
                words,
                expected: expected.as_slice(),
                budget,
                observe: OBSERVE,
            };
            backend_for(candidate.adapter).evaluate_for_corpus(candidate, &mut ctx)
        })
        .collect();
    // Folded here, after the closure's mutable borrow of `cache` ends, so every path out of this function contributes its divergence exactly once.
    for candidate in &evaluated {
        cache.absorb_divergence(candidate.evaluation.divergence);
    }
    evaluated
}

/// **Assess ACCURACY — "did we undergenerate?" — with ZERO full-HC confirmation calls per
/// candidate.**
///
/// This is the fast path the whole objective turns on: a rough pass/fail over the eligible
/// vocabulary, cheap enough to run as a regression gate on every change, rather than a full
/// certification battery. Read `crate::backend_accuracy`'s module doc first — it carries the
/// soundness argument, the one hazard, and the reasons this is NOT a certification.
///
/// # What it does per candidate
/// 1. Realizes the candidate's network exactly as `evaluate_plans_with_cache` does, through the
///    same backend. A different network would make the verdict a statement about something else.
/// 2. Proposes over the corpus through `crate::composite::propose_union_peel_with_diagnostics`,
///    the same propose-UNION-peel the certification path uses.
/// 3. Checks admission-key containment against the oracle result THIS RUN already prepared once per
///    occurrence.
///
/// # What it deliberately does not do
/// - **It builds no `pg_parse::Morpher`.** That is the expensive confirm-side object
///   (`Morpher::new` runs `RuleCache::build`, compiling every matcher FST in the grammar), and there
///   is none in scope here — so "zero confirmation calls" is enforced by what this function can
///   reach, not by a counter it remembers to keep at zero. The counter is reported anyway, from the
///   same diagnostics field the certification path reads its `Score::confirmation` from, so the claim
///   is checkable rather than merely asserted.
/// - **It computes no `Score` and moves no ranking.** Containment cannot price a compilation; the
///   objective stays `confirmation_work`-led and untouched.
/// - **It never truncates a proposal set and never caps per-candidate work.** Either would be
///   indistinguishable from the recall failure this exists to find.
///
/// # Corpus eligibility is the certification path's, unchanged
/// The same all-or-nothing rule applies: if ANY requested occurrence was excluded (a work-capped
/// oracle result, an unprepared row), every candidate comes back
/// `crate::backend_accuracy::AccuracyVerdict::NotDetermined` rather than assessed over a silently
/// narrowed corpus. Assessing a subset under the requested corpus's name is the failure mode
/// `PreparedCorpus` exists to prevent, and it is no more acceptable for a cheap verdict than for
/// an expensive one.
pub fn assess_accuracy_with_cache(
    grammar: &Grammar,
    plans: &[LoweredCandidate],
    words: &[String],
    budget: RuntimeBudget,
    cache: &mut RunEvaluationCache,
) -> Vec<CandidateAccuracy> {
    let selection = cache.select(words);
    if !selection.exclusions.is_empty() {
        let stage = if selection.capped {
            "oracle-capped"
        } else {
            "corpus-incomplete"
        };
        return plans
            .iter()
            .map(|plan| CandidateAccuracy {
                requested_strategy: plan.strategy(),
                realized_strategy: plan.strategy(),
                verdict: AccuracyVerdict::NotDetermined {
                    reason: format!(
                        "corpus eligibility refused the batch ({stage}): {} of {} requested \
                         occurrences excluded",
                        selection.exclusions.len(),
                        words.len()
                    ),
                },
                counters: AccuracyCounters::default(),
                apply_refusal_witnesses: Vec::new(),
            })
            .collect();
    }
    // Grammar-static and reusable across every candidate: the peeler is a pure function of the grammar taking &self. Note what is NOT built beside it — no morpher, no morpheme-owner map, since nothing here confirms.
    let peeler = crate::peel::ReduplicationPeeler::new(grammar);
    let peel_budget = ComposeBudget::from_env();
    let apply_budget = budget.resolved_apply_budget();
    plans
        .iter()
        .map(|candidate| {
            let realized = realize_accuracy_proposer(candidate, grammar);
            let (realized_strategy, proposer) = match realized {
                Ok(ready) => ready,
                Err((realized_strategy, reason)) => {
                    return CandidateAccuracy {
                        requested_strategy: candidate.strategy(),
                        realized_strategy,
                        verdict: AccuracyVerdict::NotDetermined { reason },
                        counters: AccuracyCounters::default(),
                        apply_refusal_witnesses: Vec::new(),
                    }
                }
            };
            assess_one(
                candidate.strategy(),
                realized_strategy,
                grammar,
                proposer,
                &peeler,
                &peel_budget,
                &apply_budget,
                &selection,
            )
        })
        .collect()
}

/// Realizes one candidate into the proposer the accuracy check will propose from — the same network the certification path measures, by construction. Dispatches through `crate::backend::Backend::realize_accuracy_proposer`.
fn realize_accuracy_proposer(
    candidate: &LoweredCandidate,
    grammar: &Grammar,
) -> Result<(EmissionStrategy, FomaProposer), (EmissionStrategy, String)> {
    let backend = backend_for(candidate.adapter);
    backend
        .realize_accuracy_proposer(candidate, grammar)
        .map(|proposer| (backend.strategy(), proposer))
        .map_err(|reason| (backend.strategy(), reason))
}

/// `crate::backend::LexcMainline`'s own proposer realization, called through `crate::backend::Backend::realize_accuracy_proposer`.
pub(crate) fn realize_tuned_surface_proposer(grammar: &Grammar) -> Result<FomaProposer, String> {
    crate::analyzer::compile_proposer(grammar)
        .map_err(|e| format!("tuned emit path failed to build: {e}"))
}

/// Proposes over the eligible corpus and checks containment; no confirmation engine is reachable from here, deliberately.
// Two strategies, a grammar, a proposer and three budgets: each is read independently, so a bundle would only rename them.
#[allow(clippy::too_many_arguments)]
fn assess_one(
    requested_strategy: EmissionStrategy,
    realized_strategy: EmissionStrategy,
    grammar: &Grammar,
    mut proposer: FomaProposer,
    peeler: &crate::peel::ReduplicationPeeler,
    peel_budget: &ComposeBudget,
    apply_budget: &ApplyBudget,
    selection: &PreparedSelection,
) -> CandidateAccuracy {
    let mut counters = AccuracyCounters::default();
    let mut misses = Vec::new();
    let mut apply_refusal_witnesses = Vec::new();
    for (occurrence_ordinal, (word, oracle)) in selection.expected.iter().enumerate() {
        // Bounded, and the bound is a refusal (apply_refusals forces NotDetermined, never Undergenerated), not unbounded: an unbounded apply_up path count can grow large enough to kill the process outright on a pathological word, and a dead process reports no verdict at all — strictly worse than an honest NotDetermined.
        let proposed = crate::composite::propose_union_peel_with_diagnostics(
            grammar,
            &mut proposer,
            peeler,
            peel_budget,
            word,
            apply_budget,
        );
        let (proposals, _peel_used, peel_chain_depth_error, diagnostics, proposal_calls) =
            match proposed {
                Ok(complete) => complete,
                Err((dimension, value, limit, diagnostics, proposal_calls)) => {
                    // This occurrence is counted as checked and refused, never silently skipped, so the refusal share in verdict_from's message is over the real denominator.
                    apply_refusal_witnesses.push(crate::backend_accuracy::ApplyRefusalWitness {
                        word: word.clone(),
                        dimension: dimension.label(),
                        value: value as u64,
                        limit: limit as u64,
                    });
                    counters.absorb(AccuracyCounters {
                        occurrences_checked: 1,
                        apply_refusals: 1,
                        raw_paths: diagnostics.raw_paths as u64,
                        proposal_calls: proposal_calls as u64,
                        ..AccuracyCounters::default()
                    });
                    continue;
                }
            };
        let mut occurrence = crate::backend_accuracy::check_occurrence(
            word,
            occurrence_ordinal as u64,
            oracle,
            &proposals,
            &mut misses,
        );
        occurrence.raw_paths = diagnostics.raw_paths as u64;
        occurrence.proposal_calls = proposal_calls as u64;
        // A refused peel means this occurrence's proposal set is incomplete; verdict_from turns any non-zero total into NotDetermined, since a containment verdict over a truncated set is not a verdict.
        occurrence.peel_refusals = u64::from(peel_chain_depth_error.is_some());
        counters.absorb(occurrence);
    }
    CandidateAccuracy {
        requested_strategy,
        realized_strategy,
        verdict: crate::backend_accuracy::verdict_from(&counters, misses),
        counters,
        apply_refusal_witnesses,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
