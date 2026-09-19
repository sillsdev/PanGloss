//! Per-stratum analysis/synthesis orchestration and the affix-template battery.
//!
//! Composes `crate::rewrite`, `crate::morph`, and `crate::cascade` into the per-stratum drivers,
//! porting C#'s `AnalysisStratumRule` (prules, then interleaved templates and the mrule cascade,
//! then shape-merge dedup), `SynthesisStratumRule`, the affix-template rules, and `RuleBatch` (a
//! union of rule outputs; disjunctive = early exit).
//!
//! Termination is not the cascades' own doing. They are multi-application and every unapplication
//! grows the word's `mrule_apps`, so their `key(input) != key(result)` self-loop guard is always
//! true and the walk stops only when rules stop applying — on a k!-Unordered stratum, potentially
//! never. A `StepBudget` shared across the whole `parse_word` call bounds it instead: the cascades
//! run uncapped, and an exhausted budget reads to them as "rule didn't apply", so the
//! mutually-recursive template/mrule descent unwinds cleanly with one counter and no cascade edits.

use std::cell::Cell;
use std::rc::Rc;
// `std::time::Instant` panics on wasm32-unknown-unknown; `web_time` substitutes only `Instant`, reusing std's `Duration` unchanged.
use web_time::{Duration, Instant};

use crate::analysis_state_key::{AnalysisStateKey, MorphHistoryKey};
use pg_featstruct::{is_unifiable, subsumes, subtract, union};
use pg_grammar::model::{
    AllomorphId, AllomorphOwner, Grammar, MRuleId, MorphRuleDef, MorphRuleOrder, SlotDef,
    StratumId, TemplateId,
};
use pg_shape::Shape;
use rustc_hash::{FxHashMap as HashMap, FxHashSet};

/// Callback injected so compounding analysis can prune non-heads against the lexicon without
/// `pg-rules` depending on `pg-parse` (the dependency runs the other way). The signature matches
/// `RootAllomorphIndex::search`'s return shape, so `pg-parse` hands its own method straight in.
///
/// Only the lexicon *search* crosses the boundary — do not widen this beyond a raw shape search.
/// The syntactic-FS and MPR-productivity checks that follow each matched root, the per-allomorph
/// resolution, and the per-subrule dedup all stay on this side (`morph::resolve_non_head_roots`),
/// because `Grammar` already carries everything they need. `+ Sync` so parallelizing batch parsing
/// later is not a breaking API change.
pub type NonHeadRootFilter<'a> =
    &'a (dyn Fn(StratumId, &Shape) -> Vec<crate::word::ResolvedRoot> + Sync);

/// The admission unit C#'s `Morpher.RuleSelector` gates: one variant per rule kind with its own
/// selector read site. Rust has no shared `IHCRule` object to hand back, so the caller's closure
/// switches on the variant instead of doing a type test — a deviation in mechanism only, since the
/// SET of admissible rules a predicate computes is what parity requires, not its shape.
///
/// Phonological-rule-level gating has no variant here, deliberately: C#'s own predicate keeps every
/// phonological rule permanently open, so nothing is blocked by the absence.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RuleRef {
    /// Mirrors C#'s stratum-level gate (`AnalysisLanguageRule`/`SynthesisStratumRule`).
    Stratum(StratumId),
    /// Mirrors C#'s template-level gate (`AnalysisAffixTemplateRule`).
    Template(TemplateId),
    /// The morphological-rule-level gate; one `MRuleId` covers affix-process, compounding, and realizational rules alike.
    MRule(MRuleId),
}

/// The Rust mirror of `Morpher.RuleSelector` (`Func<IHCRule, bool>`) — see `RuleRef`'s doc for
/// exactly which gates this predicate reaches. `None` (every pre-existing caller) means
/// "every rule admitted", byte-identical to C#'s default `rule => true`.
pub type RuleFilter<'a> = &'a (dyn Fn(RuleRef) -> bool + Sync);

/// Decided per stratum by the parse owner from grammar facts. The analyzer receives this decision
/// rather than inspecting grammar-wide partiality or template ownership itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FinalTemplateAnalysisPolicy {
    pub enforce: bool,
    pub all_templates_final: bool,
}

/// Synthesis-side override for the final-template rescue gate. Kept separate from analysis policy
/// because synthesis decides against each candidate's own partiality and rule metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FinalTemplateSynthesisPolicy {
    pub always_enforce: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuleInvocationRole {
    Ordinary,
    TemplateSlot,
}

use crate::cache::RuleCache;
use crate::cascade::Cascade;
use crate::stats::{PRuleStatsCtx, StatsCollector};
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::{estimate_word_bytes, runtime_id, FinalTemplateState, Word, WordKey};
use crate::{metathesis, morph, rewrite};

#[cfg(test)]
#[path = "stratum/template_analysis_tests.rs"]
mod template_analysis_tests;

/// Where `apply_mrules`/`apply_templates` push each produced word instead of returning an owned `Vec<Word>` for the caller to concatenate -- the fix for the multiplicative re-flattening `docs/research/live-frontier-memory-bound.md` measured (`push` is the stratum's own dedup fold; `remaining`, when set, is `AnalyzerConfig::max_unapplications`'s live budget).
struct WordSink<'x> {
    push: &'x mut dyn FnMut(Word),
    remaining: Option<&'x Cell<usize>>,
}

impl WordSink<'_> {
    /// Once the output cap is reached, further pushes are refused; the descent keeps recursing but stops growing the output.
    fn done(&self) -> bool {
        self.remaining.is_some_and(|r| r.get() == 0)
    }

    fn push(&mut self, w: Word) {
        if !self.done() {
            (self.push)(w);
        }
    }
}

/// `HC_FRONTIER_STATS=1` counters: the peak size of the *live search frontier* the per-arrival
/// cascade builds — the raw cascade's output, its dedup accumulator, the template battery's
/// output, and the interleaved `apply_mrules`/`apply_templates` recursion depth. This is the
/// bound that survived the memo's removal: it measures the in-flight set one call builds, which
/// no retention policy ever bounded
/// (see `docs/research/live-frontier-memory-bound.md`). Off by default; every field is a plain
/// thread-local max, so disabled cost is one cached env read, no allocation.
pub mod frontier_profile {
    use std::cell::Cell;

    thread_local! {
        static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
        static CUR_DEPTH: Cell<u64> = const { Cell::new(0) };
        static MAX_DEPTH: Cell<u64> = const { Cell::new(0) };
        static MAX_LOCAL_LEN: Cell<u64> = const { Cell::new(0) };
        static MAX_LOCAL_BYTES: Cell<u64> = const { Cell::new(0) };
        static MAX_DEDUP_LEN: Cell<u64> = const { Cell::new(0) };
        static MAX_DEDUP_BYTES: Cell<u64> = const { Cell::new(0) };
        static MAX_RAW_CASCADE_LEN: Cell<u64> = const { Cell::new(0) };
        static MAX_RAW_CASCADE_BYTES: Cell<u64> = const { Cell::new(0) };
        static MAX_TEMPLATE_LEN: Cell<u64> = const { Cell::new(0) };
        static MAX_TEMPLATE_BYTES: Cell<u64> = const { Cell::new(0) };
        static MAX_APPLY_MRULES_LEN: Cell<u64> = const { Cell::new(0) };
        static MAX_APPLY_MRULES_BYTES: Cell<u64> = const { Cell::new(0) };
        static MAX_APPLY_TEMPLATES_LEN: Cell<u64> = const { Cell::new(0) };
        static MAX_APPLY_TEMPLATES_BYTES: Cell<u64> = const { Cell::new(0) };
        static MAX_LIVE_WORDS: Cell<u64> = const { Cell::new(0) };
        static MAX_LIVE_BYTES: Cell<u64> = const { Cell::new(0) };
    }

    /// Cached `HC_FRONTIER_STATS` read (one env lookup per thread).
    pub fn enabled() -> bool {
        ENABLED.with(|c| {
            if let Some(v) = c.get() {
                return v;
            }
            let v = std::env::var("HC_FRONTIER_STATS").is_ok();
            c.set(Some(v));
            v
        })
    }

    /// RAII depth tracker for the mutually-recursive `apply_mrules`/`apply_templates`
    /// descent: construct on entry to a frame, drop restores the caller's
    /// depth. Only ever constructed when `enabled()` (via `.then(DepthGuard::enter)`), so the
    /// no-op cost when disabled is a single `bool` check, no `Cell` traffic.
    pub struct DepthGuard;
    impl DepthGuard {
        pub fn enter() -> Self {
            let d = CUR_DEPTH.with(|c| {
                let d = c.get() + 1;
                c.set(d);
                d
            });
            MAX_DEPTH.with(|c| c.set(c.get().max(d)));
            DepthGuard
        }
    }
    impl Drop for DepthGuard {
        fn drop(&mut self) {
            CUR_DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
        }
    }

    /// The live recursion depth right now, for callers combining it with a durable accumulator's own length into one live-word estimate.
    pub fn current_depth() -> u64 {
        CUR_DEPTH.with(Cell::get)
    }

    macro_rules! recorder {
        ($fn_name:ident, $len_cell:ident, $bytes_cell:ident) => {
            pub fn $fn_name(len: usize, bytes: usize) {
                $len_cell.with(|c| c.set(c.get().max(len as u64)));
                $bytes_cell.with(|c| c.set(c.get().max(bytes as u64)));
            }
        };
    }
    recorder!(record_local, MAX_LOCAL_LEN, MAX_LOCAL_BYTES);
    recorder!(record_dedup, MAX_DEDUP_LEN, MAX_DEDUP_BYTES);
    recorder!(
        record_raw_cascade,
        MAX_RAW_CASCADE_LEN,
        MAX_RAW_CASCADE_BYTES
    );
    recorder!(record_template, MAX_TEMPLATE_LEN, MAX_TEMPLATE_BYTES);
    recorder!(
        record_apply_mrules,
        MAX_APPLY_MRULES_LEN,
        MAX_APPLY_MRULES_BYTES
    );
    recorder!(
        record_apply_templates,
        MAX_APPLY_TEMPLATES_LEN,
        MAX_APPLY_TEMPLATES_BYTES
    );

