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

use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry;
use std::rc::Rc;
// `std::time::Instant` panics on wasm32-unknown-unknown; `web_time` substitutes only `Instant`, reusing std's `Duration` unchanged.
use web_time::{Duration, Instant};

use pg_featstruct::{add, is_unifiable, subsumes, subtract, unify};
use pg_grammar::model::{
    AllomorphId, AllomorphOwner, Grammar, MRuleId, MorphRuleDef, MorphRuleOrder, SlotDef,
    StratumId, TemplateId,
};
use pg_memo::{AnalysisScope, AnalysisStateKey, MemoEntry};
use pg_shape::Shape;
use rustc_hash::FxHashMap as HashMap;

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

use crate::cache::RuleCache;
use crate::cascade::Cascade;
use crate::stats::{PRuleStatsCtx, StatsCollector};
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::{Word, WordKey};
use crate::{metathesis, morph, rewrite};

/// The per-parse memo carrier this module threads through the analysis cascade. `pg-parse` owns one
/// per `parse_word` call (see `pg_memo::AnalysisScope`) and hands it in via `analyze_stratum_scoped`.
pub type MemoScope = RefCell<AnalysisScope<Word>>;

/// A key→word dedup set preserving first-seen order, analogous to the plain `Cascade`'s internal `Acc`.
struct OrderedDedup {
    seen: HashMap<WordKey, ()>,
    items: Vec<Word>,
}

impl OrderedDedup {
    fn new() -> Self {
        OrderedDedup {
            seen: HashMap::default(),
            items: Vec::new(),
        }
    }

    fn add(&mut self, w: Word) {
        if let Entry::Vacant(e) = self.seen.entry(w.dedup_key()) {
            e.insert(());
            self.items.push(w);
        }
    }

