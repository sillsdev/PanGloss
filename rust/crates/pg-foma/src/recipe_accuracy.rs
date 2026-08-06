//! **The accuracy question, answered without confirmation.**
//!
//! # Two questions, two mechanisms
//!
//! "Does this compilation work?" and "is this compilation as cheap as it could be?" are different
//! questions, and one mechanism was being asked to answer both. `crate::recipe_optimizer::Score`
//! ranks candidates by DETERMINISTIC WORK, and its leading component is `confirmation_steps` — so
//! any attempt to replace confirmation with a cheaper check does not speed the same answer up, it
//! redefines what "best" means. That is why such attempts were rejected, and rightly.
//!
//! This module answers only the FIRST question:
//!
//! - **Accuracy** — "did we undergenerate? did we regress?" Answered here, by set containment
//!   against the run's shared oracle result, at a cost of ZERO full-HC confirmation calls per
//!   candidate.
//! - **Speed** — "is this as cheap as it could be?" NOT answered here, and deliberately not
//!   answerable here. Containment says nothing about cost. `crate::recipe_optimizer::Score` and
//!   the certification path remain the whole answer to that, untouched.
//!
//! # Why containment against the oracle is free
//!
//! `crate::recipe_runtime::PreparedCorpus` already runs the ground-truth `pg_parse::Morpher` ONCE
//! per corpus occurrence and shares the result across every candidate in the run
//! (`oracle_calls == words.len()`), and it is COMPLETE for every occurrence that passes eligibility
//! — a step-capped occurrence refuses the whole corpus rather than contributing a partial
//! expectation. So the ground truth for the whole vocabulary is already in hand before the first
//! candidate is built. What remains per candidate is proposing, and then pure set membership.
//!
//! # Why proposing the key is enough
//!
//! `crate::confirm::confirm_batch` routes a confirmed analysis to a candidate by exactly
//! `crate::parity::admission_key` — `(ordered morpheme ordinals, root_index)` — and confirmation
//! is candidate-INDEPENDENT: `confirm_batch` takes no net, no plan, no strategy and no gate
//! partition, so whether a given derivation is VALID cannot vary by candidate. Together those two
//! facts mean: if a candidate proposes the admission key of an oracle analysis, that candidate's own
//! pins admit that analysis, and confirmation would have returned it. Containment therefore detects
//! undergeneration EXACTLY, with no confirmation performed.
//!
//! # The one hazard, and why it is measured elsewhere
//!
//! The converse direction — that confirmation can never yield an identity the oracle's set lacks —
//! is what makes "no undergeneration" equivalent to "confirms". It is strongly argued (the
//! candidate's confirm is a RESTRICTED `parse_word_selected`; the oracle is the same engine
//! unrestricted) but not airtight: `pg_rules::word::WordKey` excludes `syn_fs` while
//! `pg_parse::identity::AnalysisIdentity::category` is projected from it, so first-wins dedup order
//! — which the restriction perturbs — could in principle surface a candidate-only category. That is
//! inference, never an observation, so it is COUNTED on the ordinary certification path rather than
//! assumed: `crate::parity::IdentityDivergence::candidate_only_identities`, reachable as
//! `RunEvaluationCache::identity_divergence`. A verdict from this module is a claim about
//! undergeneration whatever that count says; it is equivalent to full certification only while that
//! count is zero.
//!
//! Relatedly, an admission key is COARSER than an identity (it carries no category), so
//! `AccuracyCounters::oracle_key_ambiguities` counts the occurrences where one oracle admission
//! key stood for analyses of more than one category. Zero means the two granularities coincide on
//! that corpus; non-zero is reported rather than hidden, because there this check is weaker than
//! identity containment and a reader must be able to see that.
//!
//! # What this module refuses to do
//!
//! - It never truncates a word's proposal set. A truncated set reads as undergeneration, which is
//!   the one thing this mechanism exists to detect.
//! - It carries no wall clock. Nothing here is an eligibility or verdict input; a word must not be
//!   able to change classification under machine load.
//! - It never caps per-candidate work. A silent prune is indistinguishable from a recall failure.

use std::collections::BTreeSet;

use pg_parse::WordAnalysis;

use crate::enumerate::EmissionStrategy;
use crate::parity::{admission_key, AdmissionKey};
use crate::tags::Candidate;