    /// The size of the one durable per-stratum dedup accumulator plus the live recursion depth at
    /// the moment a word reached it -- the direct replacement metric for the removed
    /// `apply_mrules`/`apply_templates` `Vec<Word>` lengths above, which this accumulator's own
    /// length now bounds instead of multiplying (`docs/research/live-frontier-memory-bound.md`).
    pub fn record_live_words(live: u64) {
        MAX_LIVE_WORDS.with(|c| c.set(c.get().max(live)));
    }

    /// Bytes-denominated companion to [`record_live_words`]: the caller's own running total of
    /// `crate::word::estimate_word_bytes` over every `Word` this stratum call has retained so far
    /// (pushed as a new canonical or folded into one's `alternatives` — either way the payload is
    /// retained, so both count), independent of the word-count metric above (a plain frontier
    /// depth, not a byte quantity, so it has no byte analog to add in here).
    pub fn record_live_bytes(bytes: u64) {
        MAX_LIVE_BYTES.with(|c| c.set(c.get().max(bytes)));
    }

    /// One word's whole cumulative frontier picture -- snapshot only, never reset.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct FrontierProfileSnapshot {
        pub max_depth: u64,
        pub max_local_len: u64,
        pub max_local_bytes: u64,
        pub max_dedup_len: u64,
        pub max_dedup_bytes: u64,
        pub max_raw_cascade_len: u64,
        pub max_raw_cascade_bytes: u64,
        pub max_template_len: u64,
        pub max_template_bytes: u64,
        pub max_apply_mrules_len: u64,
        pub max_apply_mrules_bytes: u64,
        pub max_apply_templates_len: u64,
        pub max_apply_templates_bytes: u64,
        pub max_live_words: u64,
        pub max_live_bytes: u64,
    }

    pub fn snapshot() -> FrontierProfileSnapshot {
        FrontierProfileSnapshot {
            max_depth: MAX_DEPTH.with(Cell::get),
            max_local_len: MAX_LOCAL_LEN.with(Cell::get),
            max_local_bytes: MAX_LOCAL_BYTES.with(Cell::get),
            max_dedup_len: MAX_DEDUP_LEN.with(Cell::get),
            max_dedup_bytes: MAX_DEDUP_BYTES.with(Cell::get),
            max_raw_cascade_len: MAX_RAW_CASCADE_LEN.with(Cell::get),
            max_raw_cascade_bytes: MAX_RAW_CASCADE_BYTES.with(Cell::get),
            max_template_len: MAX_TEMPLATE_LEN.with(Cell::get),
            max_template_bytes: MAX_TEMPLATE_BYTES.with(Cell::get),
            max_apply_mrules_len: MAX_APPLY_MRULES_LEN.with(Cell::get),
            max_apply_mrules_bytes: MAX_APPLY_MRULES_BYTES.with(Cell::get),
            max_apply_templates_len: MAX_APPLY_TEMPLATES_LEN.with(Cell::get),
            max_apply_templates_bytes: MAX_APPLY_TEMPLATES_BYTES.with(Cell::get),
            max_live_words: MAX_LIVE_WORDS.with(Cell::get),
            max_live_bytes: MAX_LIVE_BYTES.with(Cell::get),
        }
    }

    /// Test-only control the real `HC_FRONTIER_STATS=1` env gate can't give unit tests: force this
    /// thread's counters on and zero them, independent of process environment and other threads.
    #[cfg(test)]
    pub fn test_reset_and_enable() {
        ENABLED.with(|c| c.set(Some(true)));
        CUR_DEPTH.with(|c| c.set(0));
        MAX_DEPTH.with(|c| c.set(0));
        MAX_LOCAL_LEN.with(|c| c.set(0));
        MAX_LOCAL_BYTES.with(|c| c.set(0));
        MAX_DEDUP_LEN.with(|c| c.set(0));
        MAX_DEDUP_BYTES.with(|c| c.set(0));
        MAX_RAW_CASCADE_LEN.with(|c| c.set(0));
        MAX_RAW_CASCADE_BYTES.with(|c| c.set(0));
        MAX_TEMPLATE_LEN.with(|c| c.set(0));
        MAX_TEMPLATE_BYTES.with(|c| c.set(0));
        MAX_APPLY_MRULES_LEN.with(|c| c.set(0));
        MAX_APPLY_MRULES_BYTES.with(|c| c.set(0));
        MAX_APPLY_TEMPLATES_LEN.with(|c| c.set(0));
        MAX_APPLY_TEMPLATES_BYTES.with(|c| c.set(0));
        MAX_LIVE_WORDS.with(|c| c.set(0));
        MAX_LIVE_BYTES.with(|c| c.set(0));
    }
}

/// The search-step budget shared across one whole `parse_word` call — every stratum and every
/// candidate word — so the effective bound is `cap`, not `cap × #stratum-analyze calls`. Test call
/// sites with no natural "one parse_word" scope build their own per call.
///
/// Two independent bounds: the step cap, and an optional wall-clock deadline. Synthesis counting is
/// off by default so a heavy analysis cannot starve the candidates it just found of confirmation
/// steps; bounded diagnostic generation opts in via `with_synthesis_counting` to bound the whole
/// exploratory walk with one counter.
///
/// Once a deadline is armed the clock is read on EVERY `over_budget` call, never on a step-count
/// cadence: per-tick cost is not uniform, so a word whose entire run is shorter than one cadence
/// interval would sample the clock once at construction and never again. Reads happen at
/// rule-attempt granularity, where `Instant::now()` is negligible; with no deadline the clock is
/// never read at all. Pinned by
/// `wall_clock_deadline_fires_even_when_total_ticks_never_reach_the_old_check_interval`.
pub struct StepBudget {
    cap: usize,
    steps: Cell<usize>,
    capped: Cell<bool>,
    /// The `--word-timeout-ms` deadline, a second bound orthogonal to `cap`, or `None` for no wall-clock bound.
    deadline: Option<Instant>,
    timed_out: Cell<bool>,
    synthesis_counting: bool,
}

impl StepBudget {
    pub fn new(cap: usize) -> Self {
        StepBudget {
            cap,
            steps: Cell::new(0),
            capped: Cell::new(false),
            deadline: None,
            timed_out: Cell::new(false),
            synthesis_counting: false,
        }
    }

    /// Arm an optional wall-clock deadline alongside the step cap; whichever fires first wins.
    /// `None` is a complete no-op, so callers without `--word-timeout-ms` pay nothing extra.
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.deadline = timeout.map(|d| Instant::now() + d);
        self
    }

    /// Makes synthesis consume this same step counter. Ordinary parse/generation callers leave
    /// this disabled, preserving the historical independent synthesis cap; bounded diagnostic
    /// generation enables it so one budget measures the actual engine walk across many calls.
    pub fn with_synthesis_counting(mut self) -> Self {
        self.synthesis_counting = true;
        self
    }

    /// True (and latches `capped`/`timed_out`) once either bound is exhausted; the cheaper step cap is checked first.
    fn over_budget(&self) -> bool {
        if self.steps.get() >= self.cap {
            self.capped.set(true);
            return true;
        }
        self.deadline_expired()
    }

    /// Wall-clock-only, deliberately omitting the step-cap branch so analysis effort cannot starve synthesis.
    fn deadline_expired(&self) -> bool {
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.timed_out.set(true);
                return true;
            }
        }
        false
    }

    fn synthesis_over_budget(&self) -> bool {
        if !self.synthesis_counting {
            return self.deadline_expired();
        }
        if self.over_budget() {
            return true;
        }
        self.tick();
        false
    }

    fn tick(&self) {
        self.steps.set(self.steps.get() + 1);
    }

    /// Whether this budget's step cap fired at any point during its lifetime (partial results
    /// possible). Never true because of a `--word-timeout-ms` deadline — see `Self::timed_out`.
    pub fn capped(&self) -> bool {
        self.capped.get()
    }

    /// Whether the wall-clock deadline fired. Independent of `Self::capped` — a word can time out
    /// with steps to spare, or hit the step cap inside its deadline. Deliberately not conflated, so
    /// the batch writer can report a distinct `TIMEOUT` outcome.
    pub fn timed_out(&self) -> bool {
        self.timed_out.get()
    }

    /// Raw tick count so far (diagnostic only): how many (un)application attempts a `parse_word`
    /// call consumed, independent of whether the cap was hit.
    pub fn steps(&self) -> usize {
        self.steps.get()
    }
}

#[cfg(test)]
mod step_budget_timeout_tests {
    use super::*;

    /// An uncapped (`usize::MAX`) budget with a short deadline armed must break out promptly, not run the huge iteration bound to completion.
    #[test]
    fn wall_clock_deadline_fires_independent_of_an_uncapped_step_cap() {
        const N_HUGE: u64 = 200_000_000; // large enough to run far longer than the deadline below on any dev/CI machine
        let timeout = Duration::from_millis(30);
        let budget = StepBudget::new(usize::MAX).with_timeout(Some(timeout));

        let start = Instant::now();
        let mut i: u64 = 0;
        while i < N_HUGE {
            if budget.over_budget() {
                break;
            }
            budget.tick();
            i += 1;
        }
        let elapsed = start.elapsed();

        assert!(
            budget.timed_out(),
            "budget must report timed_out() once the deadline elapses (i={i} of {N_HUGE})"
        );
        assert!(
            !budget.capped(),
            "the step cap (usize::MAX) must never fire — timeout and step-cap are independent bounds"
        );
        assert!(
            i < N_HUGE,
            "the loop must break out well before the artificial huge bound, not run to completion"
        );
        // Generous for slow CI machines, but far tighter than a full N_HUGE run.
        assert!(
            elapsed < Duration::from_secs(2),
            "elapsed {elapsed:?} should stay close to the {timeout:?} deadline, not balloon toward \
             an unbounded run"
        );
    }