    fn into_items(self) -> Vec<Word> {
        self.items
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

/// Configuration for a stratum (un)application run. C# reads these off the `Morpher`; here they are
/// explicit so callers/tests can pin them.
#[derive(Clone, Copy, Debug)]
pub struct AnalyzerConfig {
    /// Mirrors C# `Morpher.MergeEquivalentAnalyses` (default `true`): collapse this stratum's
    /// candidates that share a `Shape` into one canonical word, folding the repeats into its
    /// `Word::alternatives`. A de-duplication, not a pruning — synthesis re-expands them.
    pub merge_equivalent: bool,
    /// Mirrors C# `Morpher.MaxUnapplications`: stop once the analysis output reaches this many
    /// candidates (`0` = unlimited).
    pub max_unapplications: usize,
    /// Mirrors C# `Morpher.MaxStemCount` (default `2`): refuse to unapply a compounding rule once
    /// `non_heads.len() + 1 >= max_stem_count`. Without it, a compounding subrule whose patterns
    /// are "1+ of any segment" matches every split of every substring at every depth — a
    /// Catalan-scale blowup that either explodes the memo or burns the whole step budget.
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
    analyze_stratum_scoped(g, stratum, input, cfg, None, budget)
}

/// Analyze `input` through `stratum` with the order-invariant memo active: an `AnalysisScope`
/// carries the nogood/positive/template memo across the whole descent, and `pg-parse` reuses one
/// scope for every stratum and input word of a single `parse_word`. Passing `None` (via
/// `analyze_stratum`) reproduces the unmemoized engine byte-for-byte — the fair A/B baseline.
pub fn analyze_stratum_scoped(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    scope: Option<&MemoScope>,
    budget: &StepBudget,
) -> StratumAnalysis {
    analyze_stratum_scoped_filtered(g, stratum, input, cfg, scope, None, None, budget)
}

/// Identical to `analyze_stratum_scoped`, plus the compounding non-head root filter (C#'s
/// `AnalysisCompoundingRule.Apply` root-allomorph-search gate). Production callers pass
/// `Some(cache)` — the cache is built once per `Morpher` and shared across every stratum,
/// candidate, and worker of a parse. `None` recompiles matchers per call, which is what the
/// unfiltered entry points above hand in: hand-built fixtures do not always register their
/// `AffixAllomorphDef.id`s in `Grammar::allomorph_owners`, which the cache requires — see
/// `crate::cache`'s module doc.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_scoped_filtered(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    scope: Option<&MemoScope>,
    non_head_root_filter: Option<NonHeadRootFilter>,
    cache: Option<&RuleCache>,
    budget: &StepBudget,
) -> StratumAnalysis {
    analyze_stratum_scoped_filtered_ruled(
        g,
        stratum,
        input,
        cfg,
        scope,
        non_head_root_filter,
        None,
        cache,
        budget,
    )
}

/// Identical to `analyze_stratum_scoped_filtered`, plus the morphological-rule/template-level
/// selector — see `RuleFilter`. `None` admits every rule, exactly as passing no filter does.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_scoped_filtered_ruled(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    scope: Option<&MemoScope>,
    non_head_root_filter: Option<NonHeadRootFilter>,
    rule_filter: Option<RuleFilter>,
    cache: Option<&RuleCache>,
    budget: &StepBudget,
) -> StratumAnalysis {
    analyze_stratum_scoped_filtered_ruled_traced(
        g,
        stratum,
        input,
        cfg,
        scope,
        non_head_root_filter,
        rule_filter,
        cache,
        budget,
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
}

/// `analyze_stratum_scoped_filtered_ruled`'s traced sibling — identical in every other respect.
/// The intended caller is `pg_parse::Morpher::parse_word_selected_traced`; see `crate::morph`'s
/// analysis-tracing docs and `StratumAnalyzer`'s `trace`/`parent` fields. `stats` is `None` for
/// every existing caller — gated collection is `pg-parse`'s decision, not this layer's.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_scoped_filtered_ruled_traced(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    scope: Option<&MemoScope>,
    non_head_root_filter: Option<NonHeadRootFilter>,
    rule_filter: Option<RuleFilter>,
    cache: Option<&RuleCache>,
    budget: &StepBudget,
    stats: Option<&StatsCollector>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> StratumAnalysis {
    StratumAnalyzer::new(
        g,
        stratum,
        *cfg,
        scope,
        non_head_root_filter,
        rule_filter,
        cache,
        budget,
        stats,
        trace,
        parent,
    )
    .analyze(input)
}

/// The stratum orchestrator. Borrows the caller's `StepBudget` rather than owning its own step counter.
struct StratumAnalyzer<'g, 's, 'f, 'r, 'c, 'b, 't> {
    g: &'g Grammar,
    stratum_id: StratumId,
    stratum: &'g pg_grammar::model::StratumDef,
    order: MorphRuleOrder,
    /// The stratum's morphological rules reversed; the cascade indexes this list, so the closure maps `i -> reversed[i]` to record the correct `MRuleId`.
    reversed_mrules: Vec<MRuleId>,
    cfg: AnalyzerConfig,
    budget: &'b StepBudget,
    /// The order-invariant memo, or `None` for the unmemoized baseline; see `analyze_stratum_scoped`.
    scope: Option<&'s MemoScope>,
    /// The non-head lexicon filter, or `None` for unfiltered. See `NonHeadRootFilter`.
    non_head_root_filter: Option<NonHeadRootFilter<'f>>,
    /// The mrule/template selector, or `None` to admit every rule. See `RuleFilter`.
    rule_filter: Option<RuleFilter<'r>>,
    /// The compile-once FST cache; `None` recompiles per call. See `analyze_stratum_scoped` for why the fallback is still needed.
    cache: Option<&'c RuleCache>,
    /// The gated `--stats` collector, or `None` when stats collection is off; see `crate::stats`.
    stats: Option<&'b StatsCollector>,
    /// The analysis-side trace sink; every entry point but `analyze_stratum_scoped_filtered_ruled_traced` passes `NoopSink`.
    trace: &'t dyn TraceSink,
    /// The ambient trace cursor; call sites resolve `word.trace.unwrap_or(parent)` so successful (un)applications nest under the deepest event on that branch.
    parent: TraceHandle,
}

