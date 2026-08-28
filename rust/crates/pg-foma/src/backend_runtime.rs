//! Production evaluator for backend plans.

use crate::analyzer::FomaProposer;
use crate::backend_accuracy::{AccuracyCounters, AccuracyVerdict, CandidateAccuracy};
use crate::backend_optimizer::{
    Certification, CorpusCompletenessEvidence, CorpusExclusion, OracleEligibilityConfig, Score,
};
use crate::build::build_controllable;
use crate::compose_budget::{ApplyBudget, ComposeBudget, ComposeError};
use crate::composite::{FomaAnalyzer, ProfiledFomaApplyOutcomeWithCandidates};
use crate::emit::surface_table;
use crate::enumerate::{EmissionStrategy, LoweredCandidate};
use crate::lowering_adapter::LoweringAdapter;
use crate::parity::{certified_occurrence, IdentityDivergence, OccurrenceIdentities, ParitySide};
use crate::replace::SegAlphabet;
use crate::tags::Candidate;
use foma::options::FomaOptions;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::WordAnalysis;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

/// What the ground-truth oracle concluded about ONE corpus occurrence: deliberately no `TimedOut`/`MemoryCapped` variant, since a wall-clock outcome is unreproducible under load and can mask a real step-cap exhaustion, so both become whole-run `OraclePreparationFault`s instead of per-word exclusions.
#[derive(Debug, Clone)]
enum OracleOutcome {
    /// The oracle finished this occurrence within its step cap. The analyses are the ground truth.
    Complete(Vec<WordAnalysis>),
    /// The oracle exhausted its step cap on this occurrence — the only eligibility classifier, deterministic per (grammar, word, cap).
    StepCapped,
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
    /// The wall-clock liveness net tripped before the step cap could classify this word, so the honest report is "could not determine", never "excluded".
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
struct RssSampler(sysinfo::System);

#[cfg(not(target_arch = "wasm32"))]
impl RssSampler {
    fn new() -> Self {
        Self(sysinfo::System::new())
    }

