//! What a shadow run configures, and what it reports back.
//!
//! Shadow mode exists to answer one question a fire count cannot: for the candidates a pass would
//! have killed, what did the confirmer actually spend on them? A pass can fire on every candidate
//! and save nothing, because the confirmer may already reject those candidates for free — so the
//! only figure that argues for moving a check into a filter is the cost of the specific candidates
//! that check removes.
//!
//! [`ShadowCostAttribution`] therefore reports a distribution, not a mean. Per-candidate
//! confirmation cost is heavy-tailed: one expensive candidate pays for thousands of filter
//! evaluations, and an average over a long tail says nothing about either end. It also separates
//! what is exactly known from what is not, rather than dividing a group total by its membership —
//! a per-candidate number invented that way would be the measurement's own answer fed back to it.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use crate::candidate_filter::decision::StablePassId;
use crate::candidate_filter::index::FilterIndex;
use crate::candidate_filter::pipeline::{
    CandidateFilter, FilterBudget, FilterCompletion, FilterMode,
};
use crate::candidate_filter::report::{CandidateDeath, LedgerCaps, PassCounters};
use crate::confirm::ConfirmChunkCost;

/// How a `FomaAnalyzer` filters candidates before confirming them.
///
/// A filter can only be installed by a caller who already holds a [`CandidateFilter`], and pass
/// lists are constructible only through this crate's own test-support seam, so an ordinary build
/// has no way to reach anything but [`FilterMode::Off`]. That is the intent: the seam that supplies
/// rejection authority is the dangerous one, not this configuration.
#[derive(Clone)]
pub struct CandidateFilterSettings {
    mode: FilterMode,
    filter: Option<Arc<CandidateFilter>>,
    index: Option<Arc<FilterIndex>>,
    grammar_revision: u64,
    lexicon_revision: u64,
    budget: FilterBudget,
    ledger_caps: LedgerCaps,
}

/// No per-event record by default: the counters answer every question a shadow run asks.
const DEFAULT_LEDGER_CAPS: LedgerCaps = LedgerCaps {
    max_events: 0,
    max_candidate_deaths: usize::MAX,
};

impl CandidateFilterSettings {
    /// No pass runs and confirmation is reached exactly as it was before any filter existed.
    pub fn off() -> Self {
        Self {
            mode: FilterMode::Off,
            filter: None,
            index: None,
            grammar_revision: 0,
            lexicon_revision: 0,
            budget: FilterBudget::unlimited(),
            ledger_caps: DEFAULT_LEDGER_CAPS,
        }
    }

    pub fn new(
        mode: FilterMode,
        filter: Arc<CandidateFilter>,
        index: Arc<FilterIndex>,
        grammar_revision: u64,
        lexicon_revision: u64,
    ) -> Self {
        Self {
            mode,
            filter: Some(filter),
            index: Some(index),
            grammar_revision,
            lexicon_revision,
            budget: FilterBudget::unlimited(),
            ledger_caps: DEFAULT_LEDGER_CAPS,
        }
    }

    pub fn mode(&self) -> FilterMode {
        self.mode
    }

    /// The declared pass order, which is what identifies a profile to a report.
    pub fn pass_ids(&self) -> Vec<StablePassId> {
        self.filter
            .as_ref()
            .map(|filter| filter.pass_ids())
            .unwrap_or_default()
    }

    pub fn index(&self) -> Option<&Arc<FilterIndex>> {
        self.index.as_ref()
    }

    pub fn grammar_revision(&self) -> u64 {
        self.grammar_revision
    }

    pub fn lexicon_revision(&self) -> u64 {
        self.lexicon_revision
    }

    pub fn budget(&self) -> FilterBudget {
        self.budget
    }

    pub fn ledger_caps(&self) -> LedgerCaps {
        self.ledger_caps
    }

    /// Whether any pass will actually be evaluated.
    pub fn is_active(&self) -> bool {
        !matches!(self.mode, FilterMode::Off) && self.filter.is_some()
    }

    pub(crate) fn filter(&self) -> Option<&CandidateFilter> {
        self.filter.as_deref()
    }
}

impl Default for CandidateFilterSettings {
    fn default() -> Self {
        Self::off()
    }
}

