//! Production evaluator for recipe plans.

use crate::analyzer::FomaProposer;
use crate::build::build_controllable;
use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::composite::FomaAnalyzer;
use crate::emit::surface_table;
use crate::enumerate::{CandidatePlan, EmissionStrategy};
use crate::recipe_optimizer::{Certification, CorpusCompletenessEvidence, CorpusExclusion, Score};
use crate::replace::SegAlphabet;
use crate::tags::Candidate;
use foma::options::FomaOptions;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::WordAnalysis;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
#[derive(Debug, Clone)]
struct PreparedWord {
    word: String,
    expected: Option<Vec<WordAnalysis>>,
    capped: bool,
    timed_out: bool,
}

/// A run-scoped corpus prepared once from the oracle. The ground truth and exclusion latches are
/// shared by the pilot and every candidate evaluation in that run.
#[derive(Debug)]
pub struct PreparedCorpus {
    words: Vec<PreparedWord>,
    oracle_calls: usize,
}

impl PreparedCorpus {
    pub fn prepare(grammar: &Grammar, words: &[String], budget: RuntimeBudget) -> Self {
        let cap = budget.oracle_step_cap.unwrap_or(DEFAULT_ORACLE_STEP_CAP);
        let timeout = budget
            .oracle_word_timeout
            .unwrap_or(DEFAULT_ORACLE_WORD_TIMEOUT);
        let morpher = pg_parse::Morpher::new(grammar, cap).with_word_timeout(Some(timeout));
        let mut records = Vec::with_capacity(words.len());
        for word in words {
            let outcome = morpher.parse_word(word);
            records.push(PreparedWord {
                word: word.clone(),
                expected: (!outcome.capped && !outcome.timed_out).then_some(outcome.structured),
                capped: outcome.capped,
                timed_out: outcome.timed_out,
            });
        }
        Self {
            words: records,
            oracle_calls: words.len(),
        }
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
        let mut timed_out = false;
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
            capped |= prepared.capped;
            timed_out |= prepared.timed_out;
            if let Some(analyses) = &prepared.expected {
                comparable.push(word.clone());
                expected.push((word.clone(), analyses.clone()));
            } else {
                let reason = match (prepared.capped, prepared.timed_out) {
                    (true, true) => "oracle-capped-and-timeout",
                    (true, false) => "oracle-capped",
                    (false, true) => "oracle-timeout",
                    (false, false) => "oracle-excluded",
                };
                exclusions.push(CorpusExclusion {
                    requested_ordinal: requested_ordinal as u64,
                    word: word.clone(),
                    reason: reason.into(),
                });
            }
        }
        PreparedSelection {
            comparable,
            expected,
            capped,
            timed_out,
            exclusions,
        }
    }
}

#[derive(Debug)]
struct PreparedSelection {
    comparable: Vec<String>,
    expected: Vec<(String, Vec<WordAnalysis>)>,
    capped: bool,
    timed_out: bool,
    exclusions: Vec<CorpusExclusion>,
}

/// All prepared, reusable evaluation inputs for one optimizer run.
#[derive(Debug)]
pub struct RunEvaluationCache {
    corpus: PreparedCorpus,
    emission_report: Option<crate::emit::EmitReport>,
    emission_report_calls: usize,
}

impl RunEvaluationCache {
    pub fn prepare(grammar: &Grammar, words: &[String], budget: RuntimeBudget) -> Self {
        Self {
            corpus: PreparedCorpus::prepare(grammar, words, budget),
            emission_report: None,
            emission_report_calls: 0,
        }
    }