    /// A deadline already in the past at construction time must fire on the first `over_budget()` check.
    #[test]
    fn zero_deadline_fires_on_the_first_check() {
        let budget = StepBudget::new(usize::MAX).with_timeout(Some(Duration::from_millis(0)));
        // Give the already-past deadline a moment's daylight against timer granularity.
        std::thread::sleep(Duration::from_millis(1));
        assert!(
            budget.over_budget(),
            "an already-past deadline must fire on the first check"
        );
        assert!(budget.timed_out());
        assert!(!budget.capped());
    }

    /// Fewer ticks than one cadence interval, with real time elapsing between them: reading the clock only at step 0 would run to completion.
    #[test]
    fn wall_clock_deadline_fires_even_when_total_ticks_never_reach_the_old_check_interval() {
        const N: u64 = 200; // well under the old 1024-tick cadence interval
        let timeout = Duration::from_millis(50);
        let budget = StepBudget::new(usize::MAX).with_timeout(Some(timeout));

        let start = Instant::now();
        let mut fired = false;
        let mut i: u64 = 0;
        while i < N {
            if budget.over_budget() {
                fired = true;
                break;
            }
            budget.tick();
            std::thread::sleep(Duration::from_millis(1));
            i += 1;
        }
        let elapsed = start.elapsed();

        assert!(
            fired,
            "the 50ms deadline must fire even though the loop only reaches {i} of {N} ticks -- \
             far short of the old 1024-tick cadence interval"
        );
        assert!(budget.timed_out());
        assert!(
            !budget.capped(),
            "the step cap (usize::MAX) must never fire"
        );
        assert!(
            i < N,
            "must break out before exhausting all {N} ticks (i={i}) -- the pre-fix cadence ran to \
             completion here because it never re-sampled the clock after step 0"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "elapsed {elapsed:?} should stay close to the {timeout:?} deadline, not run all {N} \
             ticks worth of sleeps (~{N}ms) unchecked"
        );
    }

    /// `with_timeout(None)` must be a complete no-op, behaving exactly as a plain `StepBudget::new(cap)` would.
    #[test]
    fn no_timeout_never_times_out() {
        let budget = StepBudget::new(5).with_timeout(None);
        for _ in 0..5 {
            assert!(!budget.over_budget());
            budget.tick();
        }
        assert!(budget.over_budget(), "step cap must still fire on its own");
        assert!(budget.capped());
        assert!(
            !budget.timed_out(),
            "no deadline was armed, so timed_out() must stay false"
        );
    }
}

#[cfg(test)]
mod final_template_policy_tests {
    use super::*;

    #[test]
    fn policy_defaults_to_pruning_off() {
        let policy = FinalTemplateAnalysisPolicy::default();
        assert!(!policy.enforce);
        assert!(!policy.all_templates_final);
    }

    #[test]
    fn analysis_state_uses_the_actual_invocation_role() {
        assert_eq!(
            analysis_state_after_mrule(RuleInvocationRole::Ordinary, false),
            FinalTemplateState::NonTemplate
        );
        assert_eq!(
            analysis_state_after_mrule(RuleInvocationRole::TemplateSlot, false),
            FinalTemplateState::None
        );
    }

    #[test]
    fn realizational_rules_clear_the_analysis_state_in_either_role() {
        assert_eq!(
            analysis_state_after_mrule(RuleInvocationRole::Ordinary, true),
            FinalTemplateState::None
        );
        assert_eq!(
            analysis_state_after_mrule(RuleInvocationRole::TemplateSlot, true),
            FinalTemplateState::None
        );
    }
}

#[inline]
fn analysis_state_after_mrule(
    role: RuleInvocationRole,
    is_realizational: bool,
) -> FinalTemplateState {
    match (role, is_realizational) {
        (_, true) | (RuleInvocationRole::TemplateSlot, false) => FinalTemplateState::None,
        (RuleInvocationRole::Ordinary, false) => FinalTemplateState::NonTemplate,
    }
}

/// Configuration for a stratum (un)application run. C# reads these off the `Morpher`; here they are
/// explicit so callers/tests can pin them.
#[derive(Clone, Copy, Debug)]
pub struct AnalyzerConfig {
    /// Mirrors C# `Morpher.MergeEquivalentAnalyses` (default `true`): collapse this stratum's
    /// candidates that share an analysis state (or an equal `WordKey` differing only in
    /// syntactic FS) into one canonical word, folding the repeats into its `Word::alternatives`. A
    /// de-duplication, not a pruning — synthesis re-expands them.
    pub merge_equivalent: bool,
    /// Mirrors C# `Morpher.MaxUnapplications`: stop once the analysis output reaches this many
    /// candidates (`0` = unlimited).
    pub max_unapplications: usize,
    /// Mirrors C# `Morpher.MaxStemCount` (default `2`): refuse to unapply a compounding rule once
    /// `non_heads.len() + 1 >= max_stem_count`. Without it, a compounding subrule whose patterns
    /// are "1+ of any segment" matches every split of every substring at every depth — a
    /// Catalan-scale blowup that burns the whole step budget.
    pub max_stem_count: u32,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        AnalyzerConfig {
            merge_equivalent: true,
            max_unapplications: 0,
            max_stem_count: 2,
        }
    }
}

/// The result of running a stratum's analysis (unapplication) rule.
pub struct StratumAnalysis {
    /// The deduplicated candidate set (the post-prule input word is always the first element — the
    /// "nothing unapplied" candidate that C# seeds `output` with).
    pub words: Vec<Word>,
    /// Whether the step budget fired (partial results). See the module docs.
    pub capped: bool,
}

// Analysis stratum rule.

/// Analyze (unapply) `input` through `stratum` — a faithful port of C#'s `AnalysisStratumRule`.
/// The primary analysis entry point: lexical lookup runs this per stratum, deepest first, and
/// matches root allomorphs against each candidate's shape.
pub fn analyze_stratum(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    budget: &StepBudget,
) -> StratumAnalysis {
    analyze_stratum_filtered(g, stratum, input, cfg, None, None, budget)
}

/// Identical to `analyze_stratum`, plus the compounding non-head root filter (C#'s
/// `AnalysisCompoundingRule.Apply` root-allomorph-search gate). Production callers pass
/// `Some(cache)` — the cache is built once per `Morpher` and shared across every stratum,
/// candidate, and worker of a parse. `None` recompiles matchers per call, which is what the
/// unfiltered entry points above hand in: hand-built fixtures do not always register their
/// `AffixAllomorphDef.id`s in `Grammar::allomorph_owners`, which the cache requires — see
/// `crate::cache`'s module doc.
pub fn analyze_stratum_filtered(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    non_head_root_filter: Option<NonHeadRootFilter>,
    cache: Option<&RuleCache>,
    budget: &StepBudget,
) -> StratumAnalysis {
    analyze_stratum_filtered_ruled(
        g,
        stratum,
        input,
        cfg,
        non_head_root_filter,
        None,
        cache,
        budget,
    )
}

