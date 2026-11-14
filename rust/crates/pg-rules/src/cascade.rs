//! Rule cascades (M4b) — faithful ports of `src/SIL.Machine/Rules/{Linear,Permutation,
//! Combination}RuleCascade.cs` + their base `RuleCascade.cs`.
//!
//! A cascade drives repeated rule application to a fixpoint, accumulating every reachable
//! result into a deduplicated set. HermitCrab's analysis stratum uses:
//! - `LinearRuleCascade` for phonological rules (fixed order),
//! - `PermutationRuleCascade` (multi-application) for `Linear` morphological-rule strata, and
//! - `CombinationRuleCascade` (multi-application) for `Unordered` strata — the k!-walk over rule
//!   subsets the plan's memoization (M6) later collapses. The `Parallel*` variants are NOT ported
//!   (plan §7: within-word parallelism has no Rust descendant; the sequential Combination cascade
//!   is the SINGLE_THREADED branch of `AnalysisStratumRule`).
//!
//! ## Generic over the data + a dedup key
//! The C# classes are generic over `TData` with an `IEqualityComparer<TData>`. Here the cascade is
//! generic over `T` (the in-flight datum, e.g. `crate::word::Word`) with:
//! - `apply_rule: Fn(usize, &T) -> Vec<T>` — C# `RuleCascade.ApplyRule(rule, index, input)` (the
//!   default `rule.Apply(input)`; the stratum passes a closure dispatching to the compiled rule),
//! - `key: Fn(&T) -> K` where `K: Hash + Eq` — C# `IEqualityComparer<TData>` (`FreezableEquality`),
//!   used both to dedup the output set and for the `input == result` self-loop guard.
//!
//! ## Invocation cap (M4b addition; not in C#)
//! The multi-application cascades terminate only when rules stop producing *new* (non-input-equal)
//! results. Unmemoized on real grammars that can be a 158k-expansion k!-walk (the exact cost M6
//! memoizes). A `Cascade::cap` on total `apply_rule` invocations makes tests/gates terminate,
//! returning partial results (C# has no such cap — it relies on `MaxUnapplications` bounding the
//! stratum's output count instead). The cap is a safety valve, not a semantic change: when it does
//! not fire, output is byte-identical to the uncapped C# walk.

use std::any::Any;
use std::hash::Hash;

use rustc_hash::FxHashMap as HashMap;

/// Cascade configuration + the shared accumulator threaded through the recursion.
pub struct Cascade<'a> {
    /// C# `MultipleApplication`: a rule may re-apply to its own output (recurse at the same index
    /// / no applied-rule blocking) rather than only to later rules.
    pub multi_app: bool,
    /// Max total `apply_rule` invocations before the walk bails with partial results (see module
    /// docs). `usize::MAX` = uncapped (C# behavior).
    pub cap: usize,
    work_owner: Option<CascadeWorkOwner<'a>>,
}

struct CascadeWorkOwner<'a> {
    stats: &'a crate::stats::StatsCollector,
    stratum: pg_grammar_model::model::StratumId,
    rules: &'a [pg_grammar_model::model::MRuleId],
    direction: crate::stats::Direction,
}

/// Accumulated output: a dedup set keyed by `K`, preserving first-seen insertion order.
struct Acc<T, K: Hash + Eq> {
    seen: HashMap<K, usize>,
    items: Vec<T>,
    steps: usize,
    /// Set once the cap fires so the caller can report/log truncation (plan: "no silent caps").
    capped: bool,
    memory: pg_fst::work::WordMemory,
}

impl<T: 'static, K: Hash + Eq> Acc<T, K> {
    fn new() -> Self {
        Acc {
            seen: HashMap::default(),
            items: Vec::new(),
            steps: 0,
            capped: false,
            memory: pg_fst::work::WordMemory::active(),
        }
    }

    /// C# `HashSet<TData>.Add` under the comparer: insert iff no equal element is present.
    fn add(&mut self, key: K, item: T) {
        if let std::collections::hash_map::Entry::Vacant(e) = self.seen.entry(key) {
            if let Some(word) = (&item as &dyn Any).downcast_ref::<crate::word::Word>() {
                if !self.memory.charge(crate::word::retained_word_bytes(word)) {
                    self.capped = true;
                    return;
                }
            }
            e.insert(self.items.len());
            self.items.push(item);
        }
    }
}

/// The result of a cascade run: the deduplicated reachable set and whether the work cap fired.
pub struct CascadeOutput<T> {
    pub words: Vec<T>,
    pub capped: bool,
    pub(crate) memory: pg_fst::work::WordMemory,
}