/// One proposal's `AdmissionKey`. The mirror of `admission_key` on the propose side, and
/// verbatim what `propose UNION peel` already deduplicates by.
pub fn candidate_admission_key(candidate: &Candidate) -> AdmissionKey {
    (
        candidate.morphemes.iter().map(|m| m.0).collect(),
        candidate.root_index,
    )
}

/// How many missing analyses a verdict NAMES before it stops listing them.
///
/// The counts in `AccuracyCounters` are always exact; this bounds only the witness list, so a
/// badly broken candidate produces a readable report instead of one row per lost analysis. Same
/// reasoning as `recipe_runtime`'s own mismatch-detail sample bound.
pub const MISS_WITNESS_SAMPLE: usize = 16;

/// One analysis the oracle found that the candidate never proposed — a RECALL FAILURE, named
/// exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccuracyMiss {
    pub word: String,
    /// Which occurrence of `word` in the checked corpus. Repeated corpus rows are separate
    /// observations, exactly as they are for the parity relation.
    pub occurrence_ordinal: u64,
    /// The oracle analysis's ordered morpheme ordinals.
    pub morpheme_ids: Vec<u32>,
    pub root_index: i32,
}

impl std::fmt::Display for AccuracyMiss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} (occurrence {}): oracle analysis morphemes={:?} root_index={} was never proposed",
            self.word, self.occurrence_ordinal, self.morpheme_ids, self.root_index
        )
    }
}

/// Deterministic counters for one candidate's accuracy pass. No wall clock, by construction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccuracyCounters {
    /// Corpus occurrences actually checked.
    pub occurrences_checked: u64,
    /// Distinct oracle admission keys the candidate had to offer, summed over occurrences.
    pub oracle_keys_required: u64,
    /// How many of those it did offer.
    pub oracle_keys_matched: u64,
    /// How many it did not — the exact recall-failure count, even when the witness list is capped
    /// at `MISS_WITNESS_SAMPLE`.
    pub oracle_keys_missed: u64,
    /// Distinct proposal admission keys, summed over occurrences. Larger than
    /// `oracle_keys_required` is EXPECTED and is not a defect: the FST proposes and HC prunes, so
    /// over-proposal is the design. This counter prices that work; it never fails a verdict.
    pub proposal_keys: u64,
    /// **The fire counter.** Set-membership tests performed. Zero means the accuracy check did not
    /// execute — an inert mechanism, which is exactly the failure mode a merged-but-never-fired
    /// budget produced once already.
    pub membership_tests: u64,
    /// Occurrences where one oracle admission key stood for oracle analyses of more than one
    /// category, i.e. where this key-level check is coarser than identity-level comparison. See the
    /// module doc.
    pub oracle_key_ambiguities: u64,
    /// `apply_up` paths the proposer yielded, before decode/dedup — real propose-side work.
    pub raw_paths: u64,
    /// Proposal calls made (direct plus every reduplication-peel residual query).
    pub proposal_calls: u64,
    /// Occurrences whose reduplication peel was REFUSED by its chain-depth budget (ADR 0003), so
    /// that occurrence's proposal set is incomplete through no fault of the compilation.
    ///
    /// This must never be zero-and-forgotten. A refused peel contributes zero candidates of its own,
    /// so a missing key on such an occurrence is indistinguishable from a genuine recall failure —
    /// which is exactly the "never truncate a word's proposal set" rule stated as a counter.
    /// A non-zero count here is `AccuracyVerdict::NotDetermined` in `verdict_from`, never
    /// undergeneration, pinned by
    /// `a_truncated_proposal_set_is_undetermined_rather_than_a_recall_failure`. Production's
    /// default leaves `chain_depth_cap` unset, so this is 0 unless
    /// `HC_COMPOSE_CHAIN_DEPTH_BUDGET` says otherwise.
    pub peel_refusals: u64,
    /// Occurrences whose PROPOSAL was refused by the per-word apply-path envelope
    /// (`crate::compose_budget::DEFAULT_EVALUATION_APPLY_PATH_BUDGET`), so that occurrence's
    /// proposal set is incomplete through no fault of the compilation.
    ///
    /// Exactly `Self::peel_refusals`'s contract, one dimension over, and it exists for the same
    /// reason stated in the same words: a refused proposal contributes fewer candidates than the
    /// unbounded walk would, so a missing key on such an occurrence is indistinguishable from a
    /// genuine recall failure. A non-zero count here is `AccuracyVerdict::NotDetermined` in
    /// `verdict_from`, never undergeneration -- the identical branch shape `peel_refusals` has,
    /// pinned by `a_truncated_proposal_set_is_undetermined_rather_than_a_recall_failure`.
    ///
    /// This path proposes with a bounded
    /// `crate::compose_budget::DEFAULT_EVALUATION_APPLY_PATH_BUDGET` rather than
    /// `ApplyBudget::unbounded()`: a bounded proposal set that trips would otherwise read as
    /// undergeneration, which this counter fixes by making the refusal visible instead. Leaving
    /// it unbounded is not a safe fallback -- `machine:edge-cases/deep-optional-affix-nesting`
    /// reaches 12^12 raw `apply_up` paths for one 13-character word and exhausts committed memory
    /// (see that budget's own measurement table).
    pub apply_refusals: u64,
    /// **Full-HC confirmation calls performed. Structurally zero.**
    ///
    /// Read from the SAME `crate::composite::FomaWordDiagnostics` field the certification path
    /// reads its `Score::confirmation` from, so this is not a cosmetic zero: it reads zero because
    /// nothing incremented it, and the accuracy path has no `pg_parse::Morpher` in scope to
    /// increment it with (see `crate::composite::propose_union_peel_with_diagnostics`).
    pub confirmation_calls: u64,
    /// Full-HC step ticks consumed. Structurally zero, same argument.
    pub confirmation_steps: u64,
}