/// Identical to `analyze_stratum_filtered`, plus the morphological-rule/template-level
/// selector — see `RuleFilter`. `None` admits every rule, exactly as passing no filter does.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_filtered_ruled(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    non_head_root_filter: Option<NonHeadRootFilter>,
    rule_filter: Option<RuleFilter>,
    cache: Option<&RuleCache>,
    budget: &StepBudget,
) -> StratumAnalysis {
    analyze_stratum_filtered_ruled_traced(
        g,
        stratum,
        input,
        cfg,
        non_head_root_filter,
        rule_filter,
        cache,
        budget,
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
}

/// `analyze_stratum_filtered_ruled`'s traced sibling — identical in every other respect.
/// The intended caller is `pg_parse::Morpher::parse_word_selected_traced`; see `crate::morph`'s
/// analysis-tracing docs and `StratumAnalyzer`'s `trace`/`parent` fields. `stats` is `None` for
/// every existing caller — gated collection is `pg-parse`'s decision, not this layer's.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_filtered_ruled_traced(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    non_head_root_filter: Option<NonHeadRootFilter>,
    rule_filter: Option<RuleFilter>,
    cache: Option<&RuleCache>,
    budget: &StepBudget,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> StratumAnalysis {
    analyze_stratum_filtered_ruled_traced_with_policy(
        g,
        stratum,
        input,
        cfg,
        non_head_root_filter,
        rule_filter,
        cache,
        budget,
        FinalTemplateAnalysisPolicy::default(),
        stats,
        trace,
        parent,
    )
}

/// Policy-aware sibling of `analyze_stratum_filtered_ruled_traced`. Existing wrappers keep
/// pruning disabled for API compatibility; production parse callers use this sibling after
/// selecting a policy from the grammar's precomputed facts.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_filtered_ruled_traced_with_policy(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    non_head_root_filter: Option<NonHeadRootFilter>,
    rule_filter: Option<RuleFilter>,
    cache: Option<&RuleCache>,
    budget: &StepBudget,
    policy: FinalTemplateAnalysisPolicy,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> StratumAnalysis {
    StratumAnalyzer::new(
        g,
        stratum,
        *cfg,
        non_head_root_filter,
        rule_filter,
        cache,
        budget,
        policy,
        stats,
        trace,
        parent,
    )
    .analyze(input)
}

/// The stratum orchestrator. Borrows the caller's `StepBudget` rather than owning its own step counter.
struct StratumAnalyzer<'g, 'f, 'r, 'c, 'b, 't> {
    g: &'g Grammar,
    stratum_id: StratumId,
    stratum: &'g pg_grammar::model::StratumDef,
    order: MorphRuleOrder,
    /// The stratum's morphological rules reversed; the cascade indexes this list, so the closure maps `i -> reversed[i]` to record the correct `MRuleId`.
    reversed_mrules: Vec<MRuleId>,
    cfg: AnalyzerConfig,
    budget: &'b StepBudget,
    /// The non-head lexicon filter, or `None` for unfiltered. See `NonHeadRootFilter`.
    non_head_root_filter: Option<NonHeadRootFilter<'f>>,
    /// The mrule/template selector, or `None` to admit every rule. See `RuleFilter`.
    rule_filter: Option<RuleFilter<'r>>,
    /// The compile-once FST cache; `None` recompiles per call. See `analyze_stratum_filtered` for why the fallback is still needed.
    cache: Option<&'c RuleCache>,
    /// The gated `--stats` collector, or `None` when stats collection is off; see `crate::stats`.
    stats: Option<&'b StatsCollector>,
    /// Caller-decided final-template prune policy for this stratum.
    policy: FinalTemplateAnalysisPolicy,
    /// The analysis-side trace sink; every entry point but `analyze_stratum_filtered_ruled_traced` passes `NoopSink`.
    trace: &'t dyn TraceSink,
    /// The ambient trace cursor; call sites resolve `word.trace.unwrap_or(parent)` so successful (un)applications nest under the deepest event on that branch.
    parent: TraceHandle,
}

impl<'g, 'f, 'r, 'c, 'b, 't> StratumAnalyzer<'g, 'f, 'r, 'c, 'b, 't> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        g: &'g Grammar,
        stratum_id: StratumId,
        cfg: AnalyzerConfig,
        non_head_root_filter: Option<NonHeadRootFilter<'f>>,
        rule_filter: Option<RuleFilter<'r>>,
        cache: Option<&'c RuleCache>,
        budget: &'b StepBudget,
        policy: FinalTemplateAnalysisPolicy,
        stats: Option<&'b StatsCollector>,
        trace: &'t dyn TraceSink,
        parent: TraceHandle,
    ) -> Self {
        let stratum = &g.strata[stratum_id.0 as usize];
        let reversed_mrules: Vec<MRuleId> = stratum.mrules.iter().rev().copied().collect();
        StratumAnalyzer {
            g,
            stratum_id,
            stratum,
            order: stratum.mrule_order,
            reversed_mrules,
            cfg,
            budget,
            non_head_root_filter,
            rule_filter,
            cache,
            stats,
            policy,
            trace,
            parent,
        }
    }

    /// Rule admission — `true` when no filter was supplied, matching C#'s `rule => true` default.
    #[inline]
    fn rule_admitted(&self, r: RuleRef) -> bool {
        self.rule_filter.is_none_or(|f| f(r))
    }

    /// The order-independent state key for `w` (`AnalyzerConfig::merge_equivalent`'s fold key), with each rule's count saturated at its `max_apps` -- the only reader compares `count >= max_apps`, so counts above it are behaviorally identical (C# does not saturate, `AnalysisStateKey.cs:14-34` -- deliberate divergence).
    fn state_key(&self, w: &Word) -> AnalysisStateKey {
        let morph_history = w
            .morphs
            .iter()
            .map(|morph| MorphHistoryKey {
                allomorph: morph.allomorph,
                morpheme: morph.morpheme,
                order: morph.order,
                status: morph.status,
                runtime_identity: runtime_id(morph.runtime_root.as_deref()).map(str::to_owned),
            })
            .collect();
        let rule_counts = w
            .unapplied_rule_counts
            .iter()
            .filter_map(|(&id, &count)| {
                let cap = self.g.mrules[id.0 as usize].max_apps();
                let saturated = count.min(u32::from(cap));
                (saturated > 0).then_some((id, saturated))
            })
            .collect();
        AnalysisStateKey::new(
            w.shape.clone(),
            w.stratum,
            w.syn_fs.clone(),
            w.real_fs.clone(),
            w.non_heads.len() as u32,
            rule_counts,
            w.flags.final_template_state,
            morph_history,
        )
    }

    /// True once the shared budget is exhausted; delegates to the shared `StepBudget`.
    fn over_budget(&self) -> bool {
        self.budget.over_budget()
    }

    fn tick(&self) {
        self.budget.tick()
    }

    /// Unapply a single morphological rule and record the bookkeeping on every output; `morph::analyze` stays semantics-pure.
    fn apply_one_mrule(&self, id: MRuleId, w: &Word, role: RuleInvocationRole) -> Vec<Word> {
        // Checked before the budget tick: a rejected-by-gate rule was never attempted.
        if !self.rule_admitted(RuleRef::MRule(id)) {
            return Vec::new();
        }
        if self.over_budget() {
            return Vec::new();
        }
        let rule = &self.g.mrules[id.0 as usize];
        // Depth gate; see `AnalyzerConfig::max_stem_count`. Outside `tick()`, same reason as the gate above.
        if matches!(rule, MorphRuleDef::Compounding(_))
            && w.non_heads.len() as u32 + 1 >= self.cfg.max_stem_count
        {
            return Vec::new();
        }
        // Once unapplied on `w`'s trail `max_apps` times, skipped for this candidate; outside `tick()`.
        if w.unapplied_rule_counts.get(&id).copied().unwrap_or(0) >= u32::from(rule.max_apps()) {
            return Vec::new();
        }
        self.tick();
        // `morph`'s allomorph loops record attempts/work/outputs themselves now; see `crate::stats::MRuleStatsCtx`.
        let mstats = self.stats.map(|stats| crate::stats::MRuleStatsCtx {
            stats,
            stratum: self.stratum_id,
            id,
            direction: crate::stats::Direction::Analysis,
        });
        // Threaded into `morph::ana_compound` rather than post-filtering: root-allomorph resolution must join `ana_compound_subrule`'s own per-subrule dedup scope.
        let node_parent = w.trace.unwrap_or(self.parent);
        // Book this rule's self time to its own report row.
        let _obj_time = self.stats.map(|stats| {
            stats.time_enter(
                crate::stats::ObjectKind::MorphRule,
                self.stratum_id,
                id.0,
                crate::stats::ALLOMORPH_NONE,
                crate::stats::Direction::Analysis,
            )
        });
        let mut outs = match (rule, self.non_head_root_filter) {
            (MorphRuleDef::Compounding(_), Some(filter)) => match self.cache {
                Some(cache) => morph::analyze_cached_with_root_filter_traced(
                    self.g,
                    id,
                    w,
                    rule,
                    cache,
                    filter,
                    mstats,
                    self.trace,
                    node_parent,
                ),
                None => morph::analyze_with_root_filter_stats(self.g, w, rule, filter, mstats),
            },
            _ => match self.cache {
                Some(cache) => morph::analyze_cached_traced(
                    self.g,
                    id,
                    w,
                    rule,
                    cache,
                    mstats,
                    self.trace,
                    node_parent,
                ),
                None => morph::analyze_stats(self.g, w, rule, mstats),
            },
        };
        drop(_obj_time);
        for o in &mut outs {
            // Analysis always records the known rule; the null case only arises from generation seeding a bare non-head directly.
            o.mrule_apps.push(Some(id));
            o.mrule_app_index = o.mrule_apps.len() as i32 - 1;
            // Paired with the `mrule_apps.push` above; only `Self::state_key` reads it.
            o.record_unapplication(id);
            // `morph::ana_compound` already pushed the split-off non-head; this pairs that push with the index bump.
            o.non_head_app_index = o.non_heads.len() as i32 - 1;
            if self.policy.enforce {
                o.flags.final_template_state = analysis_state_after_mrule(
                    role,
                    matches!(rule, MorphRuleDef::Realizational(_)),
                );
            }
        }
        outs
    }

    /// The mrule cascade over the reversed rule list: permutation for `Linear`, combination for `Unordered`, deduped by full word key.
    fn run_mrule_cascade(&self, input: &Word) -> Vec<Word> {
        let apply_rule = |i: usize, w: &Word| {
            self.apply_one_mrule(self.reversed_mrules[i], w, RuleInvocationRole::Ordinary)
        };
        let key = |w: &Word| w.dedup_key();
        let casc = Cascade::new(true, usize::MAX);
        let n = self.reversed_mrules.len();
        let out = match self.order {
            MorphRuleOrder::Linear => casc.permutation(n, input.clone(), &apply_rule, &key),
            MorphRuleOrder::Unordered => casc.combination(n, input.clone(), &apply_rule, &key),
        };
        if frontier_profile::enabled() {
            frontier_profile::record_raw_cascade(
                out.words.len(),
                crate::word::estimate_words_bytes(&out.words),
            );
        }
        out.words
    }

    /// Run the mrule cascade, then per stratum order interleave templates, streaming each output into `sink` (see `WordSink`) rather than returning an owned subtree.
    fn apply_mrules(&self, input: &Word, sink: &mut WordSink<'_>) {
        if self.over_budget() || sink.done() {
            return;
        }
        let _depth = frontier_profile::enabled().then(frontier_profile::DepthGuard::enter);
        // `.Distinct(...)` in C# is redundant here — the cascade already deduped by key.
        for w in self.run_mrule_cascade(input) {
            if sink.done() {
                break;
            }
            match self.order {
                MorphRuleOrder::Linear => sink.push(w),
                MorphRuleOrder::Unordered => {
                    self.apply_templates(&w, &mut *sink);
                    sink.push(w);
                }
            }
        }
    }

    /// Run the template battery, then per stratum order interleave mrules and stream the template output when it changed the word, into `sink` (see `WordSink`) rather than returning an owned subtree.
    fn apply_templates(&self, input: &Word, sink: &mut WordSink<'_>) {
        if self.over_budget() || sink.done() {
            return;
        }
        let _depth = frontier_profile::enabled().then(frontier_profile::DepthGuard::enter);
        // Reject an all-final battery before any template work.
        if self.policy.enforce
            && input.flags.final_template_state == crate::word::FinalTemplateState::NonTemplate
            && self.policy.all_templates_final
        {
            if let Some(stats) = self.stats {
                stats.record_template_battery_skipped(
                    self.stratum_id,
                    crate::stats::Direction::Analysis,
                );
            }
            return;
        }
        let in_key = input.dedup_key();
        for t in self.run_template_batch(input) {
            if sink.done() {
                break;
            }
            let changed = t.dedup_key() != in_key;
            match self.order {
                MorphRuleOrder::Linear => {
                    self.apply_mrules(&t, &mut *sink);
                    if changed {
                        sink.push(t);
                    }
                }
                MorphRuleOrder::Unordered => {
                    if changed {
                        self.apply_mrules(&t, &mut *sink);
                        sink.push(t);
                    }
                }
            }
        }
    }

    /// The template battery: non-disjunctive union of every affix template's output, deduped by key.
    fn run_template_batch(&self, input: &Word) -> Vec<Word> {
        let mut seen: HashMap<WordKey, usize> = HashMap::default();
        let mut out: Vec<Word> = Vec::new();
        for &tid in &self.stratum.templates {
            for w in self.analyze_template(tid, input) {
                let key = w.dedup_key();
                match seen.get(&key) {
                    // WordKey ignores syntactic features, so a collision retains their existing generalization.
                    Some(&idx) => {
                        generalize_syn_fs(&mut out[idx], &w, &|f| self.g.syn_features.mask(f))
                    }
                    None => {
                        seen.insert(key, out.len());
                        out.push(w);
                    }
                }
            }
        }
        if frontier_profile::enabled() {
            frontier_profile::record_template(out.len(), crate::word::estimate_words_bytes(&out));
        }
        out
    }

    /// Required syntactic features gate admission; only slot analysis changes output features.
    fn analyze_template(&self, tid: TemplateId, input: &Word) -> Vec<Word> {
        if !self.rule_admitted(RuleRef::Template(tid)) {
            return Vec::new();
        }
        let tmpl = &self.g.templates[tid.0 as usize];
        // In mixed strata, reject final templates before required-FS admission and slot walking.
        if self.policy.enforce
            && input.flags.final_template_state == crate::word::FinalTemplateState::NonTemplate
            && tmpl.is_final
        {
            if let Some(stats) = self.stats {
                stats.record_final_template_skipped(
                    self.stratum_id,
                    crate::stats::Direction::Analysis,
                );
            }
            return Vec::new();
        }
        if let Some(stats) = self.stats {
            stats.record_template_entry(self.stratum_id, crate::stats::Direction::Analysis);
        }
        let req = self.g.fs_interner.get(tmpl.required_syn_fs);
        if !is_unifiable(&input.syn_fs, req) {
            return Vec::new();
        }
        // Fires once per `Apply`, right after the required-syn-FS gate and before the slot walk.
        let node_parent = input.trace.unwrap_or(self.parent);
        if self.trace.is_tracing() {
            self.trace.begin_unapply_template(node_parent, tid, input);
        }
        let mut out: HashMap<WordKey, Word> = HashMap::default();
        // Descend from the last slot.
        self.template_unapply_slots(tid, tmpl, input, tmpl.slots.len() as isize - 1, &mut out);
        out.into_values().collect()
    }

    /// Fires `EndUnapplyTemplate` against `w`'s own resolved cursor, if tracing is on at all.
    fn end_unapply_template(&self, tid: TemplateId, w: &Word, unapplied: bool) {
        if self.trace.is_tracing() {
            let node_parent = w.trace.unwrap_or(self.parent);
            self.trace
                .end_unapply_template(node_parent, tid, w, unapplied);
        }
    }

    /// From `index` down, unapply each slot's rule batch and recurse into earlier slots; a non-optional slot forces a return, since its material had to be consumed here.
    fn template_unapply_slots(
        &self,
        tid: TemplateId,
        tmpl: &pg_grammar::model::AffixTemplateDef,
        in_word: &Word,
        index: isize,
        out: &mut HashMap<WordKey, Word>,
    ) {
        if self.over_budget() {
            return;
        }
        let mut i = index;
        while i >= 0 {
            let slot = &tmpl.slots[i as usize];
            for ow in self.apply_slot_batch(slot, in_word) {
                self.template_unapply_slots(tid, tmpl, &ow, i - 1, out);
            }
            if !slot_optional(slot) {
                // This level's `in_word` could not get past a non-optional slot.
                self.end_unapply_template(tid, in_word, false);
                return;
            }
            i -= 1;
        }
        // Fell through every slot: all optional, or consumed.
        self.end_unapply_template(tid, in_word, true);
        out.entry(in_word.dedup_key())
            .or_insert_with(|| in_word.clone());
    }

    /// One slot's non-disjunctive `RuleBatch`: the deduped union of its alternative rules' outputs.
    fn apply_slot_batch(&self, slot: &SlotDef, in_word: &Word) -> Vec<Word> {
        let mut seen: HashMap<WordKey, usize> = HashMap::default();
        let mut out: Vec<Word> = Vec::new();
        for &rid in &slot.rules {
            for w in self.apply_one_mrule(rid, in_word, RuleInvocationRole::TemplateSlot) {
                let key = w.dedup_key();
                match seen.get(&key) {
                    // Same FS-blind collapse as `run_template_batch_raw`, one slot rule down: widen rather than drop.
                    Some(&idx) => {
                        generalize_syn_fs(&mut out[idx], &w, &|f| self.g.syn_features.mask(f))
                    }
                    None => {
                        seen.insert(key, out.len());
                        out.push(w);
                    }
                }
            }
        }
        out
    }

    /// Port of `AnalysisStratumRule.Apply`.
    // map_entry: saving one hash here would restructure the dedup block this port mirrors statement for statement.
    #[allow(clippy::map_entry)]
    fn analyze(&self, mut input: Word) -> StratumAnalysis {
        // A stratum starts with a clean interleaving state.
        input.flags.final_template_state = crate::word::FinalTemplateState::None;
        // Fires against the word exactly as received, before the clone below; the resolved parent is reused for the matching end-event calls.
        let node_parent = input.trace.unwrap_or(self.parent);
        if self.trace.is_tracing() {
            self.trace
                .begin_unapply_stratum(node_parent, self.stratum_id, &input);
        }

        // Records the incoming word as `Source` so `expand_alternatives` can later walk the per-stratum spine.
        let source = Rc::new(input.clone());
        input.stratum = self.stratum_id;
        input.source = Some(source.clone());
        input.alternatives.clear();

        // A linear cascade over the prules reversed, applied in place with no per-prule cursor advance (a deliberately coarser trace depth).
        for &pid in self.stratum.prules.iter().rev() {
            // The one (un)application site here needing its own budget check: deadline only, never the step cap.
            if self.budget.synthesis_over_budget() {
                break;
            }
            let prule_stats = self.stats.map(|stats| PRuleStatsCtx {
                stats,
                stratum: self.stratum_id,
                id: pid,
                direction: crate::stats::Direction::Analysis,
            });
            let result = match &self.g.prules[pid.0 as usize] {
                pg_grammar::model::PhonRuleDef::Rewrite(r) => match self.cache {
                    Some(cache) => rewrite::analyze_cached_traced(
                        self.g,
                        pid,
                        r,
                        &input.shape,
                        cache,
                        prule_stats,
                        self.trace,
                        self.parent,
                    ),
                    None => rewrite::analyze_traced(
                        self.g,
                        pid,
                        r,
                        &input.shape,
                        prule_stats,
                        self.trace,
                        self.parent,
                    ),
                },
                pg_grammar::model::PhonRuleDef::Metathesis(r) => match self.cache {
                    Some(cache) => metathesis::analyze_cached_traced(
                        pid,
                        r,
                        &input.shape,
                        cache.prule_metathesis(pid),
                        self.trace,
                        self.parent,
                    ),
                    None => metathesis::analyze_traced(
                        self.g,
                        pid,
                        r,
                        &input.shape,
                        self.trace,
                        self.parent,
                    ),
                },
            };
            if let Some(s) = result.into_iter().next() {
                input.shape = s;
            }
        }

        // The first end-event, for `input` itself, placed before any nested template/mrule event since C#'s lazy evaluation reaches it first there and this port is eager.
        if self.trace.is_tracing() {
            self.trace
                .end_unapply_stratum(node_parent, self.stratum_id, &input);
        }

        // WordKey -> its index in `words`, so an identity-fallback fold (below) can find its canonical.
        let mut output_keys: HashMap<WordKey, usize> = HashMap::default();
        // AnalysisStateKey -> canonical word index; the seed's key is deliberately not registered (AnalysisStratumRule.cs never seeds `wordCache` from `input`).
        let mut key_word: HashMap<AnalysisStateKey, usize> = HashMap::default();
        let mut words: Vec<Word> = Vec::new();
        output_keys.insert(input.dedup_key(), 0);
        // This call's own running byte total, the `record_live_bytes` companion to `words.len()`.
        let live_bytes = Cell::new(estimate_word_bytes(&input) as u64);
        // Per-canonical dedup state for `dedup_alternative`, below.
        let mut alt_keys: HashMap<usize, FxHashSet<AltKey>> = HashMap::default();
        words.push(input.clone());

        // A live count-down on candidates beyond the seed, unset by default -- a plain stop past the cap, no exception.
        let remaining =
            (self.cfg.max_unapplications > 0).then(|| Cell::new(self.cfg.max_unapplications));
        // The stratum's own dedup fold, now also the sink `apply_templates`/`apply_mrules` stream into directly -- the one durable accumulator this stratum call ever holds (`docs/research/live-frontier-memory-bound.md`).
        let mut fold = |mut w: Word| {
            w.source = Some(source.clone());
            w.flags.final_template_state = crate::word::FinalTemplateState::None;
            let dedup_key = w.dedup_key();
            let state_key = self.cfg.merge_equivalent.then(|| self.state_key(&w));
            if let Some(state_key) = &state_key {
                if let Some(&idx) = key_word.get(state_key) {
                    if let Some(w) =
                        dedup_alternative(&mut words, &mut alt_keys, idx, &dedup_key, w, &|f| {
                            self.g.syn_features.mask(f)
                        })
                    {
                        live_bytes.set(live_bytes.get() + estimate_word_bytes(&w) as u64);
                        words[idx].alternatives.push(Rc::new(w));
                    }
                    return;
                }
                // `WordKey` ignores syntactic FS, so a distinct state key can still collide here; fold rather than let output dedup drop it silently.
                if let Some(&idx) = output_keys.get(&dedup_key) {
                    if let Some(w) =
                        dedup_alternative(&mut words, &mut alt_keys, idx, &dedup_key, w, &|f| {
                            self.g.syn_features.mask(f)
                        })
                    {
                        live_bytes.set(live_bytes.get() + estimate_word_bytes(&w) as u64);
                        words[idx].alternatives.push(Rc::new(w));
                    }
                    return;
                }
            }
            // The second end-event, once per surviving candidate; must NOT be gated on the `output_keys` insert below, since a key-duplicate still fires it.
            if self.trace.is_tracing() {
                let w_parent = w.trace.unwrap_or(node_parent);
                self.trace
                    .end_unapply_stratum(w_parent, self.stratum_id, &w);
            }
            if !output_keys.contains_key(&dedup_key) {
                // Registered only once the word is certain to reach the output, so a rejected word never becomes a canonical.
                if let Some(state_key) = state_key {
                    key_word.insert(state_key, words.len());
                }
                output_keys.insert(dedup_key, words.len());
                live_bytes.set(live_bytes.get() + estimate_word_bytes(&w) as u64);
                words.push(w);
                if let Some(r) = &remaining {
                    r.set(r.get().saturating_sub(1));
                }
            }
            // The durable accumulator's size plus the live recursion depth: everything this stratum call can hold at once, now that no level returns an owned subtree (`docs/research/live-frontier-memory-bound.md`).
            if frontier_profile::enabled() {
                frontier_profile::record_live_words(
                    words.len() as u64 + frontier_profile::current_depth(),
                );
                frontier_profile::record_live_bytes(live_bytes.get());
            }
        };
        let mut sink = WordSink {
            push: &mut fold,
            remaining: remaining.as_ref(),
        };
        self.apply_templates(&input, &mut sink);
        self.apply_mrules(&input, &mut sink);

        // `HC_WORD_STATS=1`: attribute this stratum pass's `words` to their own fields (docs/research/word-memory-trace.md).
        crate::word_stats::record_live_words(&words);

        StratumAnalysis {
            words,
            capped: self.budget.capped(),
        }
    }
}