impl<'g, 's, 'f, 'r, 'c, 'b, 't> StratumAnalyzer<'g, 's, 'f, 'r, 'c, 'b, 't> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        g: &'g Grammar,
        stratum_id: StratumId,
        cfg: AnalyzerConfig,
        scope: Option<&'s MemoScope>,
        non_head_root_filter: Option<NonHeadRootFilter<'f>>,
        rule_filter: Option<RuleFilter<'r>>,
        cache: Option<&'c RuleCache>,
        budget: &'b StepBudget,
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
            scope,
            non_head_root_filter,
            rule_filter,
            cache,
            stats,
            trace,
            parent,
        }
    }

    /// Rule admission — `true` when no filter was supplied, matching C#'s `rule => true` default.
    #[inline]
    fn rule_admitted(&self, r: RuleRef) -> bool {
        self.rule_filter.is_none_or(|f| f(r))
    }

    /// The order-independent memo key for `w`; clones the same fields `WordKey` already clones per dedup.
    fn state_key(&self, w: &Word) -> AnalysisStateKey {
        AnalysisStateKey::new(
            w.shape.clone(),
            w.stratum,
            w.syn_fs.clone(),
            w.real_fs.clone(),
            w.non_heads.len() as u32,
            w.unapplied_rule_counts.clone(),
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
    fn apply_one_mrule(&self, id: MRuleId, w: &Word) -> Vec<Word> {
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
        }
        outs
    }

    /// The mrule cascade over the reversed rule list: permutation for `Linear`, combination for `Unordered`, deduped by full word key.
    fn run_mrule_cascade(&self, input: &Word) -> Vec<Word> {
        // Memoization targets only the Unordered `k!` walk; Linear strata use the plain cascade.
        if let (MorphRuleOrder::Unordered, Some(scope)) = (self.order, self.scope) {
            return self.mrule_cascade_memoized(input, scope);
        }
        let apply_rule = |i: usize, w: &Word| self.apply_one_mrule(self.reversed_mrules[i], w);
        let key = |w: &Word| w.dedup_key();
        let casc = Cascade::new(true, usize::MAX);
        let n = self.reversed_mrules.len();
        let out = match self.order {
            MorphRuleOrder::Linear => casc.permutation(n, input.clone(), &apply_rule, &key),
            MorphRuleOrder::Unordered => casc.combination(n, input.clone(), &apply_rule, &key),
        };
        out.words
    }

    /// The memoized analog of `Cascade::combination`, memoizing every interior node's subtree, not just the top-level entry.
    fn mrule_cascade_memoized(&self, input: &Word, scope: &MemoScope) -> Vec<Word> {
        let mut out = OrderedDedup::new();
        self.memo_apply_rules(input, &mut out, scope);
        out.into_items()
    }

    /// The memo wrapper around one node's subtree expansion; `out` is the shared deduped accumulator.
    fn memo_apply_rules(
        &self,
        input: &Word,
        out: &mut OrderedDedup,
        scope: &MemoScope,
    ) -> Vec<Word> {
        if self.over_budget() {
            return Vec::new();
        }
        let (key, hit_replayed) = {
            let key = self.state_key(input);
            let s = scope.borrow();
            // Positive-replay or nogood hit: replay each stored result onto this arrival's own trail/non-head prefix.
            let replayed = s.memo.get(&key).map(|entry| {
                entry
                    .results
                    .iter()
                    .map(|stored| {
                        stored.replay_onto(
                            input,
                            entry.mrule_trail_prefix_length,
                            entry.non_head_prefix_length,
                        )
                    })
                    .collect::<Vec<Word>>()
            });
            (key, replayed)
        };
        if let Some(replayed) = hit_replayed {
            for r in &replayed {
                out.add(r.clone());
            }
            return replayed;
        }

        // In-flight re-entry guard: a key already expanding falls through to a plain unmemoized expansion (correctness-neutral; cannot fire in analysis).
        let fresh = scope.borrow_mut().in_progress.insert(key.clone());
        if !fresh {
            return self.memo_apply_rules_raw(input, out, scope);
        }

        let results = self.memo_apply_rules_raw(input, out, scope);

        // Clear the guard, then store if under the cap; prefix lengths let a replay split each stored result.
        {
            let mut s = scope.borrow_mut();
            s.in_progress.remove(&key);
            if s.has_memo_capacity() {
                let cloned_results = results.clone();
                s.memo.insert(
                    key,
                    MemoEntry::new(
                        cloned_results,
                        input.mrule_apps.len(),
                        input.non_heads.len(),
                    ),
                );
            }
        }
        results
    }

    /// One node's un-memoized expansion, with the descent itself memoized; no reachable-root gate, matching the baseline cascade.
    fn memo_apply_rules_raw(
        &self,
        input: &Word,
        out: &mut OrderedDedup,
        scope: &MemoScope,
    ) -> Vec<Word> {
        let mut local = Vec::new();
        let in_key = input.dedup_key();
        for i in 0..self.reversed_mrules.len() {
            for result in self.apply_one_mrule(self.reversed_mrules[i], input) {
                local.push(result.clone());
                out.add(result.clone());
                // Self-loop guard. Always false here — every unapplication changes the key.
                let is_self_loop = in_key == result.dedup_key();
                if is_self_loop {
                    continue;
                }
                local.extend(self.memo_apply_rules(&result, out, scope));
            }
        }
        local
    }

    /// Run the mrule cascade, then per stratum order interleave templates.
    fn apply_mrules(&self, input: &Word) -> Vec<Word> {
        if self.over_budget() {
            return Vec::new();
        }
        let mut result = Vec::new();
        // `.Distinct(...)` in C# is redundant here — the cascade already deduped by key.
        for w in self.run_mrule_cascade(input) {
            match self.order {
                MorphRuleOrder::Linear => result.push(w),
                MorphRuleOrder::Unordered => {
                    result.extend(self.apply_templates(&w));
                    result.push(w);
                }
            }
        }
        result
    }

    /// Run the template batch, then per stratum order interleave mrules and yield the template output when it changed the word.
    fn apply_templates(&self, input: &Word) -> Vec<Word> {
        if self.over_budget() {
            return Vec::new();
        }
        let in_key = input.dedup_key();
        let mut result = Vec::new();
        for t in self.run_template_batch(input) {
            let changed = t.dedup_key() != in_key;
            match self.order {
                MorphRuleOrder::Linear => {
                    result.extend(self.apply_mrules(&t));
                    if changed {
                        result.push(t);
                    }
                }
                MorphRuleOrder::Unordered => {
                    if changed {
                        result.extend(self.apply_mrules(&t));
                        result.push(t);
                    }
                }
            }
        }
        result
    }

    /// The template `RuleBatch`, memoized separately from the mrule memo when a scope is present.
    fn run_template_batch(&self, input: &Word) -> Vec<Word> {
        let Some(scope) = self.scope else {
            return self.run_template_batch_raw(input);
        };
        let key = self.state_key(input);
        {
            let s = scope.borrow();
            if let Some(entry) = s.template_memo.get(&key) {
                let replayed: Vec<Word> = entry
                    .results
                    .iter()
                    .map(|stored| {
                        stored.replay_onto(
                            input,
                            entry.mrule_trail_prefix_length,
                            entry.non_head_prefix_length,
                        )
                    })
                    .collect();
                drop(s);
                return replayed;
            }
        }
        let fresh = scope.borrow_mut().template_in_progress.insert(key.clone());
        if !fresh {
            return self.run_template_batch_raw(input);
        }
        let results = self.run_template_batch_raw(input);
        {
            let mut s = scope.borrow_mut();
            s.template_in_progress.remove(&key);
            if s.has_template_capacity() {
                s.template_memo.insert(
                    key,
                    MemoEntry::new(
                        results.clone(),
                        input.mrule_apps.len(),
                        input.non_heads.len(),
                    ),
                );
            }
        }
        results
    }

    /// The un-memoized template battery: non-disjunctive union of every affix template's output, deduped by key.
    fn run_template_batch_raw(&self, input: &Word) -> Vec<Word> {
        let mut seen: HashMap<WordKey, ()> = HashMap::default();
        let mut out = Vec::new();
        for &tid in &self.stratum.templates {
            for w in self.analyze_template(tid, input) {
                if seen.insert(w.dedup_key(), ()).is_none() {
                    out.push(w);
                }
            }
        }
        out
    }

    /// Gate on the template's required syntactic FS via `unify` (narrows), unapply slots top-down, then `add` (widening union) onto each output's syntactic FS.
    fn analyze_template(&self, tid: TemplateId, input: &Word) -> Vec<Word> {
        if !self.rule_admitted(RuleRef::Template(tid)) {
            return Vec::new();
        }
        let tmpl = &self.g.templates[tid.0 as usize];
        let req = self.g.fs_interner.get(tmpl.required_syn_fs);
        if !is_unifiable(&input.syn_fs, req) {
            return Vec::new();
        }
        let fs = unify(&input.syn_fs, req).unwrap_or_else(|| input.syn_fs.clone());
        // Fires once per `Apply`, right after the required-syn-FS gate and before the slot walk.
        let node_parent = input.trace.unwrap_or(self.parent);
        if self.trace.is_tracing() {
            self.trace.begin_unapply_template(node_parent, tid, input);
        }
        let mut out: HashMap<WordKey, Word> = HashMap::default();
        // Descend from the last slot.
        self.template_unapply_slots(tid, tmpl, input, tmpl.slots.len() as isize - 1, &mut out);
        let mut result: Vec<Word> = out.into_values().collect();
        // Union, not overwrite; see `add`'s doc.
        for w in &mut result {
            w.syn_fs = add(&w.syn_fs, &fs, &|f| self.g.syn_features.mask(f));
        }
        result
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
        let mut seen: HashMap<WordKey, ()> = HashMap::default();
        let mut out = Vec::new();
        for &rid in &slot.rules {
            for w in self.apply_one_mrule(rid, in_word) {
                if seen.insert(w.dedup_key(), ()).is_none() {
                    out.push(w);
                }
            }
        }
        out
    }

    /// Port of `AnalysisStratumRule.Apply`.
    fn analyze(&self, mut input: Word) -> StratumAnalysis {
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

        let mut mrule_out = self.apply_templates(&input);
        mrule_out.extend(self.apply_mrules(&input));
        // Every stratum output points back at the seed.
        for w in &mut mrule_out {
            w.source = Some(source.clone());
        }

        let mut output_keys: HashMap<WordKey, ()> = HashMap::default();
        // Shape -> canonical word index; the seed's shape is deliberately not registered.
        let mut shape_word: HashMap<Shape, usize> = HashMap::default();
        let mut words: Vec<Word> = Vec::new();
        output_keys.insert(input.dedup_key(), ());
        words.push(input);

        for w in mrule_out {
            if self.cfg.merge_equivalent {
                // A repeat shape folds into the canonical word's alternatives instead of entering the output.
                if let Some(&idx) = shape_word.get(&w.shape) {
                    words[idx].alternatives.push(w);
                    continue;
                }
            }
            // The second end-event, once per surviving candidate; must NOT be gated on the `output_keys.insert` below, since a key-duplicate still fires it.
            if self.trace.is_tracing() {
                let w_parent = w.trace.unwrap_or(node_parent);
                self.trace
                    .end_unapply_stratum(w_parent, self.stratum_id, &w);
            }
            if output_keys.insert(w.dedup_key(), ()).is_none() {
                if self.cfg.merge_equivalent {
                    shape_word.insert(w.shape.clone(), words.len());
                }
                words.push(w);
            }
            if self.cfg.max_unapplications > 0 && words.len() >= self.cfg.max_unapplications {
                break;
            }
        }

        StratumAnalysis {
            words,
            capped: self.budget.capped(),
        }
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
    let apply =
        |g: &Grammar, rid: MRuleId, w: &Word| morph::synthesize(g, w, &g.mrules[rid.0 as usize]);
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
) -> Vec<Word> {
    let tmpl = &g.templates[tid.0 as usize];
    let mut out: HashMap<WordKey, Word> = HashMap::default();
    let apply = |g: &Grammar, rid: MRuleId, w: &Word| {
        guided_synth(g, stratum, rid, w, cache, stats, trace, parent)
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
    let mut outs = morph::synthesize_cached_traced(
        g,
        id,
        w,
        &g.mrules[id.0 as usize],
        cache,
        mstats,
        trace,
        node_parent,
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
        guided_synth(g, stratum, sd.mrules[i], w, cache, stats, trace, parent)
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
                g, stratum, sd, &w, cap, steps, cache, stats, trace, parent, budget,
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
            g, stratum, tid, input, cap, steps, cache, stats, trace, parent, budget,
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
                        g, stratum, sd, &t, cap, steps, cache, stats, trace, parent, budget,
                    ) {
                        out.entry(m.dedup_key()).or_insert(m);
                    }
                }
            }
        }
    }

    out.into_values().collect()
}