    pub fn oracle_calls(&self) -> usize {
        self.corpus.oracle_calls()
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

/// Default oracle wall-clock deadline, used whenever [`RuntimeBudget::oracle_word_timeout`] is left
/// `None`. Independent axis from [`DEFAULT_ORACLE_STEP_CAP`] — a word can burn its clock on very few,
/// very expensive steps just as easily as it can burn its step budget on very fast ones — so both
/// bounds are armed together; whichever trips first is the one that stops a given word, and running
/// both costs nothing on well-behaved words. Justified by the same measurement: an otherwise-uncapped
/// `Morpher` with `word_timeout = 2s` returns in 2.83s reporting `timed_out: true` on the same
/// pathological word, instead of never returning.
pub const DEFAULT_ORACLE_WORD_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordEvidence {
    pub word: String,
    pub expected: Vec<WordAnalysis>,
    pub actual: Vec<WordAnalysis>,
    /// The final deduplicated candidate vector sent to confirmation for this word. This is
    /// populated only by the opt-in observed evaluator; ordinary optimizer runs do not retain it.
    pub proposals: Vec<Candidate>,
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

/// Compare analyses as multisets: order is irrelevant, multiplicity is not.
pub fn certify_word(
    word: impl Into<String>,
    expected: Vec<WordAnalysis>,
    actual: Vec<WordAnalysis>,
) -> Certification {
    let word = word.into();
    if expected.len() != actual.len() {
        return Certification::MultiplicityMismatch {
            word,
            expected: expected.len() as u64,
            actual: actual.len() as u64,
        };
    }
    let mut remaining = actual;
    for (i, e) in expected.iter().enumerate() {
        if let Some(p) = remaining.iter().position(|a| a == e) {
            remaining.remove(p);
        } else {
            return Certification::IdentityMismatch {
                word,
                detail: format!("analysis {i} has no matching actual analysis: {e:?}"),
            };
        }
    }
    Certification::FullHcConfirmed {
        words: 1,
        corpus_hash: "runtime".into(),
    }
}

fn corpus_completeness_evidence(
    requested: &[String],
    included: &[String],
    exclusions: &[CorpusExclusion],
) -> CorpusCompletenessEvidence {
    CorpusCompletenessEvidence::from_selection(requested, included, exclusions.to_vec())
}

pub fn certify_corpus(
    expected: &[(String, Vec<WordAnalysis>)],
    actual: &[(String, Vec<WordAnalysis>)],
) -> Certification {
    if expected.len() != actual.len() {
        return Certification::Truncated {
            stage: "full-hc".into(),
            corpus: None,
        };
    }
    let mut failures = expected
        .iter()
        .zip(actual)
        .filter_map(
            |((expected_word, expected_analyses), (actual_word, actual_analyses))| {
                if expected_word != actual_word {
                    return Some(Certification::Truncated {
                        stage: "full-hc-word-order".into(),
                        corpus: None,
                    });
                }
                let verdict = certify_word(
                    expected_word,
                    expected_analyses.clone(),
                    actual_analyses.clone(),
                );
                (!verdict.selectable()).then_some(verdict)
            },
        )
        .collect::<Vec<_>>();
    failures.sort_by_key(|verdict| {
        verdict
            .shortest_disagreement()
            .map(|word| (word.chars().count(), word.to_owned()))
    });
    if let Some(failure) = failures.into_iter().next() {
        return failure;
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
        return Certification::Truncated {
            stage: "no-analyzable-words".into(),
            corpus: None,
        };
    }
    Certification::FullHcConfirmed {
        words: expected.len() as u64,
        corpus_hash: "runtime".into(),
    }
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
    /// Ground-truth oracle wall-clock deadline. Same "`None` = use the default, not unbounded"
    /// convention as `oracle_step_cap` immediately above; resolves to
    /// [`DEFAULT_ORACLE_WORD_TIMEOUT`].
    pub oracle_word_timeout: Option<Duration>,
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
#[allow(clippy::too_many_arguments)]
fn measure_and_certify(
    realized_strategy: EmissionStrategy,
    analyzer: &mut FomaAnalyzer,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
    states: u64,
    arcs: u64,
    build: u64,
) -> RuntimeEvaluation {
    measure_and_certify_inner::<false>(
        realized_strategy,
        analyzer,
        words,
        expected,
        budget,
        states,
        arcs,
        build,
    )
    .evaluation
}

#[allow(clippy::too_many_arguments)]
fn measure_and_certify_observed(
    realized_strategy: EmissionStrategy,
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
    let certification = match breach {
        Some((d, v, Some(l))) => Certification::ResourceBreach {
            dimension: d.into(),
            value: v,
            limit: l,
        },
        _ => match certify_corpus(expected, &actual) {
            Certification::FullHcConfirmed {
                words: word_count, ..
            } => Certification::FullHcConfirmed {
                words: word_count,
                corpus_hash: corpus_hash(words),
            },
            failure => failure,
        },
    };
    let words = observed_proposals.map(|proposals| {
        expected
            .iter()
            .zip(actual.into_iter())
            .zip(proposals)
            .map(
                |(((word, expected), (_, actual)), proposals)| WordEvidence {
                    word: word.clone(),
                    expected: expected.clone(),
                    actual,
                    proposals,
                },
            )
            .collect()
    });
    EvaluatedPlan {
        evaluation: RuntimeEvaluation {
            certification,
            score,
            realized_strategy,
        },
        words,
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

fn failed_evaluated(
    realized_strategy: EmissionStrategy,
    certification: Certification,
    build: u64,
) -> EvaluatedPlan {
    EvaluatedPlan {
        evaluation: failed_evaluation(realized_strategy, certification, build),
        words: None,
    }
}

fn build_failed_evaluated(
    realized_strategy: EmissionStrategy,
    reason: String,
    build: u64,
) -> EvaluatedPlan {
    failed_evaluated(
        realized_strategy,
        Certification::BuildFailed { reason },
        build,
    )
}

fn evaluate_via_tuned_emit(
    grammar: &Grammar,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
) -> RuntimeEvaluation {
    evaluate_via_tuned_emit_mode::<false>(grammar, words, expected, budget).evaluation
}

fn evaluate_via_tuned_emit_observed(
    grammar: &Grammar,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
) -> EvaluatedPlan {
    evaluate_via_tuned_emit_mode::<true>(grammar, words, expected, budget)
}

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
            &mut analyzer,
            words,
            expected,
            budget,
            states.max(0) as u64,
            arcs.max(0) as u64,
            build,
        )
    } else {
        EvaluatedPlan {
            evaluation: measure_and_certify(
                EmissionStrategy::TunedSurfaceProbed,
                &mut analyzer,
                words,
                expected,
                budget,
                states.max(0) as u64,
                arcs.max(0) as u64,
                build,
            ),
            words: None,
        }
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
fn evaluate_via_templated_emit(
    grammar: &Grammar,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
) -> RuntimeEvaluation {
    evaluate_via_templated_emit_mode::<false>(grammar, words, expected, budget).evaluation
}

fn evaluate_via_templated_emit_observed(
    grammar: &Grammar,
    words: &[String],
    expected: &[(String, Vec<WordAnalysis>)],
    budget: RuntimeBudget,
) -> EvaluatedPlan {
    evaluate_via_templated_emit_mode::<true>(grammar, words, expected, budget)
}

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
            )
        }
    };
    let build = elapsed_ns(t).max(1);
    let (states, arcs) = output.proposer.network_counts();
    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(grammar, output.proposer);
    if OBSERVE {
        measure_and_certify_observed(
            EmissionStrategy::TemplatedUnderlyingTokens,
            &mut analyzer,
            words,
            expected,
            budget,
            states.max(0) as u64,
            arcs.max(0) as u64,
            build,
        )
    } else {
        EvaluatedPlan {
            evaluation: measure_and_certify(
                EmissionStrategy::TemplatedUnderlyingTokens,
                &mut analyzer,
                words,
                expected,
                budget,
                states.max(0) as u64,
                arcs.max(0) as u64,
                build,
            ),
            words: None,
        }
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
) -> Vec<RuntimeEvaluation> {
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
) -> Vec<RuntimeEvaluation> {
    let mut cache = RunEvaluationCache::prepare(grammar, words, budget);
    evaluate_plans_marked_with_cache(grammar, plans, words, budget, is_baseline, &mut cache)
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
    let oracle_timed_out = selection.timed_out;
    let exclusions = selection.exclusions;
    let corpus_evidence = corpus_completeness_evidence(words, &comparable, &exclusions);
    let words = &comparable[..];
    // CRITICAL: a capped or timed-out oracle result must NEVER reach `certify_corpus`. The FST side
    // may legitimately produce analyses the truncated oracle never found — that would surface as a
    // bogus `IdentityMismatch`/`MultiplicityMismatch` (a phantom "grammar bug" that is actually an
    // oracle bug), or, worse, a genuinely incomplete candidate could look right against an equally
    // truncated ground truth and wrongly certify. So this returns before `build_candidate` is even
    // called for any plan in this batch — evidence about a network built against a `expected` that
    // is known-partial is not evidence at all. `oracle_capped` is checked before `oracle_timed_out`
    // only because it is the more actionable diagnosis (raise `--oracle-step-cap`); a word that
    // tripped both is reported under whichever this checks first, which is fine since the outcome
    // (non-certifying) is identical either way.
    // Certification is all-or-nothing over the requested corpus. Even when other words have
    // complete expectations, dropping one excluded word would silently certify a subset under the
    // requested corpus's name and hash only that subset. Refuse the whole batch instead, retaining
    // the requested/included/excluded evidence for the report.
    if !exclusions.is_empty() {
        let stage = if oracle_capped {
            "oracle-capped"
        } else if oracle_timed_out {
            "oracle-timeout"
        } else {
            "corpus-incomplete"
        };
        return plans
            .iter()
            .map(|plan| {
                // Nothing compiled -- the corpus itself was refused -- so the honest answer is the
                // strategy that was requested.
                failed_evaluated(
                    plan.strategy,
                    Certification::Truncated {
                        stage: stage.into(),
                        corpus: Some(corpus_evidence.clone()),
                    },
                    0,
                )
            })
            .collect();
    }
    plans
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
                        evaluate_via_tuned_emit_observed(grammar, words, &expected, budget)
                    } else {
                        EvaluatedPlan {
                            evaluation: evaluate_via_tuned_emit(grammar, words, &expected, budget),
                            words: None,
                        }
                    }
                }
                EmissionStrategy::TemplatedUnderlyingTokens => {
                    return if OBSERVE {
                        evaluate_via_templated_emit_observed(grammar, words, &expected, budget)
                    } else {
                        EvaluatedPlan {
                            evaluation: evaluate_via_templated_emit(
                                grammar,
                                words,
                                &expected,
                                budget,
                            ),
                            words: None,
                        }
                    }
                }
            }
            // Only the plan-composed strategy consumes this report, so whole-grammar candidates
            // do not pay an unconditional duplicate emission.
            let report = cache.emission_report(grammar);
            let t = Instant::now();
            let built = build_candidate(candidate, &opts, grammar, &alphabet, &prules, &compose);
            let build = elapsed_ns(t).max(1);
            let Ok(mut built) = built else {
                return build_failed_evaluated(
                    EmissionStrategy::PlanComposed,
                    "build failed".into(),
                    build,
                );
            };
            let Some(net) = built.net.take() else {
                return failed_evaluated(
                    EmissionStrategy::PlanComposed,
                    Certification::Truncated {
                        stage: "empty-network".into(),
                        corpus: None,
                    },
                    build,
                );
            };
            // Mandatory finish step, not an optimization: without the boundary-token cleanup compose
            // and re-minimize, the net still carries the inter-morph boundary tokens `uflexc` emits,
            // which a surface query never contains -- every `apply_up` returns nothing and recall
            // reads as zero. See `crate::build::finish_controllable_net`.
            let net = match crate::build::finish_controllable_net(
                &opts,
                net,
                surface_table(grammar),
                &alphabet,
                &compose,
            ) {
                Ok(net) => net,
                Err(e) => {
                    return build_failed_evaluated(
                        EmissionStrategy::PlanComposed,
                        format!("boundary-cleanup finish failed: {e}"),
                        build,
                    );
                }
            };
            let score0 = (net.statecount as u64, net.arccount as u64);
            let mut analyzer = FomaAnalyzer::from_precompiled_proposer(
                grammar,
                FomaProposer::from_precompiled_network(&net, report.clone())
                    .with_segment_query_encoder(surface_table(grammar)),
            );
            let measured = if OBSERVE {
                measure_and_certify_observed(
                    EmissionStrategy::PlanComposed,
                    &mut analyzer,
                    words,
                    &expected,
                    budget,
                    score0.0,
                    score0.1,
                    build,
                )
            } else {
                EvaluatedPlan {
                    evaluation: measure_and_certify(
                        EmissionStrategy::PlanComposed,
                        &mut analyzer,
                        words,
                        &expected,
                        budget,
                        score0.0,
                        score0.1,
                        build,
                    ),
                    words: None,
                }
            };
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
                    evaluate_via_tuned_emit_observed(grammar, words, &expected, budget)
                } else {
                    EvaluatedPlan {
                        evaluation: evaluate_via_tuned_emit(grammar, words, &expected, budget),
                        words: None,
                    }
                };
            }
            // A permutation, though, cannot be rescued: the tuned path derives topology from a plan it
            // builds itself, so putting a permutation through it would measure the BASELINE network and
            // report it as this permutation -- a fabricated comparison. Refuse, naming why.
            let EvaluatedPlan { evaluation, words } = measured;
            EvaluatedPlan {
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
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wa(n: u32) -> WordAnalysis {
        WordAnalysis {
            morpheme_ids: vec![n],
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: pg_featstruct::FeatureStruct::EMPTY,
            mpr: pg_grammar::model::MprSet::EMPTY,
            guessed: false,
            provenance: pg_parse::AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None],
        }
    }
    #[test]
    fn reordered_equal_multiset() {
        assert!(certify_word("w", vec![wa(1), wa(2)], vec![wa(2), wa(1)]).selectable());
    }
    #[test]
    fn identity_mismatch() {
        assert!(matches!(
            certify_word("w", vec![wa(1)], vec![wa(2)]),
            Certification::IdentityMismatch { .. }
        ));
    }
    #[test]
    fn multiplicity_mismatch() {
        assert!(matches!(
            certify_word("w", vec![wa(1), wa(1)], vec![wa(1)]),
            Certification::MultiplicityMismatch { .. }
        ));
    }
    #[test]
    fn duplicate_corpus_words_preserve_occurrence_multiplicity() {
        let expected = vec![("w".into(), vec![wa(1)]), ("w".into(), vec![wa(2)])];
        assert!(certify_corpus(&expected, &expected).selectable());
        let changed = vec![("w".into(), vec![wa(1)]), ("w".into(), vec![wa(1)])];
        assert!(matches!(
            certify_corpus(&expected, &changed),
            Certification::IdentityMismatch { .. }
        ));
    }
    #[test]
    fn missing_truncated() {
        assert!(matches!(
            certify_corpus(&[("w".into(), vec![])], &[]),
            Certification::Truncated { .. }
        ));
    }
}
