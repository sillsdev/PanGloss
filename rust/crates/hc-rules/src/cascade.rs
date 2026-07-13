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
//! generic over `T` (the in-flight datum, e.g. [`crate::word::Word`]) with:
//! - `apply_rule: Fn(usize, &T) -> Vec<T>` — C# `RuleCascade.ApplyRule(rule, index, input)` (the
//!   default `rule.Apply(input)`; the stratum passes a closure dispatching to the compiled rule),
//! - `key: Fn(&T) -> K` where `K: Hash + Eq` — C# `IEqualityComparer<TData>` (`FreezableEquality`),
//!   used both to dedup the output set and for the `input == result` self-loop guard.
//!
//! ## Step cap (M4b addition; not in C#)
//! The multi-application cascades terminate only when rules stop producing *new* (non-input-equal)
//! results. Unmemoized on real grammars that can be a 158k-expansion k!-walk (the exact cost M6
//! memoizes). A [`Cascade::cap`] on total `apply_rule` invocations makes tests/gates terminate,
//! returning partial results (C# has no such cap — it relies on `MaxUnapplications` bounding the
//! stratum's output count instead). The cap is a safety valve, not a semantic change: when it does
//! not fire, output is byte-identical to the uncapped C# walk.

use std::hash::Hash;

use rustc_hash::FxHashMap as HashMap;

/// Cascade configuration + the shared accumulator threaded through the recursion.
pub struct Cascade {
    /// C# `MultipleApplication`: a rule may re-apply to its own output (recurse at the same index
    /// / no applied-rule blocking) rather than only to later rules.
    pub multi_app: bool,
    /// Max total `apply_rule` invocations before the walk bails with partial results (see module
    /// docs). `usize::MAX` = uncapped (C# behavior).
    pub cap: usize,
}

/// Accumulated output: a dedup set keyed by `K`, preserving first-seen insertion order so callers
/// that later sort (the signature path) see a deterministic pre-sort sequence.
struct Acc<T, K: Hash + Eq> {
    seen: HashMap<K, usize>,
    items: Vec<T>,
    steps: usize,
    /// Set once the cap fires so the caller can report/log truncation (plan: "no silent caps").
    capped: bool,
}

impl<T, K: Hash + Eq> Acc<T, K> {
    fn new() -> Self {
        Acc {
            seen: HashMap::default(),
            items: Vec::new(),
            steps: 0,
            capped: false,
        }
    }

    /// C# `HashSet<TData>.Add` under the comparer: insert iff no equal element is present.
    fn add(&mut self, key: K, item: T) {
        if let std::collections::hash_map::Entry::Vacant(e) = self.seen.entry(key) {
            e.insert(self.items.len());
            self.items.push(item);
        }
    }
}

/// The result of a cascade run: the deduplicated reachable set and whether the step cap fired.
pub struct CascadeOutput<T> {
    pub words: Vec<T>,
    pub capped: bool,
}

impl Cascade {
    pub fn new(multi_app: bool, cap: usize) -> Self {
        Cascade { multi_app, cap }
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
        T: Clone,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        let mut acc = Acc::new();
        self.linear_rec(rule_count, &input, 0, apply_rule, key, &mut acc);
        CascadeOutput {
            words: acc.items,
            capped: acc.capped,
        }
    }

