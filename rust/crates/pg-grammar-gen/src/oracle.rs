//! Bounded Morpher-as-generator sweep (design doc §3): `pg_parse::Morpher::generate_words` runs
//! the REAL synthesis pipeline + validity gate, so it is ground truth for what a correct engine
//! should recall -- this module's own job is only the BULK SWEEP (root × applicable-rule-subset)
//! around that single-call primitive, under the mandatory safety bounds design doc §3 requires.
//!
//! ## Why this lives behind the `oracle` feature, not as a plain dev-dependency
//! `pg-foma`'s own gate files (`tests/phase_c_*.rs`) need to call this module too, and those are a
//! DIFFERENT crate (`pg-foma`) that depends on `pg-grammar-gen` as an ordinary dev-dependency --
//! a plain `#[cfg(test)]`-only or `[dev-dependencies]`-only oracle would be invisible to them (a
//! dev-dependency's dev-dependencies, and a crate's own `#[cfg(test)]` items, are never part of
//! its public library surface). Gating the `pg-parse` dependency behind a Cargo feature (this
//! crate's `Cargo.toml`, `oracle = ["dep:pg-parse"]`) is the design doc's own named alternative
//! ("if a lib dep is cleaner, use a feature") and keeps `pg-grammar-gen`'s DEFAULT build
//! `pg-grammar`-only, matching design doc §2's "Dep: pg-grammar only" for the render/recipe
//! surface -- `oracle` is opt-in, read by nobody unless a caller enables it.
//!
//! ## MANDATORY safety bounds (design doc §3 -- hangs are documented repo history, not folklore)
//! 1. Never `Morpher::new(g, usize::MAX)` -- a bounded step cap ([`OracleOpts::step_cap`],
//!    default 20,000, the same default the P6/Aweti investigation settled on after `usize::MAX`
//!    hung >10 minutes on a real corpus word).
//! 2. [`OracleOpts::word_timeout`] as the ORTHOGONAL wall-clock bound (`Morpher::
//!    with_word_timeout`) -- the step cap alone under-bounds a bulk sweep because synthesis-side
//!    `StepBudget` is per-(stratum, candidate), not cumulative across the whole sweep.
//! 3. The sweep itself is bounded: [`OracleOpts::max_rules_per_root`] caps how many single-rule
//!    "other morphemes" combinations are tried per root (depth 1 -- bare root, plus each
//!    individually-applicable rule once; stage 2's own scale-sweep recipes are expected to widen
//!    this if a deeper combination is ever needed), and [`OracleOpts::max_total_words`] caps the
//!    deduplicated, deterministically-truncated (sorted, then take) total word list size.
//!
//! Stage-1 recipes are sized so the oracle is cheap BY CONSTRUCTION (design doc §3's own
//! framing) -- these bounds are a safety net for a mis-sized recipe, not something stage-1 gates
//! are expected to ever hit.

use std::time::Duration;

use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};
use pg_parse::morpher::GenMorpheme;
use pg_parse::Morpher;

/// Bounds for [`sweep`] (module doc's numbered list).
#[derive(Debug, Clone, Copy)]
pub struct OracleOpts {
    /// Bound 1: `Morpher::new`'s own step cap.
    pub step_cap: usize,
    /// Bound 2: `Morpher::with_word_timeout`'s wall-clock deadline.
    pub word_timeout: Option<Duration>,
    /// Bound 3a: how many single-rule combinations to try per root (depth 1 -- see module doc).
    pub max_rules_per_root: usize,
    /// Bound 3b: total sweep output size, after dedup, deterministic (sorted-then-truncated).
    pub max_total_words: usize,
}

impl Default for OracleOpts {
    fn default() -> Self {
        OracleOpts {
            step_cap: 20_000,
            word_timeout: Some(Duration::from_millis(500)),
            max_rules_per_root: 8,
            max_total_words: 1_000,
        }
    }
}

/// One oracle-generated word: which root/rule combination produced it, and the surface form
/// itself. `mrule` is `None` for a bare-root generation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct OracleWord {
    pub root: LexEntryId,
    pub mrule: Option<MRuleId>,
    pub surface: String,
}

/// Every `AffixProcess`-kind rule in the grammar, in document order, capped at
/// `opts.max_rules_per_root` (module doc bound 3a). `Realizational`/`Compounding` rules are
/// skipped -- `Morpher::generate_words`'s own `others: &[GenMorpheme]` contract (that function's
/// doc) only ever takes `GenMorpheme::Rule` for an ordinary affix-process rule or
/// `GenMorpheme::NonHead` for a compounding non-head root, and stage 1's circumfix recipe has no
/// compounding/realizational material to exercise anyway.
fn candidate_rules(g: &Grammar, opts: &OracleOpts) -> Vec<MRuleId> {
    g.mrules
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            matches!(r, MorphRuleDef::AffixProcess(_)).then_some(MRuleId(i as u32))
        })
        .take(opts.max_rules_per_root)
        .collect()
}

/// Bounded sweep (module doc): for every entry in `roots`, generate the bare-root word plus one
/// word per candidate rule in `rules` (depth 1, module doc bound 3a) via
/// `Morpher::generate_words`, deduplicate `(root, mrule, surface)` triples, sort them
/// (determinism), and truncate to `opts.max_total_words` (module doc bound 3b).
///
/// `rules`, not auto-discovered from the whole grammar, is the caller's OWN candidate list
/// (already capped by [`candidate_rules`] if the caller wants the "every `AffixProcess` rule"
/// default) -- callers that already know exactly which rule(s) their recipe cares about (GATE 2:
/// its own circumfix rules) should pass those directly rather than re-deriving them.
pub fn sweep(
    g: &Grammar,
    roots: &[LexEntryId],
    rules: &[MRuleId],
    opts: &OracleOpts,
) -> Vec<OracleWord> {
    let morpher = Morpher::new(g, opts.step_cap).with_word_timeout(opts.word_timeout);

    let mut out: Vec<OracleWord> = Vec::new();
    for &root in roots {
        for surface in morpher.generate_words(root, &[], pg_featstruct::FeatureStruct::EMPTY) {
            out.push(OracleWord {
                root,
                mrule: None,
                surface,
            });
        }
        for &mrule in rules {
            for surface in morpher.generate_words(
                root,
                &[GenMorpheme::Rule(mrule)],
                pg_featstruct::FeatureStruct::EMPTY,
            ) {
                out.push(OracleWord {
                    root,
                    mrule: Some(mrule),
                    surface,
                });
            }
        }
    }

    out.sort();
    out.dedup();
    out.truncate(opts.max_total_words);
    out
}

/// [`sweep`], but with `rules` auto-discovered from the grammar (module doc's [`candidate_rules`])
/// rather than caller-supplied -- convenient for `tests/self_check.rs`'s generic per-builder
/// round-trip, where the exact rule ids aren't already in hand.
pub fn sweep_all_rules(g: &Grammar, roots: &[LexEntryId], opts: &OracleOpts) -> Vec<OracleWord> {
    let rules = candidate_rules(g, opts);
    sweep(g, roots, &rules, opts)
}