/// Widen the canonical's syntactic FS over the folded alternative's, since only the canonical is un-applied further (C# `AnalysisStratumRule.GeneralizeSyntacticFeatureStruct`).
fn generalize_syn_fs(
    canonical: &mut Word,
    alternative: &Word,
    mask_of: &impl Fn(pg_featstruct::FeatId) -> u64,
) {
    if canonical.syn_fs != alternative.syn_fs {
        canonical.syn_fs = union(&canonical.syn_fs, &alternative.syn_fs, mask_of);
    }
}

/// `WordKey` plus the two fields `Word::expand_alternatives`/`is_word_valid_traced` read off an
/// alternative that `WordKey` omits (`docs/research/alt-yield.md` §pruning).
type AltKey = (
    WordKey,
    pg_featstruct::FeatureStruct,
    Vec<pg_featstruct::FeatId>,
);

/// An `AltKey` match against the canonical's frozen state or an earlier-kept alternative replays
/// identically, so `w` is pure duplication; see docs/research/alt-yield.md.
fn dedup_alternative(
    words: &mut [Word],
    alt_keys: &mut HashMap<usize, FxHashSet<AltKey>>,
    idx: usize,
    dedup_key: &WordKey,
    w: Word,
    mask_of: &impl Fn(pg_featstruct::FeatId) -> u64,
) -> Option<Word> {
    let seen = alt_keys.entry(idx).or_insert_with(|| {
        let mut s = FxHashSet::default();
        s.insert((
            words[idx].dedup_key(),
            words[idx].syn_fs.clone(),
            words[idx].obligatory.clone(),
        ));
        s
    });
    let novel = seen.insert((dedup_key.clone(), w.syn_fs.clone(), w.obligatory.clone()));
    generalize_syn_fs(&mut words[idx], &w, mask_of);
    novel.then_some(w)
}

