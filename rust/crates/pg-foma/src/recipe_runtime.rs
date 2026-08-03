//! Production evaluator for recipe plans.

use crate::analyzer::FomaProposer;
use crate::build::build_controllable;
use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::composite::FomaAnalyzer;
use crate::emit::surface_table;
use crate::enumerate::{CandidatePlan, EmissionStrategy};
use crate::parity::{
    certified_occurrence, IdentityDivergence, OccurrenceIdentities, ParitySide,
};
use crate::recipe_accuracy::{AccuracyCounters, AccuracyVerdict, CandidateAccuracy};
use crate::recipe_optimizer::{
    Certification, CorpusCompletenessEvidence, CorpusExclusion, OracleEligibilityConfig, Score,
};
use crate::replace::SegAlphabet;
use crate::tags::Candidate;
use foma::options::FomaOptions;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::WordAnalysis;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

/// What the ground-truth oracle concluded about ONE corpus occurrence.
///
/// # The absent variant is the point
/// There is deliberately no `TimedOut` and no `MemoryCapped` here. Wall-clock and memory outcomes
/// are **unrepresentable as eligibility outcomes**: they can only be
/// [`OraclePreparationFault`]s, which abort the whole preparation run. That is a type-level
/// property, not a convention someone must remember — as long as a clock could produce an
/// eligibility outcome, raising the clock's value only moves the race, never removes it.
///
/// Measured motivation (2026-08-01/02): Amharic word U+1264 U+1273 PASSED the oracle in a 673-row
/// run and was excluded as `oracle-timeout` in a 573-row run — same grammar, same caps, same
/// binary, only machine load differed. A digest over a set that a concurrent build can change is
/// not a digest. Separately, the two bounds masked each other: re-running 669 words with a 120s net
/// instead of 2s moved the step-capped count from 4 to 12, so words that would exhaust their step
/// budget were being misrecorded as timeouts, and 80 Amharic words were called intractable that
/// never were.
#[derive(Debug, Clone)]
enum OracleOutcome {
    /// The oracle finished this occurrence within its step cap. The analyses are the ground truth.
    Complete(Vec<WordAnalysis>),
    /// The oracle exhausted its step cap on this occurrence. THE eligibility classifier, and the
    /// only one: deterministic per (grammar, word, cap), so the eligible set is reproducible.
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
/// [`OracleOutcome`] for why that is a type-level guarantee rather than a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OraclePreparationFault {
    /// The wall-clock liveness net tripped. The net exists only so a pathological word cannot hang
    /// the run forever; a trip means the step cap never got the chance to classify this word, so
    /// the honest report is "could not determine", never "excluded".
    LivenessNetTripped {
        word: String,
        requested_ordinal: u64,
        net: Duration,
    },
    /// The declared resident-memory ceiling was exceeded during preparation. Previously this was
    /// a job-object kill with no row emitted at all — "I could not look" reading as an outcome.
    MemoryCeilingExceeded {
        word: String,
        requested_ordinal: u64,
        ceiling_bytes: u64,
        observed_bytes: u64,
    },
    /// A memory ceiling was declared but this build cannot read the process's resident set (no
    /// sampler on this target). Refusing is deliberate: a declared-but-unenforced ceiling would
    /// put a bound in the evidence that nothing ever checked.
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

/// Samples THIS process's resident set size in bytes.
///
/// [`RssSampler::sample`] returning `None` means "could not look", never "fine":
/// [`PreparedCorpus::prepare`] turns it into
/// [`OraclePreparationFault::MemoryCeilingUnobservable`] whenever a ceiling is actually declared.
/// Two cfg'd shapes with one signature, so the preparation loop has no `cfg` in it — `wasm32` has
/// no process table to read, and `sysinfo` is not even in that target's dependency graph.
#[cfg(not(target_arch = "wasm32"))]
struct RssSampler(sysinfo::System);

#[cfg(not(target_arch = "wasm32"))]
impl RssSampler {
    fn new() -> Self {
        Self(sysinfo::System::new())
    }

