//! Per-stratum analysis/synthesis orchestration + the affix-template battery (M4b, part 2/2).
//!
//! This composes the already-built engines — [`crate::rewrite`] (phonological analyze/synthesize),
//! [`crate::morph`] (morphological rule analyze/synthesize), and [`crate::cascade`] (the
//! Linear/Permutation/Combination rule cascades) — into the per-stratum drivers, faithful ports of:
//! - `SIL.Machine.Morphology.HermitCrab/AnalysisStratumRule.cs` (analysis: prules → interleaved
//!   templates + mrule cascade, then shape-merge dedup),
//! - `SynthesisStratumRule.cs` (synthesis: interleaved mrules + templates → prules, final gating),
//! - `AffixTemplate.cs` / `AffixTemplateSlot.cs` / `Analysis|SynthesisAffixTemplateRule.cs` /
//!   `SynthesisAffixTemplatesRule.cs` (the slot battery), and
//! - `SIL.Machine/Rules/RuleBatch.cs` (union of rule outputs; disjunctive = early-exit).
//!
//! ## Termination (unmemoized; M6 adds the memo that makes this cheap)
//! The mrule cascades are multi-application (`multi_app=true`): a rule may re-apply to its own
//! output. Because every unapplication grows the word's `mrule_apps` list, the cascade's
//! `key(input) != key(result)` self-loop guard is *always* true, so the walk terminates only when
//! rules stop applying — which, unmemoized on a k!-Unordered stratum, can blow up. Rather than the
//! cascade's per-call cap (which cannot be shared across the mutually-recursive
//! [`StratumAnalyzer::apply_templates`] ↔ [`StratumAnalyzer::apply_mrules`] descent), this module
//! threads a [`StepBudget`] that the `apply_rule` closure and every recursion entry consult. The
//! cascades are run *uncapped*; when the budget is exhausted the closure returns no results, the
//! cascade sees "rule didn't apply", and the whole descent unwinds cleanly. This needs zero edits
//! to `cascade.rs` and bounds both cascade breadth and recursion depth with a single counter.
//!
//! **The budget is shared across the whole `parse_word` call, not just one `StratumAnalyzer`'s own
//! descent** (see [`StepBudget`]'s doc / `docs/budget-model.md` — this closes an amplifier where a
//! fresh per-instance counter let a multi-stratum, multi-candidate word explore `cap × (#stratum-
//! analyze calls)` steps instead of `cap`).

use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry;
use std::rc::Rc;
// std::time::Instant panics ("time not implemented on this platform") on wasm32-unknown-unknown
// -- this module's deadline check and always-on profiling timers (below) are unconditional on
// every parse, so this crate's `web-time` dependency (already used the same way in `morph.rs` and
// `pg-fst::traverse`, see those modules' own comments) belongs here too; this was the one spot the
// wasm32 sweep missed (found by inspection while investigating a wasm32-runtime panic during P4
// gate F4 work -- not itself reached by that panic, which turned out to be in a dependency, see
// docs/fst-plan/foma-fst-plan.md's P4 report for the full story).
// web_time re-exports std's `Duration` unchanged, so this is a pure drop-in for `Instant` alone.
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
/// `pg-rules` depending on `pg-parse` (crate-boundary: `pg-parse` depends on `pg-rules`, not the
/// reverse — plan §5.1 crate map). The signature matches `pg-parse`'s
/// `RootAllomorphIndex::search`'s exact return shape (`(AllomorphId, LexEntryId)` pairs), so
/// `pg-parse` can hand its own method straight in as the closure body.
///
/// Only the lexicon *search* crosses the boundary this way. The two C# sub-checks that follow each
/// matched root (syntactic-FS unifiability, MPR productivity match —
/// AnalysisCompoundingRule.cs:67-97), the per-allomorph resolution/pin (cs:119-124), and the
/// per-subrule dedup (cs:99-117) are all done on the `pg-rules` side instead (`morph::
/// resolve_non_head_roots`, called from `morph::ana_compound_subrule` — plan Tier-2 #7), because
/// `Grammar` already carries everything they need (the rule's `non_head_required_syn_fs` /
/// `non_head_prod_restrictions_mpr`, every `LexEntryDef`'s `syn_fs`/`mpr`/`allomorphs`, and
/// `pg_rules::shape_feat::segment_with_features` to re-segment the matched allomorph's text) — no
/// need to widen the callback itself beyond a raw shape search.
///
/// `+ Sync` even though nothing is threaded yet: M7 parallelizes batch parsing across words, and a
/// non-`Sync` closure type baked in now would be a breaking API change later — cheap to get right
/// now.
pub type NonHeadRootFilter<'a> =
    &'a (dyn Fn(StratumId, &Shape) -> Vec<crate::word::ResolvedRoot> + Sync);

/// F1 (HYBRID_FST_RUST_PLAN.md §7.1 item 1): the admission unit C#'s `Morpher.RuleSelector` gates —
/// one variant per `IHCRule`-implementing kind that has its own `RuleSelector` read site (see the
/// grep inventory in `docs/fst-plan/F1_QUIRK_AUDIT.md`'s "Selector read-site inventory"). Mirrors
/// C#'s `Func<IHCRule, bool>` taking a *type-tagged* rule reference rather than a shared supertype,
/// since Rust has no single `IHCRule` object to hand back — the caller's closure switches on the
/// variant instead of doing an `is` check, an approved deviation in mechanism only (§7.1 preamble):
/// the SET of admissible rules a given predicate computes is what parity requires, not how the
/// predicate is shaped.
///
/// **F1 scope note**: only [`RuleRef::Stratum`], [`RuleRef::Template`], and [`RuleRef::MRule`] are
/// wired into the analysis cascade this milestone (`StratumAnalyzer::apply_one_mrule`/
/// `analyze_template`, plus `pg-parse::Morpher`'s stratum-descent loop on both the analysis AND
/// synthesis side). Phonological-rule-level gating (`AnalysisRewriteRule`/`AnalysisMetathesisRule`/
/// `SynthesisRewriteRule`/`SynthesisMetathesisRule`'s own `RuleSelector` checks) and synthesis-side
/// mrule/template gating are NOT wired yet — deferred to F5 (`replay.rs`), the first milestone that
/// actually needs to restrict anything beyond a stratum/template/mrule set (`FstReplay`'s own
/// predicate keeps every phonological rule permanently open, `r is IPhonologicalRule` unconditionally
/// true — HYBRID_FST_RUST_PLAN.md §4.3 quirk #8 — so no F1-F4 code path is blocked by this deferral).
/// Flagged here, not silently dropped; a `PRule` variant will be added when F5/F7 needs one.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RuleRef {
    /// `AnalysisLanguageRule.cs:29` / `SynthesisStratumRule.cs:51`'s stratum-level gate.
    Stratum(StratumId),
    /// `AnalysisAffixTemplateRule.cs:34`'s template-level gate.
    Template(TemplateId),
    /// The morphological-rule-level gate shared, textually identically, by
    /// `AnalysisAffixProcessRule.cs:42`, `AnalysisCompoundingRule.cs:42`, and
    /// `AnalysisRealizationalAffixProcessRule.cs:42` (one `MRuleId` covers all three C# rule kinds,
    /// matching `pg_grammar::model::MorphRuleDef`'s own unification of them under one id space).
    MRule(MRuleId),
}

/// The Rust mirror of `Morpher.RuleSelector` (`Func<IHCRule, bool>`) — see [`RuleRef`]'s doc for
/// exactly which gates this predicate reaches at F1. `None` (every pre-existing caller) means
/// "every rule admitted", byte-identical to C#'s default `rule => true` (`Morpher.cs:72`).
pub type RuleFilter<'a> = &'a (dyn Fn(RuleRef) -> bool + Sync);

use crate::cache::RuleCache;
use crate::cascade::Cascade;
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::{Word, WordKey};
use crate::{metathesis, morph, rewrite};

/// The per-parse memo carrier this module threads through the analysis cascade. `pg-parse` owns one
/// per `parse_word` call (see `pg_memo::AnalysisScope`) and hands it in via [`analyze_stratum_scoped`].
pub type MemoScope = RefCell<AnalysisScope<Word>>;

/// A key→word dedup set preserving first-seen order — the memoized cascade's accumulator, the exact
/// analog of the plain [`Cascade`]'s internal `Acc` (and C# `MemoizedCombinationRuleCascade`'s
/// `output` `HashSet<Word>`). Callers sort downstream (the signature path), so insertion order is
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