impl<'a> Cascade<'a> {
    pub fn new(multi_app: bool, cap: usize) -> Self {
        Cascade {
            multi_app,
            cap,
            work_owner: None,
        }
    }

    /// Attribute each successful cascade reservation to the rule it is about to try.
    pub fn with_rule_work_owner(
        mut self,
        stats: Option<&'a crate::stats::StatsCollector>,
        stratum: pg_grammar_model::model::StratumId,
        rules: &'a [pg_grammar_model::model::MRuleId],
        direction: crate::stats::Direction,
    ) -> Self {
        self.work_owner = stats.map(|stats| CascadeWorkOwner {
            stats,
            stratum,
            rules,
            direction,
        });
        self
    }

    fn consume_attempt(&self, index: usize) -> bool {
        let _owner = self.work_owner.as_ref().map(|owner| {
            owner.stats.time_enter(
                crate::stats::ObjectKind::MorphRule,
                owner.stratum,
                owner.rules[index].0,
                crate::stats::ALLOMORPH_NONE,
                owner.direction,
            )
        });
        pg_fst::work::consume()
    }

    /// Port of `LinearRuleCascade.Apply` (LinearRuleCascade.cs:25-56).
    pub fn linear<T, K, A, F>(
        &self,
        rule_count: usize,
        input: T,
        apply_rule: &A,
        key: &F,
    ) -> CascadeOutput<T>
    where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        self.linear_at(input, &|_| 0..rule_count, apply_rule, key)
    }

    /// `linear` where `positions` names, per input, the ascending rule indices worth attempting; the rest are never attempted or charged.
    pub fn linear_at<T, K, A, F, C, I>(
        &self,
        input: T,
        positions: &C,
        apply_rule: &A,
        key: &F,
    ) -> CascadeOutput<T>
    where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
        C: Fn(&T) -> I,
        I: IntoIterator<Item = usize>,
    {
        let mut acc = Acc::new();
        self.linear_rec(&input, 0, positions, apply_rule, key, &mut acc);
        CascadeOutput {
            words: acc.items,
            capped: acc.capped,
            memory: acc.memory,
        }
    }

    /// `LinearRuleCascade.ApplyRules` -- returns `true` iff no rule applied at or after `rule_index`.
    #[allow(clippy::too_many_arguments)]
    fn linear_rec<T, K, A, F, C, I>(
        &self,
        input: &T,
        rule_index: usize,
        positions: &C,
        apply_rule: &A,
        key: &F,
        acc: &mut Acc<T, K>,
    ) -> bool
    where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
        C: Fn(&T) -> I,
        I: IntoIterator<Item = usize>,
    {
        for i in positions(input).into_iter().filter(|&i| i >= rule_index) {
            if acc.steps >= self.cap || !self.consume_attempt(i) {
                acc.capped = true;
                return false;
            }
            acc.steps += 1;
            let results = apply_rule(i, input);
            let mut applied = false;
            for result in results {
                // avoid infinite loop (LinearRuleCascade.cs:40)
                if !self.multi_app || key(input) != key(&result) {
                    let next = if self.multi_app { i } else { i + 1 };
                    if self.linear_rec(&result, next, positions, apply_rule, key, acc) {
                        acc.add(key(&result), result);
                    }
                } else {
                    acc.add(key(&result), result);
                }
                applied = true;
            }
            if applied {
                return false;
            }
        }
        true
    }

    /// Port of `PermutationRuleCascade.Apply` (PermutationRuleCascade.cs:25-44): index-ordered
    /// subsets, every intermediate result added.
    pub fn permutation<T, K, A, F>(
        &self,
        rule_count: usize,
        input: T,
        apply_rule: &A,
        key: &F,
    ) -> CascadeOutput<T>
    where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        self.permutation_prepared(
            rule_count,
            input,
            &|_, _, _| (),
            &|i, w, &()| apply_rule(i, w),
            key,
        )
    }

    pub fn permutation_prepared<T, K, P, A, F, B>(
        &self,
        rule_count: usize,
        input: T,
        prepare: &B,
        apply_rule: &A,
        key: &F,
    ) -> CascadeOutput<T>
    where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T, &P) -> Vec<T>,
        F: Fn(&T) -> K,
        B: Fn(&T, usize, Option<&[bool]>) -> P,
    {
        let mut acc = Acc::new();
        self.permutation_rec(rule_count, &input, 0, prepare, apply_rule, key, &mut acc);
        CascadeOutput {
            words: acc.items,
            capped: acc.capped,
            memory: acc.memory,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn permutation_rec<T, K, P, A, F, B>(
        &self,
        rule_count: usize,
        input: &T,
        rule_index: usize,
        prepare: &B,
        apply_rule: &A,
        key: &F,
        acc: &mut Acc<T, K>,
    ) where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T, &P) -> Vec<T>,
        F: Fn(&T) -> K,
        B: Fn(&T, usize, Option<&[bool]>) -> P,
    {
        let prepared = prepare(input, rule_index, None);
        for i in rule_index..rule_count {
            if acc.steps >= self.cap || !self.consume_attempt(i) {
                acc.capped = true;
                return;
            }
            acc.steps += 1;
            for result in apply_rule(i, input, &prepared) {
                // avoid infinite loop (PermutationRuleCascade.cs:39)
                if !self.multi_app || key(input) != key(&result) {
                    let next = if self.multi_app { i } else { i + 1 };
                    self.permutation_rec(rule_count, &result, next, prepare, apply_rule, key, acc);
                }
                acc.add(key(&result), result);
            }
        }
    }

    /// Port of `CombinationRuleCascade.Apply` (CombinationRuleCascade.cs:25-54): recurse from rule
    /// 0 each level, skipping already-applied rules (tracked in `applied` — `None` when
    /// `multi_app`, so nothing is blocked). This is the unordered k!-walk over rule subsets.
    pub fn combination<T, K, A, F>(
        &self,
        rule_count: usize,
        input: T,
        apply_rule: &A,
        key: &F,
    ) -> CascadeOutput<T>
    where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        self.combination_at(
            rule_count,
            input,
            &|_| 0..rule_count,
            &|_, _, _| (),
            &|i, w, &()| apply_rule(i, w),
            key,
        )
    }

    /// `combination_prepared` where `positions` names, per input, the rule indices worth attempting (ascending); the rest are never attempted or charged.
    pub fn combination_at<T, K, P, A, F, B, C, I>(
        &self,
        rule_count: usize,
        input: T,
        positions: &C,
        prepare: &B,
        apply_rule: &A,
        key: &F,
    ) -> CascadeOutput<T>
    where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T, &P) -> Vec<T>,
        F: Fn(&T) -> K,
        B: Fn(&T, usize, Option<&[bool]>) -> P,
        C: Fn(&T) -> I,
        I: IntoIterator<Item = usize>,
    {
        let mut acc = Acc::new();
        // C#: `!MultipleApplication ? new HashSet<int>() : null`.
        let applied: Option<Vec<bool>> = if self.multi_app {
            None
        } else {
            Some(vec![false; rule_count])
        };
        self.combination_rec(
            &input, applied, positions, prepare, apply_rule, key, &mut acc,
        );
        CascadeOutput {
            words: acc.items,
            capped: acc.capped,
            memory: acc.memory,
        }
    }

    pub fn combination_prepared<T, K, P, A, F, B>(
        &self,
        rule_count: usize,
        input: T,
        prepare: &B,
        apply_rule: &A,
        key: &F,
    ) -> CascadeOutput<T>
    where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T, &P) -> Vec<T>,
        F: Fn(&T) -> K,
        B: Fn(&T, usize, Option<&[bool]>) -> P,
    {
        self.combination_at(
            rule_count,
            input,
            &|_| 0..rule_count,
            prepare,
            apply_rule,
            key,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn combination_rec<T, K, P, A, F, B, C, I>(
        &self,
        input: &T,
        applied: Option<Vec<bool>>,
        positions: &C,
        prepare: &B,
        apply_rule: &A,
        key: &F,
        acc: &mut Acc<T, K>,
    ) where
        T: Clone + 'static,
        K: Hash + Eq,
        A: Fn(usize, &T, &P) -> Vec<T>,
        F: Fn(&T) -> K,
        B: Fn(&T, usize, Option<&[bool]>) -> P,
        C: Fn(&T) -> I,
        I: IntoIterator<Item = usize>,
    {
        let prepared = prepare(input, 0, applied.as_deref());
        for i in positions(input) {
            if applied.as_ref().is_none_or(|a| !a[i]) {
                if acc.steps >= self.cap || !self.consume_attempt(i) {
                    acc.capped = true;
                    return;
                }
                acc.steps += 1;
                for result in apply_rule(i, input, &prepared) {
                    // avoid infinite loop (CombinationRuleCascade.cs:41)
                    if key(input) != key(&result) {
                        let next = applied.as_ref().map(|a| {
                            let mut a2 = a.clone();
                            a2[i] = true;
                            a2
                        });
                        self.combination_rec(
                            &result, next, positions, prepare, apply_rule, key, acc,
                        );
                    }
                    acc.add(key(&result), result);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
