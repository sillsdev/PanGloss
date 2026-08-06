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
// `std::time::Instant` panics on wasm32-unknown-unknown, and the deadline check below runs on every
// parse. `web_time` re-exports std's `Duration` unchanged, so this substitutes `Instant` alone.
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
    /// The morphological-rule-level gate shared, textually identically, by C#'s
    /// `AnalysisAffixProcessRule`, `AnalysisCompoundingRule`, and
    /// `AnalysisRealizationalAffixProcessRule` — one `MRuleId` covers all three.
    MRule(MRuleId),
}

/// The Rust mirror of `Morpher.RuleSelector` (`Func<IHCRule, bool>`) — see `RuleRef`'s doc for
/// exactly which gates this predicate reaches. `None` (every pre-existing caller) means
/// "every rule admitted", byte-identical to C#'s default `rule => true`.
pub type RuleFilter<'a> = &'a (dyn Fn(RuleRef) -> bool + Sync);

use crate::cache::RuleCache;
use crate::cascade::Cascade;
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::{Word, WordKey};
use crate::{metathesis, morph, rewrite};

/// The per-parse memo carrier this module threads through the analysis cascade. `pg-parse` owns one
/// per `parse_word` call (see `pg_memo::AnalysisScope`) and hands it in via `analyze_stratum_scoped`.
pub type MemoScope = RefCell<AnalysisScope<Word>>;

/// A key→word dedup set preserving first-seen order — the memoized cascade's accumulator, the exact
/// analog of the plain `Cascade`'s internal `Acc`. Callers sort downstream, so insertion order is
/// only for determinism.
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
    /// The `--word-timeout-ms` deadline as an absolute instant, or `None` for no wall-clock bound.
    /// A second, orthogonal bound — not a re-expression of `cap`.
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

    /// True (and latches `capped`/`timed_out`) once either bound is exhausted. Consulted at every
    /// (un)application attempt and every recursion entry. The step cap is checked first, being the
    /// cheaper of the two; see this type's doc for why the clock is then read unconditionally.
    fn over_budget(&self) -> bool {
        if self.steps.get() >= self.cap {
            self.capped.set(true);
            return true;
        }
        self.deadline_expired()
    }

    /// The wall-clock-only check ordinary parsing and generation use for synthesis. It deliberately
    /// omits the step-cap branch so analysis effort cannot starve synthesis and `--step-cap`
    /// behavior stays byte-identical; with no deadline armed it is a complete no-op.
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

    /// The deadline must fire from the wall-clock path, not the step cap: an uncapped
    /// (`usize::MAX`) budget with a short real deadline armed, ticked as fast as possible, must
    /// break out promptly rather than running the artificially huge iteration bound to completion.
    #[test]
    fn wall_clock_deadline_fires_independent_of_an_uncapped_step_cap() {
        const N_HUGE: u64 = 200_000_000; // large enough that running it to completion unchecked
                                         // takes far longer than the deadline below on any dev/CI
                                         // machine — the "step-cap-irrelevant infinite-ish loop".
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
        // Generous upper bound for slow CI machines, yet far tighter than running the full N_HUGE
        // loop would ever be — loose enough to absorb scheduler jitter around the 30ms deadline.
        assert!(
            elapsed < Duration::from_secs(2),
            "elapsed {elapsed:?} should stay close to the {timeout:?} deadline, not balloon toward \
             an unbounded run"
        );
    }

    /// A deadline that is already in the past at construction time (the `--word-timeout-ms=0`
    /// smoke-test shape) must fire on the very first `over_budget()` check.
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

    /// The failure shape a step-count-gated wall-clock cadence misses: fewer ticks in total than one
    /// cadence interval, with real time elapsing between them. Reading the clock only at step 0
    /// would run the loop to completion; reading it on every armed call fires the deadline on time.
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

    /// `with_timeout(None)` (the default, `--word-timeout-ms` omitted) must be a complete no-op:
    /// the budget behaves exactly as a plain `StepBudget::new(cap)` would, for as many steps as
    /// the cap allows.
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

// =================================================================================================
// Analysis stratum rule.
// =================================================================================================

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
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
}