// Unit-tested here directly: the differing-FS case is unreachable through the public analysis API (an equal state key already forces equal syn_fs).
#[cfg(test)]
mod generalize_syn_fs_tests {
    use super::*;
    use pg_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
    use pg_shape::ShapeBuilder;

    const FA: FeatId = FeatId(0);

    fn fs_with(bits: u64) -> FeatureStruct {
        let mut b = FeatureStructBuilder::new();
        b.add(FA, FeatureValue::Symbolic(SymbolBits(bits)));
        b.build()
    }

    fn bare_word() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
    }

    fn mask3(_: FeatId) -> u64 {
        0b111
    }

    #[test]
    fn widens_the_canonical_to_the_union_when_the_alternative_differs() {
        let mut canonical = bare_word();
        canonical.syn_fs = fs_with(0b001);
        let mut alternative = bare_word();
        alternative.syn_fs = fs_with(0b010);

        generalize_syn_fs(&mut canonical, &alternative, &mask3);

        assert_eq!(
            canonical.syn_fs,
            union(&fs_with(0b001), &fs_with(0b010), &mask3)
        );
        assert_eq!(canonical.syn_fs, fs_with(0b011));
    }

    #[test]
    fn leaves_the_canonical_unchanged_when_the_alternative_matches() {
        let mut canonical = bare_word();
        canonical.syn_fs = fs_with(0b011);
        let alternative_same = bare_word_with_syn_fs(fs_with(0b011));

        generalize_syn_fs(&mut canonical, &alternative_same, &mask3);

        assert_eq!(
            canonical.syn_fs,
            fs_with(0b011),
            "an equal syn_fs (the only reachable case on the key-hit fold, since an equal \
             AnalysisStateKey already forces equal syn_fs) must be a no-op"
        );
    }

    fn bare_word_with_syn_fs(fs: FeatureStruct) -> Word {
        let mut w = bare_word();
        w.syn_fs = fs;
        w
    }
}

#[cfg(test)]
mod dedup_alternative_tests {
    use super::*;
    use pg_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
    use pg_shape::ShapeBuilder;

    const FA: FeatId = FeatId(0);

    fn mask_none(_: FeatId) -> u64 {
        0
    }

    fn fs_with(bits: u64) -> FeatureStruct {
        let mut b = FeatureStructBuilder::new();
        b.add(FA, FeatureValue::Symbolic(SymbolBits(bits)));
        b.build()
    }

    fn bare_word() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
    }

    #[test]
    fn a_byte_identical_alternative_is_pruned() {
        let mut words = vec![bare_word()];
        let mut alt_keys: HashMap<usize, FxHashSet<AltKey>> = HashMap::default();
        let dup = bare_word();
        let key = dup.dedup_key();

        let kept = dedup_alternative(&mut words, &mut alt_keys, 0, &key, dup, &mask_none);

        assert!(
            kept.is_none(),
            "a duplicate of the canonical must be dropped"
        );
        assert!(words[0].alternatives.is_empty());
    }

    #[test]
    fn same_word_key_but_different_syn_fs_is_kept() {
        // `WordKey` excludes `syn_fs`, so a genuinely different value must still survive.
        let mut words = vec![bare_word()];
        let mut alt_keys: HashMap<usize, FxHashSet<AltKey>> = HashMap::default();
        let mut alt = bare_word();
        alt.syn_fs = fs_with(0b01);
        let key = alt.dedup_key();

        let kept = dedup_alternative(&mut words, &mut alt_keys, 0, &key, alt, &mask_none);

        assert!(kept.is_some(), "a differing syn_fs must not be pruned");
    }

    #[test]
    fn a_second_identical_alternative_is_pruned_after_the_first_is_kept() {
        let mut words = vec![bare_word()];
        let mut alt_keys: HashMap<usize, FxHashSet<AltKey>> = HashMap::default();
        let mut alt = bare_word();
        alt.obligatory.push(FeatId(1));
        let key = alt.dedup_key();

        let first = dedup_alternative(&mut words, &mut alt_keys, 0, &key, alt.clone(), &mask_none);
        assert!(first.is_some());
        if let Some(w) = first {
            words[0].alternatives.push(Rc::new(w));
        }
        let second = dedup_alternative(&mut words, &mut alt_keys, 0, &key, alt, &mask_none);

        assert!(
            second.is_none(),
            "an identical second alternative must be pruned"
        );
    }
}

/// A slot with no rules is always optional; otherwise its declared flag.
fn slot_optional(slot: &SlotDef) -> bool {
    slot.rules.is_empty() || slot.optional
}

// Synthesis affix-template rule (self-contained; the driver below is gated on lexicon state).