impl AccuracyCounters {
    pub fn absorb(&mut self, other: Self) {
        self.occurrences_checked = self
            .occurrences_checked
            .saturating_add(other.occurrences_checked);
        self.oracle_keys_required = self
            .oracle_keys_required
            .saturating_add(other.oracle_keys_required);
        self.oracle_keys_matched = self
            .oracle_keys_matched
            .saturating_add(other.oracle_keys_matched);
        self.oracle_keys_missed = self
            .oracle_keys_missed
            .saturating_add(other.oracle_keys_missed);
        self.proposal_keys = self.proposal_keys.saturating_add(other.proposal_keys);
        self.membership_tests = self.membership_tests.saturating_add(other.membership_tests);
        self.oracle_key_ambiguities = self
            .oracle_key_ambiguities
            .saturating_add(other.oracle_key_ambiguities);
        self.raw_paths = self.raw_paths.saturating_add(other.raw_paths);
        self.proposal_calls = self.proposal_calls.saturating_add(other.proposal_calls);
        self.peel_refusals = self.peel_refusals.saturating_add(other.peel_refusals);
        self.apply_refusals = self.apply_refusals.saturating_add(other.apply_refusals);
        self.confirmation_calls = self
            .confirmation_calls
            .saturating_add(other.confirmation_calls);
        self.confirmation_steps = self
            .confirmation_steps
            .saturating_add(other.confirmation_steps);
    }
}

/// What the accuracy check concluded about one candidate.
///
/// Note what is NOT here: no "confirmed", no score, no ranking position. This verdict is not a
/// certification and must never be substituted for one — a candidate may only be SELECTED on
/// full-HC confirmation, which is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccuracyVerdict {
    /// Every oracle admission key on every checked occurrence was proposed. No undergeneration.
    NoLoss,
    /// At least one oracle analysis was never proposed; witnesses capped at `MISS_WITNESS_SAMPLE`, `oracle_keys_missed` stays exact.
    Undergenerated { misses: Vec<AccuracyMiss> },
    /// The check could not be performed (build failure or a refused corpus) -- never a pass.
    NotDetermined { reason: String },
}

impl AccuracyVerdict {
    /// Whether this verdict is a positive statement that nothing was lost. `false` for
    /// `Self::NotDetermined`, deliberately.
    pub fn is_no_loss(&self) -> bool {
        matches!(self, Self::NoLoss)
    }
}