/// `analyze_stratum_scoped_filtered_ruled`'s traced sibling — identical in every other respect.
/// The intended caller is `pg_parse::Morpher::parse_word_selected_traced`; see `crate::morph`'s
/// analysis-tracing docs and `StratumAnalyzer`'s `trace`/`parent` fields.
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
        trace,
        parent,
    )
    .analyze(input)
}

/// The stratum orchestrator. Borrows the caller's `StepBudget` (see that type's doc) rather than
/// owning its own step counter — one `StratumAnalyzer` no longer means one budget.
struct StratumAnalyzer<'g, 's, 'f, 'r, 'c, 'b, 't> {
    g: &'g Grammar,
    stratum_id: StratumId,
    stratum: &'g pg_grammar::model::StratumDef,
    order: MorphRuleOrder,
    /// The stratum's morphological rules **reversed** — C# reverses for both Linear and Unordered.
    /// The cascade indexes this reversed list; the closure maps `i -> reversed[i]` so the correct
    /// `MRuleId` is recorded regardless of order.
    reversed_mrules: Vec<MRuleId>,
    cfg: AnalyzerConfig,
    budget: &'b StepBudget,
    /// The order-invariant memo, or `None` for the unmemoized baseline. Threaded here rather than
    /// through every recursion argument. See `analyze_stratum_scoped`.
    scope: Option<&'s MemoScope>,
    /// The non-head lexicon filter, or `None` for unfiltered. See `NonHeadRootFilter`.
    non_head_root_filter: Option<NonHeadRootFilter<'f>>,
    /// The mrule/template selector, or `None` to admit every rule. See `RuleFilter`.
    rule_filter: Option<RuleFilter<'r>>,
    /// The compile-once FST cache; `None` recompiles per call. See `crate::cache`'s module doc,
    /// and `analyze_stratum_scoped` for why the fallback is still needed.
    cache: Option<&'c RuleCache>,
    /// The analysis-side trace sink. Only `analyze_stratum_scoped_filtered_ruled_traced` passes a
    /// real one; every other entry point passes `NoopSink`, so `trace.is_tracing()` is false and
    /// every gated call below takes its fast path.
    trace: &'t dyn TraceSink,
    /// The ambient trace cursor. Call sites resolve `word.trace.unwrap_or(parent)`, so a chain of
    /// successful (un)applications nests under the deepest event fired on that branch.
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
            trace,
            parent,
        }
    }

    /// Rule admission — `true` when no filter was supplied, matching C#'s `rule => true` default.
    #[inline]
    fn rule_admitted(&self, r: RuleRef) -> bool {
        self.rule_filter.is_none_or(|f| f(r))
    }

    /// The order-independent memo key for `w` (C# `new AnalysisStateKey(word)`). Clones the shape +
    /// both feature structs + the unapplication multiset — the same clone cost `WordKey` already pays
    /// per dedup, and the memo removes far more expansions than it adds key-builds.
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

    /// True once the shared budget is exhausted. Consulted at every (un)application attempt and
    /// every recursion entry. Delegates to the shared `StepBudget` — see that type's doc.
    fn over_budget(&self) -> bool {
        self.budget.over_budget()
    }

    fn tick(&self) {
        self.budget.tick()
    }

    /// Unapply a single morphological rule and record C#'s `Word.MorphologicalRuleUnapplied`
    /// bookkeeping on every output. Analysis only ever grows these lists, so each dedup-key index is
    /// always `len - 1`. `morph::analyze` is left semantics-pure — the bookkeeping lives here.
    fn apply_one_mrule(&self, id: MRuleId, w: &Word) -> Vec<Word> {
        // C#'s shared top-of-`Apply` selector gate, checked before the budget tick: a
        // rejected-by-gate rule was never attempted (same convention as the two gates below).
        if !self.rule_admitted(RuleRef::MRule(id)) {
            return Vec::new();
        }
        if self.over_budget() {
            return Vec::new();
        }
        let rule = &self.g.mrules[id.0 as usize];
        // `Morpher.MaxStemCount` depth gate; see `AnalyzerConfig::max_stem_count`. Outside `tick()`
        // for the same reason as the gate above.
        if matches!(rule, MorphRuleDef::Compounding(_))
            && w.non_heads.len() as u32 + 1 >= self.cfg.max_stem_count
        {
            return Vec::new();
        }
        // `MaxApplicationCount`: once this rule has been unapplied on `w`'s trail `max_apps` times
        // it is skipped for this candidate. Outside `tick()`, and uniform across rule kinds,
        // matching C#'s identical placement in both rule types' `Apply` wrappers.
        if w.unapplied_rule_counts.get(&id).copied().unwrap_or(0) >= u32::from(rule.max_apps()) {
            return Vec::new();
        }
        self.tick();
        // The non-head root filter is threaded into `morph::ana_compound` rather than post-filtering
        // its returned words: root-allomorph resolution and the per-candidate pin must join the same
        // per-subrule duplicate-elimination scope C# uses, which only `ana_compound_subrule` has.
        let node_parent = w.trace.unwrap_or(self.parent);
        let mut outs = match (rule, self.non_head_root_filter) {
            (MorphRuleDef::Compounding(_), Some(filter)) => match self.cache {
                Some(cache) => morph::analyze_cached_with_root_filter_traced(
                    self.g,
                    id,
                    w,
                    rule,
                    cache,
                    filter,
                    self.trace,
                    node_parent,
                ),
                None => morph::analyze_with_root_filter(self.g, w, rule, filter),
            },
            _ => match self.cache {
                Some(cache) => morph::analyze_cached_traced(
                    self.g,
                    id,
                    w,
                    rule,
                    cache,
                    self.trace,
                    node_parent,
                ),
                None => morph::analyze(self.g, w, rule),
            },
        };
        for o in &mut outs {
            // Analysis always records the *known* rule (never C#'s null "unknown compounding rule"
            // — that only arises from generation seeding a bare non-head directly).
            o.mrule_apps.push(Some(id));
            o.mrule_app_index = o.mrule_apps.len() as i32 - 1;
            // The order-invariant count multiset, paired with the `mrule_apps.push` above. Consumed
            // only by `Self::state_key`, so the unmemoized path computes it unread.
            o.record_unapplication(id);
            // Compounding analysis (`morph::ana_compound`) has already pushed the split-off
            // non-head; C# `NonHeadUnapplied` pairs that push with this index bump.
            o.non_head_app_index = o.non_heads.len() as i32 - 1;
        }
        outs
    }

    /// Run the morphological-rule cascade over the reversed rule list: a permutation cascade
    /// (multi-app) for a `Linear` stratum, a combination cascade for `Unordered`. Deduped by full
    /// word key; run uncapped, with the shared budget enforced inside the closure.
    fn run_mrule_cascade(&self, input: &Word) -> Vec<Word> {
        // The memoized walk replaces the plain one only on an Unordered stratum with a scope
        // present — the `k!` walk is what memoization targets. Linear strata and the no-scope
        // baseline use the plain cascade unchanged.
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

    /// The memoized analog of `Cascade::combination`: the same deduped reachable set (results of ≥1
    /// unapplication, not `input` itself), with every interior node's subtree memoized by
    /// `AnalysisStateKey`. Memoizing only the top-level entry forfeits the win.
    fn mrule_cascade_memoized(&self, input: &Word, scope: &MemoScope) -> Vec<Word> {
        let mut out = OrderedDedup::new();
        self.memo_apply_rules(input, &mut out, scope);
        out.into_items()
    }

    /// The memo wrapper around one node's subtree expansion. Returns the subtree-local results for
    /// storage and replay; `out` is the shared deduped accumulator the caller reads.
    fn memo_apply_rules(
        &self,
        input: &Word,
        out: &mut OrderedDedup,
        scope: &MemoScope,
    ) -> Vec<Word> {
        if self.over_budget() {
            return Vec::new();
        }
        let key = self.state_key(input);

        // Positive-replay or nogood hit: replay each stored result onto this arrival's own
        // trail/non-head prefix. An empty entry (a nogood) replays nothing.
        {
            let s = scope.borrow();
            if let Some(entry) = s.memo.get(&key) {
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
                for r in &replayed {
                    out.add(r.clone());
                }
                return replayed;
            }
        }

        // In-flight re-entry guard: a key already expanding on the stack falls through to a plain
        // unmemoized expansion for this arrival (correctness-neutral). In analysis this cannot fire
        // — every unapplication grows the rule multiset — but it is ported faithfully.
        let fresh = scope.borrow_mut().in_progress.insert(key.clone());
        if !fresh {
            return self.memo_apply_rules_raw(input, out, scope);
        }

        let results = self.memo_apply_rules_raw(input, out, scope);

        // Clear the guard, then store (nogood or positive) if under the cap. The prefix lengths are
        // this node's own trail/non-head lengths, so a replay can split each stored result.
        {
            let mut s = scope.borrow_mut();
            s.in_progress.remove(&key);
            if s.has_memo_capacity() {
                s.memo.insert(
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

    /// One node's un-memoized expansion: recurse from rule 0 each level, self-loop-guarded, with the
    /// descent itself memoized. C#'s `HasReachableRoot` gate is intentionally omitted — the baseline
    /// cascade has no such gate, so adding it would break the memo-on == memo-off invariant.
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
                if in_key == result.dedup_key() {
                    continue;
                }
                local.extend(self.memo_apply_rules(&result, out, scope));
            }
        }
        local
    }

    /// Port of `AnalysisStratumRule.ApplyMorphologicalRules`: run the mrule cascade, then per
    /// stratum order interleave templates.
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

    /// Port of `AnalysisStratumRule.ApplyTemplates`: run the template batch, then per stratum order
    /// interleave mrules and yield the template output when it changed the word.
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

    /// The template `RuleBatch`, memoized in the separate `TemplateMemo` table when a scope is
    /// present. On a pathological word the battery runs far more often than there are distinct
    /// keys, so collapsing equal-keyed arrivals to one run plus replay is the bulk of the memo win.
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

    /// The un-memoized template battery: non-disjunctive union of every affix template's analysis
    /// output, deduped by key (no early exit).
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

    /// Port of `AnalysisAffixTemplateRule.Apply`: gate on the template's required syntactic FS via a
    /// real `unify` (this narrows), unapply slots top-down, then `add` — a widening union, never
    /// `unify` or `priority_union` — the unified FS onto each output's syntactic FS.
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
        // `BeginUnapplyTemplate` fires once per `Apply`, right after the required-syn-FS gate and
        // before the slot walk. `input.trace` is untouched above, so the parent is resolved once
        // here and reused below.
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

    /// Port of `AnalysisAffixTemplateRule.ApplySlots`: from `index` down, unapply each slot's rule
    /// batch and recurse into earlier slots. Reaching a non-optional slot forces a return — its
    /// material had to be consumed here, so that path survives only via the recursion its batch
    /// spawned. Falling past slot 0 adds the fully-unapplied word.
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

    /// One slot's non-disjunctive `RuleBatch`: the union of its alternative rules' analysis
    /// outputs, deduped.
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
        // `BeginUnapplyStratum` fires against the word exactly as received, before the clone below.
        // Only rule-level events reassign a word's own `.trace`, never this local binding, so the
        // parent resolved here is reused for the matching `EndUnapplyStratum` calls.
        let node_parent = input.trace.unwrap_or(self.parent);
        if self.trace.is_tracing() {
            self.trace
                .begin_unapply_stratum(node_parent, self.stratum_id, &input);
        }

        // Every word this stratum produces records the incoming word as its `Source`, so
        // `expand_alternatives` can later walk the per-stratum spine. The snapshot is taken WITH the
        // previous stratum's alternatives; the seed's own are then cleared, as C#'s clone does.
        let source = Rc::new(input.clone());
        input.stratum = self.stratum_id;
        input.source = Some(source.clone());
        input.alternatives.clear();

        // C#'s `_prulesRule.Apply(input)`: a linear cascade over the prules reversed, applied in
        // place — C# discards the cascade's return value and uses `input` itself. Modelled as an
        // in-place fold, with no per-prule cursor advance (a deliberately coarser trace depth).
        for &pid in self.stratum.prules.iter().rev() {
            // The one (un)application site in this module needing its own budget check. Deadline
            // only, never the step cap — see `StepBudget::deadline_expired` for why conflating the
            // two here would risk golden parity.
            if self.budget.synthesis_over_budget() {
                break;
            }
            let result = match &self.g.prules[pid.0 as usize] {
                pg_grammar::model::PhonRuleDef::Rewrite(r) => match self.cache {
                    Some(cache) => rewrite::analyze_cached_traced(
                        self.g,
                        pid,
                        r,
                        &input.shape,
                        cache,
                        self.trace,
                        self.parent,
                    ),
                    None => rewrite::analyze_traced(
                        self.g,
                        pid,
                        r,
                        &input.shape,
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

        // The FIRST `EndUnapplyStratum` exit, for `input` itself — the post-prule candidate that
        // always survives as its own analysis. C# reaches it before any nested template/mrule event
        // because its `mruleOutWords` is lazy; this port is eager, so the call is placed first.
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
        // Shape -> index into `words` of the canonical word for that shape. The seed's shape is
        // deliberately NOT registered, so a mrule output matching it becomes its own canonical.
        let mut shape_word: HashMap<Shape, usize> = HashMap::default();
        let mut words: Vec<Word> = Vec::new();
        output_keys.insert(input.dedup_key(), ());
        words.push(input);

        for w in mrule_out {
            if self.cfg.merge_equivalent {
                // A repeat shape does not enter the output; it folds into the canonical word's
                // alternatives and is skipped.
                if let Some(&idx) = shape_word.get(&w.shape) {
                    words[idx].alternatives.push(w);
                    continue;
                }
            }
            // The SECOND `EndUnapplyStratum` exit, once per surviving candidate. C# adds to the
            // output set unconditionally before this call, so an exact key-duplicate still fires
            // the event — do NOT gate it on the `output_keys.insert` below.
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

/// C# `AffixTemplateSlot.Optional`: a slot with no rules is always optional; otherwise its declared
/// flag.
fn slot_optional(slot: &SlotDef) -> bool {
    slot.rules.is_empty() || slot.optional
}

// =================================================================================================
// Synthesis affix-template rule (self-contained; the driver below is gated on lexicon state).
// =================================================================================================

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
    // No natural `parse_word`-scoped deadline here, so this builds its own budget with none armed:
    // the deadline check is then a no-op and this entry point stays cap-only.
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

/// The production pipeline's slot walk: `synthesize_template`'s structure, but each rule goes
/// through `guided_synth` (confirms the analysis, decrements `mrule_app_index`) and the caller's
/// step budget is shared. `BeginApplyTemplate` fires once here, at the top-level entry;
/// `EndApplyTemplate` is per-recursion-level and so lives in `synth_slots_generic`.
#[allow(clippy::too_many_arguments)]
fn guided_template_apply(
    g: &Grammar,
    tid: TemplateId,
    input: &Word,
    cap: usize,
    steps: &Cell<usize>,
    cache: &RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
    budget: &StepBudget,
) -> Vec<Word> {
    let tmpl = &g.templates[tid.0 as usize];
    let mut out: HashMap<WordKey, Word> = HashMap::default();
    let apply = |g: &Grammar, rid: MRuleId, w: &Word| guided_synth(g, rid, w, cache, trace, parent);
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

/// Port of `SynthesisAffixTemplateRule.ApplySlots` (bottom-up), parameterized by `apply` so one walk
/// serves the ungated and guided callers; an empty return means "did not apply". `budget` is
/// consulted for its deadline only — `cap`/`steps` stay the sole step-count authority, since
/// conflating the two would risk golden parity (see `StepBudget::deadline_expired`).
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

// =================================================================================================
// Synthesis stratum rule.
// =================================================================================================

/// Guided single-rule synthesis: C#'s `SynthesisAffixProcessRule.Apply` entry gate plus its
/// `MorphologicalRuleApplied` bookkeeping, which together make synthesis **confirm the analysis** —
/// a rule may re-apply only if it is the current expected rule on the word's unapplication stack,
/// and applying it advances that stack (decrementing `mrule_app_index`, and `non_head_app_index`
/// too for a compounding rule). A null pending slot matches any compounding rule; only
/// `generate_words` seeding a bare non-head directly produces one.
///
/// C#'s `MaxApplicationCount` is **not** enforced: it needs a per-word count this port does not
/// carry. The guided index bounds re-applications to the analysis history length, so the only
/// remaining over-generation is a multi-application rule (reduplication).
///
/// Trace events for both success and failure are emitted inside `morph::synthesize_cached_traced`,
/// never here — emitting in both places would double-fire.
fn guided_synth(
    g: &Grammar,
    id: MRuleId,
    w: &Word,
    cache: &RuleCache,
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
    // The resolved cursor is threaded INTO `synthesize_cached_traced` rather than applied after the
    // fact: the `synth_*_cached` functions fire both the applied and not-applied events at their own
    // internal gates, with the real subrule index, and set each successful output's `.trace`.
    let node_parent = w.trace.unwrap_or(parent);
    let mut outs = morph::synthesize_cached_traced(
        g,
        id,
        w,
        &g.mrules[id.0 as usize],
        cache,
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

/// The stratum a morphological rule belongs to (C# `IMorphologicalRule.Stratum`): the stratum whose
/// `mrules` list or affix-template slots contain `id`. Used only for `has_remaining_rules_from_stratum`
/// filtering of the (few) final candidates, so a linear scan is fine.
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

/// Port of `Word.HasRemainingRulesFromStratum`: the word still has a pending unapplied rule
/// belonging to `stratum`, so it left that stratum without finishing — a partial parse. When the
/// pending slot is the null "unknown compounding rule", the pending non-head's own stratum stands
/// in for the not-yet-chosen compounding rule's.
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
    // No production call site: `pg-parse` threads its own per-`parse_word` budget through
    // `synthesize_stratum_traced` directly. A freshly built no-timeout budget makes the deadline
    // check a no-op for this function's test callers.
    let budget = StepBudget::new(cap);
    synthesize_stratum_traced(
        g,
        stratum,
        input,
        cap,
        cache,
        &budget,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
}

/// `synthesize_stratum`'s traced sibling and the single source of truth both share. The caller
/// passes a real handle once, at the top of the per-word synthesis pipeline; every deeper call
/// resolves the cursor off the `Word` itself, mirroring C#'s `Word.CurrentTrace`.
#[allow(clippy::too_many_arguments)]
pub fn synthesize_stratum_traced(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cap: usize,
    cache: &RuleCache,
    budget: &StepBudget,
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
        let mut nw = w.clone();
        // Trailing in-place prule fold, then clear the final flag. `synthesize_with_mpr_cached`
        // rather than the bare `synthesize`: a subrule's POS/MPR applicability gate must see this
        // word's actual syntactic FS and MPR set, never assumed-empty ones. `break` (not `return`)
        // leaves `nw.shape` however far the fold got and falls through to this candidate's own
        // bookkeeping, matching the analysis-side prule loop.
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
    // The guided cascade: a rule applies only if it is the word's current expected unapplication.
    // Because `guided_synth` strictly decrements `mrule_app_index`, the multi-application cascade
    // terminates once the stack is exhausted — synthesis's analog of the analysis step budget. The
    // wall-clock check belongs inside the closure: neither `cap` nor the cascade's own internal cap
    // is a step COUNT with any notion of elapsed time.
    let apply_rule = |i: usize, w: &Word| -> Vec<Word> {
        if steps.get() >= cap {
            return Vec::new();
        }
        if budget.synthesis_over_budget() {
            return Vec::new();
        }
        steps.set(steps.get() + 1);
        guided_synth(g, sd.mrules[i], w, cache, trace, parent)
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
                g, stratum, sd, &w, cap, steps, cache, trace, parent, budget,
            ));
        }
    }
    result
}

/// Whether `word`'s root *entry* is flagged partial. Distinct from `Word::flags.is_partial`, which
/// starts from the same flag but can also be set by a partial affix rule. The guessed-root arm is
/// load-bearing, not defensive: this runs at the top of every `synth_apply_templates` call, and
/// indexing `allomorph_owners` with the guessed sentinel panics. It resolves the pattern entry the
/// guess was fabricated from — a real `LexEntryId` — which is where C# copies `IsPartial` from too.
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

/// `SynthesisAffixTemplatesRule.ChooseInflectionalStem`. If the root belongs to a family and
/// `real_fs` is non-empty, walk the family in document order for the most specific applicable
/// relative. `best` starts as `input` and is swapped when a relative's lexical syntactic FS is
/// `real_fs`-unifiable, is subsumed by `best`'s CURRENT FS (the comparison must be against `best`,
/// which may already have been swapped, not against the original `input`), and adds a non-empty
/// remainder that is itself `real_fs`-unifiable — so the extra specificity is something the
/// realizational features actually select. A swap reseeds from `input`'s `real_fs`, not the
/// relative's own FS, since a bare `LexEntry` has no realizational FS.
///
/// A guessed root returns unchanged, and that is faithful rather than merely safe: C#'s guess
/// fabrication never sets the fabricated entry's family, so it would see none there either.
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
    // Try each applicable template, marking each output final when the word is partial or the
    // template is. The realizational/syntactic unifiability check returns early BEFORE
    // `choose_inflectional_stem` runs — it is checked against the word as handed in.
    if !is_unifiable(&input.real_fs, &input.syn_fs) {
        return Vec::new();
    }
    // C# rebinds `input` to the chosen stem, so every use below reads the POST-swap word. Shadow
    // the parameter to match.
    let input = choose_inflectional_stem(g, input);
    let input = &input;
    let in_key = input.dedup_key();
    let mut out: HashMap<WordKey, Word> = HashMap::default();
    // A template applies only if the root morpheme is not partial. The root does not change across
    // templates, so the check is hoisted; a partial root leaves the passthrough below to fire.
    let root_partial = root_is_partial(g, input);
    let mut applicable = false;
    for &tid in &sd.templates {
        let tmpl = &g.templates[tid.0 as usize];
        let req = g.fs_interner.get(tmpl.required_syn_fs);
        if !is_unifiable(&input.syn_fs, req) || root_partial {
            continue;
        }
        applicable = true;
        for w in guided_template_apply(g, tid, input, cap, steps, cache, trace, parent, budget) {
            let final_flag = w.flags.is_partial || tmpl.is_final;
            let mut w = w;
            w.flags.is_last_applied_rule_final = Some(final_flag);
            out.entry(w.dedup_key()).or_insert(w);
        }
    }
    // No template output: pass the input through (marked final) UNLESS it is non-partial AND some
    // template was applicable, in which case the applicable-but-unapplied word is dropped. Gating
    // on `!applicable` alone would wrongly drop a partial word that had an applicable template.
    //
    // This insertion must happen BEFORE the mrule recursion below. The passthrough is as much a
    // recursion candidate as a genuine template hit; computing it afterwards would make a stratum
    // with no templates at all skip that recursion on every call, since `out` would still be empty.
    if out.is_empty() {
        if input.flags.is_partial || !applicable {
            let mut w = input.clone();
            if w.flags.is_last_applied_rule_final != Some(true) {
                w.flags.is_last_applied_rule_final = Some(true);
            }
            out.insert(w.dedup_key(), w);
        } else if trace.is_tracing() {
            // The complementary branch adds NOTHING to the output, unlike the passthrough above:
            // the word is dropped, just with a trace event recorded first.
            let node_parent = input.trace.unwrap_or(parent);
            trace.applicable_templates_not_applied(node_parent, stratum, input);
        }
    }

    match sd.mrule_order {
        MorphRuleOrder::Linear => {}
        MorphRuleOrder::Unordered => {
            // For each changed template output — including the passthrough inserted just above,
            // when present — also run the mrules on it.
            let templated: Vec<Word> = out.values().cloned().collect();
            for t in templated {
                if t.dedup_key() != in_key {
                    for m in synth_apply_mrules(
                        g, stratum, sd, &t, cap, steps, cache, trace, parent, budget,
                    ) {
                        out.entry(m.dedup_key()).or_insert(m);
                    }
                }
            }
        }
    }

    out.into_values().collect()
}