/// The shared per-`parse_word` search-step budget (see `docs/budget-model.md`). One is constructed
/// per `Morpher::parse_word` call and threaded by reference through every
/// `analyze_stratum_scoped_filtered` invocation of that word's analysis descent — across every
/// stratum AND every candidate word — closing the old per-`StratumAnalyzer`-instance amplifier
/// (previously a fresh `Cell::new(0)` was built per stratum-analyze call, so the effective budget
/// was `cap × #such calls`, not `cap`). Test call sites with no natural "one parse_word" scope
/// construct their own `StepBudget::new(cap)` per call — identical to the old per-instance-`Cell`
/// behavior for an isolated single call.
///
/// Synthesis (`synthesize_stratum`/`synthesize_template`) deliberately does NOT share this budget —
/// see `docs/budget-model.md`'s "scope of this change" section: synthesis is a confirmation walk
/// bounded by the word's own unapplication-stack length, and sharing a single counter across both
/// phases would let a heavy analysis starve the very candidates it just found of synthesis steps.
/// **O1b history:** `over_budget()` used to re-sample the wall clock only every
/// `WALL_CLOCK_CHECK_INTERVAL` (1024) ticks, gated on `self.steps.get().is_multiple_of(1024)` —
/// the reasoning being that `Instant::now()` is more expensive than the `Cell` compare the step-cap
/// check already does, so reading the clock on every call seemed wasteful. This was a real bug, not
/// just a missed optimization: per-tick cost is NOT uniform (a single (un)application attempt can
/// legitimately cost microseconds or, for an Optional-flooded affix-matcher shape, over a second —
/// `docs/phase2-completed/narrowing-budget-w8.md`'s "46s FST traversal" finding, pre-guard era but
/// illustrative of the range), so a cadence gated on the *global step count landing on an exact
/// multiple of 1024* only fires as often as "1024 ticks accrue" — completely decoupled from wall
/// time. A word whose entire natural run takes fewer than 1024 ticks (measured on real Amharic
/// pathological words: ~1-1.5s per tick, so a few hundred ticks spans several minutes) gets exactly
/// ONE wall-clock sample, at construction (`steps() == 0`), and then NEVER AGAIN — the deadline
/// check silently stops mattering for the rest of that word's run, no matter how long it takes.
/// This is exactly the mechanism behind `rust-optimizations-phase2.md`'s O1b finding: some words
/// (e.g. በመጨረሻ, 489073ms against a 120000ms deadline) ran ~4x past their deadline and reported `ok`
/// instead of `TIMEOUT`, while others (whose total tick count happens to cross a 1024 boundary
/// before their deadline elapses) correctly timed out. No smaller *step-count* interval fixes this
/// in principle — it only pushes the same failure mode onto slower words; the only sound fix is to
/// stop gating the wall-clock read on step count at all: read the clock on **every** `over_budget()`
/// call once a deadline is armed. `over_budget()` fires at rule-attempt/recursion-entry granularity
/// (a few times per tick, not per innermost-loop-iteration of an FST traversal — that finer-grained
/// hot loop is `pg-fst::traverse`, deliberately NOT touched here), so `Instant::now()`'s real cost
/// (tens of nanoseconds) is negligible next to the per-call work it gates. No deadline (`None`, the
/// default) still never reads the clock at all — zero cost when `--word-timeout-ms` is unused.
pub struct StepBudget {
    cap: usize,
    steps: Cell<usize>,
    capped: Cell<bool>,
    /// The `--word-timeout-ms` deadline, resolved to an absolute instant at construction time
    /// (`with_timeout`), or `None` — no wall-clock bound at all (the default). Independent of
    /// `cap`/`capped`: this is a *second*, orthogonal bound (`docs/budget-model.md`'s addendum),
    /// not a re-expression of the step cap.
    deadline: Option<Instant>,
    timed_out: Cell<bool>,
}

impl StepBudget {
    pub fn new(cap: usize) -> Self {
        StepBudget {
            cap,
            steps: Cell::new(0),
            capped: Cell::new(false),
            deadline: None,
            timed_out: Cell::new(false),
        }
    }

    /// Arm (or leave unarmed) an optional wall-clock deadline alongside the existing step cap —
    /// two independent bounds on the same `parse_word` call, whichever fires first wins
    /// (`docs/budget-model.md`'s addendum). `timeout: None` is a complete no-op (matches `new`'s
    /// default), so callers that never pass `--word-timeout-ms` pay nothing extra.
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.deadline = timeout.map(|d| Instant::now() + d);
        self
    }

    /// True (and latches `capped`/`timed_out`) once either bound is exhausted. Consulted at every
    /// (un)application attempt and every recursion entry, mirroring the old
    /// `StratumAnalyzer::over_budget`. The step cap is checked first (cheapest, and preserves the
    /// pre-existing `capped` semantics exactly); the wall clock is sampled on every call once a
    /// deadline was armed — see this struct's O1b doc for why a step-count-based cadence cannot be
    /// trusted to bound wall-clock time.
    fn over_budget(&self) -> bool {
        if self.steps.get() >= self.cap {
            self.capped.set(true);
            return true;
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.timed_out.set(true);
                return true;
            }
        }
        false
    }

    /// Fix 2 (synthesis-side `--word-timeout-ms` enforcement, `docs/budget-model.md`): the
    /// **wall-clock-only** half of [`Self::over_budget`], deliberately WITHOUT the step-cap branch.
    /// Synthesis (`synthesize_stratum`/`synth_apply_mrules`/`synth_apply_templates`/
    /// `guided_template_apply`/`synth_slots_generic`) keeps its own local `cap: usize` + `Cell<usize>`
    /// step counter, entirely separate from this budget's `cap`/`steps` (see this struct's own doc,
    /// "Synthesis ... deliberately does NOT share this budget") — that step-count semantics must stay
    /// byte-identical (the Sena `--step-cap 500000` goldens), so synthesis must never call
    /// [`Self::over_budget`] itself (its step-cap branch reads THIS budget's `steps`/`cap`, which
    /// belong to the analysis phase of the same `parse_word` call and could already be exhausted by
    /// analysis alone, tripping a cap-shaped bail at a point synthesis never checked before). This
    /// method only ever reads `deadline`, exactly like [`Self::over_budget`]'s own deadline branch,
    /// and sets `timed_out` the same way (so the CLI's existing `TIMEOUT` row plumbing, gated on
    /// `timed_out()`, fires regardless of which phase actually caught the deadline). When `deadline`
    /// is `None` (`--word-timeout-ms` omitted — the Sena golden runs, which pass only `--step-cap`)
    /// this is `false` unconditionally, a complete no-op: every step-cap-only synthesis call site
    /// that gains a check against this method behaves exactly as before this fix.
    fn deadline_expired(&self) -> bool {
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.timed_out.set(true);
                return true;
            }
        }
        false
    }

    fn tick(&self) {
        self.steps.set(self.steps.get() + 1);
    }

    /// Whether this budget's step cap fired at any point during its lifetime (partial results
    /// possible). Never true because of a `--word-timeout-ms` deadline — see [`Self::timed_out`].
    pub fn capped(&self) -> bool {
        self.capped.get()
    }

    /// Whether this budget's wall-clock deadline (`--word-timeout-ms`) fired at any point during
    /// its lifetime. Independent of [`Self::capped`] — a word can time out with steps to spare, or
    /// hit the step cap well inside its deadline; the two are deliberately not conflated so
    /// downstream callers (the batch TSV writer) can report a distinct `TIMEOUT` outcome instead of
    /// folding it into the existing step-cap-exhausted reporting.
    pub fn timed_out(&self) -> bool {
        self.timed_out.get()
    }

    /// Raw tick count so far (diagnostic only — see `docs/budget-model.md`). Lets callers report how
    /// many (un)application attempts a `parse_word` call actually consumed, independent of whether
    /// the cap was hit; used to compare against C#'s `--rule-stats` attempt counts on specific words.
    pub fn steps(&self) -> usize {
        self.steps.get()
    }
}

#[cfg(test)]
mod step_budget_timeout_tests {
    use super::*;

    /// The regression guard for the `--word-timeout-ms` wall-clock deadline: a tight, cheap loop
    /// that ticks the budget as fast as possible with the step cap left uncapped (`usize::MAX` —
    /// this must NEVER fire via the cap) and a short real deadline armed. Before `with_timeout`/
    /// `timed_out` existed this test could not even compile (red); after, `over_budget()` must
    /// still catch the deadline — via the per-call wall-clock check (see this module's O1b doc) —
    /// not the step-cap path — well before the artificially huge iteration bound below, and it
    /// must do so promptly (the whole point of a wall-clock bound: elapsed real time stays close
    /// to the deadline, not to "however long N_HUGE iterations would otherwise take").
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
        // Generous upper bound for slow CI machines: comfortably tighter than "ran the full
        // N_HUGE loop" would ever be (that takes at least hundreds of ms even for a no-op loop
        // body, let alone one that calls tick()/over_budget() every iteration), yet loose enough
        // to absorb scheduler jitter around the 30ms deadline.
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