    /// Refreshes only this pid, never a system-wide scan, matching worker.rs's compile-child guardrail.
    fn sample(&mut self) -> Option<u64> {
        let pid = sysinfo::get_current_pid().ok()?;
        self.0
            .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        self.0.process(pid).map(|process| process.memory())
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

/// A run-scoped corpus prepared once from the oracle. The ground truth and the step-cap exclusion
/// latches are shared by the pilot and every candidate evaluation in that run — which is what makes
/// exclusions CANDIDATE-INDEPENDENT by construction: nothing a candidate does can reach this.
#[derive(Debug)]
pub struct PreparedCorpus {
    words: Vec<PreparedWord>,
    oracle_calls: usize,
    oracle: OracleEligibilityConfig,
}

impl PreparedCorpus {
    /// Runs the ground-truth oracle over `words` and classifies each occurrence by STEP CAP alone.
    ///
    /// Returns `Err` — aborting the whole run — if the wall-clock liveness net or the declared
    /// memory ceiling trips. Neither can produce a row.
    pub fn prepare(
        grammar: &Grammar,
        words: &[String],
        budget: RuntimeBudget,
    ) -> Result<Self, OraclePreparationFault> {
        let oracle = budget.oracle_eligibility_config();
        let cap = budget.resolved_oracle_step_cap();
        let net = budget.resolved_oracle_liveness_net();
        let ceiling = budget.resolved_oracle_memory_ceiling();
        let morpher = pg_parse::Morpher::new(grammar, cap).with_word_timeout(Some(net));
        let mut sampler = RssSampler::new();
        let mut records = Vec::with_capacity(words.len());
        for (requested_ordinal, word) in words.iter().enumerate() {
            let outcome = morpher.parse_word(word);
            // Checked first and unconditionally: `capped` can be true alongside `timed_out`, and treating that as an exclusion would mask a genuine timeout as a step-cap exclusion.
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
                    OracleOutcome::StepCapped
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
                OracleOutcome::StepCapped => {
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

/// Deterministic work a net-level dedup hit did NOT have to do.
///
/// Every field is a COUNT, never an elapsed time. That is not stylistic: this crate ranks candidates
/// by deterministic work precisely because time decided ties by noise (`Score::key`), and a saving
/// reported in nanoseconds could not be asserted by a gate at all. `nets_deduped` provably reads 0
/// with `RunEvaluationCache::without_net_dedup` and non-zero without it, which is what makes "the
/// mechanism engaged" a measurement rather than a claim.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetDedupSavings {
    /// Candidates whose entire measurement was served from an earlier candidate's identical network.
    pub nets_deduped: usize,
    /// Distinct finished networks RECORDED as reusable. Not simply "candidates that paid a corpus
    /// pass": a candidate whose certification is a `Certification::ResourceBreach` pays the pass and
    /// is deliberately never recorded (see `RunEvaluationCache::record_net_measurement`), so this can
    /// be smaller than the number of full passes performed.
    pub distinct_nets: usize,
    /// Corpus word applications (propose calls) skipped: one per comparable word per deduped
    /// candidate. Skipping propose as well as confirm is the point — memoizing confirmation alone
    /// would leave the whole corpus traversal in place.
    pub propose_calls_avoided: u64,
    /// Full-HC confirmation CALLS skipped, summed from each donor's own measured count.
    pub confirmation_calls_avoided: u64,
    /// Full-HC confirmation STEPS skipped — the unit `Score::key` ranks on.
    pub confirmation_steps_avoided: u64,
}

/// All prepared, reusable evaluation inputs for one optimizer run.
#[derive(Debug)]
pub struct RunEvaluationCache {
    corpus: PreparedCorpus,
    divergence: IdentityDivergence,
    /// Measurements indexed by net_reuse_key; `None` disables net-level dedup for this cache (see `Self::without_net_dedup`), enabling a negative-control gate.
    net_measurements: Option<std::collections::HashMap<String, EvaluatedPlan>>,
    savings: NetDedupSavings,
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
            net_measurements: Some(std::collections::HashMap::new()),
            savings: NetDedupSavings::default(),
        })
    }

    /// Turn net-level dedup OFF for this cache.
    ///
    /// Provided for exactly one reason: to make every claim about dedup falsifiable. A gate that
    /// compared the dedup path against itself would pass whatever the mechanism did; a gate that
    /// compares it against this cannot. It is also the escape hatch if a duplicate-network claim ever
    /// turns out to be wrong for some compiler — a caller can opt out without reverting anything.
    pub fn without_net_dedup(mut self) -> Self {
        self.net_measurements = None;
        self
    }

    pub fn net_dedup_enabled(&self) -> bool {
        self.net_measurements.is_some()
    }

    pub fn nets_deduped(&self) -> usize {
        self.savings.nets_deduped
    }

    pub fn distinct_nets(&self) -> usize {
        self.savings.distinct_nets
    }

    pub fn propose_calls_avoided(&self) -> u64 {
        self.savings.propose_calls_avoided
    }

    pub fn confirmation_calls_avoided(&self) -> u64 {
        self.savings.confirmation_calls_avoided
    }

    pub fn confirmation_steps_avoided(&self) -> u64 {
        self.savings.confirmation_steps_avoided
    }

    /// A previously measured candidate whose network, grammar, corpus, and evidence mode are all identical to `key`'s.
    fn net_measurement(&self, key: &str) -> Option<&EvaluatedPlan> {
        self.net_measurements.as_ref()?.get(key)
    }

    /// Records `measured` as the reusable measurement for `key`, and counts it as a distinct network — skipping a `Certification::ResourceBreach`, since its build-time breach is a function of the candidate's own wall clock, not the network, and caching it would make a verdict depend on evaluation order.
    fn record_net_measurement(&mut self, key: String, measured: &EvaluatedPlan) {
        let Some(measurements) = self.net_measurements.as_mut() else {
            return;
        };
        if matches!(
            measured.evaluation.certification,
            Certification::ResourceBreach { .. }
        ) {
            return;
        }
        if measurements.insert(key, measured.clone()).is_none() {
            self.savings.distinct_nets += 1;
        }
    }

    fn count_net_dedup_hit(&mut self, corpus_words: usize, donor: Score) {
        self.savings.nets_deduped += 1;
        self.savings.propose_calls_avoided = self
            .savings
            .propose_calls_avoided
            .saturating_add(corpus_words as u64);
        self.savings.confirmation_calls_avoided = self
            .savings
            .confirmation_calls_avoided
            .saturating_add(donor.confirmation);
        self.savings.confirmation_steps_avoided = self
            .savings
            .confirmation_steps_avoided
            .saturating_add(donor.confirmation_steps);
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
    /// `None` deliberately covers two different facts — "not prepared" and "prepared but step-capped"
    /// — because neither is a set of analyses, and a caller must not be able to treat a step-capped
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

// Identity digests: grammar, finished network, and the composite reuse key

/// "These bytes are the whole state of one FINISHED proposer network."
///
/// The preimage is everything `foma::types::Fsm` carries that `apply_up` can observe — see
/// `finished_net_digest` for the field-by-field account and the two deliberate exclusions.
pub const FINISHED_NET_PROJECTION: &str = "pangloss.foma.finished-net/v1";

/// "This is the same loaded grammar." Preimage is the grammar's own derived `Debug` projection; see
/// `grammar_identity`.
pub const GRAMMAR_IDENTITY_PROJECTION: &str = "pangloss.foma.grammar-identity/v1";

/// "Another candidate's measurement over this corpus is reusable verbatim for this one."
pub const NET_REUSE_KEY_PROJECTION: &str = "pangloss.foma.net-reuse-key/v1";

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
/// into an FST (`build_candidate` plus `crate::build::finish_controllable_net`) — a strictly more
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

/// The key under which one candidate's whole measurement may be served to another.
///
/// # Why the net digest alone would be wrong
/// A network decides what gets PROPOSED. It does not decide what confirmation makes of those
/// proposals (that is the grammar's full-HC `Morpher`), nor which words are traversed (that is the
/// corpus), nor whether per-word proposal evidence is retained (that is the observed mode). Keyed on
/// the net alone, a cached result could cross grammars — silently, and in the reassuring direction,
/// because the reused verdict would usually be a pass.
///
/// So all four are bound, each length-prefixed:
/// - `grammar_identity` — see `grammar_identity`.
/// - `corpus_hash` of the COMPARABLE words, which is the slice actually applied and the slice whose
///   hash a `Certification::FullHcConfirmed` carries. It also fixes `expected`: the prepared oracle
///   result for a given word text is a pure function of (grammar, word, step cap), so equal
///   comparable slices imply equal ground truth.
/// - `observed` — the observed evaluator retains per-word proposal evidence and the ordinary one does
///   not, so serving an ordinary result to an observed caller would silently drop evidence. That is a
///   recall-shaped regression, and keying on the mode makes it unrepresentable.
/// - the finished net digest.
///
/// Exposed (hidden) so `net_dedup_gate` can assert the four-way discrimination directly. A gate that
/// could only observe the key through a full evaluator run would have to manufacture two grammars that
/// compile to the same network in order to test the one property that matters most.
#[doc(hidden)]
pub fn net_reuse_key(
    grammar_identity: &str,
    corpus_hash: &str,
    observed: bool,
    net_digest: &str,
) -> String {
    let mut hash = Sha256::new();
    framed(&mut hash, NET_REUSE_KEY_PROJECTION.as_bytes());
    framed(&mut hash, grammar_identity.as_bytes());
    framed(&mut hash, corpus_hash.as_bytes());
    hash.update([u8::from(observed)]);
    framed(&mut hash, net_digest.as_bytes());
    format!("{:x}", hash.finalize())
}

/// Default oracle (ground-truth `pg_parse::Morpher`) step cap, used whenever
/// `RuntimeBudget::oracle_step_cap` is left `None`.
///
/// Justified by measurement: on the
/// deep-truncation-chain stress grammar, the pathological corpus word that the fully-unbounded
/// `Morpher::new(g, usize::MAX)` call never returns for (>20s, previously observed >10 minutes)
/// completes in 91.6ms with `cap = 20_000`, reporting `capped: true` and 2 analyses. That is also
/// the exact cap `examples/p6_templated_q3_oracle_bounds.rs` already uses for the same grammar/word,
/// for the same reason. Large enough that no reference/staged grammar's real analyses come close to
/// it (the step cap stays a no-op for every well-formed word); small enough that a pathological word
/// is stopped in well under a second instead of hanging the whole evaluator call.
pub const DEFAULT_ORACLE_STEP_CAP: usize = 20_000;

/// Default wall-clock LIVENESS NET, used whenever `RuntimeBudget::oracle_liveness_net` is `None`.
///
/// # This is not a classifier, and it used to be one
/// It exists for exactly one purpose: a word whose single step is pathologically expensive must not
/// hang the run forever. Tripping it is an `OraclePreparationFault` that aborts preparation; it
/// can never exclude a word. See `OracleOutcome` for the measured incidents behind that.
///
/// # Why 300 seconds and not 2
/// The old 2-second value was small enough to trip BEFORE a word reached its step cap, which both
/// made exclusions load-sensitive and masked the deterministic axis. A net's only job is to be
/// unreachable by anything except a genuine hang, so it is set far above any legitimate per-word
/// cost: the pathological deep-truncation-chain word that motivated these bounds completes in
/// 91.6ms at `cap = 20_000`. Raising it costs
/// nothing on a healthy corpus and makes an abort mean what it says.
pub const DEFAULT_ORACLE_LIVENESS_NET: Duration = Duration::from_secs(300);

/// Default declared resident-memory ceiling for oracle preparation, used whenever
/// `RuntimeBudget::oracle_memory_ceiling` is `None`.
///
/// # Why memory is a declared axis at all
/// Measured: Aweti at a 200k step cap OOMed against a 16GB job-object ceiling, and a job-object
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

/// One oracle-required analysis identity that `WordEvidence::proposals` did not offer at
/// sufficient multiplicity for `WordEvidence::word`.
///
/// The identity is the same `(morpheme id sequence, root index)` pair
/// `tests/cross_compiler_equivalence_gate.rs`'s `candidate_key`/`analysis_key` already compared by
/// hand for one pinned fixture; this type is what that comparison now returns instead of asserting
/// directly, so a second caller (a fixture-wide faithfulness sweep) can classify the gap rather than
/// panic on it.
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
/// CONTAIN every oracle identity `evidence.expected` names, at the oracle's own multiplicity?
///
/// This is containment, not equality: a proposal set may offer MORE than the oracle found (over-
/// generation) and still pass here -- `check_proposal_ratio` is the separate, existing guard against
/// that. It is also the honest question for a propose+confirm pipeline: confirmation can only ever
/// select from what was proposed, so an emitter that silently drops a construct's material makes the
/// proposal set under-generate, and this is where that failure first becomes visible -- before
/// confirmation, which would otherwise just report a smaller `actual` with no way to tell "the oracle
/// found less" apart from "the proposer never offered it".
pub fn word_proposal_containment(evidence: &WordEvidence) -> Result<(), ContainmentGap> {
    let mut proposed = std::collections::BTreeMap::<(Vec<u32>, i32), usize>::new();
    for candidate in &evidence.proposals {
        let key = (
            candidate.morphemes.iter().map(|m| m.0).collect(),
            candidate.root_index,
        );
        *proposed.entry(key).or_default() += 1;
    }
    let mut oracle = std::collections::BTreeMap::<(Vec<u32>, i32), usize>::new();
    for analysis in &evidence.expected {
        let key = (analysis.morpheme_ids.clone(), analysis.root_morpheme_index);
        *oracle.entry(key).or_default() += 1;
    }
    for ((morpheme_ids, root_morpheme_index), required) in oracle {
        let offered = proposed
            .get(&(morpheme_ids.clone(), root_morpheme_index))
            .copied()
            .unwrap_or_default();
        if offered < required {
            return Err(ContainmentGap {
                word: evidence.word.clone(),
                morpheme_ids,
                root_morpheme_index,
                required,
                offered,
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

/// `Clone` so `RunEvaluationCache` can serve one candidate's whole measurement to the next candidate compiling to the identical network; the clone is bounded by the distinct-net count, not the plan count.
#[derive(Debug, Clone)]
struct EvaluatedPlan {
    evaluation: RuntimeEvaluation,
    words: Option<Vec<WordEvidence>>,
    /// Counted parity-set divergence for this candidate's corpus pass, folded into the run cache so a caller reads one run-scoped number instead of reconstructing it from first-failure-only verdicts.
    divergence: IdentityDivergence,
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
pub fn certify_corpus_measured(
    grammar: &Grammar,
    expected: &[(String, Vec<WordAnalysis>)],
    actual: &[(String, Vec<WordAnalysis>)],
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
    // Agreeing about nothing is not agreement: if the HC oracle produced no analysis for any word, every per-word comparison is empty-set against empty-set, which certify_word calls equal, so an all-empty corpus would "confirm" any candidate, including one whose network is empty.
    let analyses: usize = expected.iter().map(|(_, a)| a.len()).sum();
    if analyses == 0 {
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
/// Builds a candidate's network with the plan-composing interpreter.
///
/// # Panics
/// If `candidate`'s `LoweringAdapter` is not the one that interprets a plan. That is deliberate,
/// and it is a refusal rather than a fallback: this function can only ever produce
/// `build_controllable`'s controllable-subtree network, so honouring such a candidate by building it
/// anyway would hand the caller a network from a DIFFERENT compiler than the one the candidate
/// names, with nothing in the result saying so. Every measurement drawn from it would then be
/// attributed to a compiler that never ran. Callers holding mixed candidates must dispatch on
/// `candidate.adapter` (as `evaluate_plans_with_cache` does) or filter on
/// `LoweringAdapter::interprets_plan` first.
pub fn build_candidate(
    candidate: &LoweredCandidate,
    opts: &FomaOptions,
    grammar: &Grammar,
    alphabet: &SegAlphabet<'_>,
    prules: &[&PhonRuleDef],
) -> Result<crate::gate::GatedCompileResult, ComposeError> {
    assert!(
        candidate.adapter.interprets_plan(),
        "build_candidate cannot realize {:?}: it only ever composes a plan into the controllable \
         subtree's network, so building this candidate here would measure a different compiler than \
         the one it names. Dispatch on `candidate.adapter` instead.",
        candidate.adapter
    );
    build_controllable(&candidate.plan, opts, grammar, alphabet, prules)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeBudget {
    pub states: Option<u64>,
    pub arcs: Option<u64>,
    pub build: Option<u64>,
    pub apply: Option<u64>,
    pub proposals: Option<u64>,
    pub confirmation: Option<u64>,
    /// Ground-truth oracle step cap. UNLIKE every field above, `None` here does NOT mean
    /// "unbounded" — it means "caller did not override the default", because unbounded is exactly
    /// the defect this field exists to close (an unbounded oracle `Morpher` call is what hung the
    /// deep-truncation-chain grammar's pilot indefinitely). `evaluate_plans` resolves `None`
    /// to `DEFAULT_ORACLE_STEP_CAP`. A caller that genuinely wants the old unbounded behavior must
    /// say so explicitly with `Some(usize::MAX)`.
    pub oracle_step_cap: Option<usize>,
    /// Ground-truth oracle wall-clock LIVENESS NET — not a classifier. Same "`None` = use the
    /// default, not unbounded" convention as `oracle_step_cap` immediately above; resolves to
    /// `DEFAULT_ORACLE_LIVENESS_NET`. Tripping it aborts preparation with
    /// `OraclePreparationFault::LivenessNetTripped`.
    pub oracle_liveness_net: Option<Duration>,
    /// Declared resident-memory ceiling for oracle preparation, in bytes. Same `None` convention;
    /// resolves to `DEFAULT_ORACLE_MEMORY_CEILING_BYTES`. `Some(u64::MAX)` declares no ceiling
    /// (and is recorded as such in the evidence, so "unbounded" is a stated choice, not a silence).
    pub oracle_memory_ceiling: Option<u64>,
    /// CANDIDATE-side per-word raw `apply_up` path ceiling. Same "`None` = caller did not override
    /// the default, NOT unbounded" convention as `oracle_step_cap` above, and for the same reason in
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

    pub fn resolved_oracle_step_cap(&self) -> usize {
        self.oracle_step_cap.unwrap_or(DEFAULT_ORACLE_STEP_CAP)
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
            step_cap: self.resolved_oracle_step_cap() as u64,
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
    /// These differ, and the difference is invisible without this field. Marker-bearing
    /// `PlanComposed` candidates are rejected before partial-network measurement; the whole-grammar
    /// adapters are measured only for candidates that select them. Anything attributing a
    /// measurement -- a report field, diagram caption, or comparison -- must read THIS, not the
    /// declared strategy.
    pub realized_strategy: EmissionStrategy,
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
    let mut actual = Vec::new();
    let mut observed_proposals = OBSERVE.then(|| Vec::with_capacity(words.len()));
    let mut apply: u64 = 0;
    let mut proposals: u64 = 0;
    let mut confirmation: u64 = 0;
    let mut confirmation_steps: u64 = 0;
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
                            confirmation_steps,
                            raw_paths: raw_paths.saturating_add(diagnostics.raw_paths as u64),
                        },
                        realized_strategy,
                    },
                    words: None,
                    divergence: IdentityDivergence::not_compared(expected.len() as u64),
                };
            }
        };
        apply = apply.saturating_add(elapsed_ns(t).max(1));
        proposals = proposals.saturating_add(outcome.candidates_generated as u64);
        confirmation = confirmation.saturating_add(diagnostics.confirmation_calls as u64);
        confirmation_steps =
            confirmation_steps.saturating_add(diagnostics.confirmation_steps as u64);
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
        confirmation_steps,
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
            let (verdict, divergence) = certify_corpus_measured(grammar, expected, &actual);
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
            .zip(actual.into_iter())
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
        },
        words,
        divergence,
    }
}

/// Shared constructor for every evaluation outcome whose `Score` is zeroed except `build`, so every future `Score` field addition has exactly one place to account for it instead of drifting across re-inlined literals.
fn failed_evaluation(
    realized_strategy: EmissionStrategy,
    certification: Certification,
    build: u64,
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
            confirmation_steps: 0,
            raw_paths: 0,
        },
    }
}

/// A failure that happened before any occurrence could be compared, recorded as `not_compared` so this run cannot report the clean zero of one that actually compared.
fn failed_evaluated_over(
    realized_strategy: EmissionStrategy,
    certification: Certification,
    build: u64,
    occurrences: u64,
) -> EvaluatedPlan {
    EvaluatedPlan {
        evaluation: failed_evaluation(realized_strategy, certification, build),
        words: None,
        divergence: IdentityDivergence::not_compared(occurrences),
    }
}

fn build_failed_evaluated(
    realized_strategy: EmissionStrategy,
    reason: String,
    build: u64,
    occurrences: u64,
) -> EvaluatedPlan {
    failed_evaluated_over(
        realized_strategy,
        Certification::BuildFailed { reason },
        build,
        occurrences,
    )
}

/// `EmissionStrategy::TunedSurfaceProbed`: the default compilation of this grammar, through `FomaProposer::new` (emit -> lexc -> foma compile) rather than `build_controllable`.
fn evaluate_via_tuned_emit_mode<const OBSERVE: bool>(
    grammar: &Grammar,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
) -> EvaluatedPlan {
    let t = Instant::now();
    let proposer = match FomaProposer::new(grammar) {
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
    // No with_segment_query_encoder here, unlike the controllable path: this net is in plain surface space, queried with plain NFD.
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

/// `EmissionStrategy::TemplatedUnderlyingTokens`: compiles the whole grammar through `emit_underlying_templated` plus a real compiled rewrite cascade, deriving its own topology like the tuned path, so it must only ever be offered as its own candidate, never as the realization of another candidate's plan; its char-def TOKEN-space lexc must keep the segment query encoder `compile_templated_morphotactics` already attaches, since adding or omitting a second one is the space-mismatch that manufactures false zero-candidate results.
fn evaluate_via_templated_emit_mode<const OBSERVE: bool>(
    grammar: &Grammar,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
) -> EvaluatedPlan {
    let t = Instant::now();
    let output = match crate::templated_compile::compile_templated_morphotactics(grammar) {
        Ok(output) => output,
        Err(e) => {
            return build_failed_evaluated(
                EmissionStrategy::TemplatedUnderlyingTokens,
                format!("templated underlying-token path failed to build: {e}"),
                elapsed_ns(t).max(1),
                expected.len() as u64,
            )
        }
    };
    let build = elapsed_ns(t).max(1);
    let (states, arcs) = output.proposer.network_counts();
    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(grammar, output.proposer);
    if OBSERVE {
        measure_and_certify_observed(
            EmissionStrategy::TemplatedUnderlyingTokens,
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
            EmissionStrategy::TemplatedUnderlyingTokens,
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

/// Evaluates every candidate through its own `LoweringAdapter` and the production
/// propose→confirm pipeline. The caller-provided order is preserved.
///
/// A plan that needs composite/structural marker subtrees is refused because
/// `build_controllable` cannot build those subtrees. The whole-grammar adapters are measured only
/// for candidates that select them; no candidate is rerouted to a different adapter.
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
/// oracle and a single net-dedup ledger.
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

/// One `LoweringAdapter::ControllablePlanCompose` candidate, realized into an owned, apply-ready proposer.
enum RealizedPlanComposed {
    Ready {
        proposer: FomaProposer,
        states: u64,
        arcs: u64,
        build: u64,
        /// `finished_net_digest` of the net this proposer was built from, taken while the net still exists as an `Fsm`.
        net_digest: String,
    },
    Failed {
        certification: Certification,
        build: u64,
    },
}

fn unbuildable_marker_reason(candidate: &LoweredCandidate) -> Option<String> {
    let markers = crate::build::unbuildable_markers(&candidate.plan);
    (!markers.is_empty()).then(|| {
        format!(
            "plan structure cannot be honoured by the plan-composed compiler: its plan requires \
             subtrees build_controllable does not build ({}); use a whole-grammar backend",
            markers
                .iter()
                .map(|marker| format!("{marker:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// Builds one plan-composed candidate's network and turns it into a proposer, kept as its own function so the confirmation-free accuracy path and the certification path always propose from the SAME compiled network; returns an owned proposer because `from_precompiled_network`'s `apply_init` deep-clones the `Fsm`, matching `FomaProposer::new`'s own documented drop-after-`apply_init` lifetime.
fn realize_plan_composed(
    candidate: &LoweredCandidate,
    grammar: &Grammar,
    opts: &FomaOptions,
    alphabet: &SegAlphabet<'_>,
    prules: &[&PhonRuleDef],
) -> RealizedPlanComposed {
    let t = Instant::now();
    let built = build_candidate(candidate, opts, grammar, alphabet, prules);
    let build = elapsed_ns(t).max(1);
    let Ok(mut built) = built else {
        return RealizedPlanComposed::Failed {
            certification: Certification::BuildFailed {
                reason: "build failed".into(),
            },
            build,
        };
    };
    let Some(net) = built.net.take() else {
        return RealizedPlanComposed::Failed {
            certification: Certification::Truncated {
                stage: "empty-network".into(),
                corpus: None,
            },
            build,
        };
    };
    // Mandatory finish step, not an optimization: without it the net still carries the inter-morph boundary tokens uflexc emits, which a surface query never contains, so apply_up returns nothing and recall reads as zero.
    let mut net =
        crate::build::finish_controllable_net(opts, net, surface_table(grammar), alphabet);
    // Closes an asymmetry: `FomaProposer::new` calls `prepare_network_for_apply`, but `from_precompiled_network` (every plan-composed candidate's constructor) deliberately did not, so above ARC_SORT_MIN_ARCS the hand-spun baseline got foma's binary-search arc traversal and its plan-composed comparison point did not.
    // See `docs/research/pg-foma-recipe-runtime-design-notes.md` for why this is worth keeping despite measuring inert on every current fixture.
    crate::analyzer::prepare_network_for_apply(&mut net);
    let (states, arcs) = (net.statecount as u64, net.arccount as u64);
    // Digested here, after every mutation the net will ever receive (including the arc sort above) and before from_precompiled_network deep-clones it away; digesting earlier would key on a net that isn't the one queried.
    let net_digest = finished_net_digest(&net);
    RealizedPlanComposed::Ready {
        proposer: FomaProposer::from_precompiled_network_without_emit_report(&net)
            .with_segment_query_encoder(surface_table(grammar)),
        states,
        arcs,
        build,
        net_digest,
    }
}

/// **The sizing instrument for net-level dedup: how many DISTINCT networks does a plan set actually
/// produce?**
///
/// Builds each plan-composed candidate exactly the way `evaluate_plans_with_cache` does —
/// `build_candidate`, `crate::build::finish_controllable_net`,
/// `crate::analyzer::prepare_network_for_apply` — and returns `finished_net_digest` for each. It
/// runs NO oracle, NO propose and NO confirm, so a census over many fixtures costs only the build
/// half.
///
/// `Err` carries the reason there is no digest to report (whole-grammar strategy, build failure,
/// empty network), because "this fixture produced fewer digests than plans" must never be readable as
/// evidence of dedup opportunity when the real cause was a build that failed.
#[doc(hidden)]
pub fn finished_net_digests(
    grammar: &Grammar,
    plans: &[LoweredCandidate],
) -> Vec<Result<String, String>> {
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = crate::enumerate::prules_in_order(grammar);
    let opts = FomaOptions::default();
    plans
        .iter()
        .map(|candidate| {
            if !candidate.adapter.interprets_plan() {
                return Err(format!(
                    "whole-grammar adapter {:?} is not realized by build_controllable",
                    candidate.adapter
                ));
            }
            if let Some(reason) = unbuildable_marker_reason(candidate) {
                return Err(reason);
            }
            match realize_plan_composed(candidate, grammar, &opts, &alphabet, &prules) {
                RealizedPlanComposed::Ready { net_digest, .. } => Ok(net_digest),
                RealizedPlanComposed::Failed { certification, .. } => {
                    Err(format!("{certification:?}"))
                }
            }
        })
        .collect()
}

fn evaluate_plans_with_cache_mode<const OBSERVE: bool>(
    grammar: &Grammar,
    plans: &[LoweredCandidate],
    words: &[String],
    budget: RuntimeBudget,
    cache: &mut RunEvaluationCache,
) -> Vec<EvaluatedPlan> {
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = crate::enumerate::prules_in_order(grammar);
    let opts = FomaOptions::default();
    let selection = cache.select(words);
    let comparable = selection.comparable;
    let expected = selection.expected;
    let oracle_capped = selection.capped;
    let exclusions = selection.exclusions;
    let corpus_evidence =
        corpus_completeness_evidence(words, &comparable, &exclusions, cache.corpus.oracle);
    let words = &comparable[..];
    // A step-capped oracle result must never reach certify_corpus: a genuinely incomplete candidate could look right against an equally truncated ground truth and wrongly certify, so this returns before build_candidate runs for any plan in the batch. Certification is all-or-nothing over the requested corpus — refusing the whole batch rather than silently certifying a subset under the full corpus's name and hash.
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
                )
            })
            .collect::<Vec<_>>();
        for candidate in &refused {
            cache.absorb_divergence(candidate.divergence);
        }
        return refused;
    }
    // The confirm side's grammar-static pieces, built at most once for the whole run and handed back after each candidate — each is a pure function of `grammar`, and lazy rather than eager so an all-whole-grammar-strategy run never pays for the expensive `Morpher::new` it doesn't use.
    let mut confirm_pieces: Option<(
        crate::peel::ReduplicationPeeler,
        Vec<Option<crate::confirm::MorphemeOwner>>,
        pg_parse::Morpher<'_>,
    )> = None;
    // The two run-invariant halves of every net reuse key, derived once and lazily, so a batch of whole-grammar candidates (which return before reaching the composed path) pays nothing for a dedup it cannot use.
    let mut reuse_prefix: Option<(String, String)> = None;
    let evaluated: Vec<EvaluatedPlan> = plans
        .iter()
        .map(|candidate| {
            if candidate.adapter.interprets_plan() {
                if let Some(reason) = unbuildable_marker_reason(candidate) {
                    return failed_evaluated_over(
                        EmissionStrategy::PlanComposed,
                        Certification::Unsupported { reason },
                        0,
                        expected.len() as u64,
                    );
                }
            }
            // Adapter dispatch comes first: the two whole-grammar adapters never touch build_controllable, so routing them through the composed path below would attribute the wrong compiler's network to the candidate.
            match candidate.adapter {
                LoweringAdapter::ControllablePlanCompose => {}
                LoweringAdapter::TunedSurfaceEmit => {
                    return if OBSERVE {
                        evaluate_via_tuned_emit_mode::<true>(grammar, words, &expected, budget)
                    } else {
                        evaluate_via_tuned_emit_mode::<false>(grammar, words, &expected, budget)
                    }
                }
                LoweringAdapter::TemplatedUnderlyingEmit => {
                    return if OBSERVE {
                        evaluate_via_templated_emit_mode::<true>(grammar, words, &expected, budget)
                    } else {
                        evaluate_via_templated_emit_mode::<false>(grammar, words, &expected, budget)
                    }
                }
            }
            let (proposer, score0, build, net_digest) =
                match realize_plan_composed(candidate, grammar, &opts, &alphabet, &prules) {
                    RealizedPlanComposed::Ready {
                        proposer,
                        states,
                        arcs,
                        build,
                        net_digest,
                    } => (proposer, (states, arcs), build, net_digest),
                    RealizedPlanComposed::Failed {
                        certification,
                        build,
                    } => {
                        return failed_evaluated_over(
                            EmissionStrategy::PlanComposed,
                            certification,
                            build,
                            expected.len() as u64,
                        )
                    }
                };
            // Net-level dedup: an earlier candidate's measurement is served verbatim except `build` (this candidate's own) and `apply` (reported as 0, never the donor's), with the breach ladder re-run over the reconstructed score rather than copied.
            // See `docs/research/pg-foma-recipe-runtime-design-notes.md` for why each of those three exclusions is load-bearing rather than incidental.
            let reuse_key = (cache.net_dedup_enabled() && budget.apply.is_none()).then(|| {
                let (identity, corpus) = reuse_prefix
                    .get_or_insert_with(|| (grammar_identity(grammar), corpus_hash(words)));
                net_reuse_key(identity, corpus, OBSERVE, &net_digest)
            });
            let reused = reuse_key
                .as_deref()
                .and_then(|key| cache.net_measurement(key).cloned());
            let measured = match reused {
                Some(donor) => {
                    let mut reused = donor;
                    reused.evaluation.score.build = build;
                    reused.evaluation.score.apply = 0;
                    let score = reused.evaluation.score;
                    if let Some((dimension, value, limit)) = budget_breach(&score, budget) {
                        reused.evaluation.certification = Certification::ResourceBreach {
                            dimension: dimension.into(),
                            value,
                            limit,
                        };
                        reused.divergence = IdentityDivergence::not_compared(expected.len() as u64);
                    }
                    cache.count_net_dedup_hit(words.len(), score);
                    reused
                }
                None => {
                    let (peeler, owners, morpher) = confirm_pieces.take().unwrap_or_else(|| {
                        (
                            crate::peel::ReduplicationPeeler::new(grammar),
                            crate::confirm::build_morpheme_owners(grammar),
                            pg_parse::Morpher::new(grammar, usize::MAX),
                        )
                    });
                    let mut analyzer = FomaAnalyzer::from_cached_with_morpher(
                        grammar,
                        proposer,
                        peeler,
                        owners,
                        morpher,
                        crate::candidate_filter::CandidateFilterSettings::off(),
                    );
                    let measured = if OBSERVE {
                        measure_and_certify_observed(
                            EmissionStrategy::PlanComposed,
                            grammar,
                            &mut analyzer,
                            words,
                            &expected,
                            budget,
                            score0.0,
                            score0.1,
                            build,
                        )
                    } else {
                        measure_and_certify(
                            EmissionStrategy::PlanComposed,
                            grammar,
                            &mut analyzer,
                            words,
                            &expected,
                            budget,
                            score0.0,
                            score0.1,
                            build,
                        )
                    };
                    // Hand the grammar-static confirm pieces back for the next candidate: confirm never mutates them, so the next candidate gets objects indistinguishable from a fresh rebuild.
                    let (_spent_proposer, peeler, owners, morpher, _filter) =
                        analyzer.into_parts_with_morpher();
                    confirm_pieces = Some((peeler, owners, morpher));
                    if let Some(reuse_key) = reuse_key {
                        cache.record_net_measurement(reuse_key, &measured);
                    }
                    measured
                }
            };
            measured
        })
        .collect();
    // Folded here, after the closure's mutable borrow of `cache` ends, so every path out of this function contributes its divergence exactly once.
    for candidate in &evaluated {
        cache.absorb_divergence(candidate.divergence);
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
/// 1. Realizes the candidate's network exactly as `evaluate_plans_with_cache` does — the
///    SAME `realize_plan_composed` for composed plans, the same two compilers for the whole-grammar
///    strategies. A different network would make the verdict a statement about something else.
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
///   objective stays `confirmation_steps`-led and untouched.
/// - **It never truncates a proposal set and never caps per-candidate work.** Either would be
///   indistinguishable from the recall failure this exists to find.
///
/// # Corpus eligibility is the certification path's, unchanged
/// The same all-or-nothing rule applies: if ANY requested occurrence was excluded (a step-capped
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
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = crate::enumerate::prules_in_order(grammar);
    let opts = FomaOptions::default();
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
            let realized =
                realize_accuracy_proposer(candidate, grammar, &opts, &alphabet, &prules, cache);
            let (realized_strategy, proposer) = match realized {
                Ok(ready) => ready,
                Err((realized_strategy, reason)) => {
                    return CandidateAccuracy {
                        requested_strategy: candidate.strategy(),
                        realized_strategy,
                        verdict: AccuracyVerdict::NotDetermined { reason },
                        counters: AccuracyCounters::default(),
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

/// Realizes one candidate into the proposer the accuracy check will propose from — the same network the certification path measures, by construction.
#[allow(clippy::type_complexity)]
fn realize_accuracy_proposer(
    candidate: &LoweredCandidate,
    grammar: &Grammar,
    opts: &FomaOptions,
    alphabet: &SegAlphabet<'_>,
    prules: &[&PhonRuleDef],
    _cache: &mut RunEvaluationCache,
) -> Result<(EmissionStrategy, FomaProposer), (EmissionStrategy, String)> {
    if candidate.adapter.interprets_plan() {
        if let Some(reason) = unbuildable_marker_reason(candidate) {
            return Err((EmissionStrategy::PlanComposed, reason));
        }
    }
    match candidate.adapter {
        LoweringAdapter::TunedSurfaceEmit => FomaProposer::new(grammar)
            .map(|proposer| (EmissionStrategy::TunedSurfaceProbed, proposer))
            .map_err(|e| {
                (
                    EmissionStrategy::TunedSurfaceProbed,
                    format!("tuned emit path failed to build: {e}"),
                )
            }),
        LoweringAdapter::TemplatedUnderlyingEmit => {
            crate::templated_compile::compile_templated_morphotactics(grammar)
                .map(|output| (EmissionStrategy::TemplatedUnderlyingTokens, output.proposer))
                .map_err(|e| {
                    (
                        EmissionStrategy::TemplatedUnderlyingTokens,
                        format!("templated underlying-token path failed to build: {e}"),
                    )
                })
        }
        LoweringAdapter::ControllablePlanCompose => {
            match realize_plan_composed(candidate, grammar, opts, alphabet, prules) {
                RealizedPlanComposed::Ready { proposer, .. } => {
                    Ok((EmissionStrategy::PlanComposed, proposer))
                }
                RealizedPlanComposed::Failed { certification, .. } => Err((
                    EmissionStrategy::PlanComposed,
                    format!("candidate network could not be realized: {certification:?}"),
                )),
            }
        }
    }
}

/// Proposes over the eligible corpus and checks containment; no confirmation engine is reachable from here, deliberately.
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
    for (occurrence_ordinal, (word, oracle)) in selection.expected.iter().enumerate() {
        // Bounded, and the bound is a refusal (apply_refusals forces NotDetermined, never Undergenerated), not unbounded: an unbounded apply_up path count can grow large enough to kill the process outright on a pathological word, and a dead process reports no verdict at all — strictly worse than an honest NotDetermined.
        let proposed = crate::composite::propose_union_peel_with_diagnostics(
            grammar,
            &mut proposer,
            peeler,
            peel_budget,
            word,
            &apply_budget,
        );
        let (proposals, _peel_used, peel_chain_depth_error, diagnostics, proposal_calls) =
            match proposed {
                Ok(complete) => complete,
                Err((dimension, value, limit, diagnostics, proposal_calls)) => {
                    // This occurrence is counted as checked and refused, never silently skipped, so the refusal share in verdict_from's message is over the real denominator.
                    eprintln!(
                        "accuracy: proposal REFUSED for {word:?} -- per-word apply {} reached \
                         {value} against a limit of {limit}",
                        dimension.label()
                    );
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
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    //! The parity relation, exercised at the certification seam.
    //!
    //! These used to run against hand-built `WordAnalysis` values with no grammar at all, because
    //! full structural equality needs no model. Deduplicated identity comparison DOES need one --
    //! the whole point is that dense ordinals are projected to stable source keys -- so they now
    //! compile `test_support::PARITY_FIXTURE_XML`, three unrelated entries whose only job is to give
    //! three morpheme ordinals something to resolve to.

    use super::*;
    use crate::test_support::{parity_analysis, parity_fixture_grammar};

    fn wa(n: u32) -> WordAnalysis {
        parity_analysis(n)
    }

    /// Same identity as `wa(n)`, differing only in `mpr` — a field `AnalysisIdentity` does not capture and `WordAnalysis::Eq` does.
    fn wa_other_mpr(n: u32) -> WordAnalysis {
        WordAnalysis {
            mpr: pg_grammar::model::MprSet(1),
            ..wa(n)
        }
    }

    fn fixture() -> Grammar {
        parity_fixture_grammar()
    }

    #[test]
    fn order_is_irrelevant() {
        let g = fixture();
        assert!(certify_word(&g, "w", &[wa(0), wa(1)], &[wa(1), wa(0)]).selectable());
    }

    #[test]
    fn analyses_differing_only_outside_identity_are_the_same_analysis() {
        // wa(0) and wa_other_mpr(0) are unequal as WordAnalysis values but must certify: AnalysisIdentity captures only stable morpheme keys, root position, and category — mpr is engine-internal payload, not identity.
        let g = fixture();
        assert_ne!(wa(0), wa_other_mpr(0), "the fixture must actually differ");
        let verdict = certify_word(&g, "w", &[wa(0)], &[wa_other_mpr(0)]);
        assert!(
            verdict.selectable(),
            "identity-invisible payload must not read as disagreement: {verdict:?}"
        );
    }

    #[test]
    fn genuinely_different_identities_still_disagree() {
        // Widening the equivalence must not make everything equal: different morphemes are different identities.
        let g = fixture();
        assert!(matches!(
            certify_word(&g, "w", &[wa(0)], &[wa(1)]),
            Certification::IdentityMismatch { .. }
        ));
        // ... and so is the same morpheme sequence at a different root position.
        let original = WordAnalysis {
            morpheme_ids: vec![0, 1],
            root_morpheme_index: 0,
            morpheme_roots: vec![None, None],
            ..wa(0)
        };
        let moved = WordAnalysis {
            root_morpheme_index: 1,
            ..original.clone()
        };
        assert!(matches!(
            certify_word(&g, "w", &[original], &[moved]),
            Certification::IdentityMismatch { .. }
        ));
    }

    #[test]
    fn duplicate_paths_collapse_without_changing_the_verdict() {
        // The parity relation is deduplicated identity SET equality: an oracle reaching one analysis by two derivational paths and a candidate reaching it by one have found the same set and agree — multiplicity is evidence about redundant proposal work, never a grammar difference.
        let g = fixture();
        assert!(certify_word(&g, "w", &[wa(0), wa(0)], &[wa(0)]).selectable());
        assert!(certify_word(&g, "w", &[wa(0)], &[wa(0), wa(0), wa(0)]).selectable());
    }

    #[test]
    fn collapsed_duplicate_paths_survive_as_evidence() {
        // The verdict ignores duplicate paths, but the evidence must not lose them, or "these agree" would be indistinguishable from "these agree and one side did three times the work".
        let g = fixture();
        let projected =
            crate::parity::OccurrenceIdentities::project(&[wa(0), wa(0), wa(0)], &g).unwrap();
        assert_eq!(projected.len(), 1);
        assert_eq!(projected.entries()[0].duplicate_paths, 3);
        assert_eq!(projected.collapsed_paths(), 2);
    }

    #[test]
    fn repeated_corpus_rows_stay_separate_observations() {
        // Deduplication is within an occurrence: two rows for the same word are two observations, and a candidate disagreeing on the second row fails even though the union of both rows' identities would match.
        let g = fixture();
        let expected = vec![("w".into(), vec![wa(0)]), ("w".into(), vec![wa(1)])];
        assert!(certify_corpus(&g, &expected, &expected).selectable());
        let changed = vec![("w".into(), vec![wa(0)]), ("w".into(), vec![wa(0)])];
        assert!(matches!(
            certify_corpus(&g, &expected, &changed),
            Certification::IdentityMismatch { .. }
        ));
        // Collapsing the two rows into one would also change the row count, which is refused outright rather than certified against a shorter corpus.
        let collapsed = vec![("w".to_string(), vec![wa(0), wa(1)])];
        assert!(matches!(
            certify_corpus(&g, &expected, &collapsed),
            Certification::Truncated { .. }
        ));
    }

    #[test]
    fn a_projection_failure_is_a_typed_fault_never_a_mismatch_and_never_a_pass() {
        // An analysis referencing a morpheme its own model lacks is an internal inconsistency: reporting it as a mismatch would blame the grammar for an engine bug, and — more dangerous — the two sides here are identical, so a lazy or missing projection would call this a full confirmation.
        let g = fixture();
        let unresolvable = wa(9_999);
        let verdict = certify_word(
            &g,
            "w",
            std::slice::from_ref(&unresolvable),
            std::slice::from_ref(&unresolvable),
        );
        assert!(
            !verdict.selectable(),
            "an unprojectable analysis must never certify: {verdict:?}"
        );
        assert!(
            matches!(&verdict, Certification::Truncated { stage, .. }
                if stage == "identity-projection-failed-oracle"),
            "expected a typed projection truncation naming its side, got {verdict:?}"
        );
        // A candidate-side-only fault is named as such, so a report can tell which engine is broken.
        let candidate_side = certify_word(&g, "w", &[wa(0)], std::slice::from_ref(&unresolvable));
        assert!(
            matches!(&candidate_side, Certification::Truncated { stage, .. }
                if stage == "identity-projection-failed-candidate"),
            "got {candidate_side:?}"
        );
    }

    #[test]
    fn guessing_is_refused_by_the_v1_certification_scope() {
        let g = fixture();
        let guessed = WordAnalysis {
            guessed: true,
            ..wa(0)
        };
        // Identical on both sides, so a comparison that only compared would confirm.
        let verdict = certify_word(
            &g,
            "w",
            std::slice::from_ref(&guessed),
            std::slice::from_ref(&guessed),
        );
        assert!(
            matches!(&verdict, Certification::Truncated { stage, .. }
                if stage == "guessing-refused-oracle"),
            "a guessed analysis must be refused, not certified: {verdict:?}"
        );
        let by_provenance = WordAnalysis {
            provenance: pg_parse::AnalysisProvenance::Guessed,
            ..wa(0)
        };
        assert!(
            matches!(
                certify_word(&g, "w", &[wa(0)], std::slice::from_ref(&by_provenance)),
                Certification::Truncated { ref stage, .. } if stage == "guessing-refused-candidate"
            ),
            "the provenance tag must be refused on its own, not only the boolean"
        );
    }

    #[test]
    fn supplied_roots_are_refused_by_the_v1_certification_scope() {
        let g = fixture();
        let supplied = WordAnalysis {
            provenance: pg_parse::AnalysisProvenance::Supplied {
                entry_id: "runtime-entry".into(),
            },
            ..wa(0)
        };
        let verdict = certify_word(
            &g,
            "w",
            std::slice::from_ref(&supplied),
            std::slice::from_ref(&supplied),
        );
        assert!(
            matches!(&verdict, Certification::Truncated { stage, .. }
                if stage == "supplied-root-refused-oracle"),
            "a supplied root must be refused, not certified: {verdict:?}"
        );
    }

    #[test]
    fn a_total_lexical_miss_reaches_the_oracle_as_a_miss_not_a_guess() {
        // PreparedCorpus::prepare reads its oracle with guess_root: false and no SuppliedRootOverlay, so a word the lexicon cannot reach comes back with zero analyses rather than a fabricated root; this is a preservation guard, not the v1 scope gate, which the refusal tests above enforce.
        let g = fixture();
        let miss = vec!["cb".to_string()];
        let prepared = PreparedCorpus::prepare(&g, &miss, RuntimeBudget::default())
            .expect("preparation must not trip the liveness net on a one-word corpus");
        let selection = prepared.select(&miss);
        assert!(
            selection.exclusions.is_empty(),
            "an unanalyzable word is comparable, not excluded: {:?}",
            selection.exclusions
        );
        let analyses = &selection.expected[0].1;
        assert!(
            analyses.iter().all(|a| !a.guessed
                && a.supplied_root.is_none()
                && matches!(a.provenance, pg_parse::AnalysisProvenance::Grammar)),
            "the oracle must produce grammar-provenance analyses only: {analyses:?}"
        );
    }

    #[test]
    fn a_corpus_of_only_unanalyzable_words_is_still_refused() {
        // The vacuous-pass guard: an empty set equals an empty set under set equality too, which is exactly why this guard exists.
        let g = fixture();
        assert!(matches!(
            certify_corpus(&g, &[("w".into(), vec![])], &[("w".into(), vec![])]),
            Certification::Truncated { ref stage, .. } if stage == "no-analyzable-words"
        ));
    }

    #[test]
    fn missing_truncated() {
        let g = fixture();
        assert!(matches!(
            certify_corpus(&g, &[("w".into(), vec![])], &[]),
            Certification::Truncated { .. }
        ));
    }
}