    /// `LinearRuleCascade.ApplyRules` — returns `true` iff no rule applied at or after `rule_index`
    /// (a terminal derivation, which is what gets added to the output; LinearRuleCascade.cs:32-56).
    #[allow(clippy::too_many_arguments)]
    fn linear_rec<T, K, A, F>(
        &self,
        rule_count: usize,
        input: &T,
        rule_index: usize,
        apply_rule: &A,
        key: &F,
        acc: &mut Acc<T, K>,
    ) -> bool
    where
        T: Clone,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        for i in rule_index..rule_count {
            if acc.steps >= self.cap {
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
                    if self.linear_rec(rule_count, &result, next, apply_rule, key, acc) {
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
        T: Clone,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        let mut acc = Acc::new();
        self.permutation_rec(rule_count, &input, 0, apply_rule, key, &mut acc);
        CascadeOutput {
            words: acc.items,
            capped: acc.capped,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn permutation_rec<T, K, A, F>(
        &self,
        rule_count: usize,
        input: &T,
        rule_index: usize,
        apply_rule: &A,
        key: &F,
        acc: &mut Acc<T, K>,
    ) where
        T: Clone,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        for i in rule_index..rule_count {
            if acc.steps >= self.cap {
                acc.capped = true;
                return;
            }
            acc.steps += 1;
            for result in apply_rule(i, input) {
                // avoid infinite loop (PermutationRuleCascade.cs:39)
                if !self.multi_app || key(input) != key(&result) {
                    let next = if self.multi_app { i } else { i + 1 };
                    self.permutation_rec(rule_count, &result, next, apply_rule, key, acc);
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
        T: Clone,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        let mut acc = Acc::new();
        // C#: `!MultipleApplication ? new HashSet<int>() : null`.
        let applied: Option<Vec<bool>> = if self.multi_app {
            None
        } else {
            Some(vec![false; rule_count])
        };
        self.combination_rec(rule_count, &input, applied, apply_rule, key, &mut acc);
        CascadeOutput {
            words: acc.items,
            capped: acc.capped,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn combination_rec<T, K, A, F>(
        &self,
        rule_count: usize,
        input: &T,
        applied: Option<Vec<bool>>,
        apply_rule: &A,
        key: &F,
        acc: &mut Acc<T, K>,
    ) where
        T: Clone,
        K: Hash + Eq,
        A: Fn(usize, &T) -> Vec<T>,
        F: Fn(&T) -> K,
    {
        for i in 0..rule_count {
            if applied.as_ref().is_none_or(|a| !a[i]) {
                if acc.steps >= self.cap {
                    acc.capped = true;
                    return;
                }
                acc.steps += 1;
                for result in apply_rule(i, input) {
                    // avoid infinite loop (CombinationRuleCascade.cs:41)
                    if key(input) != key(&result) {
                        let next = applied.as_ref().map(|a| {
                            let mut a2 = a.clone();
                            a2[i] = true;
                            a2
                        });
                        self.combination_rec(rule_count, &result, next, apply_rule, key, acc);
                    }
                    acc.add(key(&result), result);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A mock rule set over integers: each "rule" i adds a distinct power of ten if a decimal digit
    // is free, so applications commute and terminate — letting us reason the reachable set by hand
    // and check each cascade's traversal against the C# recursion exactly.

    /// rule i: if digit i (10^i place) of `n` is 0, set it to (i+1); else no output (doesn't apply).
    fn digit_rule(i: usize, n: &i64) -> Vec<i64> {
        let place = 10i64.pow(i as u32);
        let digit = (n / place) % 10;
        if digit == 0 {
            vec![n + (i as i64 + 1) * place]
        } else {
            vec![]
        }
    }

    const UNCAPPED: usize = usize::MAX;

    #[test]
    fn linear_applies_first_applicable_then_recurses_in_order() {
        // Linear over rules [0,1]: from 0, rule0 fires →1, then rule1 fires →21. rule0 can't refire
        // (multi_app=false, recurse at i+1). Terminal = 21. Non-multi: each rule at most once, in
        // order. Output = {21} (only the terminal derivation is added).
        let c = Cascade::new(false, UNCAPPED);
        let out = c.linear(2, 0i64, &digit_rule, &|n: &i64| *n);
        assert_eq!(out.words, vec![21]);
        assert!(!out.capped);
    }

    #[test]
    fn permutation_adds_every_intermediate_subset_in_index_order() {
        // Permutation (multi_app=false) over [0,1] from 0: adds rule0's result (1), then from 1
        // adds rule1's result (21); also at top level rule1 from 0 → 20. Index-ordered subsets:
        // {1, 21, 20}. Every intermediate is added (unlike linear).
        let c = Cascade::new(false, UNCAPPED);
        let mut got = c.permutation(2, 0i64, &digit_rule, &|n: &i64| *n).words;
        got.sort();
        assert_eq!(got, vec![1, 20, 21]);
    }

    #[test]
    fn combination_explores_all_orderings() {
        // Combination (multi_app=false, applied-set) over [0,1] from 0 reaches every subset in any
        // order: {1, 20, 21} — same *set* as permutation here because the rules commute (21==12),
        // but combination recurses from 0 each level (all orderings), which matters when order
        // changes reachability. Dedup collapses 21 and the 12-ordering to one.
        let c = Cascade::new(false, UNCAPPED);
        let mut got = c.combination(2, 0i64, &digit_rule, &|n: &i64| *n).words;
        got.sort();
        assert_eq!(got, vec![1, 20, 21]);
    }

    #[test]
    fn combination_order_sensitive_reachability() {
        // Rules where order changes what's reachable: r0 adds 1 only to an even number; r1 doubles.
        // From 2: r0→3 (then r1→6), r1→4 (r0 can't, 4 even→r0→5)... we just assert combination
        // reaches strictly more than a fixed order would by checking a value only reachable via
        // r1-then-r0.
        let r = |i: usize, n: &i64| -> Vec<i64> {
            match i {
                0 if n % 2 == 0 => vec![n + 1],
                1 => vec![n * 2],
                _ => vec![],
            }
        };
        let c = Cascade::new(false, UNCAPPED);
        let got = c.combination(2, 2i64, &r, &|n: &i64| *n).words;
        // 2 --r1--> 4 --r0--> 5 is reachable only because combination retries r0 after r1.
        assert!(
            got.contains(&5),
            "combination must reach 5 via r1-then-r0; got {got:?}"
        );
    }

    #[test]
    fn cap_fires_and_reports_partial() {
        // A rule that always produces a new value → unbounded without the cap.
        let r = |_i: usize, n: &i64| vec![n + 1];
        let c = Cascade::new(true, 50);
        let out = c.permutation(1, 0i64, &r, &|n: &i64| *n);
        assert!(out.capped, "cap must fire on an unbounded multi-app walk");
        assert!(out.words.len() <= 51);
    }

    #[test]
    fn dedup_by_key_collapses_equal_results() {
        // Two rules producing the same value → output holds one copy (HashSet semantics).
        let r = |_i: usize, _n: &i64| vec![7i64];
        let c = Cascade::new(false, UNCAPPED);
        let out = c.permutation(2, 0i64, &r, &|n: &i64| *n);
        assert_eq!(out.words, vec![7]);
    }
}