/// One candidate's accuracy result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateAccuracy {
    /// The strategy the candidate DECLARED.
    pub requested_strategy: EmissionStrategy,
    /// The compiler that actually produced the network the proposals came from. These differ (see
    /// `crate::recipe_runtime::RuntimeEvaluation::realized_strategy`), and anything attributing a
    /// measurement must read this one.
    pub realized_strategy: EmissionStrategy,
    pub verdict: AccuracyVerdict,
    pub counters: AccuracyCounters,
}

/// Check ONE corpus occurrence: is every oracle admission key present among the candidate's
/// proposals?
///
/// Pure set membership over already-computed inputs — no engine call of any kind. `misses` is
/// appended to (never truncated in place) and the caller decides how many witnesses to keep.
pub fn check_occurrence(
    word: &str,
    occurrence_ordinal: u64,
    oracle: &[WordAnalysis],
    proposals: &[Candidate],
    misses: &mut Vec<AccuracyMiss>,
) -> AccuracyCounters {
    let proposed: BTreeSet<AdmissionKey> = proposals.iter().map(candidate_admission_key).collect();
    let required: BTreeSet<AdmissionKey> = oracle.iter().map(admission_key).collect();
    let mut counters = AccuracyCounters {
        occurrences_checked: 1,
        oracle_keys_required: required.len() as u64,
        proposal_keys: proposed.len() as u64,
        oracle_key_ambiguities: u64::from(oracle_key_is_ambiguous(oracle)),
        ..AccuracyCounters::default()
    };
    for key in required {
        counters.membership_tests += 1;
        if proposed.contains(&key) {
            counters.oracle_keys_matched += 1;
        } else {
            counters.oracle_keys_missed += 1;
            misses.push(AccuracyMiss {
                word: word.to_owned(),
                occurrence_ordinal,
                morpheme_ids: key.0,
                root_index: key.1,
            });
        }
    }
    counters
}

/// Whether any admission key in `oracle` stands for more than one category; compares `pos_id` (injective) rather than the projected category, so this needs no grammar.
fn oracle_key_is_ambiguous(oracle: &[WordAnalysis]) -> bool {
    let mut seen: Vec<(AdmissionKey, Option<u32>)> = Vec::with_capacity(oracle.len());
    for analysis in oracle {
        let key = admission_key(analysis);
        if seen
            .iter()
            .any(|(other, pos)| *other == key && *pos != analysis.pos_id)
        {
            return true;
        }
        seen.push((key, analysis.pos_id));
    }
    false
}