impl std::fmt::Debug for CandidateFilterSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CandidateFilterSettings")
            .field("mode", &self.mode)
            .field("passes", &self.pass_ids())
            .field("grammar_revision", &self.grammar_revision)
            .field("lexicon_revision", &self.lexicon_revision)
            .finish()
    }
}

/// What the confirmer spent on the candidates a run would have removed.
///
/// The four `would_die_*` counts partition [`Self::would_die_candidates`]. Only two of them carry
/// an exact per-candidate cost: a candidate that entered no chunk cost exactly zero, and a
/// candidate that was its chunk's only member cost exactly that chunk's figure. The other two are
/// reported as counts and deliberately contribute no number to the distribution.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShadowCostAttribution {
    /// Candidates every witness of which a pass rejected.
    pub would_die_candidates: usize,
    /// Would-die candidates the run never sent to the confirmer, so nothing about their cost was
    /// observed. Always zero in shadow mode, which is the mode that measures.
    pub would_die_not_presented: usize,
    /// Would-die candidates that entered no confirmation chunk: pin resolution turned them away
    /// before any parse, so the confirmer spent exactly nothing on them.
    pub would_die_never_grouped: usize,
    /// Would-die candidates that were the only member of their chunk, whose cost is therefore
    /// exactly that candidate's.
    pub would_die_sole_member: usize,
    /// Would-die candidates that shared a chunk, whose individual cost this measurement cannot
    /// separate out.
    pub would_die_shared_member: usize,
    /// Sum, median, 90th percentile and maximum over the exactly-attributed candidates only.
    pub exact_steps_total: usize,
    pub exact_steps_median: usize,
    pub exact_steps_p90: usize,
    pub exact_steps_max: usize,
    /// Exactly-attributed candidates whose confirmation cost was zero. A pass whose deaths are
    /// mostly here is mirroring work the confirmer already skips.
    pub exact_zero_cost: usize,
    /// Chunks every member of which would die. Only these disappear entirely under enforcement;
    /// a chunk that merely loses members still runs.
    pub removable_chunks: usize,
    pub removable_steps: usize,
    /// Steps belonging to chunks that would keep at least one member, and so would still be paid.
    pub surviving_chunk_steps: usize,
    /// Observations, never assertions: a duration is not reproducible.
    pub exact_elapsed: Duration,
    pub removable_elapsed: Duration,
}

/// Everything one word's filter evaluation established, alongside what confirmation then did.
#[derive(Clone, Debug)]
pub struct FilterShadowReport {
    pub mode: FilterMode,
    /// Candidate identities the proposer offered, before any witness was built.
    pub raw_candidate_identities: usize,
    /// Witnesses those identities were filtered through; zero when no pass ran.
    pub candidate_witnesses: usize,
    /// One pass's visit to one witness.
    pub filter_steps: u64,
    pub filter_keeps: u64,
    pub filter_defers: u64,
    /// Witnesses ended by a rejection.
    pub filter_rejections: u64,
    /// Candidates every witness of which was rejected. In shadow mode they are still confirmed.
    pub filter_candidates_removed: u64,
    /// Pass evaluations that unwound; nonzero means a pass is broken, never that recall was lost.
    pub filter_pass_panics: u64,
    /// Would-die candidates the confirmer then returned an analysis for. Nonzero means a pass is
    /// unsound and no profile containing it may be enforced.
    pub shadow_false_rejections: u64,
    pub hc_candidates_received: usize,
    pub completion: FilterCompletion,
    pub per_pass: BTreeMap<StablePassId, PassCounters>,
    pub attribution: ShadowCostAttribution,
    /// The death record behind each false rejection, bounded by the run's ledger caps.
    pub false_rejection_deaths: Vec<CandidateDeath>,
    pub death_records_omitted: u64,
}