/// Mirrors C#'s `SynthesisAffixTemplateRule.Apply` + `ApplySlots`: slots applied bottom-up, a
/// non-optional slot that produced nothing terminating the path. The **ungated** walk — every slot
/// rule applies unconditionally, unlike production's `guided_template_apply`, which confirms the
/// analysis first. Independent of lexicon state, so usable standalone; `cap` bounds total attempts.
pub fn synthesize_template(g: &Grammar, tid: TemplateId, input: &Word, cap: usize) -> Vec<Word> {
    let tmpl = &g.templates[tid.0 as usize];
    let steps = Cell::new(0usize);
    let mut out: HashMap<WordKey, Word> = HashMap::default();
    let apply = |g: &Grammar, rid: MRuleId, w: &Word| {
        morph::synthesize_with_policy_and_role(
            g,
            w,
            &g.mrules[rid.0 as usize],
            FinalTemplateSynthesisPolicy::default(),
            RuleInvocationRole::TemplateSlot,
        )
    };
    // Builds its own budget with none armed, so this entry point stays cap-only.
    let budget = StepBudget::new(cap);
    synth_slots_generic(
        g,
        tmpl,
        input,
        0,
        &mut out,
        cap,
        &steps,
        &apply,
        &crate::trace::NoopSink,
        tid,
        TraceHandle::DUMMY,
        &budget,
    );
    out.into_values().collect()
}

/// `synthesize_template`'s structure, but each rule goes through `guided_synth` and the caller's step budget is shared.
#[allow(clippy::too_many_arguments)]
fn guided_template_apply(
    g: &Grammar,
    stratum: StratumId,
    tid: TemplateId,
    input: &Word,
    cap: usize,
    steps: &Cell<usize>,
    cache: &RuleCache,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
    budget: &StepBudget,
    policy: FinalTemplateSynthesisPolicy,
) -> Vec<Word> {
    let tmpl = &g.templates[tid.0 as usize];
    let mut out: HashMap<WordKey, Word> = HashMap::default();
    let apply = |g: &Grammar, rid: MRuleId, w: &Word| {
        guided_synth(
            g,
            stratum,
            rid,
            w,
            cache,
            stats,
            trace,
            parent,
            policy,
            RuleInvocationRole::TemplateSlot,
        )
    };
    if trace.is_tracing() {
        let node_parent = input.trace.unwrap_or(parent);
        trace.begin_apply_template(node_parent, tid, input);
    }
    synth_slots_generic(
        g, tmpl, input, 0, &mut out, cap, steps, &apply, trace, tid, parent, budget,
    );
    out.into_values().collect()
}

/// Fires `EndApplyTemplate` against `w`'s own resolved cursor, if tracing is on at all.
fn end_apply_template(
    trace: &dyn TraceSink,
    tid: TemplateId,
    w: &Word,
    parent: TraceHandle,
    applied: bool,
) {
    if trace.is_tracing() {
        trace.end_apply_template(w.trace.unwrap_or(parent), tid, w, applied);
    }
}

/// Bottom-up, parameterized by `apply` so one walk serves the ungated and guided callers; `budget` is consulted for its deadline only, never step-count.
#[allow(clippy::too_many_arguments)]
fn synth_slots_generic<F>(
    g: &Grammar,
    tmpl: &pg_grammar::model::AffixTemplateDef,
    input: &Word,
    index: usize,
    out: &mut HashMap<WordKey, Word>,
    cap: usize,
    steps: &Cell<usize>,
    apply: &F,
    trace: &dyn TraceSink,
    tid: TemplateId,
    parent: TraceHandle,
    budget: &StepBudget,
) where
    F: Fn(&Grammar, MRuleId, &Word) -> Vec<Word>,
{
    if steps.get() >= cap {
        return;
    }
    if budget.synthesis_over_budget() {
        return;
    }
    let mut i = index;
    while i < tmpl.slots.len() {
        let slot = &tmpl.slots[i];
        // The slot's non-disjunctive `RuleBatch`, in the synthesis direction.
        let mut seen: HashMap<WordKey, ()> = HashMap::default();
        for &rid in &slot.rules {
            if steps.get() >= cap {
                return;
            }
            if budget.synthesis_over_budget() {
                return;
            }
            steps.set(steps.get() + 1);
            for w in apply(g, rid, input) {
                if seen.insert(w.dedup_key(), ()).is_none() {
                    synth_slots_generic(
                        g,
                        tmpl,
                        &w,
                        i + 1,
                        out,
                        cap,
                        steps,
                        apply,
                        trace,
                        tid,
                        parent,
                        budget,
                    );
                }
            }
        }
        if !slot_optional(slot) {
            end_apply_template(trace, tid, input, parent, false);
            return;
        }
        i += 1;
    }
    end_apply_template(trace, tid, input, parent, true);
    out.entry(input.dedup_key())
        .or_insert_with(|| input.clone());
}

// Synthesis stratum rule.

/// Guided single-rule synthesis confirms the analysis: a rule re-applies only as the word's current expected unapplication; `MaxApplicationCount` is not enforced (needs a per-word count this port lacks).
#[allow(clippy::too_many_arguments)]
fn guided_synth(
    g: &Grammar,
    stratum: StratumId,
    id: MRuleId,
    w: &Word,
    cache: &RuleCache,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
    policy: FinalTemplateSynthesisPolicy,
    role: RuleInvocationRole,
) -> Vec<Word> {
    if w.mrule_app_index < 0 {
        return Vec::new();
    }
    let idx = w.mrule_app_index as usize;
    if idx >= w.mrule_apps.len() {
        return Vec::new();
    }
    let is_compound = matches!(&g.mrules[id.0 as usize], MorphRuleDef::Compounding(_));
    let applicable = match w.mrule_apps[idx] {
        Some(cur) => cur == id,
        None => is_compound,
    };
    if !applicable {
        return Vec::new();
    }
    // The synthesis-direction counterpart of `Analyzer::apply_one_mrule`'s ctx: this invocation is the confirm-pass reapplication of `id`.
    let mstats = stats.map(|stats| crate::stats::MRuleStatsCtx {
        stats,
        stratum,
        id,
        direction: crate::stats::Direction::Synthesis,
    });
    // Threaded INTO `synthesize_cached_traced` rather than applied after: it fires applied/not-applied events at its own internal gates and sets each output's `.trace`.
    let node_parent = w.trace.unwrap_or(parent);
    let mut outs = morph::synthesize_cached_traced_with_policy_and_role(
        g,
        id,
        w,
        &g.mrules[id.0 as usize],
        cache,
        mstats,
        trace,
        node_parent,
        policy,
        role,
    );
    for o in &mut outs {
        o.mrule_app_index -= 1;
        if is_compound {
            o.non_head_app_index -= 1;
        }
    }
    outs
}

/// The stratum whose `mrules` list or affix-template slots contain `id`; a linear scan is fine since only a few final candidates use it.
fn owning_stratum(g: &Grammar, id: MRuleId) -> Option<StratumId> {
    for (si, sd) in g.strata.iter().enumerate() {
        if sd.mrules.contains(&id) {
            return Some(StratumId(si as u8));
        }
        for &tid in &sd.templates {
            for slot in &g.templates[tid.0 as usize].slots {
                if slot.rules.contains(&id) {
                    return Some(StratumId(si as u8));
                }
            }
        }
    }
    None
}

/// The word still has a pending unapplied rule belonging to `stratum`, so it left without finishing; a null pending slot's stratum comes from the pending non-head instead.
fn has_remaining_rules_from_stratum(g: &Grammar, w: &Word, stratum: StratumId) -> bool {
    if w.mrule_app_index < 0 {
        return false;
    }
    let idx = w.mrule_app_index as usize;
    match w.mrule_apps.get(idx) {
        Some(&Some(cur)) => owning_stratum(g, cur) == Some(stratum),
        Some(&None) => w.current_non_head().map(|nh| nh.stratum) == Some(stratum),
        None => false,
    }
}