    /// Refreshes ONLY this pid, never a system-wide scan — the same discipline `worker.rs` applies
    /// to the compile child's RSS guardrail.
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
            // Checked FIRST and unconditionally: a timed-out word has no eligibility answer at all,
            // so it must not fall through to the step-cap classification below even when the step
            // cap also latched. (`capped` can be true alongside `timed_out`; treating that as an
            // exclusion is precisely the masking defect this ordering removes.)
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
                        // Historical spelling, retained: with the wall clock demoted this reason is
                        // now unambiguous -- "capped" can only mean the step cap.
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
    emission_report: Option<crate::emit::EmitReport>,
    emission_report_calls: usize,
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
            emission_report: None,
            emission_report_calls: 0,
            divergence: IdentityDivergence::default(),
        })
    }

    pub fn oracle_calls(&self) -> usize {
        self.corpus.oracle_calls()
    }

    /// This run's accumulated parity-set divergence across every candidate evaluated against this
    /// cache.
    ///
    /// [`IdentityDivergence::candidate_only_identities`] is the number the
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
    /// [`CorpusCompletenessEvidence`]/[`Certification::Truncated`] exist to carry; this accessor is
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

    pub fn emission_report_calls(&self) -> usize {
        self.emission_report_calls
    }

    fn select(&self, words: &[String]) -> PreparedSelection {
        self.corpus.select(words)
    }

    fn emission_report(&mut self, grammar: &Grammar) -> crate::emit::EmitReport {
        if self.emission_report.is_none() {
            self.emission_report_calls += 1;
            self.emission_report = Some(crate::emit::emit(grammar).report);
        }
        self.emission_report
            .as_ref()
            .expect("emission report was initialized")
            .clone()
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

/// Default oracle (ground-truth `pg_parse::Morpher`) step cap, used whenever
/// [`RuntimeBudget::oracle_step_cap`] is left `None`.
///
/// Justified by measurement (`docs/fst-plan/deep-chain-pilot-non-completion.md`): on the
/// deep-truncation-chain stress grammar, the pathological corpus word that the fully-unbounded
/// `Morpher::new(g, usize::MAX)` call never returns for (>20s, previously observed >10 minutes)
/// completes in 91.6ms with `cap = 20_000`, reporting `capped: true` and 2 analyses. That is also
/// the exact cap `examples/p6_templated_q3_oracle_bounds.rs` already uses for the same grammar/word,
/// for the same reason. Large enough that no reference/staged grammar's real analyses come close to
/// it (the step cap stays a no-op for every well-formed word); small enough that a pathological word
/// is stopped in well under a second instead of hanging the whole evaluator call.
pub const DEFAULT_ORACLE_STEP_CAP: usize = 20_000;

/// Default wall-clock LIVENESS NET, used whenever [`RuntimeBudget::oracle_liveness_net`] is `None`.
///
/// # This is not a classifier, and it used to be one
/// It exists for exactly one purpose: a word whose single step is pathologically expensive must not
/// hang the run forever. Tripping it is an [`OraclePreparationFault`] that aborts preparation; it
/// can never exclude a word. See [`OracleOutcome`] for the measured incidents behind that.
///
/// # Why 300 seconds and not 2
/// The old 2-second value was small enough to trip BEFORE a word reached its step cap, which both
/// made exclusions load-sensitive and masked the deterministic axis. A net's only job is to be
/// unreachable by anything except a genuine hang, so it is set far above any legitimate per-word
/// cost: the pathological deep-truncation-chain word that motivated these bounds completes in
/// 91.6ms at `cap = 20_000` (`docs/fst-plan/deep-chain-pilot-non-completion.md`). Raising it costs
/// nothing on a healthy corpus and makes an abort mean what it says.
pub const DEFAULT_ORACLE_LIVENESS_NET: Duration = Duration::from_secs(300);

/// Default declared resident-memory ceiling for oracle preparation, used whenever
/// [`RuntimeBudget::oracle_memory_ceiling`] is `None`.
///
/// # Why memory is a declared axis at all
/// Measured: Aweti at a 200k step cap OOMed against a 16GB job-object ceiling, and a job-object
/// kill produces NO row — the run simply vanishes, and "I could not look" reads as an outcome.
/// Declaring a ceiling below the job object's turns that into a typed, word-naming abort that a
/// reader can act on.
///
/// # Why a fixed constant and not a fraction of installed RAM
/// This number is recorded in [`CorpusCompletenessEvidence`], so a machine-derived value would make
/// two identical eligibility sets produce different evidence on different machines. A ceiling can
/// only ever ABORT, never classify, so the cost of it being wrong for a given machine is a loud
/// refusal rather than a wrong answer — which is the direction a reproducible artifact should fail
/// in. 12 GiB sits below the 16GB job ceiling that killed the Aweti run. Override with
/// [`RuntimeBudget::oracle_memory_ceiling`]; `Some(u64::MAX)` opts out entirely.
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
    /// This is the evidence half of the parity relation: [`certify_word`] compares only the
    /// identities, so without this the report could not say that a candidate found one analysis by
    /// five paths where the oracle found it by one — a real and useful property of the compilation
    /// that the verdict is deliberately blind to.
    ///
    /// `None` means the projection FAILED for this occurrence, which is exactly the case in which
    /// the certification is a [`crate::parity::ParityFault`]-derived truncation. It never means
    /// "no analyses"; that is `Some` of an empty set.
    pub expected_identities: Option<OccurrenceIdentities>,
    /// The candidate's deduplicated identity set for this occurrence. Same contract as
    /// [`Self::expected_identities`].
    pub actual_identities: Option<OccurrenceIdentities>,
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

struct EvaluatedPlan {
    evaluation: RuntimeEvaluation,
    words: Option<Vec<WordEvidence>>,
    /// Counted parity-set divergence for THIS candidate's corpus pass. Folded into the run cache by
    /// [`evaluate_plans_marked_with_cache_mode`] so a caller can read one run-scoped number rather
    /// than reconstructing it from per-candidate verdicts (which, being first-failure-only, do not
    /// carry it).
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

/// How many identities a mismatch detail names before it stops listing them.
///
/// The detail string goes into a report and a pathological word can disagree about thousands of
/// analyses; an unbounded list would make the report unreadable and the log enormous without
/// telling a reader anything the first few do not. The counts, which are what a reader actually
/// acts on, are always exact.
const MISMATCH_DETAIL_SAMPLE: usize = 4;

/// Compare one word occurrence's analyses as **deduplicated
/// [`pg_parse::identity::AnalysisIdentity`] sets**.
///
/// This is [`crate::parity`]'s relation applied; read that module for why it is the relation. The
/// two things it is NOT are worth restating at the call site, because both were previously what
/// this function did:
///
/// - It is not full `WordAnalysis` structural equality. Two analyses differing only in fields
///   `AnalysisIdentity` does not capture (`syn_fs`, `mpr`, the per-morpheme supplied-root slots) are
///   the SAME analysis, and this function now says so.
/// - It is not multiset equality. Multiplicity is not part of the relation, so a candidate that
///   reached one identity by three derivational paths agrees with an oracle that reached it by one.
///   The collapsed-path count survives as evidence on [`WordEvidence`], not as a verdict.
///
/// Deduplication is strictly WITHIN this one occurrence. Corpus rows are separate observations;
/// [`certify_corpus`] never compares one row against another.
///
/// A projection failure or a v1-scope refusal comes back as a non-selectable
/// [`Certification::Truncated`] naming the fault and the side it was found on — never as a
/// mismatch, which would report an internal fault as a grammar disagreement.
pub fn certify_word(
    grammar: &Grammar,
    word: impl Into<String>,
    expected: &[WordAnalysis],
    actual: &[WordAnalysis],
) -> Certification {
    certify_word_measured(grammar, word, expected, actual).0
}

/// [`certify_word`] plus the counted [`IdentityDivergence`] of the same comparison.
///
/// The two are one function rather than two because the divergence must be measured on the SAME
/// projection the verdict came from. A second, independent pass would both double the projection
/// cost of every ordinary run and — worse — leave open the possibility of the counter and the
/// verdict disagreeing about what they looked at, which is exactly the property a soundness counter
/// cannot afford to lose.
///
/// The divergence is [`IdentityDivergence::not_compared`] whenever the verdict is a
/// [`crate::parity::ParityFault`]-derived truncation: a fault means no comparison happened, and
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

/// [`certify_corpus`] plus the counted [`IdentityDivergence`] summed over every row.
///
/// The divergence is accumulated over ALL rows even though the verdict is decided by the first
/// failure: the verdict only needs one witness, whereas the soundness counter is a claim about the
/// whole corpus and would be worthless if it stopped at the first disagreement. See
/// [`certify_word_measured`] for why the count and the verdict share one projection pass.
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
    // Agreeing about nothing is not agreement. If the HC oracle produced no analysis for ANY word in
    // this corpus, every per-word comparison above was empty-set against empty-set, which
    // `certify_word` quite correctly calls equal -- and the corpus would then "confirm" any candidate
    // whatsoever, including one whose network is empty.
    //
    // Observed: a 3-word Amharic corpus where HC analyses none of the words certified all three
    // candidates with `proposals: 0`, `confirmation: 0`. That is the same vacuous-pass shape as a
    // corpus-gated test that silently skips -- a pass that was never earned.
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
/// If `candidate` requests a whole-grammar [`EmissionStrategy`]. That is deliberate, and it is a
/// refusal rather than a fallback: this function can only ever produce `build_controllable`'s
/// controllable-subtree network, so honouring such a candidate by building it anyway would hand the
/// caller a network from a DIFFERENT compiler than the one the candidate names, with nothing in the
/// result saying so. Every measurement drawn from it would then be attributed to a strategy that
/// never ran. Callers holding mixed candidates must either dispatch on
/// `candidate.strategy` (as `evaluate_plans_marked` does) or filter to
/// `!strategy.is_whole_grammar()` first.
pub fn build_candidate(
    candidate: &CandidatePlan,
    opts: &FomaOptions,
    grammar: &Grammar,
    alphabet: &SegAlphabet<'_>,
    prules: &[&PhonRuleDef],
    budget: &ComposeBudget,
) -> Result<crate::gate::GatedCompileResult, ComposeError> {
    assert!(
        !candidate.strategy.is_whole_grammar(),
        "build_candidate cannot realize {:?}: it only ever composes a plan into the controllable \
         subtree's network, so building this candidate here would measure a different compiler than \
         the one it names. Dispatch on `candidate.strategy` instead.",
        candidate.strategy
    );
    build_controllable(&candidate.plan, opts, grammar, alphabet, prules, budget)
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
    /// deep-truncation-chain grammar's pilot indefinitely; see
    /// `docs/fst-plan/deep-chain-pilot-non-completion.md`). `evaluate_plans_marked` resolves `None`
    /// to [`DEFAULT_ORACLE_STEP_CAP`]. A caller that genuinely wants the old unbounded behavior must
    /// say so explicitly with `Some(usize::MAX)`.
    pub oracle_step_cap: Option<usize>,
    /// Ground-truth oracle wall-clock LIVENESS NET — not a classifier. Same "`None` = use the
    /// default, not unbounded" convention as `oracle_step_cap` immediately above; resolves to
    /// [`DEFAULT_ORACLE_LIVENESS_NET`]. Tripping it aborts preparation with
    /// [`OraclePreparationFault::LivenessNetTripped`].
    pub oracle_liveness_net: Option<Duration>,
    /// Declared resident-memory ceiling for oracle preparation, in bytes. Same `None` convention;
    /// resolves to [`DEFAULT_ORACLE_MEMORY_CEILING_BYTES`]. `Some(u64::MAX)` declares no ceiling
    /// (and is recorded as such in the evidence, so "unbounded" is a stated choice, not a silence).
    pub oracle_memory_ceiling: Option<u64>,
}

impl RuntimeBudget {
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
    /// These differ, and the difference is invisible without this field. `evaluate_plans_marked`
    /// evaluates a marker-carrying baseline evidence-first: it composes the plan, and only if that
    /// FAILS does it fall back to the tuned emitter. That fallback is deliberate and must stay -- a
    /// blanket veto on marker presence previously dropped grammars whose composed baseline confirms
    /// perfectly well (`mpr-gated-exception` scores 27/38 confirmed as `PlanComposed` despite
    /// carrying a marker). But it means a candidate declaring `PlanComposed` can be measured on the
    /// tuned network: `recipe-ordered-generic`'s baseline reports 79 states / 154 arcs and 366
    /// proposals, which is the tuned network, while its declared strategy still says `PlanComposed`.
    /// Anything attributing that measurement -- a report field, a diagram caption, a comparison
    /// between candidates -- must read THIS, not the declaration.
    pub realized_strategy: EmissionStrategy,
}

/// Evaluates the BASELINE of a grammar whose plan needs composite/structural marker subtrees, using
/// the tuned [`crate::analyzer::FomaProposer::new`] path (`emit` → lexc → foma compile) instead of
/// [`build_controllable`].
///
/// Why a whole separate path rather than a flag: the two builders produce different artifact types in
/// different symbol spaces. `uflexc`'s lexc is in char-def-token space (hence the
/// `with_segment_query_encoder` the controllable path attaches), while `emit`'s is plain surface
/// space and its proposer queries with plain NFD. Composing or unioning across that boundary without
/// reconciling the spaces is how you get a network that looks fine and silently matches nothing --
/// checked the hard way: applying the token encoder to an `emit`-built net manufactures false
/// zero-candidate results.
///
/// Deliberately ignores the candidate's plan: the tuned path derives topology from a plan it builds
/// itself ([`crate::emit`]'s `plan_topology_decisions` reads two booleans off it), so it can express
/// the DEFAULT compilation of this grammar and nothing else. That is exactly why only the baseline is
/// routed here; see the caller.
/// Runs `words` through `analyzer`, scores, budget-checks, and certifies against `expected`.
///
/// Shared by EVERY evaluation strategy on purpose. The only thing that differs between the three
/// ([`EmissionStrategy`]) is how the network — and therefore the analyzer — was obtained; everything
/// from "apply the corpus" onward must be identical, or a cross-strategy comparison would be
/// comparing measurement procedures rather than compilations. This function existing is what makes
/// adding a strategy cost nothing: the previous two strategies each carried their own copy of this
/// block, which is exactly how they would have drifted.
/// The ordinary (unobserved) measurement. Returns the full [`EvaluatedPlan`] — whose `words` is
/// always `None` in this mode — rather than only its `evaluation`, so the counted
/// [`IdentityDivergence`] reaches the run cache from every strategy's call site instead of only
/// from the observed one.
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
    for w in words {
        let t = Instant::now();
        let (outcome, diagnostics, proposals_for_word) = if OBSERVE {
            let profiled = analyzer.analyze_word_with_diagnostics_and_candidates(w);
            (
                profiled.outcome,
                profiled.diagnostics,
                Some(profiled.candidates),
            )
        } else {
            let profiled = analyzer.analyze_word_with_diagnostics(w);
            (profiled.outcome, profiled.diagnostics, None)
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
    let breach = [
        ("states", score.states, budget.states),
        ("arcs", score.arcs, budget.arcs),
        ("build", build, budget.build),
        ("apply", apply, budget.apply),
        ("proposals", proposals, budget.proposals),
        ("confirmation", confirmation, budget.confirmation),
    ]
    .into_iter()
    .find(|(_, v, l)| l.is_some_and(|limit| *v > limit));
    let (certification, divergence) = match breach {
        // A breach short-circuits BEFORE any comparison happens, so the honest divergence is
        // "nothing compared", never a clean zero.
        Some((d, v, Some(l))) => (
            Certification::ResourceBreach {
                dimension: d.into(),
                value: v,
                limit: l,
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
                // Projected again here rather than threaded out of `certify_corpus`: this is the
                // opt-in observed path only, the projection is a handful of string clones per
                // analysis, and keeping the certification path free of an evidence out-parameter is
                // worth more than the saving. `.ok()` is correct rather than lossy -- a projection
                // that fails here failed there too, and the certification already carries the typed
                // fault.
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

/// Shared constructor for every evaluation outcome whose `Score` is zeroed except `build` --
/// nothing past the build step ran, so `apply`/`proposals`/`confirmation`/`confirmation_steps`/
/// `states`/`arcs` are honestly `0`, not "not yet measured" masquerading as a real reading.
/// Recipe-pipeline-hygiene D7: every zeroed-`Score` failure path in this module routes through
/// here (rather than re-inlining the same `Score { .. }` literal at each call site) so a future
/// `Score` field addition has exactly one place to account for it -- forgetting it here fails to
/// compile everywhere it matters, forgetting it at an inline literal fails silently at whichever
/// call sites nobody remembered to update.
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

/// A failure that happened BEFORE any occurrence could be compared. `occurrences` is how many the
/// caller was going to compare, recorded as [`IdentityDivergence::not_compared`] so that a run
/// which never reached the comparison cannot report the clean zero of a run that did.
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

/// [`EmissionStrategy::TunedSurfaceProbed`]: the DEFAULT compilation of this grammar, through
/// [`FomaProposer::new`] (`emit` -> lexc -> foma compile) rather than [`build_controllable`].
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
    // No `with_segment_query_encoder` here, unlike the controllable path: this net is in plain
    // surface space and the production proposer queries it with plain NFD.
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

/// [`EmissionStrategy::TemplatedUnderlyingTokens`]: compile the whole grammar through
/// `emit_underlying_templated` + a real compiled rewrite cascade, rather than through the
/// surface-probed lexc plus synthesized composite entries.
///
/// This is the first candidate in this crate that is neither the controllable-only composed network
/// nor the default surface-probed compilation — i.e. the first one whose network can differ from the
/// baseline's for a reason minimization cannot erase. Like the tuned path it ignores `plan` (this
/// compiler derives its own topology), so it must only ever be offered as its own candidate, never
/// as the realization of some other candidate's plan.
///
/// Uses the proposer `compile_templated_morphotactics` returns, and attaches nothing to it. That is
/// load-bearing rather than incidental: this strategy's lexc is in char-def TOKEN space (it emits
/// underlying tokens over a `SegAlphabet`), so it does need a segment query encoder — and that
/// compiler already attaches one itself. Adding a second here, or omitting it because the tuned
/// surface-probed path omits one, is the space-mismatch this module's own doc records as
/// manufacturing false zero-candidate results.
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

/// Evaluates every plan through build_controllable and the production propose→confirm pipeline.
/// The caller-provided order is preserved; therefore the baseline must be element zero.
///
/// One exception, and it is load-bearing: a plan that needs composite/structural marker subtrees is
/// routed to [`evaluate_via_tuned_emit`] (baseline only) or refused (any permutation), because
/// `build_controllable` cannot build those subtrees and a templated grammar keeps nearly all of its
/// productive morphology there.
pub fn evaluate_plans(
    grammar: &Grammar,
    plans: &[CandidatePlan],
    words: &[String],
    budget: RuntimeBudget,
) -> Result<Vec<RuntimeEvaluation>, OraclePreparationFault> {
    // Positional default, per this function's long-standing contract.
    let flags: Vec<bool> = (0..plans.len()).map(|i| i == 0).collect();
    evaluate_plans_marked(grammar, plans, words, budget, &flags)
}

/// Compatibility wrapper that prepares an isolated corpus for callers outside the optimizer run.
pub fn evaluate_plans_marked(
    grammar: &Grammar,
    plans: &[CandidatePlan],
    words: &[String],
    budget: RuntimeBudget,
    is_baseline: &[bool],
) -> Result<Vec<RuntimeEvaluation>, OraclePreparationFault> {
    let mut cache = RunEvaluationCache::prepare(grammar, words, budget)?;
    Ok(evaluate_plans_marked_with_cache(
        grammar,
        plans,
        words,
        budget,
        is_baseline,
        &mut cache,
    ))
}

/// Evaluate candidates against a caller-owned run cache while preserving the positional baseline
/// contract of [`evaluate_plans`].
pub fn evaluate_plans_with_cache(
    grammar: &Grammar,
    plans: &[CandidatePlan],
    words: &[String],
    budget: RuntimeBudget,
    cache: &mut RunEvaluationCache,
) -> Vec<RuntimeEvaluation> {
    let flags: Vec<bool> = (0..plans.len()).map(|i| i == 0).collect();
    evaluate_plans_marked_with_cache(grammar, plans, words, budget, &flags, cache)
}
/// [`evaluate_plans`], but the caller states which plans are the baseline instead of relying on
/// position.
///
/// This exists because position is NOT usable at the call site that matters. The production optimizer
/// evaluates candidates ONE AT A TIME -- `pg_cli`'s `CandidateEvaluator::evaluate` calls in with
/// `std::slice::from_ref(plan)` -- so every candidate is "element zero" and a positional baseline test
/// silently answers `true` for all of them. That mattered as soon as baseline-only behaviour existed:
/// every permutation of a marker-requiring plan took the baseline's tuned-emit route and was reported
/// as confirmed with the baseline's own network counts. `pg_foma`'s optimizer already tracks
/// `CandidateState::baseline`, so the caller can simply say.
pub fn evaluate_plans_marked_with_cache(
    grammar: &Grammar,
    plans: &[CandidatePlan],
    words: &[String],
    budget: RuntimeBudget,
    is_baseline: &[bool],
    cache: &mut RunEvaluationCache,
) -> Vec<RuntimeEvaluation> {
    evaluate_plans_marked_with_cache_mode::<false>(
        grammar,
        plans,
        words,
        budget,
        is_baseline,
        cache,
    )
    .into_iter()
    .map(|result| result.evaluation)
    .collect()
}

/// Evaluate candidates against a caller-owned cache while retaining exact per-word oracle,
/// confirmed-analysis, and final-candidate evidence for equivalence gates.
#[doc(hidden)]
pub fn evaluate_plans_marked_observed_with_cache(
    grammar: &Grammar,
    plans: &[CandidatePlan],
    words: &[String],
    budget: RuntimeBudget,
    is_baseline: &[bool],
    cache: &mut RunEvaluationCache,
) -> Vec<RuntimeEvaluationObservation> {
    evaluate_plans_marked_with_cache_mode::<true>(grammar, plans, words, budget, is_baseline, cache)
        .into_iter()
        .zip(plans)
        .map(|(result, plan)| RuntimeEvaluationObservation {
            requested_strategy: plan.strategy,
            evaluation: result.evaluation,
            words: result.words,
        })
        .collect()
}

/// One [`EmissionStrategy::PlanComposed`] candidate, realized into an owned, apply-ready proposer.
enum RealizedPlanComposed {
    Ready {
        proposer: FomaProposer,
        states: u64,
        arcs: u64,
        build: u64,
    },
    Failed {
        certification: Certification,
        build: u64,
    },
}

/// Build ONE plan-composed candidate's network and turn it into a proposer.
///
/// # Why this is a function and not inline in the evaluator
/// The confirmation-free accuracy path ([`assess_accuracy_with_cache`]) has to propose from the SAME
/// network the certification path measures, or the two answer questions about different
/// compilations while carrying the same candidate's name. A second copy of this sequence would
/// drift, and this module's own history says so out loud: before `measure_and_certify` existed, each
/// strategy carried its own copy of the measurement block, "which is exactly how they would have
/// drifted".
///
/// # Why it can return an OWNED proposer
/// `FomaProposer::from_precompiled_network` calls `apply_init`, which deep-clones the compiled `Fsm`
/// into the handle. The net is therefore dead the moment the proposer exists — precisely what
/// `FomaProposer::new` already relies on (see [`FomaProposer`]'s own doc: the `Fsm` "is consumed by
/// `apply_init` and can be (is) dropped once the handle exists"). So dropping `net` at the end of
/// this function is not a liberty taken here; it is the documented lifetime of that type.
fn realize_plan_composed(
    candidate: &CandidatePlan,
    grammar: &Grammar,
    opts: &FomaOptions,
    alphabet: &SegAlphabet<'_>,
    prules: &[&PhonRuleDef],
    compose: &ComposeBudget,
    report: crate::emit::EmitReport,
) -> RealizedPlanComposed {
    let t = Instant::now();
    let built = build_candidate(candidate, opts, grammar, alphabet, prules, compose);
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
    // Mandatory finish step, not an optimization: without the boundary-token cleanup compose
    // and re-minimize, the net still carries the inter-morph boundary tokens `uflexc` emits,
    // which a surface query never contains -- every `apply_up` returns nothing and recall
    // reads as zero. See `crate::build::finish_controllable_net`.
    let mut net = match crate::build::finish_controllable_net(
        opts,
        net,
        surface_table(grammar),
        alphabet,
        compose,
    ) {
        Ok(net) => net,
        Err(e) => {
            return RealizedPlanComposed::Failed {
                certification: Certification::BuildFailed {
                    reason: format!("boundary-cleanup finish failed: {e}"),
                },
                build,
            };
        }
    };
    // Closes an asymmetry, not a measured hot spot. `FomaProposer::new` (the hand-spun path)
    // calls `prepare_network_for_apply` at `crate::analyzer`; `from_precompiled_network` --
    // the constructor EVERY plan-composed candidate goes through -- deliberately prepares
    // nothing, so above `ARC_SORT_MIN_ARCS` the hand-spun baseline got foma's binary-search
    // arc traversal and the plan-composed candidate it is compared against did not.
    //
    // MEASURED, and the measurement is a null result on everything checked in: of the 45
    // discoverable conformance fixtures that build a plan-composed net, ZERO cross
    // `ARC_SORT_MIN_ARCS` (10,000) -- the largest is 479 arcs
    // (`polysynthetic-stratal-derivation-chain` / `recipe-strata-generic`). Verified by
    // reading `net.arcs_sorted_out` directly: on those nets it is `false` as built, still
    // `false` after this call, and only `true` under a forced `fsm_sort_arcs`. So on our
    // fixtures this line is provably inert, and the 1.49x-2.05x figure in
    // `ARC_SORT_MIN_ARCS`'s own doc says nothing about them. It engages only on a
    // large real grammar -- the plan-composed net for the private `sena` corpus is 21,114
    // arcs -- which is why this is worth having despite buying nothing in CI.
    //
    // Placed BEFORE the counts are read for the same reason `FomaProposer::new` reads its counts
    // after sorting: `fsm_sort_arcs` reorders arcs and never adds or removes a state or arc, so
    // `statecount`/`arccount` are identical either side of it and the score cannot move.
    crate::analyzer::prepare_network_for_apply(&mut net);
    let (states, arcs) = (net.statecount as u64, net.arccount as u64);
    RealizedPlanComposed::Ready {
        proposer: FomaProposer::from_precompiled_network(&net, report)
            .with_segment_query_encoder(surface_table(grammar)),
        states,
        arcs,
        build,
    }
}

fn evaluate_plans_marked_with_cache_mode<const OBSERVE: bool>(
    grammar: &Grammar,
    plans: &[CandidatePlan],
    words: &[String],
    budget: RuntimeBudget,
    is_baseline: &[bool],
    cache: &mut RunEvaluationCache,
) -> Vec<EvaluatedPlan> {
    assert_eq!(
        plans.len(),
        is_baseline.len(),
        "one baseline flag per plan is required -- a mismatch here is how a permutation would silently \
         be treated as the baseline"
    );
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = crate::enumerate::prules_in_order(grammar);
    let opts = FomaOptions::default();
    let compose = ComposeBudget::from_env().with_step_timeout(
        budget
            .build
            .filter(|limit| *limit != u64::MAX)
            .map(std::time::Duration::from_nanos),
    );
    let selection = cache.select(words);
    let comparable = selection.comparable;
    let expected = selection.expected;
    let oracle_capped = selection.capped;
    let exclusions = selection.exclusions;
    let corpus_evidence =
        corpus_completeness_evidence(words, &comparable, &exclusions, cache.corpus.oracle);
    let words = &comparable[..];
    // CRITICAL: a step-capped oracle result must NEVER reach `certify_corpus`. The FST side may
    // legitimately produce analyses the truncated oracle never found — that would surface as a
    // bogus `IdentityMismatch`/`MultiplicityMismatch` (a phantom "grammar bug" that is actually an
    // oracle bug), or, worse, a genuinely incomplete candidate could look right against an equally
    // truncated ground truth and wrongly certify. So this returns before `build_candidate` is even
    // called for any plan in this batch — evidence about a network built against an `expected` that
    // is known-partial is not evidence at all.
    //
    // There is no longer a `timed-out` branch here, and that absence is the fix: the wall clock is a
    // liveness net that aborts preparation (`OraclePreparationFault`), so by the time control
    // reaches this point no occurrence can have been classified by a clock.
    //
    // Certification is all-or-nothing over the requested corpus. Even when other words have
    // complete expectations, dropping one excluded word would silently certify a subset under the
    // requested corpus's name and hash only that subset. Refuse the whole batch instead, retaining
    // the requested/included/excluded evidence for the report.
    if !exclusions.is_empty() {
        let stage = if oracle_capped {
            "oracle-capped"
        } else {
            "corpus-incomplete"
        };
        let refused = plans
            .iter()
            .map(|plan| {
                // Nothing compiled -- the corpus itself was refused -- so the honest answer is the
                // strategy that was requested.
                failed_evaluated_over(
                    plan.strategy,
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
    // The confirm side's grammar-static pieces, built at most ONCE for the whole run and handed
    // back after each candidate. Every one of the three is a pure function of `grammar` and is
    // immutable in use, so all candidates in a run would otherwise rebuild identical objects; the
    // `Morpher` is the expensive one, because `Morpher::new` runs `RuleCache::build`, which compiles
    // every matcher FST in the grammar (see `FomaAnalyzer::from_cached_with_morpher`'s doc). Lazy
    // rather than eager on purpose: a run whose candidates are all whole-grammar strategies
    // (`TunedSurfaceProbed`/`TemplatedUnderlyingTokens`, returned below before this is touched)
    // must not start paying for a confirm-side morpher it never uses.
    let mut confirm_pieces: Option<(
        crate::peel::ReduplicationPeeler,
        Vec<Option<crate::confirm::MorphemeOwner>>,
        pg_parse::Morpher<'_>,
    )> = None;
    let evaluated: Vec<EvaluatedPlan> = plans
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            // Strategy dispatch comes FIRST: the two whole-grammar strategies are realized by their
            // own compilers and never touch `build_controllable`, so routing them through the
            // composed path below would build the controllable subtree and then attribute that
            // network to a candidate that asked for a different compilation entirely.
            match candidate.strategy {
                EmissionStrategy::PlanComposed => {}
                EmissionStrategy::TunedSurfaceProbed => {
                    return if OBSERVE {
                        evaluate_via_tuned_emit_mode::<true>(grammar, words, &expected, budget)
                    } else {
                        evaluate_via_tuned_emit_mode::<false>(grammar, words, &expected, budget)
                    }
                }
                EmissionStrategy::TemplatedUnderlyingTokens => {
                    return if OBSERVE {
                        evaluate_via_templated_emit_mode::<true>(grammar, words, &expected, budget)
                    } else {
                        evaluate_via_templated_emit_mode::<false>(grammar, words, &expected, budget)
                    }
                }
            }
            // Only the plan-composed strategy consumes this report, so whole-grammar candidates
            // do not pay an unconditional duplicate emission.
            let report = cache.emission_report(grammar);
            let (proposer, score0, build) = match realize_plan_composed(
                candidate,
                grammar,
                &opts,
                &alphabet,
                &prules,
                &compose,
                report,
            ) {
                RealizedPlanComposed::Ready {
                    proposer,
                    states,
                    arcs,
                    build,
                } => (proposer, (states, arcs), build),
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
            let (peeler, owners, morpher) = confirm_pieces.take().unwrap_or_else(|| {
                (
                    crate::peel::ReduplicationPeeler::new(grammar),
                    crate::confirm::build_morpheme_owners(grammar),
                    pg_parse::Morpher::new(grammar, usize::MAX),
                )
            });
            let mut analyzer =
                FomaAnalyzer::from_cached_with_morpher(grammar, proposer, peeler, owners, morpher);
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
            // Hand the grammar-static confirm pieces back for the next candidate. Nothing above
            // mutates any of them -- confirm reads `&self.morpher`/`&self.owners`, and the peeler's
            // own entry point is `peel_candidates(&self, ..)` -- so the next candidate gets objects
            // indistinguishable from the ones `from_cached` would have rebuilt for it. (Only the
            // proposer is per-candidate mutable state, and it is dropped here with its network.)
            let (_spent_proposer, peeler, owners, morpher) = analyzer.into_parts_with_morpher();
            confirm_pieces = Some((peeler, owners, morpher));
            let certification = measured.evaluation.certification.clone();
            // Evidence first, fallback second -- and ONLY on a real failure.
            //
            // Marker presence does not mean the controllable path is inadequate, it means it MIGHT be.
            // Checked: `mpr-gated-exception`'s plan carries a marker and all three of its candidates
            // confirm on the controllable net with real proposals. An earlier version of this routed on
            // marker presence alone and dropped that grammar from 3 confirmations to 1, refusing
            // permutations the controllable builder handles perfectly well. So a candidate that
            // CONFIRMED here is done: its verdict came from a network that honours its own plan, which
            // is strictly better evidence than the tuned path can give (that path cannot express a
            // permutation at all).
            if certification.selectable() {
                return measured;
            }
            let markers = crate::build::unbuildable_markers(&candidate.plan);
            if markers.is_empty() {
                // Failed on a network that fully represents its own plan: a real result, reported as is.
                return measured;
            }
            // Failed AND the plan needed subtrees `build_controllable` cannot build. On a templated
            // grammar those subtrees hold nearly all of the productive morphology -- measured, same
            // grammar: 133 states / 3307 arcs controllable-only against 6376 / 68693 from the tuned
            // `crate::emit` path, which proposed correctly where the controllable net proposed nothing
            // for 19 of 20 words. So the failure is probably the builder's, not the grammar's.
            if is_baseline[index] {
                // The tuned path CAN build them, and for the baseline its network is the right answer:
                // the default compilation of this grammar.
                return if OBSERVE {
                    evaluate_via_tuned_emit_mode::<true>(grammar, words, &expected, budget)
                } else {
                    evaluate_via_tuned_emit_mode::<false>(grammar, words, &expected, budget)
                };
            }
            // A permutation, though, cannot be rescued: the tuned path derives topology from a plan it
            // builds itself, so putting a permutation through it would measure the BASELINE network and
            // report it as this permutation -- a fabricated comparison. Refuse, naming why.
            let EvaluatedPlan {
                evaluation,
                words,
                divergence,
            } = measured;
            EvaluatedPlan {
                divergence,
                evaluation: RuntimeEvaluation {
                    realized_strategy: EmissionStrategy::PlanComposed,
                    certification: Certification::Unsupported {
                        reason: format!(
                            "plan structure cannot be honoured: it failed on the controllable-only network \
                             ({certification:?}) and requires subtrees build_controllable cannot build \
                             ({}); the tuned emit path that can build them derives topology from its own \
                             plan, so evaluating this permutation there would measure the baseline network \
                             and report it as this permutation",
                            markers
                                .iter()
                                .map(|m| format!("{m:?}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    },
                    score: evaluation.score,
                },
                words,
            }
        })
        .collect();
    // Fold each candidate's counted parity divergence into the RUN. Done here, after the closure's
    // mutable borrow of `cache` has ended, rather than inside it: the exclusion branch above has its
    // own fold for the same reason, so every path out of this function contributes exactly once.
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
/// certification battery. Read [`crate::recipe_accuracy`]'s module doc first — it carries the
/// soundness argument, the one hazard, and the reasons this is NOT a certification.
///
/// # What it does per candidate
/// 1. Realizes the candidate's network exactly as [`evaluate_plans_marked_with_cache`] does — the
///    SAME [`realize_plan_composed`] for composed plans, the same two compilers for the whole-grammar
///    strategies. A different network would make the verdict a statement about something else.
/// 2. Proposes over the corpus through [`crate::composite::propose_union_peel_with_diagnostics`],
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
/// - **It computes no [`Score`] and moves no ranking.** Containment cannot price a compilation; the
///   objective stays `confirmation_steps`-led and untouched.
/// - **It never truncates a proposal set and never caps per-candidate work.** Either would be
///   indistinguishable from the recall failure this exists to find.
///
/// # Corpus eligibility is the certification path's, unchanged
/// The same all-or-nothing rule applies: if ANY requested occurrence was excluded (a step-capped
/// oracle result, an unprepared row), every candidate comes back
/// [`crate::recipe_accuracy::AccuracyVerdict::NotDetermined`] rather than assessed over a silently
/// narrowed corpus. Assessing a subset under the requested corpus's name is the failure mode
/// [`PreparedCorpus`] exists to prevent, and it is no more acceptable for a cheap verdict than for
/// an expensive one.
pub fn assess_accuracy_with_cache(
    grammar: &Grammar,
    plans: &[CandidatePlan],
    words: &[String],
    budget: RuntimeBudget,
    is_baseline: &[bool],
    cache: &mut RunEvaluationCache,
) -> Vec<CandidateAccuracy> {
    assert_eq!(
        plans.len(),
        is_baseline.len(),
        "one baseline flag per plan is required -- the same contract \
         evaluate_plans_marked_with_cache states, for the same reason"
    );
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = crate::enumerate::prules_in_order(grammar);
    let opts = FomaOptions::default();
    let compose = ComposeBudget::from_env().with_step_timeout(
        budget
            .build
            .filter(|limit| *limit != u64::MAX)
            .map(std::time::Duration::from_nanos),
    );
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
                requested_strategy: plan.strategy,
                realized_strategy: plan.strategy,
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
    // Grammar-static and reusable across every candidate in the run: the peeler is a pure function
    // of the grammar and its entry point takes `&self`, exactly as the certification path's own
    // hand-back of `confirm_pieces` relies on. Note what is NOT built beside it -- no morpher, no
    // morpheme-owner map, because nothing here confirms.
    let peeler = crate::peel::ReduplicationPeeler::new(grammar);
    let peel_budget = ComposeBudget::from_env();
    plans
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let realized = realize_accuracy_proposer(
                candidate,
                grammar,
                &opts,
                &alphabet,
                &prules,
                &compose,
                cache,
            );
            let (realized_strategy, proposer) = match realized {
                Ok(ready) => ready,
                Err((realized_strategy, reason)) => {
                    return CandidateAccuracy {
                        requested_strategy: candidate.strategy,
                        realized_strategy,
                        verdict: AccuracyVerdict::NotDetermined { reason },
                        counters: AccuracyCounters::default(),
                    }
                }
            };
            let assessed = assess_one(
                candidate.strategy,
                realized_strategy,
                grammar,
                proposer,
                &peeler,
                &peel_budget,
                &selection,
            );
            // EVIDENCE FIRST, FALLBACK SECOND -- and only on a real failure. This mirrors
            // `evaluate_plans_marked_with_cache_mode`'s own fallback structure step for step, and the
            // ordering is the load-bearing part, not the destination.
            //
            // An earlier version of this function checked `unbuildable_markers` BEFORE proposing, on
            // the reasoning that the markers are already known and re-proposing costs work. That was
            // wrong twice over. Marker presence does not mean the controllable path is inadequate, it
            // means it MIGHT be -- `mpr-gated-exception` carries a marker and confirms on the
            // controllable net perfectly well -- so an early check routes a candidate that would have
            // succeeded to a DIFFERENT compiler, and the two paths then report different
            // `realized_strategy` for the same candidate, which is precisely the mis-attribution
            // `RuntimeEvaluation::realized_strategy`'s own doc exists to prevent. It is also slower,
            // not faster, in exactly the case that matters: it built the composed net, threw it away,
            // and compiled a second one, on the path whose entire purpose is speed.
            if assessed.verdict.is_no_loss() {
                return assessed;
            }
            if realized_strategy != EmissionStrategy::PlanComposed {
                // A whole-grammar compiler's result is its own answer; there is nothing to fall back
                // to and nothing that could rescue it.
                return assessed;
            }
            let markers = crate::build::unbuildable_markers(&candidate.plan);
            if markers.is_empty() {
                // Assessed on a network that fully represents its own plan: a real result, as is.
                return assessed;
            }
            // Assessed as lossy AND the plan needed subtrees `build_controllable` cannot build. On a
            // templated grammar those subtrees hold nearly all of the productive morphology, so the
            // loss is probably the builder's, not the grammar's.
            if !is_baseline[index] {
                // A permutation cannot be rescued: the tuned path derives topology from a plan it
                // builds itself, so assessing a permutation there would measure the BASELINE network
                // and report it as this permutation. Refuse, naming why -- and keep the counters,
                // because the check DID run; only its attribution is unavailable.
                return CandidateAccuracy {
                    requested_strategy: candidate.strategy,
                    realized_strategy: EmissionStrategy::PlanComposed,
                    verdict: AccuracyVerdict::NotDetermined {
                        reason: format!(
                            "plan structure cannot be honoured: it lost analyses on the \
                             controllable-only network ({:?}) and requires subtrees \
                             build_controllable cannot build ({}); the tuned emit path that can \
                             build them derives topology from its own plan, so assessing this \
                             permutation there would measure the baseline network and report it as \
                             this permutation",
                            assessed.verdict,
                            markers
                                .iter()
                                .map(|marker| format!("{marker:?}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    },
                    counters: assessed.counters,
                };
            }
            // The tuned path CAN build them, and for the baseline its network is the right answer:
            // the default compilation of this grammar.
            match FomaProposer::new(grammar) {
                Ok(tuned) => assess_one(
                    candidate.strategy,
                    EmissionStrategy::TunedSurfaceProbed,
                    grammar,
                    tuned,
                    &peeler,
                    &peel_budget,
                    &selection,
                ),
                Err(e) => CandidateAccuracy {
                    requested_strategy: candidate.strategy,
                    realized_strategy: EmissionStrategy::TunedSurfaceProbed,
                    verdict: AccuracyVerdict::NotDetermined {
                        reason: format!(
                            "plan needs subtrees build_controllable cannot build ({}) and the tuned \
                             emit path that can build them failed: {e}",
                            markers
                                .iter()
                                .map(|marker| format!("{marker:?}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    },
                    counters: assessed.counters,
                },
            }
        })
        .collect()
}

/// Realize one candidate into the proposer the accuracy check will propose from — the same network
/// the certification path measures, by construction (see [`realize_plan_composed`]).
#[allow(clippy::type_complexity)]
fn realize_accuracy_proposer(
    candidate: &CandidatePlan,
    grammar: &Grammar,
    opts: &FomaOptions,
    alphabet: &SegAlphabet<'_>,
    prules: &[&PhonRuleDef],
    compose: &ComposeBudget,
    cache: &mut RunEvaluationCache,
) -> Result<(EmissionStrategy, FomaProposer), (EmissionStrategy, String)> {
    match candidate.strategy {
        EmissionStrategy::TunedSurfaceProbed => FomaProposer::new(grammar)
            .map(|proposer| (EmissionStrategy::TunedSurfaceProbed, proposer))
            .map_err(|e| {
                (
                    EmissionStrategy::TunedSurfaceProbed,
                    format!("tuned emit path failed to build: {e}"),
                )
            }),
        EmissionStrategy::TemplatedUnderlyingTokens => {
            crate::templated_compile::compile_templated_morphotactics(grammar)
                .map(|output| (EmissionStrategy::TemplatedUnderlyingTokens, output.proposer))
                .map_err(|e| {
                    (
                        EmissionStrategy::TemplatedUnderlyingTokens,
                        format!("templated underlying-token path failed to build: {e}"),
                    )
                })
        }
        EmissionStrategy::PlanComposed => {
            let report = cache.emission_report(grammar);
            match realize_plan_composed(candidate, grammar, opts, alphabet, prules, compose, report)
            {
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

/// Propose over the eligible corpus and check containment. No confirmation engine is reachable from
/// here — that is the point.
fn assess_one(
    requested_strategy: EmissionStrategy,
    realized_strategy: EmissionStrategy,
    grammar: &Grammar,
    mut proposer: FomaProposer,
    peeler: &crate::peel::ReduplicationPeeler,
    peel_budget: &ComposeBudget,
    selection: &PreparedSelection,
) -> CandidateAccuracy {
    let mut counters = AccuracyCounters::default();
    let mut misses = Vec::new();
    for (occurrence_ordinal, (word, oracle)) in selection.expected.iter().enumerate() {
        // UNBOUNDED, deliberately: a bounded proposal set that trips reads as undergeneration, and a
        // per-candidate work budget that silently prunes candidates was merged once, never fired,
        // and was reverted. `ApplyBudget::unbounded()` therefore cannot report `Incomplete`.
        let proposed = crate::composite::propose_union_peel_with_diagnostics(
            grammar,
            &mut proposer,
            peeler,
            peel_budget,
            word,
            &crate::compose_budget::ApplyBudget::unbounded(),
        );
        let (proposals, _peel_used, peel_chain_depth_error, diagnostics, proposal_calls) =
            match proposed {
                Ok(complete) => complete,
                Err(_) => unreachable!("ApplyBudget::unbounded() can never report Incomplete"),
            };
        let mut occurrence = crate::recipe_accuracy::check_occurrence(
            word,
            occurrence_ordinal as u64,
            oracle,
            &proposals,
            &mut misses,
        );
        occurrence.raw_paths = diagnostics.raw_paths as u64;
        occurrence.proposal_calls = proposal_calls as u64;
        // A refused peel means this occurrence's proposal set is INCOMPLETE. Recorded rather than
        // ignored; `verdict_from` turns any non-zero total into `NotDetermined`, because a
        // containment verdict over a truncated proposal set is not a verdict.
        occurrence.peel_refusals = u64::from(peel_chain_depth_error.is_some());
        counters.absorb(occurrence);
    }
    CandidateAccuracy {
        requested_strategy,
        realized_strategy,
        verdict: crate::recipe_accuracy::verdict_from(&counters, misses),
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

    /// Same identity as `wa(n)`, differing only in `mpr` -- a field `AnalysisIdentity` does not
    /// capture and `WordAnalysis::Eq` does.
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
        // THE behavior change. `wa(0)` and `wa_other_mpr(0)` are unequal as `WordAnalysis` values
        // (different `mpr`), so the old full-structural comparison called this an
        // `IdentityMismatch`. `AnalysisIdentity` captures ordered stable morpheme keys, root
        // position, and category -- `mpr` is engine-internal payload, not identity -- so the two
        // engines agree here and must certify.
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
        // The other side of the change: widening the equivalence must not make everything equal.
        // Different morphemes are different identities.
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
        // Replaces `multiplicity_mismatch`, which asserted that expected=[x, x] against actual=[x]
        // is a `MultiplicityMismatch`. That guarantee is no longer wanted: the project's parity
        // relation is deduplicated identity SET equality, so an oracle that reached one analysis by
        // two derivational paths and a candidate that reached it by one have found the same set and
        // agree. Multiplicity was never evidence of a grammar difference; it is evidence about
        // redundant proposal work, which the next test keeps.
        let g = fixture();
        assert!(certify_word(&g, "w", &[wa(0), wa(0)], &[wa(0)]).selectable());
        assert!(certify_word(&g, "w", &[wa(0)], &[wa(0), wa(0), wa(0)]).selectable());
    }

    #[test]
    fn collapsed_duplicate_paths_survive_as_evidence() {
        // The verdict ignores duplicate paths; the evidence must not lose them, or "these agree"
        // would be indistinguishable from "these agree and one side did three times the work".
        let g = fixture();
        let projected =
            crate::parity::OccurrenceIdentities::project(&[wa(0), wa(0), wa(0)], &g).unwrap();
        assert_eq!(projected.len(), 1);
        assert_eq!(projected.entries()[0].duplicate_paths, 3);
        assert_eq!(projected.collapsed_paths(), 2);
    }

    #[test]
    fn repeated_corpus_rows_stay_separate_observations() {
        // Deduplication is WITHIN an occurrence. Two rows for the same word are two observations,
        // and a candidate that disagrees on the second row fails even though the union of both
        // rows' identities would match.
        let g = fixture();
        let expected = vec![("w".into(), vec![wa(0)]), ("w".into(), vec![wa(1)])];
        assert!(certify_corpus(&g, &expected, &expected).selectable());
        let changed = vec![("w".into(), vec![wa(0)]), ("w".into(), vec![wa(0)])];
        assert!(matches!(
            certify_corpus(&g, &expected, &changed),
            Certification::IdentityMismatch { .. }
        ));
        // Collapsing the two rows into one would also change the ROW COUNT, which is refused
        // outright rather than certified against a shorter corpus.
        let collapsed = vec![("w".to_string(), vec![wa(0), wa(1)])];
        assert!(matches!(
            certify_corpus(&g, &expected, &collapsed),
            Certification::Truncated { .. }
        ));
    }

    #[test]
    fn a_projection_failure_is_a_typed_fault_never_a_mismatch_and_never_a_pass() {
        // An analysis referencing a morpheme its own model lacks is an internal inconsistency. Both
        // wrong answers are excluded here: reporting it as a mismatch would blame the grammar for an
        // engine bug, and -- the more dangerous one -- the two sides here are IDENTICAL, so any
        // comparison that projects lazily or not at all calls this a full confirmation.
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
        // `PreparedCorpus::prepare` builds its oracle with `Morpher::new` (no `SuppliedRootOverlay`)
        // and reads it through `parse_word` (`ParseOptions::default()`, `guess_root: false`), so a
        // word the lexicon cannot reach comes back with zero analyses rather than a fabricated root.
        //
        // NOTE ON WHAT THIS DOES AND DOES NOT PROVE: this is a preservation guard, not the v1 scope
        // gate. It passes both before and after this change, and on a fixture carrying no lexical
        // patterns it could not distinguish `guess_root: true` from `false` anyway. The scope is
        // ENFORCED by the two refusal tests above, which fail before this change; this one exists so
        // that a future edit swapping `parse_word` for `parse_word_opts(.., guess_root)` or
        // attaching an overlay is caught at the oracle rather than only at the refusal.
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
        // The vacuous-pass guard: agreeing about nothing is not agreement. Unchanged by the move to
        // set equality -- an empty set equals an empty set, which is exactly why this guard exists.
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