impl FilterShadowReport {
    /// The report of a word no pass was run over.
    pub fn inactive(mode: FilterMode, candidates: usize) -> Self {
        Self {
            mode,
            raw_candidate_identities: candidates,
            candidate_witnesses: 0,
            filter_steps: 0,
            filter_keeps: 0,
            filter_defers: 0,
            filter_rejections: 0,
            filter_candidates_removed: 0,
            filter_pass_panics: 0,
            shadow_false_rejections: 0,
            hc_candidates_received: candidates,
            completion: FilterCompletion::Complete,
            per_pass: BTreeMap::new(),
            attribution: ShadowCostAttribution::default(),
            false_rejection_deaths: Vec::new(),
            death_records_omitted: 0,
        }
    }
}

impl Default for FilterShadowReport {
    fn default() -> Self {
        Self::inactive(FilterMode::Off, 0)
    }
}

/// Attributes confirmation cost back to a set of candidates something decided against.
///
/// `doomed` holds proposal ordinals; `presented` maps each position in the confirmed slice back to
/// the ordinal it came from, which is what lets a chunk's membership be read in the caller's own
/// index space.
///
/// The doomed set does not have to come from a filter. Supplying the candidates whose confirmation
/// buckets came back empty answers what a *perfect* filter could have removed, which is the ceiling
/// any real filter is measured against — and it is the same arithmetic, so it must not be a second
/// implementation of it.
pub fn attribute(
    doomed: &[usize],
    presented: &[usize],
    chunks: &[ConfirmChunkCost],
) -> ShadowCostAttribution {
    let mut attribution = ShadowCostAttribution {
        would_die_candidates: doomed.len(),
        ..ShadowCostAttribution::default()
    };

    let mut chunk_of: BTreeMap<usize, usize> = BTreeMap::new();
    for (index, chunk) in chunks.iter().enumerate() {
        for &member in &chunk.members {
            if let Some(&ordinal) = presented.get(member) {
                chunk_of.insert(ordinal, index);
            }
        }
        if chunk_is_removable(chunk, doomed, presented) {
            attribution.removable_chunks += 1;
            attribution.removable_steps = attribution.removable_steps.saturating_add(chunk.steps);
            attribution.removable_elapsed += chunk.elapsed;
        } else {
            attribution.surviving_chunk_steps = attribution
                .surviving_chunk_steps
                .saturating_add(chunk.steps);
        }
    }

    let mut exact: Vec<usize> = Vec::new();
    for ordinal in doomed {
        if !presented.contains(ordinal) {
            attribution.would_die_not_presented += 1;
            continue;
        }
        match chunk_of.get(ordinal) {
            None => {
                attribution.would_die_never_grouped += 1;
                exact.push(0);
            }
            Some(&index) if chunks[index].members.len() == 1 => {
                attribution.would_die_sole_member += 1;
                exact.push(chunks[index].steps);
                attribution.exact_elapsed += chunks[index].elapsed;
            }
            Some(_) => attribution.would_die_shared_member += 1,
        }
    }

    exact.sort_unstable();
    attribution.exact_zero_cost = exact.iter().filter(|&&steps| steps == 0).count();
    attribution.exact_steps_total = exact.iter().copied().fold(0usize, usize::saturating_add);
    attribution.exact_steps_max = exact.last().copied().unwrap_or(0);
    attribution.exact_steps_median = percentile(&exact, 50);
    attribution.exact_steps_p90 = percentile(&exact, 90);
    attribution
}

/// Whether removing the doomed candidates would delete this chunk's work entirely.
///
/// Confirmation fuses candidates into a chunk and parses the chunk once, so a chunk that loses some
/// members still costs exactly what it cost before. Only a chunk every member of which is doomed
/// disappears — which is why this predicate, and not the doomed count, is what any saving is
/// measured through.
pub fn chunk_is_removable(chunk: &ConfirmChunkCost, doomed: &[usize], presented: &[usize]) -> bool {
    if chunk.members.is_empty() {
        return false;
    }
    chunk.members.iter().all(|&member| {
        presented
            .get(member)
            .is_some_and(|ordinal| doomed.contains(ordinal))
    })
}

/// Nearest-rank percentile over an ascending slice; zero for an empty one.
fn percentile(ascending: &[usize], percent: usize) -> usize {
    if ascending.is_empty() {
        return 0;
    }
    let rank = (ascending.len() * percent).div_ceil(100).max(1);
    ascending[rank.min(ascending.len()) - 1]
}