/// Mirrors C#'s `SynthesisStratumRule.Apply`. Gates, in order: pass the word through unchanged if
/// its root's stratum is shallower than this one (depth is the strata index, and lexical lookup has
/// already set `input.stratum` from the root entry); keep only words whose last applied rule was
/// final; drop words that still owe this stratum a rule; then apply trailing in-place prules and
/// clear the final flag.
///
/// `cache` is required, unlike the analysis entry points above: this is the hot synthesis path the
/// compile-once cache exists for. See `crate::cache`'s module doc.
pub fn synthesize_stratum(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cap: usize,
    cache: &RuleCache,
) -> Vec<Word> {
    // No production call site: `pg-parse` threads its own budget through `synthesize_stratum_traced` directly; this exists for test callers.
    let budget = StepBudget::new(cap);
    synthesize_stratum_traced(
        g,
        stratum,
        input,
        cap,
        cache,
        &budget,
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
}

/// `synthesize_stratum`'s traced sibling; the caller passes a real handle once, and every deeper call resolves the cursor off the `Word` itself.
#[allow(clippy::too_many_arguments)]
pub fn synthesize_stratum_traced(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cap: usize,
    cache: &RuleCache,
    budget: &StepBudget,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    synthesize_stratum_traced_with_policy(
        g,
        stratum,
        input,
        cap,
        cache,
        budget,
        FinalTemplateSynthesisPolicy::default(),
        stats,
        trace,
        parent,
    )
}

/// Policy-aware sibling of `synthesize_stratum_traced`; the compatibility wrapper preserves the
/// historical partial-word rescue behavior.
#[allow(clippy::too_many_arguments)]
pub fn synthesize_stratum_traced_with_policy(
    g: &Grammar,
    stratum: StratumId,
    mut input: Word,
    cap: usize,
    cache: &RuleCache,
    budget: &StepBudget,
    policy: FinalTemplateSynthesisPolicy,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    input.flags.final_template_state = crate::word::FinalTemplateState::None;
    // Entry gate. C# has no trace call here either, so this stays untraced to match.
    if (input.stratum.0 as usize) > (stratum.0 as usize) {
        return vec![input];
    }

    let sd = &g.strata[stratum.0 as usize];
    let steps = Cell::new(0usize);

    let node_parent = input.trace.unwrap_or(parent);
    if trace.is_tracing() {
        trace.begin_apply_stratum(node_parent, stratum, &input);
    }

    let mut candidates = synth_apply_mrules(
        g,
        stratum,
        sd,
        &input,
        cap,
        &steps,
        cache,
        stats,
        trace,
        node_parent,
        budget,
        policy,
    );
    candidates.extend(synth_apply_templates(
        g,
        stratum,
        sd,
        &input,
        cap,
        &steps,
        cache,
        stats,
        trace,
        node_parent,
        budget,
        policy,
    ));

    let mut out: HashMap<WordKey, Word> = HashMap::default();
    for w in candidates {
        let w_parent = w.trace.unwrap_or(node_parent);
        // Only words whose last applied rule was final proceed.
        if w.flags.is_last_applied_rule_final != Some(true) {
            if trace.is_tracing() {
                trace.non_final_template_applied_last(w_parent, stratum, &w);
            }
            continue;
        }
        // Drop partial parses that still owe this stratum a rule.
        if has_remaining_rules_from_stratum(g, &w, stratum) {
            if trace.is_tracing() {
                trace.failed(w_parent, &w, FailureReason::PartialParse);
            }
            continue;
        }
        // C# `SynthesisStratumRule.Apply` never reassigns `Word.Stratum`, unlike the analysis direction.
        let mut nw = w.clone();
        // Uses `synthesize_with_mpr_cached`, not bare `synthesize`, so the POS/MPR gate sees real state; `break` (not `return`) keeps `nw.shape` as far as the fold got.
        for &pid in &sd.prules {
            if budget.synthesis_over_budget() {
                break;
            }
            let result = match &g.prules[pid.0 as usize] {
                pg_grammar::model::PhonRuleDef::Rewrite(r) => {
                    rewrite::synthesize_with_mpr_cached_traced(
                        g, pid, r, &nw, cache, trace, w_parent,
                    )
                }
                pg_grammar::model::PhonRuleDef::Metathesis(r) => {
                    metathesis::synthesize_cached_traced(
                        g,
                        pid,
                        r,
                        &nw,
                        cache.prule_metathesis(pid),
                        trace,
                        w_parent,
                    )
                }
            };
            if let Some(s) = result.into_iter().next() {
                nw.shape = s;
            }
        }
        nw.flags.is_last_applied_rule_final = None;
        if trace.is_tracing() {
            trace.end_apply_stratum(w_parent, stratum, &nw);
        }
        out.entry(nw.dedup_key()).or_insert(nw);
    }
    if trace.is_tracing() && out.is_empty() {
        trace.end_apply_stratum(node_parent, stratum, &input);
    }
    out.into_values().collect()
}

#[allow(clippy::too_many_arguments)]
fn synth_apply_mrules(
    g: &Grammar,
    stratum: StratumId,
    sd: &pg_grammar::model::StratumDef,
    input: &Word,
    cap: usize,
    steps: &Cell<usize>,
    cache: &RuleCache,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
    budget: &StepBudget,
    policy: FinalTemplateSynthesisPolicy,
) -> Vec<Word> {
    if steps.get() >= cap {
        return Vec::new();
    }
    if budget.synthesis_over_budget() {
        return Vec::new();
    }
    let key = |w: &Word| w.dedup_key();
    // The guided cascade terminates once `guided_synth`'s strictly-decrementing stack is exhausted; the wall-clock check must live inside the closure since neither cap is time-aware.
    let apply_rule = |i: usize, w: &Word| -> Vec<Word> {
        if steps.get() >= cap {
            return Vec::new();
        }
        if budget.synthesis_over_budget() {
            return Vec::new();
        }
        steps.set(steps.get() + 1);
        guided_synth(
            g,
            stratum,
            sd.mrules[i],
            w,
            cache,
            stats,
            trace,
            parent,
            policy,
            RuleInvocationRole::Ordinary,
        )
    };
    let casc = Cascade::new(true, usize::MAX);
    let n = sd.mrules.len();
    // Synthesis mrules are in declaration order — no reverse, unlike analysis.
    let cascade_out = match sd.mrule_order {
        MorphRuleOrder::Linear => casc.linear(n, input.clone(), &apply_rule, &key),
        MorphRuleOrder::Unordered => casc.combination(n, input.clone(), &apply_rule, &key),
    };
    let mut result = Vec::new();
    for w in cascade_out.words {
        // A final word yields directly; otherwise run templates on it.
        if w.flags.is_last_applied_rule_final == Some(true) {
            result.push(w);
        } else {
            result.extend(synth_apply_templates(
                g, stratum, sd, &w, cap, steps, cache, stats, trace, parent, budget, policy,
            ));
        }
    }
    result
}

/// Whether `word`'s root *entry* (distinct from `Word::flags.is_partial`) is flagged partial; the guessed-root arm is load-bearing since indexing `allomorph_owners` with the guessed sentinel panics.
fn root_is_partial(g: &Grammar, word: &Word) -> bool {
    match word.root_allomorph {
        Some(allo) if allo == AllomorphId::GUESSED => match word.root_runtime() {
            Some(crate::word::RuntimeRoot::Guessed(gr)) => {
                g.entries[gr.pattern_entry.0 as usize].partial
            }
            Some(crate::word::RuntimeRoot::Supplied(_)) | None => false,
        },
        Some(allo) => match g.allomorph_owners[allo.0 as usize] {
            AllomorphOwner::Root(le, _) => g.entries[le.0 as usize].partial,
            AllomorphOwner::Affix(..) => false,
        },
        None => false,
    }
}

/// Walks the root's family (if any) in document order for the most specific applicable relative, swapping in against `best`'s CURRENT (possibly already-swapped) FS, never the original `input`; a guessed root returns unchanged, faithfully (C#'s fabrication never sets a family either).
fn choose_inflectional_stem(g: &Grammar, input: &Word) -> Word {
    let Some(root_id) = input.root_allomorph else {
        return input.clone();
    };
    if root_id == AllomorphId::GUESSED {
        return input.clone();
    }
    let AllomorphOwner::Root(le, _) = g.allomorph_owners[root_id.0 as usize] else {
        return input.clone();
    };
    let Some(family) = g.entries[le.0 as usize].family else {
        return input.clone();
    };
    if input.real_fs.is_empty() {
        return input.clone();
    }

    let mut best = input.clone();
    for &rel in &g.families[family.0 as usize].entries {
        if rel == le {
            continue;
        }
        let rel_entry = &g.entries[rel.0 as usize];
        if g.morphemes[rel_entry.morpheme.0 as usize].stratum != input.stratum {
            continue;
        }
        let rel_syn = g.fs_interner.get(rel_entry.syn_fs);
        if !is_unifiable(&input.real_fs, rel_syn) || !subsumes(&best.syn_fs, rel_syn) {
            continue;
        }
        let remainder = subtract(rel_syn, &best.syn_fs);
        if !remainder.is_empty() && is_unifiable(&input.real_fs, &remainder) {
            best = crate::morph::seed_from_entry(g, rel, input.real_fs.clone());
        }
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn synth_apply_templates(
    g: &Grammar,
    stratum: StratumId,
    sd: &pg_grammar::model::StratumDef,
    input: &Word,
    cap: usize,
    steps: &Cell<usize>,
    cache: &RuleCache,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
    budget: &StepBudget,
    policy: FinalTemplateSynthesisPolicy,
) -> Vec<Word> {
    if steps.get() >= cap {
        return Vec::new();
    }
    if budget.synthesis_over_budget() {
        return Vec::new();
    }
    // The realizational/syntactic unifiability check returns early BEFORE `choose_inflectional_stem` runs, against the word as handed in.
    if !is_unifiable(&input.real_fs, &input.syn_fs) {
        return Vec::new();
    }
    // Shadowed so every use below reads the POST-swap word.
    let input = choose_inflectional_stem(g, input);
    let input = &input;
    let in_key = input.dedup_key();
    let mut out: HashMap<WordKey, Word> = HashMap::default();
    // The root does not change across templates, so this check is hoisted out of the loop.
    let root_partial = root_is_partial(g, input);
    let mut applicable = false;
    for &tid in &sd.templates {
        let tmpl = &g.templates[tid.0 as usize];
        let req = g.fs_interner.get(tmpl.required_syn_fs);
        if !is_unifiable(&input.syn_fs, req) || root_partial {
            continue;
        }
        applicable = true;
        for w in guided_template_apply(
            g, stratum, tid, input, cap, steps, cache, stats, trace, parent, budget, policy,
        ) {
            let final_flag = w.flags.is_partial || tmpl.is_final;
            let mut w = w;
            w.flags.is_last_applied_rule_final = Some(final_flag);
            out.entry(w.dedup_key()).or_insert(w);
        }
    }
    // No template output: pass the input through UNLESS it is non-partial AND some template was applicable, else a templateless stratum would skip the mrule recursion below entirely.
    if out.is_empty() {
        if input.flags.is_partial || !applicable {
            let mut w = input.clone();
            if w.flags.is_last_applied_rule_final != Some(true) {
                w.flags.is_last_applied_rule_final = Some(true);
            }
            out.insert(w.dedup_key(), w);
        } else if trace.is_tracing() {
            // Unlike the passthrough above, this branch drops the word, recording only a trace event.
            let node_parent = input.trace.unwrap_or(parent);
            trace.applicable_templates_not_applied(node_parent, stratum, input);
        }
    }

    match sd.mrule_order {
        MorphRuleOrder::Linear => {}
        MorphRuleOrder::Unordered => {
            // For each changed template output, including the passthrough above when present, also run the mrules on it.
            let templated: Vec<Word> = out.values().cloned().collect();
            for t in templated {
                if t.dedup_key() != in_key {
                    for m in synth_apply_mrules(
                        g, stratum, sd, &t, cap, steps, cache, stats, trace, parent, budget, policy,
                    ) {
                        out.entry(m.dedup_key()).or_insert(m);
                    }
                }
            }
        }
    }

    out.into_values().collect()
}