    /// **O1b regression guard.** The bug this module's O1b doc describes: before the fix,
    /// `over_budget()`'s wall-clock branch was gated on `self.steps.get().is_multiple_of(1024)`, so
    /// a run whose total tick count never reaches 1024 only ever sampled the clock ONCE, at
    /// construction — the deadline could then elapse arbitrarily far past without ever being
    /// re-checked. `word_timeout_pathological_gate.rs`'s existing fixture doesn't catch this (its
    /// ~13,699-step run crosses many 1024 boundaries, so the old cadence happened to work there).
    /// This test pins the exact failure shape the old cadence missed: fewer than 1024 ticks total,
    /// each with real wall-clock time elapsing between them (a `sleep`, standing in for a single
    /// expensive (un)application attempt — measured on real Amharic pathological words at ~1-1.5s
    /// per tick, `docs/budget-model.md`'s O1b addendum), with a deadline that elapses partway
    /// through. Red on the pre-fix cadence (200 ticks < 1024, so `over_budget()` only ever sampled
    /// the clock at step 0 — before the sleeps had accrued any real time — and the loop ran to
    /// completion, never firing); green after (every call reads the clock once armed, so the
    /// deadline fires within about one tick of the 50ms mark).
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
/// explicit so M5/tests can pin them.
#[derive(Clone, Copy, Debug)]
pub struct AnalyzerConfig {
    /// C# `Morpher.MergeEquivalentAnalyses` (ctor default `true`, Morpher.cs:110): collapse this
    /// stratum's analysis candidates that share a [`Shape`] into one canonical word, folding the
    /// repeats into its [`Word::alternatives`] (AnalysisStratumRule.cs:150-177). The alternatives are
    /// re-expanded at synthesis by [`super::stratum`]'s caller via [`Word::expand_alternatives`], so
    /// no candidate history is lost — this is a per-stratum de-duplication that keeps only one word
    /// per shape flowing into the deeper strata (the "second Sena perf lever", plan §13.1.1 Tier-2
    /// #14), not a pruning. Default `true`, as in C#.
    pub merge_equivalent: bool,
    /// C# `Morpher.MaxUnapplications`: stop once the analysis output reaches this many candidates
    /// (`0` = unlimited). AnalysisStratumRule.cs:144.
    pub max_unapplications: usize,
    /// C# `Morpher.MaxStemCount` (default `2`, Morpher.cs:108/174): the depth gate on recursive
    /// compounding. `AnalysisCompoundingRule.Apply` (AnalysisCompoundingRule.cs:44-49) refuses to
    /// unapply a compounding rule once `input.NonHeadCount + 1 >= MaxStemCount` — with the default
    /// of 2, at most one non-head may ever be split off, so a word can be de-compounded once, never
    /// recursively re-compounded. Without this gate, a compounding subrule whose head/non-head
    /// patterns are `1+ of any segment` (as both Indonesian compounding rules are) matches every
    /// split point of every substring at every depth — a Catalan-scale blowup in reachable states
    /// that both explodes the memoized cascade's per-node stored subtree size (M6) and, unmemoized,
    /// just burns the whole step budget on meaningless fragments. Enforced in
    /// [`StratumAnalyzer::apply_one_mrule`], mirroring where C# enforces it (the rule's own `Apply`
    /// wrapper), not inside `morph::ana_compound` itself.
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

/// Analyze (unapply) `input` through `stratum` — a faithful port of `AnalysisStratumRule`
/// (AnalysisStratumRule.cs). Returns the deduplicated candidate set; see [`StratumAnalysis`].
///
/// This is the primary M5 entry point: M5's lexical lookup runs this per stratum (deepest first)
/// and matches root allomorphs against each candidate's shape.
pub fn analyze_stratum(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    budget: &StepBudget,
) -> StratumAnalysis {
    // No production call site: only tests reach this entry point, many against hand-built grammars
    // whose `AffixAllomorphDef.id`s are never registered in `Grammar::allomorph_owners` (a `RuleCache`
    // invariant the real XML-loaded grammars maintain but ad hoc test fixtures do not always bother
    // with). Passing `None` keeps this signature — and every existing test's call site — unchanged,
    // by falling all the way back to the pre-cache recompile-every-call behavior. See
    // `crate::cache`'s module doc.
    StratumAnalyzer::new(
        g,
        stratum,
        *cfg,
        None,
        None,
        None,
        None,
        budget,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
    .analyze(input)
}

/// Analyze `input` through `stratum` with the #451 order-invariant memo active (M6). Identical to
/// [`analyze_stratum`] except an [`AnalysisScope`] carries the nogood/positive/template memo across
/// the whole descent — and, because `pg-parse` reuses one scope for every stratum + input word of a
/// single `parse_word`, across the whole parse. Passing `None` (via [`analyze_stratum`]) reproduces
/// the unmemoized engine byte-for-byte — the fair-baseline knob (plan §6.3).
pub fn analyze_stratum_scoped(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    scope: Option<&MemoScope>,
    budget: &StepBudget,
) -> StratumAnalysis {
    // See `analyze_stratum`'s doc: no production call site, and `None` keeps every test call site
    // unchanged (byte-for-byte the pre-cache behavior).
    StratumAnalyzer::new(
        g,
        stratum,
        *cfg,
        scope,
        None,
        None,
        None,
        budget,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
    .analyze(input)
}

/// Identical to [`analyze_stratum_scoped`], plus the compounding non-head root filter
/// (`AnalysisCompoundingRule.Apply`'s root-allomorph-search gate, AnalysisCompoundingRule.cs:61-125
/// — M5c). `pg-parse::Morpher` calls this (wiring in its `RootAllomorphIndex::search`); everything
/// else — `pg-rules`'s own lexicon-free tests included — keeps calling [`analyze_stratum_scoped`] /
/// [`analyze_stratum`], which pass `None` and see every candidate, unfiltered, exactly as before.
///
/// The one entry point of the three (`analyze_stratum`/`_scoped`/`_scoped_filtered`) with a real
/// production call site (`pg-parse::Morpher::parse_word`): `cache` is mandatory here — a `RuleCache`
/// built once at `Morpher` construction and shared across every stratum/candidate of a parse (and
/// across every `--threads=N` worker), the whole point of the compile-once cache (plan §13.2 step
/// 5; see `crate::cache`'s module doc). Its only test caller (`non_head_root_filter_gate.rs`) builds
/// its own `RuleCache` too — safely, since that suite's grammars are compounding-only and never mint
/// an `AllomorphId`, so they are unaffected by the registry caveat [`analyze_stratum`]'s doc raises.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_scoped_filtered(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    scope: Option<&MemoScope>,
    non_head_root_filter: Option<NonHeadRootFilter>,
    cache: &RuleCache,
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

/// Identical to [`analyze_stratum_scoped_filtered`], plus the F1 morphological-rule/template-level
/// selector (`RuleFilter`; see that type's doc for exactly which C# gates it mirrors and which it
/// defers to F5). `None` (via [`analyze_stratum_scoped_filtered`]) is every admitted, byte-identical
/// to every pre-existing caller.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_scoped_filtered_ruled(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    scope: Option<&MemoScope>,
    non_head_root_filter: Option<NonHeadRootFilter>,
    rule_filter: Option<RuleFilter>,
    cache: &RuleCache,
    budget: &StepBudget,
) -> StratumAnalysis {
    StratumAnalyzer::new(
        g,
        stratum,
        *cfg,
        scope,
        non_head_root_filter,
        rule_filter,
        Some(cache),
        budget,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
    .analyze(input)
}

/// 2026-07-17 dead-end-attribution census addition (see `crate::morph`'s analysis-tracing module
/// doc, and [`StratumAnalyzer`]'s `trace`/`parent` fields): [`analyze_stratum_scoped_filtered_ruled`]'s
/// traced sibling — identical in every other respect. The only intended caller is the census's own
/// `pg_parse::Morpher::parse_word_selected_traced` path; every pre-existing entry point above keeps
/// calling the untraced function (or passes `&NoopSink`/`TraceHandle::DUMMY` itself), so this is a
/// new, additive call path, not a change to any existing one.
#[allow(clippy::too_many_arguments)]
pub fn analyze_stratum_scoped_filtered_ruled_traced(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cfg: &AnalyzerConfig,
    scope: Option<&MemoScope>,
    non_head_root_filter: Option<NonHeadRootFilter>,
    rule_filter: Option<RuleFilter>,
    cache: &RuleCache,
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
        Some(cache),
        budget,
        trace,
        parent,
    )
    .analyze(input)
}

/// The stratum orchestrator. Borrows the caller's [`StepBudget`] (see that type's doc) rather than
/// owning its own step counter — one `StratumAnalyzer` no longer means one budget.
struct StratumAnalyzer<'g, 's, 'f, 'r, 'c, 'b, 't> {
    g: &'g Grammar,
    stratum_id: StratumId,
    stratum: &'g pg_grammar::model::StratumDef,
    order: MorphRuleOrder,
    /// The stratum's morphological rules **reversed** — C# `AnalysisStratumRule` does
    /// `stratum.MorphologicalRules.Select(Compile).Reverse()` for *both* Linear and Unordered
    /// (cs:32-34). The cascade indexes this reversed list; the closure maps `i -> reversed[i]` so
    /// the correct `MRuleId` is recorded regardless of order.
    reversed_mrules: Vec<MRuleId>,
    cfg: AnalyzerConfig,
    budget: &'b StepBudget,
    /// The #451 memo (M6), or `None` for the unmemoized baseline. Threaded here rather than through
    /// every recursion argument. See [`analyze_stratum_scoped`].
    scope: Option<&'s MemoScope>,
    /// The M5c non-head lexicon filter, or `None` (unfiltered — every pre-existing caller). See
    /// [`NonHeadRootFilter`] and [`Self::non_head_root_matches`].
    non_head_root_filter: Option<NonHeadRootFilter<'f>>,
    /// F1 (§7.1 item 1): the mrule/template selector, or `None` (every pre-existing caller — every
    /// rule admitted). See [`RuleFilter`]'s doc.
    rule_filter: Option<RuleFilter<'r>>,
    /// The compile-once FST cache (plan §13.2 step 5) — see `crate::cache`'s module doc. `None`
    /// falls all the way back to the pre-cache recompile-every-call behavior (see [`analyze_stratum`]'s
    /// doc for why that's still needed).
    cache: Option<&'c RuleCache>,
    /// 2026-07-17 dead-end-attribution census addition (see `crate::morph`'s analysis-tracing
    /// module doc): the analysis-side trace sink + cursor, threaded exactly like every synthesis-
    /// side function already does (`&NoopSink`/`TraceHandle::DUMMY` for every pre-existing caller —
    /// `analyze_stratum`/`_scoped`/`_scoped_filtered`/`_scoped_filtered_ruled` all pass this
    /// unchanged, so `trace.is_tracing()` is `false` and every gated call below takes its existing
    /// fast path). Only [`analyze_stratum_scoped_filtered_ruled_traced`] passes a real sink.
    trace: &'t dyn TraceSink,
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

    /// F1: `RuleRef::MRule`/`RuleRef::Template` admission — `true` (admit) when no filter was
    /// supplied, matching C#'s `rule => true` default.
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
    /// every recursion entry. Delegates to the shared [`StepBudget`] — see that type's doc.
    fn over_budget(&self) -> bool {
        self.budget.over_budget()
    }

    fn tick(&self) {
        self.budget.tick()
    }

    /// Unapply a single morphological rule and record the C# `Word.MorphologicalRuleUnapplied`
    /// bookkeeping on every output: append the rule id to `mrule_apps` and re-derive the two
    /// dedup-key indices (Word.cs:317-327; analysis only ever grows these lists, so the index is
    /// always `len - 1`). `morph::analyze` is left semantics-pure — the bookkeeping lives here.
    fn apply_one_mrule(&self, id: MRuleId, w: &Word) -> Vec<Word> {
        // F1 (§7.1 item 1): `AnalysisAffixProcessRule.cs:42`/`AnalysisCompoundingRule.cs:42`/
        // `AnalysisRealizationalAffixProcessRule.cs:42`'s shared top-of-`Apply` gate — checked
        // before the budget tick, matching "a rejected-by-gate rule was never attempted" (same
        // placement convention as the `MaxStemCount`/`MaxApplicationCount` gates just below).
        if !self.rule_admitted(RuleRef::MRule(id)) {
            return Vec::new();
        }
        if self.over_budget() {
            return Vec::new();
        }
        let rule = &self.g.mrules[id.0 as usize];
        // `Morpher.MaxStemCount` depth gate (AnalysisCompoundingRule.cs:44-49, Morpher.cs:108/174;
        // see the field doc on `AnalyzerConfig::max_stem_count`). Checked here, outside `tick()`,
        // matching C#'s placement in the rule's own `Apply` wrapper — a rejected-by-gate rule was
        // never "attempted" in the step-budget sense, exactly as C# never enters the loop over
        // subrules at all in this case.
        if matches!(rule, MorphRuleDef::Compounding(_))
            && w.non_heads.len() as u32 + 1 >= self.cfg.max_stem_count
        {
            return Vec::new();
        }
        // `MaxApplicationCount` gate (C# `input.GetUnapplicationCount(_rule) >= _rule.MaxApplicationCount`,
        // identically in `AnalysisAffixProcessRule.cs:46-52` and `AnalysisCompoundingRule.cs:48`):
        // once this rule has already been unapplied on `w`'s trail `max_apps` times, the rule is
        // skipped entirely for this candidate — checked here, outside `tick()`, for the same reason
        // as the `MaxStemCount` gate above (a rejected-by-gate rule was never "attempted"). Applies
        // to both rule kinds uniformly, matching C#'s identical placement in both rule types' `Apply`
        // wrappers.
        if w.unapplied_rule_counts.get(&id).copied().unwrap_or(0) >= u32::from(rule.max_apps()) {
            return Vec::new();
        }
        self.tick();
        // (Plan Tier-2 #7) The non-head root filter (AnalysisCompoundingRule.Apply's root-
        // allomorph-search gate, AnalysisCompoundingRule.cs:61-125) is now threaded straight into
        // `morph::ana_compound`/`ana_compound_cached` rather than post-filtering the returned
        // `Vec<Word>`: the root-allomorph resolution + per-candidate pin must join the same
        // **per-subrule** duplicate-elimination scope C# uses (cs:99-117), which only
        // `ana_compound_subrule` itself has in hand. `None` (`pg-rules`'s own lexicon-free tests,
        // and every `AffixProcess` rule) preserves the pre-#7 unfiltered/unresolved behavior.
        // 2026-07-17 dead-end-attribution census addition: `w.trace.unwrap_or(self.parent)` is the
        // same "resolved cursor" idiom every synthesis-side call site already uses (e.g.
        // `synth_apply_mrules`'s `w_parent`) — `w` carries forward whichever trace handle the
        // DEEPEST successful analysis event along this branch minted, so a chain of successful
        // unapplications nests correctly even though `apply_one_mrule` itself has no separate
        // "begin" event. `_traced` siblings fast-path straight back to the untraced call below when
        // `self.trace.is_tracing()` is false (every pre-existing caller — see `StratumAnalyzer::new`'s
        // doc), so this swap is a no-op on every production path.
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
            // — that only arises from generation seeding a bare non-head directly, W7).
            o.mrule_apps.push(Some(id));
            o.mrule_app_index = o.mrule_apps.len() as i32 - 1;
            // The M6 order-invariant count multiset (C# `Word.MorphologicalRuleUnapplied`), paired
            // with the `mrule_apps.push` above. Consumed only by [`Self::state_key`]; on the
            // unmemoized path it is computed but unread (additive, harmless).
            o.record_unapplication(id);
            // Compounding analysis (`morph::ana_compound`) has already pushed the split-off
            // non-head; C# `NonHeadUnapplied` pairs that push with this index bump.
            o.non_head_app_index = o.non_heads.len() as i32 - 1;
        }
        outs
    }