/// Fold one occurrence's misses and counters into a whole-corpus verdict.
///
/// Separate from `check_occurrence` so the per-occurrence check stays a pure function of its own
/// inputs, and so the witness cap is applied in exactly one place.
pub fn verdict_from(counters: &AccuracyCounters, mut misses: Vec<AccuracyMiss>) -> AccuracyVerdict {
    // Checked first: a refused peel makes the set incomplete, so neither "no loss" nor "undergenerated" would be honest.
    if counters.peel_refusals > 0 {
        return AccuracyVerdict::NotDetermined {
            reason: format!(
                "{} of {} occurrences had their reduplication peel refused by its chain-depth \
                 budget, so their proposal sets are incomplete; a containment verdict over a \
                 truncated proposal set is not a verdict",
                counters.peel_refusals, counters.occurrences_checked
            ),
        };
    }
    // Same rule for the other incompleteness dimension: an apply-path-refused proposal set can be neither a pass nor a recall failure.
    if counters.apply_refusals > 0 {
        return AccuracyVerdict::NotDetermined {
            reason: format!(
                "{} of {} occurrences had their proposal refused by the per-word apply-path \
                 envelope, so their proposal sets are incomplete; a containment verdict over a \
                 truncated proposal set is not a verdict",
                counters.apply_refusals, counters.occurrences_checked
            ),
        };
    }
    // Agreeing about nothing is not agreement: an oracle with zero analyses would make containment hold vacuously for an empty network.
    if counters.occurrences_checked > 0 && counters.oracle_keys_required == 0 {
        return AccuracyVerdict::NotDetermined {
            reason: format!(
                "the oracle produced no analysis for any of the {} checked occurrences, so \
                 containment holds vacuously and says nothing about this candidate",
                counters.occurrences_checked
            ),
        };
    }
    if counters.occurrences_checked == 0 {
        return AccuracyVerdict::NotDetermined {
            reason: "no occurrence was checked".into(),
        };
    }
    if counters.oracle_keys_missed == 0 {
        return AccuracyVerdict::NoLoss;
    }
    misses.truncate(MISS_WITNESS_SAMPLE);
    AccuracyVerdict::Undergenerated { misses }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use pg_grammar::model::MorphemeId;

    fn analysis(morphemes: &[u32], root_index: i32) -> WordAnalysis {
        WordAnalysis {
            morpheme_ids: morphemes.to_vec(),
            root_morpheme_index: root_index,
            ..crate::test_support::parity_analysis(0)
        }
    }

    fn proposal(morphemes: &[u32], root_index: i32) -> Candidate {
        Candidate {
            morphemes: morphemes.iter().copied().map(MorphemeId).collect(),
            root_index,
        }
    }

    #[test]
    fn a_proposed_key_for_every_oracle_analysis_is_no_loss_and_the_check_fires() {
        let oracle = [analysis(&[0, 1], 0), analysis(&[2], 0)];
        // Deliberately over-proposing: extra keys are the design (FST proposes, HC prunes), never a defect.
        let proposals = [proposal(&[0, 1], 0), proposal(&[2], 0), proposal(&[9], 0)];
        let mut misses = Vec::new();
        let counters = check_occurrence("w", 0, &oracle, &proposals, &mut misses);
        assert!(misses.is_empty());
        assert_eq!(counters.oracle_keys_required, 2);
        assert_eq!(counters.oracle_keys_matched, 2);
        assert_eq!(counters.oracle_keys_missed, 0);
        assert_eq!(counters.proposal_keys, 3);
        assert_eq!(
            counters.membership_tests, 2,
            "the check must actually execute once per required key"
        );
        assert_eq!(counters.confirmation_calls, 0);
        assert_eq!(counters.confirmation_steps, 0);
        assert_eq!(verdict_from(&counters, misses), AccuracyVerdict::NoLoss);
    }

    #[test]
    fn a_missing_key_is_named_exactly_and_never_rounded_off() {
        let oracle = [analysis(&[0, 1], 0), analysis(&[2], 0)];
        // `[2]` is absent, and `[0, 1]` at root_index 1 is a different key -- headedness is part of the key.
        let proposals = [proposal(&[0, 1], 0), proposal(&[0, 1], 1)];
        let mut misses = Vec::new();
        let counters = check_occurrence("w", 3, &oracle, &proposals, &mut misses);
        assert_eq!(counters.oracle_keys_missed, 1);
        assert_eq!(counters.membership_tests, 2);
        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].morpheme_ids, vec![2]);
        assert_eq!(misses[0].root_index, 0);
        assert_eq!(misses[0].occurrence_ordinal, 3);
        let verdict = verdict_from(&counters, misses);
        let AccuracyVerdict::Undergenerated { misses } = &verdict else {
            panic!("a lost oracle analysis must read as undergeneration: {verdict:?}");
        };
        assert_eq!(misses.len(), 1);
        assert!(misses[0].to_string().contains("was never proposed"));
    }

    #[test]
    fn root_index_discriminates_headedness_exactly_as_confirm_does() {
        // Same morpheme sequence, different head (`root_index`): both readings are required separately.
        let oracle = [analysis(&[0, 1], 0), analysis(&[0, 1], 1)];
        let mut misses = Vec::new();
        let counters = check_occurrence("w", 0, &oracle, &[proposal(&[0, 1], 0)], &mut misses);
        assert_eq!(counters.oracle_keys_required, 2);
        assert_eq!(counters.oracle_keys_missed, 1);
        assert_eq!(misses[0].root_index, 1);
    }

    #[test]
    fn duplicate_oracle_paths_collapse_and_do_not_demand_duplicate_proposals() {
        // Deduplicated set containment, not multiset: three derivational paths to one analysis still need only one proposed key.
        let oracle = [analysis(&[0], 0), analysis(&[0], 0), analysis(&[0], 0)];
        let mut misses = Vec::new();
        let counters = check_occurrence("w", 0, &oracle, &[proposal(&[0], 0)], &mut misses);
        assert_eq!(counters.oracle_keys_required, 1);
        assert_eq!(counters.membership_tests, 1);
        assert!(misses.is_empty());
    }

    #[test]
    fn an_ambiguous_oracle_key_is_reported_rather_than_hidden() {
        // Two oracle analyses share an admission key but differ in category; key containment is coarser than identity containment.
        let mut second = analysis(&[0], 0);
        second.pos_id = Some(7);
        let mut first = analysis(&[0], 0);
        first.pos_id = Some(1);
        let mut misses = Vec::new();
        let counters =
            check_occurrence("w", 0, &[first, second], &[proposal(&[0], 0)], &mut misses);
        assert_eq!(counters.oracle_key_ambiguities, 1);
        assert_eq!(counters.oracle_keys_required, 1, "one KEY, two identities");

        // ... and an unambiguous corpus reports zero, so the counter is not a constant.
        let plain = [analysis(&[0], 0), analysis(&[1], 0)];
        let mut misses = Vec::new();
        let counters = check_occurrence("w", 0, &plain, &[], &mut misses);
        assert_eq!(counters.oracle_key_ambiguities, 0);
    }

    #[test]
    fn not_determined_is_never_a_pass() {
        let verdict = AccuracyVerdict::NotDetermined {
            reason: "build failed".into(),
        };
        assert!(
            !verdict.is_no_loss(),
            "an undetermined verdict must not read as no-loss"
        );
        assert!(AccuracyVerdict::NoLoss.is_no_loss());
        assert!(!AccuracyVerdict::Undergenerated { misses: Vec::new() }.is_no_loss());
    }

    #[test]
    fn a_truncated_proposal_set_is_undetermined_rather_than_a_recall_failure() {
        // A refused peel makes the set incomplete -- neither blaming the compilation nor calling it no-loss is honest.
        let counters = AccuracyCounters {
            occurrences_checked: 3,
            oracle_keys_required: 4,
            oracle_keys_missed: 2,
            peel_refusals: 1,
            membership_tests: 4,
            ..AccuracyCounters::default()
        };
        let verdict = verdict_from(&counters, Vec::new());
        assert!(
            matches!(&verdict, AccuracyVerdict::NotDetermined { reason }
                if reason.contains("chain-depth")),
            "a refused peel must not be reported as undergeneration: {verdict:?}"
        );
        // Without the refusal, the SAME miss count is a real recall failure.
        let honest = AccuracyCounters {
            peel_refusals: 0,
            ..counters
        };
        assert!(matches!(
            verdict_from(&honest, Vec::new()),
            AccuracyVerdict::Undergenerated { .. }
        ));
    }

    #[test]
    fn agreeing_about_nothing_is_not_agreement() {
        // If the oracle found no analysis at all, containment holds trivially and an empty network would "pass".
        let vacuous = AccuracyCounters {
            occurrences_checked: 5,
            oracle_keys_required: 0,
            membership_tests: 0,
            ..AccuracyCounters::default()
        };
        assert!(
            matches!(verdict_from(&vacuous, Vec::new()), AccuracyVerdict::NotDetermined { reason }
                if reason.contains("vacuously")),
            "a corpus the oracle analyses not at all must not certify anything"
        );
        // Nor may an empty check.
        assert!(matches!(
            verdict_from(&AccuracyCounters::default(), Vec::new()),
            AccuracyVerdict::NotDetermined { .. }
        ));
        // One real required key is enough to make the verdict meaningful.
        let real = AccuracyCounters {
            occurrences_checked: 1,
            oracle_keys_required: 1,
            oracle_keys_matched: 1,
            membership_tests: 1,
            ..AccuracyCounters::default()
        };
        assert_eq!(verdict_from(&real, Vec::new()), AccuracyVerdict::NoLoss);
    }

    #[test]
    fn the_witness_list_is_capped_while_the_count_stays_exact() {
        let oracle: Vec<WordAnalysis> = (0..MISS_WITNESS_SAMPLE as u32 + 5)
            .map(|n| analysis(&[n], 0))
            .collect();
        let mut misses = Vec::new();
        let counters = check_occurrence("w", 0, &oracle, &[], &mut misses);
        assert_eq!(counters.oracle_keys_missed, MISS_WITNESS_SAMPLE as u64 + 5);
        let AccuracyVerdict::Undergenerated { misses } = verdict_from(&counters, misses) else {
            panic!("every key missing must read as undergeneration");
        };
        assert_eq!(misses.len(), MISS_WITNESS_SAMPLE);
    }
}