    /// Run the morphological-rule cascade over the reversed rule list: `PermutationRuleCascade`
    /// (multi-app) for a `Linear` stratum, `CombinationRuleCascade` (multi-app) for `Unordered`
    /// (the SINGLE_THREADED branch — AnalysisStratumRule.cs:42-62). Deduped by the full word key;
    /// run uncapped, with the shared budget enforced inside the closure.
    fn run_mrule_cascade(&self, input: &Word) -> Vec<Word> {
        // The memoized combination walk replaces the plain one only on an Unordered stratum with a
        // scope present (C# `MemoizedCombinationRuleCascade` is scoped to the sequential Unordered
        // cascade — the `k!` walk memoization targets). Linear strata and the no-scope baseline use
        // the plain cascade unchanged.
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

    /// The memoized analog of `Cascade::combination` (multi-app) — a faithful port of
    /// `MemoizedCombinationRuleCascade` (MemoizedCombinationRuleCascade.cs:49-143). Returns the same
    /// deduped reachable set the plain cascade would (results of ≥1 unapplication, not `input`
    /// itself), but memoizes every interior node's subtree by [`AnalysisStateKey`], so a
    /// differently-ordered re-arrival replays instead of re-searching. Memoizing only the top-level
    /// entry (not the interior recursion) would forfeit the win — the redundancy is *inside* the
    /// walk.
    fn mrule_cascade_memoized(&self, input: &Word, scope: &MemoScope) -> Vec<Word> {
        let mut out = OrderedDedup::new();
        self.memo_apply_rules(input, &mut out, scope);
        out.into_items()
    }

    /// C# `MemoizedCombinationRuleCascade.ApplyRules` (cs:62-110): the memo wrapper around one node's
    /// subtree expansion. Returns the subtree-local results (for storage/replay); `out` is the shared
    /// deduped accumulator the caller ultimately reads.
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

        // Positive-replay or nogood hit (cs:73-83): replay each stored result onto this arrival's own
        // trail/non-head prefix. An empty entry (nogood) replays nothing — the subtree yields nothing.
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

        // In-flight re-entry guard (cs:85-91): if this key is already expanding on the stack, fall
        // through to a plain unmemoized expansion for just this arrival (correctness-neutral). In
        // analysis this can never actually fire — every unapplication grows the rule multiset, so a
        // child key always differs from its parent's — but it is ported faithfully.
        let fresh = scope.borrow_mut().in_progress.insert(key.clone());
        if !fresh {
            return self.memo_apply_rules_raw(input, out, scope);
        }

        let results = self.memo_apply_rules_raw(input, out, scope);

        // Clear the guard, then store (nogood or positive) if under the cap (cs:98-108). Prefix
        // lengths = this node's own trail/non-head lengths, so a replay can split each stored result.
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

    /// C# `MemoizedCombinationRuleCascade.ApplyRulesRaw` (cs:112-143): one node's un-memoized
    /// expansion — recurse from rule 0 each level (the combination walk), self-loop-guarded, with the
    /// descent itself memoized via [`Self::memo_apply_rules`]. The Phase-5 `HasReachableRoot` gate
    /// (cs:130-137) is intentionally omitted: the Rust baseline cascade has no such gate, so including
    /// it would break the memo-on == memo-off invariant.
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
                // Self-loop guard (cs:122-123 / CombinationRuleCascade.cs:41). Always false here.
                if in_key == result.dedup_key() {
                    continue;
                }
                local.extend(self.memo_apply_rules(&result, out, scope));
            }
        }
        local
    }

    /// Port of `AnalysisStratumRule.ApplyMorphologicalRules` (cs:150-167): run the mrule cascade,
    /// then per stratum order interleave templates.
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

    /// Port of `AnalysisStratumRule.ApplyTemplates` (cs:169-192): run the template batch, then per
    /// stratum order interleave mrules and yield the template output when it changed the word.
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

    /// The template `RuleBatch` (AnalysisStratumRule.cs:26-30), memoized via the separate
    /// `TemplateMemo` table when a scope is present (AnalysisScope.cs:43-53). The battery is invoked
    /// far more often than there are distinct keys (C#: 38,840 calls vs. ~2,581 keys on atawirambo —
    /// 93% of parse time), so collapsing equal-keyed arrivals to one battery run + replay is the bulk
    /// of the M6 win.
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

    /// Port of `AnalysisAffixTemplateRule.Apply` (AnalysisAffixTemplateRule.cs:34-59): gate on the
    /// template's required syntactic FS via a real `Unify` (cs:40, this narrows), unapply slots
    /// top-down, then **`Add`** — a widening union, not `unify`/`priority_union` — the unified FS
    /// onto each output's syntactic FS (cs:65-67: `sfs.Add(fs)`, never fails).
    fn analyze_template(&self, tid: TemplateId, input: &Word) -> Vec<Word> {
        // F1 (§7.1 item 1): `AnalysisAffixTemplateRule.cs:34`'s top-of-`Apply` gate.
        if !self.rule_admitted(RuleRef::Template(tid)) {
            return Vec::new();
        }
        let tmpl = &self.g.templates[tid.0 as usize];
        let req = self.g.fs_interner.get(tmpl.required_syn_fs);
        // `input.SyntacticFeatureStruct.Unify(RequiredSyntacticFeatureStruct, out fs)` (cs:40).
        if !is_unifiable(&input.syn_fs, req) {
            return Vec::new();
        }
        let fs = unify(&input.syn_fs, req).unwrap_or_else(|| input.syn_fs.clone());
        let mut out: HashMap<WordKey, Word> = HashMap::default();
        // `ApplySlots(inWord, _rules.Count - 1, output)` — descend from the last slot.
        self.template_unapply_slots(tmpl, input, tmpl.slots.len() as isize - 1, &mut out);
        let mut result: Vec<Word> = out.into_values().collect();
        // `outWord.SyntacticFeatureStruct.Add(fs)` (cs:66) — union, not overwrite; see `add`'s doc.
        for w in &mut result {
            w.syn_fs = add(&w.syn_fs, &fs, &|f| self.g.syn_features.mask(f));
        }
        result
    }

    /// Port of `AnalysisAffixTemplateRule.ApplySlots` (cs:62-80): for each slot from `index` down,
    /// unapply its rule batch and recurse into earlier slots; a **non-optional** slot that is
    /// reached forces a `return` (its material had to be consumed here — the path only survives via
    /// the recursion its batch spawned). Falling past slot 0 adds the (fully-unapplied) word.
    fn template_unapply_slots(
        &self,
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
                self.template_unapply_slots(tmpl, &ow, i - 1, out);
            }
            if !slot_optional(slot) {
                return;
            }
            i -= 1;
        }
        out.entry(in_word.dedup_key())
            .or_insert_with(|| in_word.clone());
    }

    /// One slot's `RuleBatch<Word>(slot.Rules, disjunctive: false)` (AnalysisAffixTemplateRule.cs:
    /// 26-30): non-disjunctive union of the slot's alternative rules' analysis outputs, deduped.
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

    /// Port of `AnalysisStratumRule.Apply` (cs:114-186).
    fn analyze(&self, mut input: Word) -> StratumAnalysis {
        // `Word origInput = input; input = input.Clone();` (cs:104-106): every word produced by this
        // stratum records `origInput` as its `Source` (the seed via the clone, cs:106; each mrule
        // output explicitly, cs:165), so `expand_alternatives` can later walk the per-stratum spine.
        // We snapshot the incoming word (WITH whatever alternatives the previous stratum stashed on
        // it) into an `Rc` before mutating, then clear the seed's own alternatives so the cascade
        // below — and every word cloned from the seed — starts alternative-free, exactly as C#'s
        // alternative-dropping copy constructor leaves `input.Clone()`.
        let source = Rc::new(input.clone());
        input.stratum = self.stratum_id;
        input.source = Some(source.clone());
        input.alternatives.clear();

        // `_prulesRule.Apply(input)` — a LinearRuleCascade over the prules **reversed**, applied in
        // place: C# discards the cascade's return value and freezes/uses `input` itself (cs:111).
        // Modelled as an in-place fold: each phonological rule that unapplies replaces the shape.
        // (Zero prules in Sena; untested by any acceptance gate — flagged in the report.)
        //
        // 2026-07-17 dead-end-attribution census addition: swapped for the `_traced` siblings
        // (`rewrite::analyze_cached_traced`/`analyze_traced`, `metathesis::analyze_cached_traced`/
        // `analyze_traced`) — all four already existed, built but unwired (see their own doc
        // comments: "not yet wired into the live per-word pipeline"). Each fast-paths straight back
        // to the untraced call above when `self.trace.is_tracing()` is false, so every pre-existing
        // (production) caller is unaffected. No per-prule cursor advance (`self.parent` used
        // unchanged for every prule in this loop) — see `deadend_census.rs`'s frontier-definition
        // doc for why that coarser depth is an accepted simplification here.
        for &pid in self.stratum.prules.iter().rev() {
            // Fix 2 follow-up: this loop had no budget check at all (unlike every other
            // (un)application site in this module). A deadline-only check (never the step cap --
            // see `StepBudget::deadline_expired`'s doc for why conflating the two here would risk
            // golden parity) closes the same class of gap synthesis's own unguarded prule loop had.
            // `deadline_expired()` is a no-op when `--word-timeout-ms` is not set, so every
            // step-cap-only run (the Sena goldens) takes this loop to completion exactly as before.
            if self.budget.deadline_expired() {
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

        // mruleOutWords = ApplyTemplates(input).Concat(ApplyMorphologicalRules(input)) (cs:158).
        let mut mrule_out = self.apply_templates(&input);
        mrule_out.extend(self.apply_mrules(&input));
        // `mruleOutWord.Source = origInput` (cs:165) — every stratum output points back at the seed.
        for w in &mut mrule_out {
            w.source = Some(source.clone());
        }

        // output = HashSet<Word>(FreezableEquality) { input }  (cs:161); optional shape-merge.
        let mut output_keys: HashMap<WordKey, ()> = HashMap::default();
        // Shape -> index into `words` of the canonical word for that shape. The seed's shape is
        // deliberately NOT registered (C# never puts `input` into `shapeWord`, cs:161-171), so a
        // mrule output that happens to match the seed's shape becomes its own canonical.
        let mut shape_word: HashMap<Shape, usize> = HashMap::default();
        let mut words: Vec<Word> = Vec::new();
        output_keys.insert(input.dedup_key(), ());
        words.push(input);

        for w in mrule_out {
            if self.cfg.merge_equivalent {
                // shapeWord[shape] merge (cs:166-172): a repeat shape does not enter the output;
                // instead the repeat folds into the canonical word's `Alternatives` and is skipped.
                if let Some(&idx) = shape_word.get(&w.shape) {
                    words[idx].alternatives.push(w);
                    continue;
                }
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
/// flag (AffixTemplateSlot.cs:35-45).
fn slot_optional(slot: &SlotDef) -> bool {
    slot.rules.is_empty() || slot.optional
}

// =================================================================================================
// Synthesis affix-template rule (self-contained; the stratum-level synthesis driver below is
// gated on M5 lexicon state and flagged).
// =================================================================================================

/// Port of `SynthesisAffixTemplateRule.Apply` + `ApplySlots` (SynthesisAffixTemplateRule.cs): apply
/// slots **bottom-up** (ascending), a non-optional slot that produced nothing terminating the path.
/// Returns the set of words with the template's slots applied. Independent of M5 lexicon state, so
/// usable now (e.g. an analyze→synthesize template round trip).
///
/// This is the **ungated** slot walk: it applies each slot rule unconditionally (used by round-trip
/// tests that start from a hand-built word with no unapplication history). The M5 pipeline's
/// synthesis instead uses [`synth_apply_templates`] → [`guided_template_apply`], which threads the
/// `IsMorphologicalRuleApplicable` confirmation gate (see [`guided_synth`]).
///
/// `cap` bounds total single-rule application attempts (same safety valve as analysis).
pub fn synthesize_template(g: &Grammar, tid: TemplateId, input: &Word, cap: usize) -> Vec<Word> {
    let tmpl = &g.templates[tid.0 as usize];
    let steps = Cell::new(0usize);
    let mut out: HashMap<WordKey, Word> = HashMap::default();
    let apply =
        |g: &Grammar, rid: MRuleId, w: &Word| morph::synthesize(g, w, &g.mrules[rid.0 as usize]);
    // Fix 2: `synth_slots_generic` now also takes a `&StepBudget` for the wall-clock deadline
    // check (see that function's doc). This standalone/round-trip-test entry point has no natural
    // `parse_word`-scoped deadline to thread in, so it builds its own budget with no timeout armed
    // -- `deadline_expired()` is then a complete no-op, exactly reproducing this function's
    // pre-existing (cap-only) behavior byte-for-byte.
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

/// The M5 pipeline's slot walk: identical structure to [`synthesize_template`] but the per-rule
/// application is the **guided** [`guided_synth`] (confirms `IsMorphologicalRuleApplicable` and
/// decrements `mrule_app_index`). Shares the caller's step budget.
///
/// P12 chunk 5: `BeginApplyTemplate` fires ONCE here, at the top-level `Apply` entry
/// (`SynthesisAffixTemplateRule.cs:29-30`) -- the per-level `EndApplyTemplate` calls (both the
/// `applied=false` early-return and the `applied=true` completion) are recursive and live inside
/// [`synth_slots_generic`] itself (cs:38-53), since C#'s own `ApplySlots` fires them at every
/// recursion depth, not just the outermost one.
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

/// Port of `SynthesisAffixTemplateRule.ApplySlots` (bottom-up), parameterized by the per-rule
/// application function `apply` so the same walk serves the ungated ([`synthesize_template`]) and
/// guided ([`guided_template_apply`]) callers. `apply(g, rule_id, word)` returns the rule's synthesis
/// outputs (empty = did not apply).
///
/// P12 chunk 5: `trace`/`tid`/`parent` mirror C#'s per-recursion-depth `EndApplyTemplate` calls
/// (`SynthesisAffixTemplateRule.cs:38-53`) -- each recursion level's OWN `input` word may fire its
/// own `EndApplyTemplate(false)` (a mandatory slot at this depth had nothing recurse further with,
/// cs:43-47) or, at the walk's natural end (every slot tried/skipped), `EndApplyTemplate(true)`
/// (cs:51-53) -- neither reassigns the word's trace cursor in C# (`TraceManager.cs:204-216`, both
/// bodies just append a sibling under `word.CurrentTrace`), so `input.trace.unwrap_or(parent)` (the
/// SAME resolved fallback idiom used throughout this module) is exactly right at every depth.
/// [`synthesize_template`] (the untraced, standalone-fixture caller) passes [`crate::trace::NoopSink`]
/// and [`TraceHandle::DUMMY`] -- `tid` is passed for real (harmless; `NoopSink` never reads it).
///
/// Fix 2 (synthesis-side `--word-timeout-ms` enforcement): `budget` is consulted via
/// [`StepBudget::deadline_expired`] -- the wall-clock-only half of the budget, never its step-cap
/// (this walk's own `cap`/`steps` remain the sole step-count authority, unchanged from before this
/// fix -- see `deadline_expired`'s doc for why conflating the two would risk golden parity). A
/// `None`-deadline budget (every step-cap-only caller, including [`synthesize_template`]'s own
/// freshly-built one) makes every one of these checks a no-op, so this walk's behavior is
/// byte-for-byte unchanged when `--word-timeout-ms` is not set.
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
    if budget.deadline_expired() {
        return;
    }
    let mut i = index;
    while i < tmpl.slots.len() {
        let slot = &tmpl.slots[i];
        // `RuleBatch<Word>(slot.Rules, disjunctive: false).Apply(input)` in synthesis direction.
        let mut seen: HashMap<WordKey, ()> = HashMap::default();
        for &rid in &slot.rules {
            if steps.get() >= cap {
                return;
            }
            if budget.deadline_expired() {
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
            if trace.is_tracing() {
                let node_parent = input.trace.unwrap_or(parent);
                trace.end_apply_template(node_parent, tid, input, false);
            }
            return;
        }
        i += 1;
    }
    if trace.is_tracing() {
        let node_parent = input.trace.unwrap_or(parent);
        trace.end_apply_template(node_parent, tid, input, true);
    }
    out.entry(input.dedup_key())
        .or_insert_with(|| input.clone());
}

// =================================================================================================
// Synthesis stratum rule (M5b — the confirmation gates completed).
// =================================================================================================

/// Guided single-rule synthesis — the completion of the M4b-partial synthesis path (M5b). This is
/// the faithful port of `SynthesisAffixProcessRule.Apply`'s entry gate + `MorphologicalRuleApplied`
/// bookkeeping (SynthesisAffixProcessRule.cs:43,195 / Word.cs:344-356), which together make
/// synthesis **confirm the analysis**: a rule may re-apply only if it is the current expected rule
/// on the word's unapplication stack, and applying it advances that stack.
///
/// - `IsMorphologicalRuleApplicable(rule)` (Word.cs:237-244): `mrule_app_index >= 0` and the rule at
///   that index equals `id`, OR (Word.cs:243) that slot is C#'s null "unknown compounding rule" and
///   `id` names a `CompoundingRule` — reachable when [`pg_parse`]'s `generate_words` (W7) seeds a
///   bare non-head `LexEntry` directly (the `None` variant of [`Word::mrule_apps`]'s element type;
///   analysis itself never produces one, see that field's doc).
/// - `MorphologicalRuleApplied` (Word.cs:344-356): on success decrement `mrule_app_index`; for a
///   compounding rule also decrement `non_head_app_index`.
///
/// `MaxApplicationCount` gating (SynthesisAffixProcessRule.cs:46) is **not** enforced — it needs the
/// per-word `rule_counts` deferred to M6. The guided index already bounds re-applications to the
/// analysis history length; the only place this can over-generate is a multi-application rule
/// (reduplication). Flagged in the M5b report, not hacked.
/// P12 chunks 4/5 (the applied-event spine, landed together -- see the plan doc's DONE note for why
/// this straddles the chunk 4/5 boundary): on success, fires C#'s `MorphologicalRuleApplied`
/// (`TraceManager.cs`) and reassigns each output's trace cursor to the new node (`Word.CurrentTrace
/// = trace`, Word.cs -- §1.2's cursor-reassignment subtlety), so a LATER event on this candidate's
/// branch nests UNDER this rule's node rather than as a sibling. `subrule_index` is not yet
/// populated (`-1`/absent from this call) -- `morph::synthesize_cached` does not report which
/// allomorph/subrule fired back up to this caller; wiring that through is chunk 4's remaining,
/// not-yet-landed work (the ~27 `FailureReason`-reporting call sites `morph.rs` itself needs).
/// Failed-to-apply attempts (any of the three early returns below, or a zero-output
/// `synthesize_cached` call) are **not yet traced** -- this landing only wires the SUCCESS event,
/// which is what the design's §6 acceptance walkthrough and the "rule sequence a human can follow"
/// bar need; the `MorphologicalRuleNotApplied(reason)` wiring is explicitly deferred, flagged here
/// rather than silently dropped.
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
    // P12 chunk 4: `node_parent` resolves to the correct cursor (the word's own trace handle if
    // already set, else the caller's ambient `parent`) exactly as before, but is now threaded INTO
    // `synthesize_cached_traced` itself rather than applied after the fact -- `synth_affix_cached`/
    // `synth_compound_cached`/`synth_realizational_cached` fire BOTH `MorphologicalRuleApplied` (with
    // the REAL subrule index, closing the `-1` placeholder) and `MorphologicalRuleNotApplied`/
    // `CompoundingRuleNotApplied` (with the matching `FailureReason`, closing fidelity gap #1 --
    // failed-to-apply attempts now render as a `Failed`-reason sibling instead of a childless
    // dead-end) at every one of their own internal gates, setting each successful output's `.trace`
    // directly. This function no longer emits its own `Applied` event -- doing so here AND inside
    // the `synth_*_cached` functions would double-emit.
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
/// `mrules` list or affix-template slots contain `id`. Used only for [`HasRemainingRulesFromStratum`]
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

/// Port of `Word.HasRemainingRulesFromStratum` (Word.cs:246-255): the word still has an unapplied
/// rule pending (`mrule_app_index >= 0`) whose current rule belongs to `stratum` — i.e. it left the
/// stratum without finishing that stratum's re-application (a partial parse). When the pending slot
/// is C#'s null "unknown compounding rule" (cs:251-252: `curRule == null`), C# falls back to
/// `CurrentNonHead != null && CurrentNonHead.Stratum == stratum` — the pending non-head's own
/// stratum stands in for the (not-yet-chosen) compounding rule's stratum.
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

/// Port of `SynthesisStratumRule.Apply` (SynthesisStratumRule.cs:49-92) — M5b-complete.
///
/// Faithful gates now enforced:
/// - **Entry gate** (cs:51): pass the word through unchanged if its root's stratum is shallower than
///   this stratum (`root.Stratum.Depth > stratum.Depth`). Depth = strata index; the word's
///   `stratum` is set to the root entry's stratum by lexical lookup, so `input.stratum` *is*
///   `RootAllomorph.Morpheme.Stratum`. (`RuleSelector` is always the identity in the batch tool.)
/// - **Final gating** (cs:60): only words whose last applied rule was final proceed.
/// - **`HasRemainingRulesFromStratum`** (cs:65): drop words that still owe this stratum a rule.
/// - **Trailing in-place prules** (cs:81) then clear the final flag (cs:82).
///
/// No test calls this directly (only `pg-parse::Morpher::parse_word`'s synthesis pipeline does), so
/// — unlike `analyze_stratum`/`analyze_stratum_scoped` above — `cache` is a required parameter: this
/// is the hot synthesis path the compile-once cache exists for (see `crate::cache`'s module doc).
pub fn synthesize_stratum(
    g: &Grammar,
    stratum: StratumId,
    input: Word,
    cap: usize,
    cache: &RuleCache,
) -> Vec<Word> {
    // Fix 2: `synthesize_stratum_traced` now also takes a `&StepBudget` for the wall-clock
    // deadline check. This untraced entry point has no production call site (only
    // `pg-parse::Morpher`'s synthesis pipeline, which threads its own real per-`parse_word`
    // budget through `synthesize_stratum_traced` directly, calls this path in production) --
    // every existing (test) call site of THIS function builds no timeout, so a freshly
    // constructed no-timeout budget reproduces the pre-fix behavior byte-for-byte
    // (`deadline_expired()` is a no-op when `deadline` is `None`).
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

/// P12 chunks 4/5: [`synthesize_stratum`]'s traced sibling -- the single source of truth both share
/// (`synthesize_stratum` calls this with a [`crate::trace::NoopSink`]). `parent` falls back to
/// `input.trace` at every rule-application site (`guided_synth`), so the caller only needs to pass
/// a real handle once, at the top of the per-word synthesis pipeline (`pg-parse::Morpher`'s
/// `synthesis_pipeline`) -- every deeper call reads the cursor off the `Word` itself, exactly
/// mirroring C#'s `Word.CurrentTrace`.
///
/// P12 chunk 5: this function's body is `SynthesisStratumRule.Apply` (`SynthesisStratumRule.cs:
/// 48-90`) itself, not just `synth_apply_mrules`/`synth_apply_templates`'s caller -- wires
/// `BeginApplyStratum`/`NonFinalTemplateAppliedLast`/`Failed(PartialParse)`/`EndApplyStratum`
/// (twice: per surviving candidate, cs:83-84, AND once more for the whole call when NO candidate
/// survives, cs:88-89) at exactly C#'s call sites. `BeginApplyStratum` is a marker sibling event in
/// C# (`void` return, never reassigns `input.CurrentTrace`) -- so, unlike a rule-Applied event, its
/// return handle is discarded here; every subsequent nesting point is still `word.trace.unwrap_or
/// (node_parent)` per word, exactly the existing idiom `guided_synth` already established.
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
    // Entry gate (cs:51). RuleSelector is the identity in the batch tool. No C# trace call at this
    // gate either (a bare `return input.ToEnumerable()`), so this stays untraced, matching C#.
    if (input.stratum.0 as usize) > (stratum.0 as usize) {
        return vec![input];
    }

    let sd = &g.strata[stratum.0 as usize];
    let steps = Cell::new(0usize);

    let node_parent = input.trace.unwrap_or(parent);
    if trace.is_tracing() {
        trace.begin_apply_stratum(node_parent, stratum, &input);
    }

    // ApplyMorphologicalRules(input).Concat(ApplyTemplates(input)) (cs:58).
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
        // cs:60: only words whose last applied rule was final proceed.
        if w.flags.is_last_applied_rule_final != Some(true) {
            if trace.is_tracing() {
                trace.non_final_template_applied_last(w_parent, stratum, &w);
            }
            continue;
        }
        // cs:65: drop partial parses that still owe this stratum a rule.
        if has_remaining_rules_from_stratum(g, &w, stratum) {
            if trace.is_tracing() {
                trace.failed(w_parent, &w, FailureReason::PartialParse);
            }
            continue;
        }
        let mut nw = w.clone();
        // Trailing in-place prule application (cs:81), then clear the final flag (cs:82).
        // `synthesize_with_mpr_cached` (not the bare `synthesize`): a subrule's
        // `requiredPartsOfSpeech`/`excludedMPRFeatures`/`requiredMPRFeatures` gate
        // (`SynthesisRewriteSubruleSpec.IsApplicable`) must see this word's actual syntactic FS and
        // MPR set, not assumed-empty ones — see that function's doc for why the MPR half mattered
        // for Indonesian's `meN-` prefix (`prule5`'s `excludedMPRFeatures="mpr1"`) and the POS half
        // for Amharic's 3 `requiredPartsOfSpeech`-gated subrules (plan item 1 / wave-3).
        // P12 chunk 6: `_traced` siblings (`rewrite::synthesize_with_mpr_cached_traced`/
        // `metathesis::synthesize_cached_traced`) replace the untraced calls this loop used to make
        // -- both take the real `&nw` (not a bare `Shape`/`syn_fs`/`mpr` triple) so
        // `PhonologicalRuleApplied`/`PhonologicalRuleNotApplied` can fire with `nw`'s actual full
        // state, mirroring every other traced call site in this function (`w_parent` is already the
        // resolved cursor for this candidate). Each function's own `!trace.is_tracing()` fast path
        // delegates straight back to the untraced body, so this is a no-op when tracing is off.
        //
        // Fix 2: this loop previously had NO budget check at all -- the exact gap the bug report
        // ("synthesis never checks the deadline") describes, since every synthesis (un)application
        // above it (`synth_apply_mrules`/`synth_apply_templates`/`guided_template_apply`/
        // `synth_slots_generic`) now consults `budget.deadline_expired()` but this trailing
        // in-place prule fold did not. `break` (not `return`) mirrors the analysis-side sibling
        // gap's fix just above `StratumAnalyzer::analyze`'s own prule loop: leave `nw.shape`
        // however far the fold got and fall through to this candidate's existing bookkeeping,
        // rather than unwinding the whole function. A `None` deadline (`--word-timeout-ms` unset)
        // makes this a no-op, so every step-cap-only run is unaffected.
        for &pid in &sd.prules {
            if budget.deadline_expired() {
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
    if budget.deadline_expired() {
        return Vec::new();
    }
    let key = |w: &Word| w.dedup_key();
    // The guided cascade: each rule applies only if it is the word's current expected unapplication
    // (SynthesisAffixProcessRule.cs:43). Because `guided_synth` strictly decrements
    // `mrule_app_index` on every application, the multi-application cascade terminates once the stack
    // is exhausted — the synthesis analog of the analysis step budget.
    //
    // `trace`/`parent` are `Copy` (a fat-pointer shared reference and a small handle,
    // respectively), so capturing them here costs nothing and does not fight the cascade's `Fn`
    // (not `FnMut`) bound — see `crate::trace`'s module doc for why `TraceSink` takes `&self`.
    //
    // Fix 2: `budget.deadline_expired()` (wall-clock-only, see that method's doc) is consulted
    // alongside the pre-existing `steps.get() >= cap` local-cap check, at the same granularity --
    // every single-rule application attempt. This is the loop the "kaminuʼatpuza ran 70+s past a
    // 10s --word-timeout-ms" bug report traced to: a `--word-timeout-ms` deadline could elapse here
    // and this closure would keep being called by the cascade regardless, since neither `cap` (a
    // step COUNT) nor the cascade's own internal `usize::MAX` cap had any wall-clock notion at all.
    let apply_rule = |i: usize, w: &Word| -> Vec<Word> {
        if steps.get() >= cap {
            return Vec::new();
        }
        if budget.deadline_expired() {
            return Vec::new();
        }
        steps.set(steps.get() + 1);
        guided_synth(g, sd.mrules[i], w, cache, trace, parent)
    };
    let casc = Cascade::new(true, usize::MAX);
    let n = sd.mrules.len();
    // Synthesis mrules are in declaration order (no reverse): LinearRuleCascade (Linear) /
    // CombinationRuleCascade (Unordered) — SynthesisStratumRule.cs:27-40.
    let cascade_out = match sd.mrule_order {
        MorphRuleOrder::Linear => casc.linear(n, input.clone(), &apply_rule, &key),
        MorphRuleOrder::Unordered => casc.combination(n, input.clone(), &apply_rule, &key),
    };
    let mut result = Vec::new();
    for w in cascade_out.words {
        // cs:98-106: a final word yields directly; else run templates on it.
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

/// Whether `word`'s root morpheme is flagged partial (C# `input.RootAllomorph.Morpheme.IsPartial`,
/// SynthesisAffixTemplatesRule.cs:40). C# reads the root *entry*'s `IsPartial` here — distinct from
/// `Word.IsPartial`, which is initialized from the same flag (Word.cs:168) but can additionally be
/// set true by a partial affix rule (SynthesisAffixProcessRule.cs:196). A word with no resolved root
/// (never happens on the synthesis path, but defensive) is treated as non-partial.
///
/// P11 correction (§4.4-1's audit claimed this "unreachable for a guessed root" — confirmed WRONG
/// empirically: this runs unconditionally at the top of every `synth_apply_templates` call,
/// including on a guessed root with zero further affixes, `g.allomorph_owners[u32::MAX]` panics
/// without this guard, reproduced by `AnalyzeWord_CanGuess_ReturnsCorrectAnalysis`'s "gag" case
/// alone). For the sentinel, resolve the STRICT analog of `g.entries[le].partial` — the guessed
/// root's fabricated entry's own `IsPartial`, which §1.3 step 6 sets to `patternEntry.IsPartial`
/// (copied verbatim from the pattern's owning entry) — via `Word::guessed_root.pattern_entry`,
/// a REAL `LexEntryId` (only the fabricated root itself is sentinel-identified, never the pattern
/// it came from).
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

/// `SynthesisAffixTemplatesRule.ChooseInflectionalStem` (cs:83-115, W5). If `input`'s root
/// morpheme belongs to a `<Family>` and `input.real_fs` is non-empty, walk the family (document
/// order) for the most specific applicable relative: `best` starts as `input` itself and is
/// swapped whenever a later relative `rel`'s own lexical syntactic FS is (a) `real_fs`-unifiable,
/// (b) subsumed by `best`'s CURRENT syntactic FS (so `rel` is at least as specific as whatever
/// `best` is so far — this is why the comparison must be against `best`, which can itself have
/// already been swapped earlier in the walk, not against the original `input`), and (c) its
/// "remainder" over `best` — what `rel` adds beyond `best`'s current FS (`subtract`) — is
/// non-empty and itself `real_fs`-unifiable (so the extra specificity is something the
/// realizational features actually select, not just incidental). Each swap reseeds via
/// [`crate::morph::seed_from_entry`] (C# `new Word(relative.PrimaryAllomorph, input.
/// RealizationalFeatureStruct.Clone())` — note `input`'s `real_fs`, not `rel`'s own FS, since a
/// bare `LexEntry` has no realizational-FS concept).
///
/// P11 correction (§4.4-1's audit claimed this "unreachable for a guessed root" — confirmed WRONG
/// empirically: `g.allomorph_owners[u32::MAX]` panics without this guard, reproduced by
/// `AnalyzeWord_CanGuess_ReturnsCorrectAnalysis`'s "gag" case alone, since this runs
/// unconditionally at the top of `synth_apply_templates`). The `None`/no-swap answer this guard
/// returns is also the FAITHFUL one, not just a safe default: C#'s `LexicalGuess` fabrication
/// (`Morpher.cs:564-579`) never sets the fabricated `LexEntry.Family` at all (only
/// `MorphemeCoOccurrenceRules`/`Properties`/`Stratum`/`MprFeatures`/`SyntacticFeatureStruct`/
/// `IsPartial` are copied from the pattern's owning entry) — so a guessed root's fabricated entry
/// has NO family in C# either, and `ChooseInflectionalStem` would see `Family == null` there too
/// and return `input` unchanged, exactly what this guard does.
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
    if budget.deadline_expired() {
        return Vec::new();
    }
    // SynthesisAffixTemplatesRule.Apply (cs:35-77): try each applicable template; mark each output
    // with `IsLastAppliedRuleFinal = outWord.IsPartial || template.IsFinal`. Slot rules go through
    // the guided walk (confirmation gate).
    //
    // W5 cs:29-30: `!input.RealizationalFeatureStruct.IsUnifiable(input.SyntacticFeatureStruct)`
    // rejects outright, BEFORE `ChooseInflectionalStem` even runs (checked against the word as
    // handed in, not the swapped stem).
    if !is_unifiable(&input.real_fs, &input.syn_fs) {
        return Vec::new();
    }
    // W5 cs:34: `input = ChooseInflectionalStem(input)` rebinds `input` itself in C# — every use
    // below (the "changed since input" comparison, the partial-root check, the template gates, the
    // no-template passthrough) reads the POST-swap word, so this shadows the parameter the same way.
    let input = choose_inflectional_stem(g, input);
    let input = &input;
    let in_key = input.dedup_key();
    let mut out: HashMap<WordKey, Word> = HashMap::default();
    // Tier-2 #13, gate 2: a template applies only if the root morpheme is not partial
    // (SynthesisAffixTemplatesRule.cs:40). Because the root does not change across templates, hoist
    // the check; when the root is partial no template is applicable and the passthrough below fires.
    let root_partial = root_is_partial(g, input);
    let mut applicable = false;
    for &tid in &sd.templates {
        let tmpl = &g.templates[tid.0 as usize];
        let req = g.fs_interner.get(tmpl.required_syn_fs);
        // cs:37-41: RuleSelector (identity here) && IsUnifiable(required) && !root.IsPartial.
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
    // cs:59-77: no template output. Tier-2 #13, gate 1: C# passes the input through (marked final)
    // *unless* the input is non-partial AND some template was applicable — in which case the
    // applicable-but-unapplied word is dropped (C# only traces `ApplicableTemplatesNotApplied`).
    // The previous Rust gated the passthrough on `!applicable` alone, so a partial word with an
    // applicable template was wrongly dropped instead of passed through.
    //
    // P12 chunk 9 follow-up: this passthrough insertion must happen BEFORE the "recurse mrules on
    // differing template output" step below, not after. `out` here is the direct Rust analog of
    // C#'s `_templatesRule.Apply(input)` (`SynthesisAffixTemplatesRule.Apply`, cs:25-78) — that
    // C# method's OWN return value already includes this exact passthrough candidate (its
    // cs:64-74 else-branch), so by the time `SynthesisStratumRule.ApplyTemplates` (cs:110-130)
    // iterates "every tempOutWord in _templatesRule.Apply(input)" to decide whether to recurse
    // `ApplyMorphologicalRules` on each one, the passthrough word is just as much a candidate as a
    // genuine template hit. The previous code ran the recursion loop first (over a snapshot of
    // `out` that could not yet contain the passthrough, since the passthrough is only computed
    // afterward), so a stratum with NO templates at all -- true of every stratum in
    // `indonesian-hc.xml`, which declares zero `<AffixTemplate>` elements -- silently skipped this
    // recursion on EVERY call: `applicable` stays `false`, `out` starts (and, absent this reorder,
    // stays) empty, so `templated` was always `[]`. Confirmed empirically: a live C# trace for
    // `membaca`/`baca` shows the "meN" synthesis rule and its whole downstream phonological-rule
    // subtree applied TWICE under `Stratum Synthesis In [Stratum: Morphology]` -- once via the
    // direct `ApplyMorphologicalRules(input)` cascade, once via this exact passthrough-then-recurse
    // path -- both fully traced down to `Stratum Synthesis Out`, with only the SECOND attempt's
    // final word then silently dropped by `SynthesisStratumRule.Apply`'s own `output.Add(newWord)`
    // HashSet (`SynthesisStratumRule.cs:86`) because it structurally duplicates the first attempt's
    // `mem+baca` -- i.e. C# does NOT dedup these two attempts against each other before recursing;
    // it explores (and traces) both, and only the LAST HashSet-add step (after all trace events
    // already fired) merges them. This was a genuine control-flow gap, not merely a
    // tracing/dedup-interaction artifact as first hypothesized: reordering restores the exploration
    // C# always performs, matching the shape of C#'s own algorithm rather than a
    // tracing-only special case (contrast the analysis-side `merge_equivalent`/`AnalysisScope`
    // fix, which IS tracing-gated because C#'s own guards there are literally
    // `&& !TraceManager.IsTracing`; here C# has no such guard at all -- the doubled exploration
    // happens unconditionally, tracing or not).
    if out.is_empty() {
        if input.flags.is_partial || !applicable {
            let mut w = input.clone();
            if w.flags.is_last_applied_rule_final != Some(true) {
                w.flags.is_last_applied_rule_final = Some(true);
            }
            out.insert(w.dedup_key(), w);
        } else if trace.is_tracing() {
            // P12 chunk 5: the complementary branch (`!is_partial && applicable`,
            // `SynthesisAffixTemplatesRule.cs:59-63`) -- C# traces `ApplicableTemplatesNotApplied`
            // and adds NOTHING to `output` (unlike the passthrough branch above), so this word is
            // correctly dropped here too, just with a trace event recorded first.
            let node_parent = input.trace.unwrap_or(parent);
            trace.applicable_templates_not_applied(node_parent, stratum, input);
        }
    }

    match sd.mrule_order {
        MorphRuleOrder::Linear => {}
        MorphRuleOrder::Unordered => {
            // cs:120-126: for each changed template output (now INCLUDING the passthrough word
            // inserted just above, when present), also run the mrules on it.
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
